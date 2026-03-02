//! TCP tunnel client for connecting a NEBULA node to the proxy server.
//!
//! Handles the registration handshake and provides methods for sending
//! heartbeats, rotation commands, and reading server commands over the
//! persistent control channel.

use anyhow::{bail, Context, Result};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

use nebula_core::identity::node_id::{ClusterId, NodeId};
use nebula_core::identity::roles::NodeRole;
use nebula_core::protocol::messages::{
    Ack, ControlChannelCmd, Hello, NodeHeartBeatPayload,
};
use nebula_core::protocol::version::CURRENT_PROTO_VERSION;

use super::codec;

/// A TCP tunnel client that maintains a persistent connection to the
/// NEBULA proxy server for control-channel communication.
pub struct TunnelClient {
    server_addr: String,
    stream: Option<TcpStream>,
}

impl TunnelClient {
    /// Create a new tunnel client targeting the given server address.
    ///
    /// The address should be in `host:port` format (e.g. `"127.0.0.1:2333"`).
    /// No connection is established until `connect_and_register` is called.
    pub fn new(server_addr: &str) -> Self {
        Self {
            server_addr: server_addr.to_string(),
            stream: None,
        }
    }

    /// Returns the configured server address.
    pub fn server_addr(&self) -> &str {
        &self.server_addr
    }

    /// Returns `true` if the client currently holds an open TCP stream.
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Connect to the proxy server and register this node.
    ///
    /// Performs the full registration handshake:
    /// 1. Open a TCP connection to the server
    /// 2. Send `Hello::NodeRegistrationHello`
    /// 3. Read the server's `Ack` response
    /// 4. Return the assigned role on success
    ///
    /// The TCP stream is stored internally for subsequent control-channel
    /// communication (heartbeats, rotation, etc.).
    pub async fn connect_and_register(
        &mut self,
        node_id: NodeId,
        cluster_id: ClusterId,
    ) -> Result<NodeRole> {
        info!(
            server = %self.server_addr,
            node = %node_id,
            cluster = %cluster_id,
            "Connecting to proxy server"
        );

        let mut stream = TcpStream::connect(&self.server_addr)
            .await
            .with_context(|| format!("Failed to connect to server at {}", self.server_addr))?;

        debug!(server = %self.server_addr, "TCP connection established");

        // Send registration hello
        let hello = Hello::NodeRegistrationHello(CURRENT_PROTO_VERSION, node_id, cluster_id.clone());
        codec::write_msg(&mut stream, &hello)
            .await
            .with_context(|| "Failed to send registration hello")?;

        debug!("Registration hello sent, waiting for ack");

        // Read server's ack
        let ack = codec::read_ack(&mut stream)
            .await
            .with_context(|| "Failed to read registration ack")?;

        match ack {
            Ack::RegistrationAccepted { assigned_role } => {
                info!(
                    role = %assigned_role,
                    "Registration accepted by server"
                );
                self.stream = Some(stream);
                Ok(assigned_role)
            }
            Ack::ClusterNotFound => {
                bail!("Server rejected registration: cluster not found ({})", cluster_id);
            }
            Ack::NodeAlreadyRegistered => {
                bail!("Server rejected registration: node already registered ({})", node_id);
            }
            Ack::AuthFailed => {
                bail!("Server rejected registration: authentication failed");
            }
            other => {
                bail!("Unexpected ack from server: {:?}", other);
            }
        }
    }

    /// Send a heartbeat to the server over the control channel.
    pub async fn send_heartbeat(&mut self, payload: NodeHeartBeatPayload) -> Result<()> {
        let stream = self.stream.as_mut().context("Not connected")?;
        let cmd = ControlChannelCmd::NodeHeartBeat(payload);
        codec::write_msg(stream, &cmd)
            .await
            .with_context(|| "Failed to send heartbeat")
    }

    /// Send a rotation-prepare command to the server.
    pub async fn send_rotation_prepare(&mut self, new_master: NodeId) -> Result<()> {
        let stream = self.stream.as_mut().context("Not connected")?;
        let cmd = ControlChannelCmd::RotationPrepare { new_master };
        codec::write_msg(stream, &cmd)
            .await
            .with_context(|| "Failed to send rotation prepare")
    }

