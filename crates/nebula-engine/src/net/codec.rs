//! Wire protocol codec matching the nebula-server's length-prefixed bincode format.
//!
//! Wire format: `[u32 LE length][bincode payload]`
//!
//! This is intentionally a mirror of `nebula-server/src/protocol.rs` so that the
//! engine and server speak the exact same binary protocol over TCP.

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use nebula_core::protocol::messages::{Ack, ControlChannelCmd, Hello};

/// Maximum allowed message size (1 MB). Prevents allocation attacks.
/// Must match the server's `MAX_MESSAGE_SIZE`.
const MAX_MESSAGE_SIZE: u32 = 1024 * 1024;

/// Write a bincode-serialized message with a u32 LE length prefix.
///
/// This is wire-compatible with `nebula-server::protocol::write_msg`.
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

/// Read a length-prefixed bincode message as raw bytes.
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

/// Read an `Ack` message from the stream.
pub async fn read_ack<T: AsyncRead + Unpin>(conn: &mut T) -> Result<Ack> {
    let buf = read_msg_bytes(conn).await?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize Ack")
}

/// Read a `ControlChannelCmd` from the stream.
pub async fn read_control_cmd<T: AsyncRead + Unpin>(conn: &mut T) -> Result<ControlChannelCmd> {
    let buf = read_msg_bytes(conn).await?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize ControlChannelCmd")
}

