//! Peer connection manager -- maintains the TCP mesh for direct node-to-node
//! communication.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch, Mutex as TokioMutex, RwLock};
use tracing::{debug, error, info, warn};

use nebula_core::security::keys::KeyPair;

use super::connection::{read_peer_frame, write_peer_frame};
use super::protocol::{self, PeerMessage};

/// Health metrics for a connected peer.
#[derive(Debug, Clone)]
pub struct PeerHealth {
    pub score: f32,
    pub battery_level: u8,
    pub cpu_load: f32,
    pub memory_available_mb: u32,
    pub active_tasks: u16,
}

impl Default for PeerHealth {
    fn default() -> Self {
        Self { score: 0.0, battery_level: 0, cpu_load: 0.0, memory_available_mb: 0, active_tasks: 0 }
    }
}

/// Internal state for a single peer in the mesh.
struct PeerState {
    node_id: String,
    #[allow(dead_code)]
    address: SocketAddr,
    writer: Arc<TokioMutex<OwnedWriteHalf>>,
    #[allow(dead_code)]
    last_heartbeat: Instant,
    #[allow(dead_code)]
    health: PeerHealth,
}

/// Manages the peer-to-peer TCP mesh.
pub struct PeerManager {
    local_node_id: String,
    keys: KeyPair,
    peers: Arc<RwLock<HashMap<String, PeerState>>>,
    listen_port: u16,
    message_tx: mpsc::Sender<(String, PeerMessage)>,
    message_rx: Option<mpsc::Receiver<(String, PeerMessage)>>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl PeerManager {
    /// Create a new `PeerManager`.
    pub fn new(node_id: &str, keys: KeyPair, listen_port: u16) -> Self {
        let (message_tx, message_rx) = mpsc::channel(256);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            local_node_id: node_id.to_string(),
            keys,
            peers: Arc::new(RwLock::new(HashMap::new())),
            listen_port,
            message_tx,
            message_rx: Some(message_rx),
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Start the TCP listener and accept loop.
    pub async fn start(&self) -> Result<()> {
        let addr: SocketAddr = format!("0.0.0.0:{}", self.listen_port).parse()?;
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("Failed to bind peer listener on {}", addr))?;
        info!(port = self.listen_port, "Peer mesh listener started");

        let peers = Arc::clone(&self.peers);
        let keys = self.keys.clone();
        let message_tx = self.message_tx.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, remote_addr)) => {
                                debug!(addr = %remote_addr, "Incoming peer connection");
                                let peers = Arc::clone(&peers);
                                let keys = keys.clone();
                                let message_tx = message_tx.clone();
                                let shutdown_rx = shutdown_rx.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_incoming(
                                        stream, remote_addr, peers, keys, message_tx, shutdown_rx,
                                    ).await {
                                        warn!(addr = %remote_addr, error = %e, "Failed to handle incoming peer");
                                    }
                                });
                            }
                            Err(e) => error!(error = %e, "Failed to accept peer connection"),
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("Peer mesh listener shutting down");
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    /// Connect to a new peer and register it in the mesh.
    pub async fn add_peer(&self, node_id: &str, addr: SocketAddr) -> Result<()> {
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .with_context(|| format!("Failed to connect to peer {} at {}", node_id, addr))?;
        debug!(peer = node_id, addr = %addr, "Outbound peer connection established");

        let (reader, mut writer) = stream.into_split();

        // Send handshake heartbeat
        let handshake = PeerMessage::Heartbeat {
            node_id: self.local_node_id.clone(), score: 0.0, battery_level: 0,
            cpu_load: 0.0, memory_available_mb: 0, active_tasks: 0,
            timestamp: chrono::Utc::now().timestamp(),
        };
        let encoded = protocol::encode_message(&handshake, &self.keys.hmac, &self.keys.aes)?;
        write_peer_frame(&mut writer, &encoded).await?;

        let writer = Arc::new(TokioMutex::new(writer));
        let state = PeerState {
            node_id: node_id.to_string(), address: addr, writer: Arc::clone(&writer),
            last_heartbeat: Instant::now(), health: PeerHealth::default(),
        };
        self.peers.write().await.insert(node_id.to_string(), state);

        spawn_read_loop(
            node_id.to_string(), reader, self.keys.clone(),
            self.message_tx.clone(), Arc::clone(&self.peers), self.shutdown_rx.clone(),
        );
        info!(peer = node_id, addr = %addr, "Peer added to mesh");
        Ok(())
    }

    /// Remove a peer from the mesh.
    pub async fn remove_peer(&self, node_id: &str) -> Result<()> {
        let state = self.peers.write().await.remove(node_id);
        if let Some(state) = state {
            let mut writer = state.writer.lock().await;
            let _ = writer.shutdown().await;
            info!(peer = node_id, "Peer removed from mesh");
        } else {
            warn!(peer = node_id, "remove_peer called but peer not found");
        }
        Ok(())
    }

    /// Send a message to all connected peers.
    pub async fn broadcast(&self, msg: PeerMessage) -> Result<()> {
        let encoded = protocol::encode_message(&msg, &self.keys.hmac, &self.keys.aes)?;
        let writers: Vec<(String, Arc<TokioMutex<OwnedWriteHalf>>)> = {
            let peers = self.peers.read().await;
            peers.values().map(|s| (s.node_id.clone(), Arc::clone(&s.writer))).collect()
        };
        for (peer_id, writer) in writers {
            let mut w = writer.lock().await;
            if let Err(e) = write_peer_frame(&mut *w, &encoded).await {
                warn!(peer = %peer_id, error = %e, "Failed to send broadcast to peer");
            }
        }
        Ok(())
    }

    /// Send a message to a specific peer.
    pub async fn send_to(&self, node_id: &str, msg: PeerMessage) -> Result<()> {
        let encoded = protocol::encode_message(&msg, &self.keys.hmac, &self.keys.aes)?;
        let writer = {
            let peers = self.peers.read().await;
            peers.get(node_id).map(|s| Arc::clone(&s.writer))
                .ok_or_else(|| anyhow::anyhow!("Peer not found: {}", node_id))?
        };
        let mut w = writer.lock().await;
        write_peer_frame(&mut *w, &encoded).await?;
        Ok(())
    }

    /// Take the message receiver channel.
    pub fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<(String, PeerMessage)>> {
        self.message_rx.take()
    }

    /// Returns the number of currently connected peers.
    pub async fn connected_peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    /// Returns the node IDs of all connected peers.
    pub async fn peer_ids(&self) -> Vec<String> {
        self.peers.read().await.keys().cloned().collect()
    }

    /// Shut down the peer manager.
    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.shutdown_tx.send(true);
        let peers: HashMap<String, PeerState> = {
            let mut guard = self.peers.write().await;
            std::mem::take(&mut *guard)
        };
        for (node_id, state) in peers {
            let mut writer = state.writer.lock().await;
            if let Err(e) = writer.shutdown().await {
                warn!(peer = %node_id, error = %e, "Error shutting down peer writer");
            }
        }
        info!("Peer manager shut down");
        Ok(())
    }
}

