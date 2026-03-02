use nebula_core::identity::node_id::NodeId;
use serde::{Deserialize, Serialize};

/// Unique identifier for a task in the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

impl TaskId {
    /// Generate a new random task ID.
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The payload describing a task to be executed by a worker node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPayload {
    /// Unique task identifier.
    pub task_id: TaskId,
    /// The type of work to perform.
    pub task_type: TaskType,
    /// Opaque data payload for the task handler.
    pub data: serde_json::Value,
    /// Maximum execution time in seconds (clamped to 60-120).
    pub timeout_secs: u32,
    /// When the task was submitted (Unix milliseconds).
    pub submitted_at: i64,
    /// Execution priority.
    pub priority: TaskPriority,
}

/// Categories of tasks that can be dispatched to workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    /// Send an SMS message.
    Sms { phone: String, message: String },
    /// Execute a USSD code.
    Ussd { code: String },
    /// Invoke a plugin action.
    Custom { plugin_id: String, action: String },
    /// Simple connectivity/health test.
    Ping,
}

/// Task execution priority levels.
///
/// Higher values indicate higher priority. Tasks are dispatched in
/// priority order when multiple tasks are pending.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// The result produced after a task completes (successfully or otherwise).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// The task this result belongs to.
    pub task_id: TaskId,
    /// Final status of the task.
    pub status: TaskStatus,
    /// Result data on success, if any.
    pub data: Option<serde_json::Value>,
    /// Error description on failure, if any.
    pub error: Option<String>,
    /// Which node executed the task.
    pub executed_by: NodeId,
    /// When execution started (Unix milliseconds).
    pub started_at: i64,
    /// When execution completed (Unix milliseconds).
    pub completed_at: i64,
}