    /// Send a rotation-ready command to the server.
    pub async fn send_rotation_ready(&mut self, new_master: NodeId) -> Result<()> {
        let stream = self.stream.as_mut().context("Not connected")?;
        let cmd = ControlChannelCmd::RotationReady { new_master };
        codec::write_msg(stream, &cmd)
            .await
            .with_context(|| "Failed to send rotation ready")
    }

    /// Send a rotation-complete command to the server.
    pub async fn send_rotation_complete(
        &mut self,
        old_master: NodeId,
        new_master: NodeId,
    ) -> Result<()> {
        let stream = self.stream.as_mut().context("Not connected")?;
        let cmd = ControlChannelCmd::RotationComplete {
            old_master,
            new_master,
        };
        codec::write_msg(stream, &cmd)
            .await
            .with_context(|| "Failed to send rotation complete")
    }

    /// Read the next control command from the server.
    ///
    /// This blocks until a command is available or the connection is closed.
    pub async fn read_command(&mut self) -> Result<ControlChannelCmd> {
        let stream = self.stream.as_mut().context("Not connected")?;
        codec::read_control_cmd(stream).await
    }

    /// Disconnect from the server, dropping the TCP stream.
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(stream) = self.stream.take() {
            drop(stream);
            info!(server = %self.server_addr, "Disconnected from proxy server");
        } else {
            warn!("disconnect() called but not connected");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_disconnected_client() {
        let client = TunnelClient::new("127.0.0.1:2333");
        assert_eq!(client.server_addr(), "127.0.0.1:2333");
        assert!(!client.is_connected());
    }

    #[test]
    fn test_new_with_different_addr() {
        let client = TunnelClient::new("192.168.1.100:5000");
        assert_eq!(client.server_addr(), "192.168.1.100:5000");
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_send_heartbeat_without_connection_fails() {
        let mut client = TunnelClient::new("127.0.0.1:2333");
        let payload = NodeHeartBeatPayload {
            node_id: NodeId::generate(),
            battery_level: 80,
            cpu_load: 0.5,
            memory_available_mb: 2048,
            uptime_secs: 1000,
            active_tasks: 2,
            network_type: nebula_core::protocol::messages::NetworkType::Wifi,
            timestamp: 1700000000,
        };

        let result = client.send_heartbeat(payload).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not connected"));
    }

    #[tokio::test]
    async fn test_send_rotation_prepare_without_connection_fails() {
        let mut client = TunnelClient::new("127.0.0.1:2333");
        let result = client.send_rotation_prepare(NodeId::generate()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not connected"));
    }

    #[tokio::test]
    async fn test_send_rotation_ready_without_connection_fails() {
        let mut client = TunnelClient::new("127.0.0.1:2333");
        let result = client.send_rotation_ready(NodeId::generate()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not connected"));
    }

    #[tokio::test]
    async fn test_send_rotation_complete_without_connection_fails() {
        let mut client = TunnelClient::new("127.0.0.1:2333");
        let result = client
            .send_rotation_complete(NodeId::generate(), NodeId::generate())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not connected"));
    }

    #[tokio::test]
    async fn test_read_command_without_connection_fails() {
        let mut client = TunnelClient::new("127.0.0.1:2333");
        let result = client.read_command().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not connected"));
    }

    #[tokio::test]
    async fn test_disconnect_when_not_connected_is_ok() {
        let mut client = TunnelClient::new("127.0.0.1:2333");
        // Should not fail, just warn
        let result = client.disconnect().await;
        assert!(result.is_ok());
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_connect_to_nonexistent_server_fails() {
        let mut client = TunnelClient::new("127.0.0.1:1");
        let result = client
            .connect_and_register(
                NodeId::generate(),
                ClusterId("test-cluster".to_string()),
            )
            .await;
        assert!(result.is_err());
        assert!(!client.is_connected());
    }

    /// Integration-style test using a mock TCP server that accepts a registration.
    #[tokio::test]
    async fn test_connect_and_register_with_mock_server() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let node_id = NodeId::generate();
        let cluster_id = ClusterId("mock-cluster".to_string());

        // Spawn mock server that reads Hello and responds with RegistrationAccepted
        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            // Read the Hello
            let hello = codec::read_hello(&mut stream).await.unwrap();
            match hello {
                Hello::NodeRegistrationHello(v, nid, cid) => {
                    assert_eq!(v, CURRENT_PROTO_VERSION);
                    assert_eq!(nid, node_id);
                    assert_eq!(cid.0, "mock-cluster");
                }
                other => panic!("Expected NodeRegistrationHello, got {:?}", other),
            }

            // Send RegistrationAccepted
            let ack = Ack::RegistrationAccepted {
                assigned_role: NodeRole::Worker,
            };
            codec::write_msg(&mut stream, &ack).await.unwrap();
        });

        // Client connects and registers
        let mut client = TunnelClient::new(&addr.to_string());
        let role = client
            .connect_and_register(node_id, cluster_id)
            .await
            .unwrap();

        assert_eq!(role, NodeRole::Worker);
        assert!(client.is_connected());

        server_handle.await.unwrap();
    }

    /// Test that ClusterNotFound ack is properly handled.
    #[tokio::test]
    async fn test_connect_cluster_not_found() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _hello = codec::read_hello(&mut stream).await.unwrap();
            codec::write_msg(&mut stream, &Ack::ClusterNotFound)
                .await
                .unwrap();
        });