/// Handle an incoming TCP connection.
async fn handle_incoming(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    peers: Arc<RwLock<HashMap<String, PeerState>>>,
    keys: KeyPair,
    message_tx: mpsc::Sender<(String, PeerMessage)>,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let (mut reader, writer) = stream.into_split();
    let frame = read_peer_frame(&mut reader).await?;
    let first_msg = protocol::decode_message(&frame, &keys.hmac, &keys.aes)?;

    let (node_id, health) = match &first_msg {
        PeerMessage::Heartbeat {
            node_id, score, battery_level, cpu_load, memory_available_mb, active_tasks, ..
        } => {
            let h = PeerHealth {
                score: *score, battery_level: *battery_level, cpu_load: *cpu_load,
                memory_available_mb: *memory_available_mb, active_tasks: *active_tasks,
            };
            (node_id.clone(), h)
        }
        other => anyhow::bail!("Expected Heartbeat as handshake from {}, got {:?}", remote_addr, other),
    };

    info!(peer = %node_id, addr = %remote_addr, "Peer handshake accepted");
    let _ = message_tx.send((node_id.clone(), first_msg)).await;

    let writer = Arc::new(TokioMutex::new(writer));
    let state = PeerState {
        node_id: node_id.clone(), address: remote_addr, writer: Arc::clone(&writer),
        last_heartbeat: Instant::now(), health,
    };
    peers.write().await.insert(node_id.clone(), state);

    spawn_read_loop(node_id, reader, keys, message_tx, peers, shutdown_rx);
    Ok(())
}

