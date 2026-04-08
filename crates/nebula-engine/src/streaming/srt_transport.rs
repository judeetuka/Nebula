//! SRT transport -- accessed via plugin SDK's `platform_invoke("engine:srt:*")`,
//! not directly through FFI. All types are excluded from flutter_rust_bridge codegen.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;
use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use srt_tokio::SrtSocket;

pub struct SrtConfig {
    pub latency_ms: u32,
    pub max_bandwidth: u64,
    pub passphrase: Option<String>,
}

impl Default for SrtConfig {
    fn default() -> Self {
        Self { latency_ms: 120, max_bandwidth: 0, passphrase: None }
    }
}

pub struct SrtSender { socket: SrtSocket }

impl SrtSender {
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        let socket = SrtSocket::builder().call(addr, None).await.context("SRT caller failed to connect")?;
        Ok(Self { socket })
    }
    pub async fn send_frame(&mut self, data: &[u8]) -> Result<()> {
        let packet = (Instant::now(), Bytes::copy_from_slice(data));
        self.socket.send(packet).await.context("SRT send_frame failed")
    }
    pub async fn close(self) -> Result<()> {
        let mut socket = self.socket;
        socket.close().await.context("SRT sender close failed")
    }
}

pub struct SrtReceiver { socket: SrtSocket }

impl SrtReceiver {
    pub async fn listen(port: u16) -> Result<Self> {
        let socket = SrtSocket::builder()
            .listen_on(port)
            .await
            .context("SRT listener failed to bind")?;
        Ok(Self { socket })
    }
    pub async fn recv_frame(&mut self) -> Result<(Instant, Bytes)> {
        self.socket.next().await
            .ok_or_else(|| anyhow::anyhow!("SRT stream ended"))?
            .context("SRT recv_frame failed")
    }
    pub async fn close(self) -> Result<()> {
        drop(self.socket);
        Ok(())
    }
}

pub struct StreamManager {
    active_senders: HashMap<String, SrtSender>,
    active_receivers: HashMap<String, SrtReceiver>,
}

impl StreamManager {
    pub fn new() -> Self {
        Self { active_senders: HashMap::new(), active_receivers: HashMap::new() }
    }
    pub async fn start_sending(&mut self, stream_id: &str, target: SocketAddr) -> Result<()> {
        let sender = SrtSender::connect(target).await?;
        self.active_senders.insert(stream_id.to_string(), sender);
        Ok(())
    }
    pub async fn start_receiving(&mut self, stream_id: &str, port: u16) -> Result<()> {
        let receiver = SrtReceiver::listen(port).await?;
        self.active_receivers.insert(stream_id.to_string(), receiver);
        Ok(())
    }
    pub async fn send_frame(&mut self, stream_id: &str, data: &[u8]) -> Result<()> {
        let sender = self.active_senders.get_mut(stream_id)
            .ok_or_else(|| anyhow::anyhow!("No active sender: {stream_id}"))?;
        sender.send_frame(data).await
    }
    pub async fn stop_stream(&mut self, stream_id: &str) -> Result<()> {
        if let Some(sender) = self.active_senders.remove(stream_id) {
            sender.close().await?;
            return Ok(());
        }
        if let Some(receiver) = self.active_receivers.remove(stream_id) {
            receiver.close().await?;
            return Ok(());
        }
        anyhow::bail!("No active stream with id: {stream_id}");
    }
    pub fn active_streams(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.active_senders.keys()
            .chain(self.active_receivers.keys()).cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srt_config_defaults() {
        let cfg = SrtConfig::default();
        assert_eq!(cfg.latency_ms, 120);
        assert_eq!(cfg.max_bandwidth, 0);
        assert!(cfg.passphrase.is_none());
    }

    #[test]
    fn test_stream_manager_new_is_empty() {
        let mgr = StreamManager::new();
        assert!(mgr.active_streams().is_empty());
        assert!(mgr.active_senders.is_empty());
        assert!(mgr.active_receivers.is_empty());
    }

    #[tokio::test]
    async fn test_stop_stream_unknown_id_errors() {
        let mut mgr = StreamManager::new();
        assert!(mgr.stop_stream("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_send_frame_unknown_stream_errors() {
        let mut mgr = StreamManager::new();
        assert!(mgr.send_frame("nope", b"hello").await.is_err());
    }

    #[tokio::test]
    async fn test_roundtrip_localhost() {
        let port: u16 = 9901;
        let recv_handle = tokio::spawn(async move {
            let mut rx = SrtReceiver::listen(port).await.unwrap();
            let (_ts, data) = rx.recv_frame().await.unwrap();
            rx.close().await.unwrap();
            data
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let addr: SocketAddr = ([127, 0, 0, 1], port).into();
        let mut tx = SrtSender::connect(addr).await.unwrap();
        tx.send_frame(b"hello-srt").await.unwrap();
        tx.close().await.unwrap();
        let received = recv_handle.await.unwrap();
        assert_eq!(&received[..], b"hello-srt");
    }
}