/// Lifecycle status of a task as it moves through the system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// Queued on master, waiting for dispatch.
    Pending,
    /// Sent to a worker node.
    Dispatched { to_node: NodeId },
    /// Currently executing on a worker.
    Running { on_node: NodeId, started_at: i64 },
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
    /// Exceeded the allowed execution time.
    TimedOut,
    /// Explicitly cancelled before completion.
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_generate_uniqueness() {
        let ids: Vec<TaskId> = (0..100).map(|_| TaskId::generate()).collect();
        let unique: std::collections::HashSet<&TaskId> = ids.iter().collect();
        assert_eq!(unique.len(), 100);
    }

    #[test]
    fn test_task_id_display() {
        let id = TaskId("test-task-123".to_string());
        assert_eq!(format!("{}", id), "test-task-123");
    }

    #[test]
    fn test_task_payload_serialization_roundtrip() {
        let payload = TaskPayload {
            task_id: TaskId("task-1".to_string()),
            task_type: TaskType::Ping,
            data: serde_json::json!({}),
            timeout_secs: 90,
            submitted_at: 1700000000000,
            priority: TaskPriority::Normal,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: TaskPayload = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.task_id, payload.task_id);
        assert_eq!(deserialized.timeout_secs, 90);
        assert_eq!(deserialized.submitted_at, 1700000000000);
        assert_eq!(deserialized.priority, TaskPriority::Normal);
    }

    #[test]
    fn test_sms_task_type_serialization() {
        let task_type = TaskType::Sms {
            phone: "+1234567890".to_string(),
            message: "Hello world".to_string(),
        };

        let json = serde_json::to_string(&task_type).unwrap();
        let deserialized: TaskType = serde_json::from_str(&json).unwrap();

        match deserialized {
            TaskType::Sms { phone, message } => {
                assert_eq!(phone, "+1234567890");
                assert_eq!(message, "Hello world");
            }
            _ => panic!("Expected Sms variant"),
        }
    }

    #[test]
    fn test_ussd_task_type_serialization() {
        let task_type = TaskType::Ussd {
            code: "*123#".to_string(),
        };

        let json = serde_json::to_string(&task_type).unwrap();
        let deserialized: TaskType = serde_json::from_str(&json).unwrap();

        match deserialized {
            TaskType::Ussd { code } => assert_eq!(code, "*123#"),
            _ => panic!("Expected Ussd variant"),
        }
    }

    #[test]
    fn test_custom_task_type_serialization() {
        let task_type = TaskType::Custom {
            plugin_id: "whatsapp-bridge".to_string(),
            action: "forward_message".to_string(),
        };

        let json = serde_json::to_string(&task_type).unwrap();
        let deserialized: TaskType = serde_json::from_str(&json).unwrap();

        match deserialized {
            TaskType::Custom { plugin_id, action } => {
                assert_eq!(plugin_id, "whatsapp-bridge");
                assert_eq!(action, "forward_message");
            }
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn test_priority_ordering() {
        assert!(TaskPriority::Low < TaskPriority::Normal);
        assert!(TaskPriority::Normal < TaskPriority::High);
        assert!(TaskPriority::High < TaskPriority::Critical);
    }

    #[test]
    fn test_priority_equality() {
        assert_eq!(TaskPriority::Normal, TaskPriority::Normal);
        assert_ne!(TaskPriority::Low, TaskPriority::High);
    }

    #[test]
    fn test_task_result_serialization_roundtrip() {
        let node_id = NodeId::generate();
        let result = TaskResult {
            task_id: TaskId("task-42".to_string()),
            status: TaskStatus::Completed,
            data: Some(serde_json::json!({"output": "success"})),
            error: None,
            executed_by: node_id,
            started_at: 1700000000000,
            completed_at: 1700000005000,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.task_id, TaskId("task-42".to_string()));
        assert_eq!(deserialized.status, TaskStatus::Completed);
        assert!(deserialized.data.is_some());
        assert!(deserialized.error.is_none());
        assert_eq!(deserialized.executed_by, node_id);
        assert_eq!(deserialized.completed_at - deserialized.started_at, 5000);
    }

    #[test]
    fn test_task_result_with_error() {
        let node_id = NodeId::generate();
        let result = TaskResult {
            task_id: TaskId("task-err".to_string()),
            status: TaskStatus::Failed,
            data: None,
            error: Some("SIM card not found".to_string()),
            executed_by: node_id,
            started_at: 1700000000000,
            completed_at: 1700000001000,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.status, TaskStatus::Failed);
        assert!(deserialized.data.is_none());
        assert_eq!(deserialized.error, Some("SIM card not found".to_string()));
    }

    #[test]
    fn test_task_status_dispatched_serialization() {
        let node_id = NodeId::generate();
        let status = TaskStatus::Dispatched { to_node: node_id };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized, status);
    }

    #[test]
    fn test_task_status_running_serialization() {
        let node_id = NodeId::generate();
        let status = TaskStatus::Running {
            on_node: node_id,
            started_at: 1700000000000,
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized, status);
    }

    #[test]
    fn test_all_task_status_variants_serialize() {
        let node_id = NodeId::generate();
        let statuses: Vec<TaskStatus> = vec![
            TaskStatus::Pending,
            TaskStatus::Dispatched { to_node: node_id },
            TaskStatus::Running {
                on_node: node_id,
                started_at: 1700000000000,
            },
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::TimedOut,
            TaskStatus::Cancelled,
        ];

        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, status);
        }
    }

    #[test]
    fn test_task_payload_with_complex_data() {
        let payload = TaskPayload {
            task_id: TaskId::generate(),
            task_type: TaskType::Custom {
                plugin_id: "data-sync".to_string(),
                action: "upload".to_string(),
            },
            data: serde_json::json!({
                "files": ["a.txt", "b.txt"],
                "destination": "/remote/path",
                "compress": true,
                "max_size_mb": 50
            }),
            timeout_secs: 120,
            submitted_at: 1700000000000,
            priority: TaskPriority::High,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: TaskPayload = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.data["files"][0], "a.txt");
        assert_eq!(deserialized.data["compress"], true);
        assert_eq!(deserialized.priority, TaskPriority::High);
    }
}
