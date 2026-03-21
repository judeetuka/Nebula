use native_db::*;
use native_model::native_model;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 1. PluginStateRecord (native_model id=1)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 1, version = 1)]
#[native_db]
pub struct PluginStateRecord {
    #[primary_key]
    pub plugin_id: String,
    pub version: String,
    pub enabled: bool,
    pub installed_at: i64,
    pub last_executed_at: Option<i64>,
    pub config_json: Option<String>,
    pub checksum: Option<String>,
}

// ---------------------------------------------------------------------------
// 2. TaskQueueItem (native_model id=2)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 2, version = 1)]
#[native_db]
pub struct TaskQueueItem {
    #[primary_key]
    pub task_id: String,
    #[secondary_key]
    pub status: String,
    #[secondary_key]
    pub priority: u8,
    pub task_type: String,
    pub payload_json: String,
    pub submitted_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub assigned_node: Option<String>,
    pub timeout_secs: u32,
    pub retry_count: u8,
    pub error_message: Option<String>,
}

// ---------------------------------------------------------------------------
// 3. ClusterMemberRecord (native_model id=3)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 3, version = 1)]
#[native_db]
pub struct ClusterMemberRecord {
    #[primary_key]
    pub node_id: String,
    #[secondary_key]
    pub role: String,
    pub join_time: i64,
    pub last_heartbeat: i64,
    pub battery_level: u8,
    pub cpu_load: f32,
    pub memory_available_mb: u32,
    pub active_tasks: u16,
    pub network_type: String,
    pub peer_address: Option<String>,
    pub is_stale: bool,
}

// ---------------------------------------------------------------------------
// 4. HeartbeatRecord (native_model id=4)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 4, version = 1)]
#[native_db]
pub struct HeartbeatRecord {
    #[primary_key]
    pub id: String,
    #[secondary_key]
    pub node_id: String,
    pub timestamp: i64,
    pub battery_level: u8,
    pub cpu_load: f32,
    pub memory_available_mb: u32,
    pub active_tasks: u16,
    pub network_type: String,
}

// ---------------------------------------------------------------------------
// 5. MqttOfflineMessage (native_model id=5)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 5, version = 1)]
#[native_db]
pub struct MqttOfflineMessage {
    #[primary_key]
    pub id: String,
    #[secondary_key]
    pub topic: String,
    pub payload_bytes: Vec<u8>,
    pub qos: u8,
    pub queued_at: i64,
    pub retry_count: u8,
}

// ---------------------------------------------------------------------------
// 6. SuccessionRecord (native_model id=6)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 6, version = 1)]
#[native_db]
pub struct SuccessionRecord {
    #[primary_key]
    pub cluster_id: String,
    pub succession_json: String,
    pub computed_at: i64,
    pub computed_by: String,
}

// ---------------------------------------------------------------------------
// 7. PeerNodeRecord (native_model id=7)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 7, version = 1)]
#[native_db]
pub struct PeerNodeRecord {
    #[primary_key]
    pub node_id: String,
    #[secondary_key(optional)]
    pub role: Option<String>,
    pub address: String,
    pub port: u16,
    pub roles_json: String,
    pub capabilities_json: String,
    pub last_seen: i64,
    pub connection_state: String,
}

// ---------------------------------------------------------------------------
// 8. NodeConfigRecord (native_model id=8)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 8, version = 1)]
#[native_db]
pub struct NodeConfigRecord {
    #[primary_key]
    pub key: String,
    pub value: String,
    pub updated_at: i64,
}

// ---------------------------------------------------------------------------
// 9. EncryptedBlobRecord (native_model id=9)
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[native_model(id = 9, version = 1)]
#[native_db]
pub struct EncryptedBlobRecord {
    #[primary_key]
    pub key: String,
    #[secondary_key]
    pub category: String,
    pub encrypted_data: Vec<u8>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_state_record_serde_roundtrip() {
        let record = PluginStateRecord {
            plugin_id: "com.example.plugin".to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            installed_at: 1700000000,
            last_executed_at: Some(1700001000),
            config_json: Some(r#"{"key":"value"}"#.to_string()),
            checksum: Some("abc123".to_string()),
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: PluginStateRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_task_queue_item_serde_roundtrip() {
        let record = TaskQueueItem {
            task_id: "task-001".to_string(),
            status: "pending".to_string(),
            priority: 5,
            task_type: "compute".to_string(),
            payload_json: "{}".to_string(),
            submitted_at: 1700000000,
            started_at: None,
            completed_at: None,
            assigned_node: None,
            timeout_secs: 300,
            retry_count: 0,
            error_message: None,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: TaskQueueItem = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_cluster_member_record_serde_roundtrip() {
        let record = ClusterMemberRecord {
            node_id: "node-abc".to_string(),
            role: "worker".to_string(),
            join_time: 1700000000,
            last_heartbeat: 1700001000,
            battery_level: 85,
            cpu_load: 0.42,
            memory_available_mb: 2048,
            active_tasks: 3,
            network_type: "wifi".to_string(),
            peer_address: Some("192.168.1.10:8080".to_string()),
            is_stale: false,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ClusterMemberRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_heartbeat_record_serde_roundtrip() {
        let record = HeartbeatRecord {
            id: "hb-001".to_string(),
            node_id: "node-abc".to_string(),
            timestamp: 1700000000,
            battery_level: 90,
            cpu_load: 0.15,
            memory_available_mb: 4096,
            active_tasks: 1,
            network_type: "cellular".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: HeartbeatRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_mqtt_offline_message_serde_roundtrip() {
        let record = MqttOfflineMessage {
            id: "msg-001".to_string(),
            topic: "cluster/heartbeat".to_string(),
            payload_bytes: vec![1, 2, 3, 4],
            qos: 1,
            queued_at: 1700000000,
            retry_count: 0,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: MqttOfflineMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_succession_record_serde_roundtrip() {
        let record = SuccessionRecord {
            cluster_id: "cluster-1".to_string(),
            succession_json: r#"[{"node_id":"n1","score":100}]"#.to_string(),
            computed_at: 1700000000,
            computed_by: "node-master".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: SuccessionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_peer_node_record_serde_roundtrip() {
        let record = PeerNodeRecord {
            node_id: "peer-001".to_string(),
            role: Some("worker".to_string()),
            address: "192.168.1.20".to_string(),
            port: 9090,
            roles_json: r#"["worker","relay"]"#.to_string(),
            capabilities_json: r#"{"gpu":false}"#.to_string(),
            last_seen: 1700000000,
            connection_state: "connected".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: PeerNodeRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_node_config_record_serde_roundtrip() {
        let record = NodeConfigRecord {
            key: "max_tasks".to_string(),
            value: "10".to_string(),
            updated_at: 1700000000,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: NodeConfigRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_encrypted_blob_record_serde_roundtrip() {
        let record = EncryptedBlobRecord {
            key: "secret-key".to_string(),
            category: "credentials".to_string(),
            encrypted_data: vec![0xDE, 0xAD, 0xBE, 0xEF],
            created_at: 1700000000,
            updated_at: 1700001000,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: EncryptedBlobRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }
}
