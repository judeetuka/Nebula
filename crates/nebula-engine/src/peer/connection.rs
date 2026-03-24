//! Single peer-to-peer TCP connection.
//!
//! Provides framed, encrypted communication over a TCP stream using the
//! peer protocol wire format: `[4-byte big-endian length][encrypted payload]`.

use std::net::SocketAddr;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::debug;

use nebula_core::security::keys::{AesKey, HmacKey, KeyPair};

use super::protocol::{self, PeerMessage};

/// Maximum allowed peer frame size (4 MB).
const MAX_FRAME_SIZE: u32 = 4 * 1024 * 1024;

/// Duration after which a peer is considered dead if no message was received.
const ALIVE_TIMEOUT_SECS: u64 = 30;

/// Write a single peer frame to an async writer.
///
/// Wire format: `[4-byte big-endian length][payload]`.
pub async fn write_peer_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> Result<()> {
    let len = payload.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a single peer frame from an async reader.
///
/// Returns the payload bytes (without the length prefix).
pub async fn read_peer_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .await
        .context("Failed to read peer frame length")?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_SIZE {
        bail!("Peer frame too large: {} bytes (max {})", len, MAX_FRAME_SIZE);
    }
    let mut buf = vec![0u8; len as usize];
    reader
        .read_exact(&mut buf)
        .await
        .context("Failed to read peer frame body")?;
    Ok(buf)
}

/// A single peer-to-peer TCP connection with encrypted messaging.
pub struct PeerConnection {
    node_id: String,
    stream: TcpStream,
    hmac_key: HmacKey,
    aes_key: AesKey,
    last_seen: Instant,
}

impl PeerConnection {
    /// Connect to a peer at the given address.
    pub async fn connect(node_id: &str, addr: SocketAddr, keys: &KeyPair) -> Result<Self> {
        let stream = TcpStream::connect(addr)
            .await
            .with_context(|| format!("Failed to connect to peer {} at {}", node_id, addr))?;
        debug!(peer = node_id, addr = %addr, "TCP connection to peer established");
        Ok(Self {
            node_id: node_id.to_string(),
            stream,
            hmac_key: keys.hmac.clone(),
            aes_key: keys.aes.clone(),
            last_seen: Instant::now(),
        })
    }

    /// Wrap an already-accepted TCP stream as a peer connection.
    pub fn from_stream(node_id: &str, stream: TcpStream, keys: &KeyPair) -> Self {
        Self {
            node_id: node_id.to_string(),
            stream,
            hmac_key: keys.hmac.clone(),
            aes_key: keys.aes.clone(),
            last_seen: Instant::now(),
        }
    }

    /// Returns the remote peer's node ID.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Set the node ID.
    pub fn set_node_id(&mut self, node_id: String) {
        self.node_id = node_id;
    }

    /// Send a `PeerMessage` over this connection.
    pub async fn send(&mut self, msg: &PeerMessage) -> Result<()> {
        let encoded = protocol::encode_message(msg, &self.hmac_key, &self.aes_key)?;
        write_peer_frame(&mut self.stream, &encoded).await
    }

    /// Receive the next `PeerMessage` from this connection.
    pub async fn recv(&mut self) -> Result<PeerMessage> {
        let frame = read_peer_frame(&mut self.stream).await?;
        let msg = protocol::decode_message(&frame, &self.hmac_key, &self.aes_key)?;
        self.last_seen = Instant::now();
        Ok(msg)
    }

    /// Returns `true` if the last received message was within the alive timeout.
    pub fn is_alive(&self) -> bool {
        self.last_seen.elapsed().as_secs() < ALIVE_TIMEOUT_SECS
    }

    /// Close the connection.
    pub async fn close(&mut self) -> Result<()> {
        self.stream.shutdown().await.context("Failed to shutdown peer TCP stream")?;
        debug!(peer = %self.node_id, "Peer connection closed");
        Ok(())
    }

