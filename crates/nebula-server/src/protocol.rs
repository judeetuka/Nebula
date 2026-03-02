//! Wire protocol helpers for reading/writing nebula-core messages over async streams.
//!
//! Unlike rathole's fixed-size packet approach (using `lazy_static` pre-calculated sizes),
//! NEBULA uses length-prefixed messages because the `Hello` enum now has variable-size
//! variants (e.g. `NodeRegistrationHello` contains `NodeId` and `ClusterId`).
//!
//! Wire format: [u32 LE length][bincode payload]

use anyhow::{bail, Context, Result};
use bytes::{Bytes, BytesMut};
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::trace;

// Re-export nebula-core protocol types so the rest of the crate can use them
// through `crate::protocol::*` just like rathole did.
pub use nebula_core::protocol::messages::{
    Ack, Auth, ControlChannelCmd, DataChannelCmd, Hello,
};
pub use nebula_core::protocol::version::{ProtocolVersion, CURRENT_PROTO_VERSION};
pub use nebula_core::types::{Digest, HASH_WIDTH_IN_BYTES};

/// Compute SHA-256 digest of data (compatible with rathole's `digest` function).
pub fn digest(data: &[u8]) -> Digest {
    use sha2::{Digest as Sha2Digest, Sha256};
    let d = Sha256::new().chain_update(data).finalize();
    d.into()
}

/// Format an `Ack` as a human-readable string.
/// This is a free function instead of a `Display` impl because `Ack` is
/// defined in `nebula-core` (orphan rule).
pub fn ack_description(ack: &Ack) -> &'static str {
    match ack {
        Ack::Ok => "Ok",
        Ack::ServiceNotExist => "Service not exist",
        Ack::AuthFailed => "Incorrect token",
        Ack::ClusterNotFound => "Cluster not found",
        Ack::NodeAlreadyRegistered => "Node already registered",
        Ack::RegistrationAccepted { .. } => "Registration accepted",
        Ack::RotationInProgress => "Rotation in progress",
    }
}

// ── Length-prefixed wire helpers ─────────────────────────────────────────────

/// Maximum allowed message size (1 MB). Prevents allocation attacks.
const MAX_MESSAGE_SIZE: u32 = 1024 * 1024;

/// Write a bincode-serialized message with a u32 LE length prefix.
pub async fn write_msg<T, M>(conn: &mut T, msg: &M) -> Result<()>
where
    T: AsyncWrite + Unpin,
    M: serde::Serialize,
{
    let payload = bincode::serialize(msg).with_context(|| "Failed to serialize message")?;
    let len = payload.len() as u32;
    conn.write_all(&len.to_le_bytes()).await?;
    conn.write_all(&payload).await?;
    conn.flush().await?;
    Ok(())
}

/// Read a length-prefixed bincode message.
async fn read_msg_bytes<T>(conn: &mut T) -> Result<Vec<u8>>
where
    T: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    conn.read_exact(&mut len_buf)
        .await
        .with_context(|| "Failed to read message length")?;
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_MESSAGE_SIZE {
        bail!("Message too large: {} bytes (max {})", len, MAX_MESSAGE_SIZE);
    }
    let mut buf = vec![0u8; len as usize];
    conn.read_exact(&mut buf)
        .await
        .with_context(|| "Failed to read message body")?;
    Ok(buf)
}

/// Read a `Hello` message from the stream.
pub async fn read_hello<T: AsyncRead + Unpin>(conn: &mut T) -> Result<Hello> {
    let buf = read_msg_bytes(conn).await?;
    let hello: Hello =
        bincode::deserialize(&buf).with_context(|| "Failed to deserialize Hello")?;

    match &hello {
        Hello::ControlChannelHello(v, _) | Hello::DataChannelHello(v, _) => {
            if *v != CURRENT_PROTO_VERSION {
                bail!(
                    "Protocol version mismatch. Expected {}, got {}. Please update the client.",
                    CURRENT_PROTO_VERSION,
                    v
                );
            }
        }
        Hello::NodeRegistrationHello(v, _, _) => {
            if *v != CURRENT_PROTO_VERSION {
                bail!(
                    "Protocol version mismatch. Expected {}, got {}. Please update the node.",
                    CURRENT_PROTO_VERSION,
                    v
                );
            }
        }
    }

    Ok(hello)
}

/// Read an `Auth` message from the stream.
pub async fn read_auth<T: AsyncRead + Unpin>(conn: &mut T) -> Result<Auth> {
    let buf = read_msg_bytes(conn).await?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize Auth")
}

/// Read an `Ack` message from the stream.
#[allow(dead_code)]
pub async fn read_ack<T: AsyncRead + Unpin>(conn: &mut T) -> Result<Ack> {
    let buf = read_msg_bytes(conn).await?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize Ack")
}

/// Read a `ControlChannelCmd` from the stream.
pub async fn read_control_cmd<T: AsyncRead + Unpin>(conn: &mut T) -> Result<ControlChannelCmd> {
    let buf = read_msg_bytes(conn).await?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize ControlChannelCmd")
}

/// Read a `DataChannelCmd` from the stream.
#[allow(dead_code)]
pub async fn read_data_cmd<T: AsyncRead + Unpin>(conn: &mut T) -> Result<DataChannelCmd> {
    let buf = read_msg_bytes(conn).await?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize DataChannelCmd")
}

// ── UDP traffic helpers (from rathole) ──────────────────────────────────────

type UdpPacketLen = u16;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct UdpHeader {
    from: SocketAddr,
    len: UdpPacketLen,
}

#[derive(Debug)]
pub struct UdpTraffic {
    pub from: SocketAddr,
    pub data: Bytes,
}

impl UdpTraffic {
    pub async fn write<T: AsyncWrite + Unpin>(&self, writer: &mut T) -> Result<()> {
        let hdr = UdpHeader {
            from: self.from,
            len: self.data.len() as UdpPacketLen,
        };

        let v = bincode::serialize(&hdr).unwrap();

        trace!("Write {:?} of length {}", hdr, v.len());
        writer.write_u8(v.len() as u8).await?;
        writer.write_all(&v).await?;

        writer.write_all(&self.data).await?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn write_slice<T: AsyncWrite + Unpin>(
        writer: &mut T,
        from: SocketAddr,
        data: &[u8],
    ) -> Result<()> {
        let hdr = UdpHeader {
            from,
            len: data.len() as UdpPacketLen,
        };

        let v = bincode::serialize(&hdr).unwrap();

        trace!("Write {:?} of length {}", hdr, v.len());
        writer.write_u8(v.len() as u8).await?;
        writer.write_all(&v).await?;

        writer.write_all(data).await?;

        Ok(())
    }

    pub async fn read<T: AsyncRead + Unpin>(reader: &mut T, hdr_len: u8) -> Result<UdpTraffic> {
        let mut buf = vec![0; hdr_len as usize];
        reader
            .read_exact(&mut buf)
            .await
            .with_context(|| "Failed to read udp header")?;

        let hdr: UdpHeader =
            bincode::deserialize(&buf).with_context(|| "Failed to deserialize UdpHeader")?;

        trace!("hdr {:?}", hdr);

        let mut data = BytesMut::new();
        data.resize(hdr.len as usize, 0);
        reader.read_exact(&mut data).await?;

        Ok(UdpTraffic {
            from: hdr.from,
            data: data.freeze(),
        })
    }
}