        let mut client = TunnelClient::new(&addr.to_string());
        let result = client
            .connect_and_register(
                NodeId::generate(),
                ClusterId("nonexistent".to_string()),
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cluster not found"));
        assert!(!client.is_connected());

        server_handle.await.unwrap();
    }

    /// Test that NodeAlreadyRegistered ack is properly handled.
    #[tokio::test]
    async fn test_connect_node_already_registered() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _hello = codec::read_hello(&mut stream).await.unwrap();
            codec::write_msg(&mut stream, &Ack::NodeAlreadyRegistered)
                .await
                .unwrap();
        });

        let mut client = TunnelClient::new(&addr.to_string());
        let result = client
            .connect_and_register(
                NodeId::generate(),
                ClusterId("some-cluster".to_string()),
            )
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already registered")
        );
        assert!(!client.is_connected());

        server_handle.await.unwrap();
    }

    /// Test heartbeat send over a mock connection.
    #[tokio::test]
    async fn test_send_heartbeat_over_mock_connection() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let node_id = NodeId::generate();

        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            // Read Hello, send Ack
            let _hello = codec::read_hello(&mut stream).await.unwrap();
            let ack = Ack::RegistrationAccepted {
                assigned_role: NodeRole::Worker,
            };
            codec::write_msg(&mut stream, &ack).await.unwrap();

            // Read the heartbeat command
            let cmd = codec::read_control_cmd(&mut stream).await.unwrap();
            match cmd {
                ControlChannelCmd::NodeHeartBeat(p) => {
                    assert_eq!(p.node_id, node_id);
                    assert_eq!(p.battery_level, 75);
                }
                other => panic!("Expected NodeHeartBeat, got {:?}", other),
            }
        });

        let mut client = TunnelClient::new(&addr.to_string());
        client
            .connect_and_register(node_id, ClusterId("hb-cluster".to_string()))
            .await
            .unwrap();

        let payload = NodeHeartBeatPayload {
            node_id,
            battery_level: 75,
            cpu_load: 0.3,
            memory_available_mb: 1500,
            uptime_secs: 500,
            active_tasks: 1,
            network_type: nebula_core::protocol::messages::NetworkType::Wifi,
            timestamp: chrono::Utc::now().timestamp(),
        };
        client.send_heartbeat(payload).await.unwrap();

        server_handle.await.unwrap();
    }

    /// Test disconnect after a successful connection.
    #[tokio::test]
    async fn test_disconnect_after_connect() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _hello = codec::read_hello(&mut stream).await.unwrap();
            let ack = Ack::RegistrationAccepted {
                assigned_role: NodeRole::Master,
            };
            codec::write_msg(&mut stream, &ack).await.unwrap();
        });

        let mut client = TunnelClient::new(&addr.to_string());
        let role = client
            .connect_and_register(
                NodeId::generate(),
                ClusterId("dc-cluster".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(role, NodeRole::Master);
        assert!(client.is_connected());

        client.disconnect().await.unwrap();
        assert!(!client.is_connected());

        server_handle.await.unwrap();
    }
}