/// Read a `Hello` message from the stream.
pub async fn read_hello<T: AsyncRead + Unpin>(conn: &mut T) -> Result<Hello> {
    let buf = read_msg_bytes(conn).await?;
    bincode::deserialize(&buf).with_context(|| "Failed to deserialize Hello")
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::identity::node_id::{ClusterId, NodeId};
    use nebula_core::identity::roles::NodeRole;
    use nebula_core::protocol::messages::{
        ClusterStatus, NetworkType, NodeHeartBeatPayload,
    };
    use nebula_core::protocol::version::CURRENT_PROTO_VERSION;

    /// Helper: write with our codec, read back raw bytes, deserialize manually.
    /// Verifies the wire format is `[u32 LE len][bincode payload]`.
    async fn roundtrip_raw<M: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug>(
        msg: &M,
    ) -> M {
        let (mut writer, mut reader) = tokio::io::duplex(4096);

        write_msg(&mut writer, msg).await.unwrap();
        drop(writer); // signal EOF so reader doesn't hang

        // Read the raw wire format manually (matching server's format)
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await.unwrap();
        let len = u32::from_le_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await.unwrap();

        bincode::deserialize(&buf).unwrap()
    }

    #[tokio::test]
    async fn test_hello_roundtrip() {
        let node_id = NodeId::generate();
        let cluster_id = ClusterId("test-cluster".to_string());
        let hello = Hello::NodeRegistrationHello(CURRENT_PROTO_VERSION, node_id, cluster_id);

        let result: Hello = roundtrip_raw(&hello).await;

        match result {
            Hello::NodeRegistrationHello(v, nid, cid) => {
                assert_eq!(v, CURRENT_PROTO_VERSION);
                assert_eq!(nid, node_id);
                assert_eq!(cid.0, "test-cluster");
            }
            other => panic!("Expected NodeRegistrationHello, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_ack_registration_accepted_roundtrip() {
        let ack = Ack::RegistrationAccepted {
            assigned_role: NodeRole::Worker,
        };

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &ack).await.unwrap();
        drop(writer);

        let result = read_ack(&mut reader).await.unwrap();
        match result {
            Ack::RegistrationAccepted { assigned_role } => {
                assert_eq!(assigned_role, NodeRole::Worker);
            }
            other => panic!("Expected RegistrationAccepted, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_ack_variants_roundtrip() {
        let variants: Vec<Ack> = vec![
            Ack::Ok,
            Ack::ServiceNotExist,
            Ack::AuthFailed,
            Ack::ClusterNotFound,
            Ack::NodeAlreadyRegistered,
            Ack::RegistrationAccepted {
                assigned_role: NodeRole::Master,
            },
            Ack::RotationInProgress,
        ];

        for ack in &variants {
            let (mut writer, mut reader) = tokio::io::duplex(4096);
            write_msg(&mut writer, ack).await.unwrap();
            drop(writer);

            let result = read_ack(&mut reader).await.unwrap();

            // Compare the bincode serialized bytes to ensure exact match
            let original_bytes = bincode::serialize(ack).unwrap();
            let result_bytes = bincode::serialize(&result).unwrap();
            assert_eq!(original_bytes, result_bytes, "Ack variant mismatch: {:?}", ack);
        }
    }

    #[tokio::test]
    async fn test_control_cmd_heartbeat_roundtrip() {
        let cmd = ControlChannelCmd::HeartBeat;

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &cmd).await.unwrap();
        drop(writer);

        let result = read_control_cmd(&mut reader).await.unwrap();
        match result {
            ControlChannelCmd::HeartBeat => {}
            other => panic!("Expected HeartBeat, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_control_cmd_node_heartbeat_roundtrip() {
        let payload = NodeHeartBeatPayload {
            node_id: NodeId::generate(),
            battery_level: 85,
            cpu_load: 0.42,
            memory_available_mb: 2048,
            uptime_secs: 3600,
            active_tasks: 3,
            network_type: NetworkType::Wifi,
            timestamp: 1700000000,
        };
        let cmd = ControlChannelCmd::NodeHeartBeat(payload.clone());

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &cmd).await.unwrap();
        drop(writer);

        let result = read_control_cmd(&mut reader).await.unwrap();
        match result {
            ControlChannelCmd::NodeHeartBeat(p) => {
                assert_eq!(p.node_id, payload.node_id);
                assert_eq!(p.battery_level, 85);
                assert!((p.cpu_load - 0.42).abs() < f32::EPSILON);
                assert_eq!(p.memory_available_mb, 2048);
                assert_eq!(p.uptime_secs, 3600);
                assert_eq!(p.active_tasks, 3);
                assert_eq!(p.network_type, NetworkType::Wifi);
                assert_eq!(p.timestamp, 1700000000);
            }
            other => panic!("Expected NodeHeartBeat, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_control_cmd_rotation_prepare_roundtrip() {
        let new_master = NodeId::generate();
        let cmd = ControlChannelCmd::RotationPrepare { new_master };

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &cmd).await.unwrap();
        drop(writer);

        let result = read_control_cmd(&mut reader).await.unwrap();
        match result {
            ControlChannelCmd::RotationPrepare { new_master: nm } => {
                assert_eq!(nm, new_master);
            }
            other => panic!("Expected RotationPrepare, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_control_cmd_rotation_ready_roundtrip() {
        let new_master = NodeId::generate();
        let cmd = ControlChannelCmd::RotationReady { new_master };

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &cmd).await.unwrap();
        drop(writer);

        let result = read_control_cmd(&mut reader).await.unwrap();
        match result {
            ControlChannelCmd::RotationReady { new_master: nm } => {
                assert_eq!(nm, new_master);
            }
            other => panic!("Expected RotationReady, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_control_cmd_rotation_complete_roundtrip() {
        let old = NodeId::generate();
        let new = NodeId::generate();
        let cmd = ControlChannelCmd::RotationComplete {
            old_master: old,
            new_master: new,
        };

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &cmd).await.unwrap();
        drop(writer);

        let result = read_control_cmd(&mut reader).await.unwrap();
        match result {
            ControlChannelCmd::RotationComplete {
                old_master,
                new_master,
            } => {
                assert_eq!(old_master, old);
                assert_eq!(new_master, new);
            }
            other => panic!("Expected RotationComplete, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_control_cmd_cluster_status_roundtrip() {
        let status = ClusterStatus {
            cluster_id: ClusterId("my-cluster".to_string()),
            node_count: 5,
            master_id: Some(NodeId::generate()),
        };
        let cmd = ControlChannelCmd::ClusterStatusResponse(status.clone());

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &cmd).await.unwrap();
        drop(writer);

        let result = read_control_cmd(&mut reader).await.unwrap();
        match result {
            ControlChannelCmd::ClusterStatusResponse(s) => {
                assert_eq!(s.cluster_id.0, "my-cluster");
                assert_eq!(s.node_count, 5);
                assert!(s.master_id.is_some());
            }
            other => panic!("Expected ClusterStatusResponse, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_message_too_large_rejected() {
        let (mut writer, mut reader) = tokio::io::duplex(4096);

        // Write a fake length prefix that exceeds MAX_MESSAGE_SIZE
        let fake_len: u32 = MAX_MESSAGE_SIZE + 1;
        writer.write_all(&fake_len.to_le_bytes()).await.unwrap();
        drop(writer);

        let result = read_ack(&mut reader).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Message too large")
        );
    }

    #[tokio::test]
    async fn test_empty_stream_returns_error() {
        let (_writer, mut reader) = tokio::io::duplex(4096);
        drop(_writer); // close immediately

        let result = read_ack(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wire_format_matches_server() {
        // Verify that writing a Hello with our codec produces the exact same bytes
        // as manually serializing with bincode and prepending the u32 LE length.
        let node_id = NodeId::generate();
        let cluster_id = ClusterId("wire-test".to_string());
        let hello = Hello::NodeRegistrationHello(CURRENT_PROTO_VERSION, node_id, cluster_id);

        let expected_payload = bincode::serialize(&hello).unwrap();
        let expected_len = (expected_payload.len() as u32).to_le_bytes();

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &hello).await.unwrap();
        drop(writer);

        // Read the raw wire bytes
        let mut all_bytes = Vec::new();
        reader.read_to_end(&mut all_bytes).await.unwrap();

        // First 4 bytes = u32 LE length
        assert_eq!(&all_bytes[..4], &expected_len);
        // Remaining bytes = bincode payload
        assert_eq!(&all_bytes[4..], &expected_payload);
    }

    #[tokio::test]
    async fn test_hello_read_roundtrip() {
        let node_id = NodeId::generate();
        let cluster_id = ClusterId("read-test".to_string());
        let hello = Hello::NodeRegistrationHello(CURRENT_PROTO_VERSION, node_id, cluster_id);

        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_msg(&mut writer, &hello).await.unwrap();
        drop(writer);

        let result = read_hello(&mut reader).await.unwrap();
        match result {
            Hello::NodeRegistrationHello(v, nid, cid) => {
                assert_eq!(v, CURRENT_PROTO_VERSION);
                assert_eq!(nid, node_id);
                assert_eq!(cid.0, "read-test");
            }
            other => panic!("Expected NodeRegistrationHello, got {:?}", other),
        }
    }
}