/// Spawn a background read loop for a peer connection.
fn spawn_read_loop(
    node_id: String,
    mut reader: OwnedReadHalf,
    keys: KeyPair,
    message_tx: mpsc::Sender<(String, PeerMessage)>,
    peers: Arc<RwLock<HashMap<String, PeerState>>>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                frame_result = read_peer_frame(&mut reader) => {
                    match frame_result {
                        Ok(frame) => {
                            match protocol::decode_message(&frame, &keys.hmac, &keys.aes) {
                                Ok(msg) => {
                                    if let PeerMessage::Heartbeat {
                                        score, battery_level, cpu_load, memory_available_mb, active_tasks, ..
                                    } = &msg {
                                        let mut peers_guard = peers.write().await;
                                        if let Some(state) = peers_guard.get_mut(&node_id) {
                                            state.last_heartbeat = Instant::now();
                                            state.health = PeerHealth {
                                                score: *score, battery_level: *battery_level,
                                                cpu_load: *cpu_load, memory_available_mb: *memory_available_mb,
                                                active_tasks: *active_tasks,
                                            };
                                        }
                                    }
                                    if message_tx.send((node_id.clone(), msg)).await.is_err() {
                                        debug!(peer = %node_id, "Message channel closed");
                                        break;
                                    }
                                }
                                Err(e) => warn!(peer = %node_id, error = %e, "Failed to decode peer message"),
                            }
                        }
                        Err(_) => {
                            info!(peer = %node_id, "Peer connection closed");
                            peers.write().await.remove(&node_id);
                            break;
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    debug!(peer = %node_id, "Read loop shutting down");
                    peers.write().await.remove(&node_id);
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::security::keys::KeyPair;
    use tokio::time::{timeout, Duration};

    fn test_keys() -> KeyPair {
        KeyPair::derive_from_secret(b"manager-test-secret").unwrap()
    }

    #[test]
    fn test_peer_manager_new() {
        let keys = test_keys();
        let mgr = PeerManager::new("node-1", keys, 9000);
        assert_eq!(mgr.local_node_id, "node-1");
        assert_eq!(mgr.listen_port, 9000);
        assert!(mgr.message_rx.is_some());
    }

    #[test]
    fn test_take_message_receiver() {
        let keys = test_keys();
        let mut mgr = PeerManager::new("node-1", keys, 9000);
        assert!(mgr.take_message_receiver().is_some());
        assert!(mgr.take_message_receiver().is_none());
    }

    #[tokio::test]
    async fn test_add_peer_and_count() {
        let keys = test_keys();
        let mgr = PeerManager::new("local", keys.clone(), 0);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = listener.local_addr().unwrap();

        let keys_clone = keys.clone();
        let mock = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, _writer) = stream.into_split();
            let frame = read_peer_frame(&mut reader).await.unwrap();
            let msg = protocol::decode_message(&frame, &keys_clone.hmac, &keys_clone.aes).unwrap();
            match msg {
                PeerMessage::Heartbeat { node_id, .. } => assert_eq!(node_id, "local"),
                other => panic!("Expected Heartbeat, got {:?}", other),
            }
        });

        mgr.add_peer("remote-1", peer_addr).await.unwrap();
        assert_eq!(mgr.connected_peer_count().await, 1);
        assert_eq!(mgr.peer_ids().await, vec!["remote-1".to_string()]);
        mock.await.unwrap();
    }

    #[tokio::test]
    async fn test_add_and_remove_peer() {
        let keys = test_keys();
        let mgr = PeerManager::new("local", keys.clone(), 0);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, _) = stream.into_split();
            let _ = read_peer_frame(&mut reader).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        mgr.add_peer("remote-2", peer_addr).await.unwrap();
        assert_eq!(mgr.connected_peer_count().await, 1);
        mgr.remove_peer("remote-2").await.unwrap();
        assert_eq!(mgr.connected_peer_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_nonexistent_peer_is_ok() {
        let keys = test_keys();
        let mgr = PeerManager::new("local", keys, 0);
        assert!(mgr.remove_peer("ghost").await.is_ok());
    }

    #[tokio::test]
    async fn test_broadcast_to_multiple_peers() {
        let keys = test_keys();
        let mgr = PeerManager::new("master", keys.clone(), 0);

        let l1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a1 = l1.local_addr().unwrap();
        let l2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a2 = l2.local_addr().unwrap();

        let k1 = keys.clone();
        let p1 = tokio::spawn(async move {
            let (s, _) = l1.accept().await.unwrap();
            let (mut r, _) = s.into_split();
            let _ = read_peer_frame(&mut r).await.unwrap();
            let frame = read_peer_frame(&mut r).await.unwrap();
            let msg = protocol::decode_message(&frame, &k1.hmac, &k1.aes).unwrap();
            assert_eq!(msg, PeerMessage::Ping { timestamp: 999 });
        });

        let k2 = keys.clone();
        let p2 = tokio::spawn(async move {
            let (s, _) = l2.accept().await.unwrap();
            let (mut r, _) = s.into_split();
            let _ = read_peer_frame(&mut r).await.unwrap();
            let frame = read_peer_frame(&mut r).await.unwrap();
            let msg = protocol::decode_message(&frame, &k2.hmac, &k2.aes).unwrap();
            assert_eq!(msg, PeerMessage::Ping { timestamp: 999 });
        });

        mgr.add_peer("peer-1", a1).await.unwrap();
        mgr.add_peer("peer-2", a2).await.unwrap();
        assert_eq!(mgr.connected_peer_count().await, 2);
        mgr.broadcast(PeerMessage::Ping { timestamp: 999 }).await.unwrap();

        timeout(Duration::from_secs(5), p1).await.unwrap().unwrap();
        timeout(Duration::from_secs(5), p2).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_send_to_specific_peer() {
        let keys = test_keys();
        let mgr = PeerManager::new("sender", keys.clone(), 0);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let kc = keys.clone();
        let peer = tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            let (mut r, _) = s.into_split();
            let _ = read_peer_frame(&mut r).await.unwrap();
            let frame = read_peer_frame(&mut r).await.unwrap();
            let msg = protocol::decode_message(&frame, &kc.hmac, &kc.aes).unwrap();
            match msg {
                PeerMessage::DataRequest { request_id, action, .. } => {
                    assert_eq!(request_id, "req-42");
                    assert_eq!(action, "fetch");
                }
                other => panic!("Expected DataRequest, got {:?}", other),
            }
        });

        mgr.add_peer("target", addr).await.unwrap();
        mgr.send_to("target", PeerMessage::DataRequest {
            request_id: "req-42".into(), action: "fetch".into(), payload: vec![1, 2, 3],
        }).await.unwrap();
        timeout(Duration::from_secs(5), peer).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_send_to_nonexistent_peer_fails() {
        let keys = test_keys();
        let mgr = PeerManager::new("sender", keys, 0);
        let result = mgr.send_to("ghost", PeerMessage::Ping { timestamp: 1 }).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Peer not found"));
    }

    #[tokio::test]
    async fn test_shutdown_clears_peers() {
        let keys = test_keys();
        let mgr = PeerManager::new("shutting-down", keys.clone(), 0);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            let (mut r, _) = s.into_split();
            let _ = read_peer_frame(&mut r).await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        mgr.add_peer("peer-x", addr).await.unwrap();
        assert_eq!(mgr.connected_peer_count().await, 1);
        mgr.shutdown().await.unwrap();
        assert_eq!(mgr.connected_peer_count().await, 0);
    }

    #[test]
    fn test_peer_health_default() {
        let h = PeerHealth::default();
        assert_eq!(h.score, 0.0);
        assert_eq!(h.battery_level, 0);
    }
}