    /// Consume this connection and return the inner parts.
    pub fn into_parts(self) -> (TcpStream, HmacKey, AesKey, String) {
        (self.stream, self.hmac_key, self.aes_key, self.node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::security::keys::KeyPair;
    use tokio::net::TcpListener;

    fn test_keys() -> KeyPair {
        KeyPair::derive_from_secret(b"conn-test-secret").unwrap()
    }

    #[tokio::test]
    async fn test_frame_roundtrip() {
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        let payload = b"hello peer";
        write_peer_frame(&mut writer, payload).await.unwrap();
        drop(writer);
        let received = read_peer_frame(&mut reader).await.unwrap();
        assert_eq!(received, payload);
    }

    #[tokio::test]
    async fn test_frame_wire_format_big_endian() {
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        let payload = vec![0xAA; 300];
        write_peer_frame(&mut writer, &payload).await.unwrap();
        drop(writer);
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await.unwrap();
        assert_eq!(u32::from_be_bytes(len_buf), 300);
    }

    #[tokio::test]
    async fn test_frame_too_large_rejected() {
        let (mut writer, mut reader) = tokio::io::duplex(64);
        let fake_len: u32 = MAX_FRAME_SIZE + 1;
        writer.write_all(&fake_len.to_be_bytes()).await.unwrap();
        drop(writer);
        let result = read_peer_frame(&mut reader).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[tokio::test]
    async fn test_frame_empty_payload() {
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_peer_frame(&mut writer, &[]).await.unwrap();
        drop(writer);
        let received = read_peer_frame(&mut reader).await.unwrap();
        assert!(received.is_empty());
    }

    #[tokio::test]
    async fn test_peer_connection_send_recv() {
        let keys = test_keys();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let keys_clone = keys.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut conn = PeerConnection::from_stream("client-node", stream, &keys_clone);
            let msg = conn.recv().await.unwrap();
            assert_eq!(msg, PeerMessage::Ping { timestamp: 42 });
            conn.send(&PeerMessage::Pong { timestamp: 43 }).await.unwrap();
        });

        let mut client = PeerConnection::connect("server-node", addr, &keys).await.unwrap();
        client.send(&PeerMessage::Ping { timestamp: 42 }).await.unwrap();
        let response = client.recv().await.unwrap();
        assert_eq!(response, PeerMessage::Pong { timestamp: 43 });
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_peer_connection_multiple_messages() {
        let keys = test_keys();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let keys_clone = keys.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut conn = PeerConnection::from_stream("client", stream, &keys_clone);
            for i in 0..5 {
                let msg = conn.recv().await.unwrap();
                assert_eq!(msg, PeerMessage::Ping { timestamp: i });
                conn.send(&PeerMessage::Pong { timestamp: i }).await.unwrap();
            }
        });

        let mut client = PeerConnection::connect("server", addr, &keys).await.unwrap();
        for i in 0..5 {
            client.send(&PeerMessage::Ping { timestamp: i }).await.unwrap();
            let resp = client.recv().await.unwrap();
            assert_eq!(resp, PeerMessage::Pong { timestamp: i });
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_peer_connection_is_alive() {
        let keys = test_keys();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let keys_clone = keys.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut conn = PeerConnection::from_stream("client", stream, &keys_clone);
            conn.send(&PeerMessage::Pong { timestamp: 1 }).await.unwrap();
        });

        let mut client = PeerConnection::connect("server", addr, &keys).await.unwrap();
        assert!(client.is_alive());
        let _msg = client.recv().await.unwrap();
        assert!(client.is_alive());
    }

    #[tokio::test]
    async fn test_peer_connection_connect_to_nonexistent_fails() {
        let keys = test_keys();
        let result = PeerConnection::connect("ghost", "127.0.0.1:1".parse().unwrap(), &keys).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_peer_connection_large_data_message() {
        let keys = test_keys();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let large_payload = vec![0xABu8; 50_000];
        let large_clone = large_payload.clone();

        let keys_clone = keys.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut conn = PeerConnection::from_stream("client", stream, &keys_clone);
            let msg = conn.recv().await.unwrap();
            match msg {
                PeerMessage::DataRequest { payload, .. } => assert_eq!(payload, large_clone),
                other => panic!("Expected DataRequest, got {:?}", other),
            }
        });

        let mut client = PeerConnection::connect("server", addr, &keys).await.unwrap();
        client.send(&PeerMessage::DataRequest {
            request_id: "big".into(), action: "upload".into(), payload: large_payload,
        }).await.unwrap();
        server.await.unwrap();
    }
}
