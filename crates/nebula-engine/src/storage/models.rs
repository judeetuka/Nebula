use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 1. PluginStateRecord
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginStateRecord {
    pub plugin_id: String,
    pub version: String,
    pub enabled: bool,
    pub installed_at: i64,
    pub last_executed_at: Option<i64>,
    pub config_json: Option<String>,
    pub checksum: Option<String>,
}

// ---------------------------------------------------------------------------
// 2. TaskQueueItem
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskQueueItem {
    pub task_id: String,
    pub status: String,
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
// 3. ClusterMemberRecord
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterMemberRecord {
    pub node_id: String,
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
// 4. HeartbeatRecord
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatRecord {
    pub id: String,
    pub node_id: String,
    pub timestamp: i64,
    pub battery_level: u8,
    pub cpu_load: f32,
    pub memory_available_mb: u32,
    pub active_tasks: u16,
    pub network_type: String,
}

// ---------------------------------------------------------------------------
// 5. MqttOfflineMessage
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MqttOfflineMessage {
    pub id: String,
    pub topic: String,
    pub payload_bytes: Vec<u8>,
    pub qos: u8,
    pub queued_at: i64,
    pub retry_count: u8,
}

// ---------------------------------------------------------------------------
// 6. SuccessionRecord
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuccessionRecord {
    pub cluster_id: String,
    pub succession_json: String,
    pub computed_at: i64,
    pub computed_by: String,
}

// ---------------------------------------------------------------------------
// 7. PeerNodeRecord
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeerNodeRecord {
    pub node_id: String,
    pub role: Option<String>,
    pub address: String,
    pub port: u16,
    pub roles_json: String,
    pub capabilities_json: String,
    pub last_seen: i64,
    pub connection_state: String,
}

// ---------------------------------------------------------------------------
// 8. NodeConfigRecord
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeConfigRecord {
    pub key: String,
    pub value: String,
    pub updated_at: i64,
}

// ---------------------------------------------------------------------------
// 9. EncryptedBlobRecord
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncryptedBlobRecord {
    pub key: String,
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
}
