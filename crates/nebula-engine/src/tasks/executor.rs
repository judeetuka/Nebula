use std::collections::HashMap;

use nebula_core::identity::node_id::NodeId;

use crate::tasks::timeout::TimeoutManager;
use crate::tasks::types::{TaskId, TaskPayload, TaskResult, TaskStatus};

/// A task currently being executed on this worker node.
pub struct ActiveTask {
    pub payload: TaskPayload,
    pub started_at: i64,
}

/// Manages task execution on a worker node.
///
/// Receives tasks from the master, tracks active executions, and produces
/// results when tasks complete (successfully or otherwise).
pub struct TaskExecutor {
    node_id: NodeId,
    active_tasks: HashMap<TaskId, ActiveTask>,
    max_concurrent: usize,
    timeout_manager: TimeoutManager,
}

impl TaskExecutor {
    /// Create a new executor for the given node with the specified concurrency limit.
    pub fn new(node_id: NodeId, max_concurrent: usize) -> Self {
        Self {
            node_id,
            active_tasks: HashMap::new(),
            max_concurrent,
            timeout_manager: TimeoutManager::default(),
        }
    }

    /// Returns `true` if the executor can accept another task.
    pub fn can_accept_task(&self) -> bool {
        self.active_tasks.len() < self.max_concurrent
    }

    /// Accept and begin executing a task.
    ///
    /// Returns an error if the executor is at capacity.
    pub fn accept_task(&mut self, payload: TaskPayload) -> Result<(), String> {
        if !self.can_accept_task() {
            return Err(format!(
                "Executor at capacity ({}/{})",
                self.active_tasks.len(),
                self.max_concurrent
            ));
        }

        let now = chrono::Utc::now().timestamp_millis();
        let task_id = payload.task_id.clone();

        let active = ActiveTask {
            payload,
            started_at: now,
        };

        self.active_tasks.insert(task_id, active);
        Ok(())
    }

    /// Complete a task successfully, producing a `TaskResult`.
    ///
    /// Returns `None` if the task is not currently active.
    pub fn complete_task(
        &mut self,
        task_id: &TaskId,
        result_data: Option<serde_json::Value>,
    ) -> Option<TaskResult> {
        let active = self.active_tasks.remove(task_id)?;
        let now = chrono::Utc::now().timestamp_millis();

        Some(TaskResult {
            task_id: task_id.clone(),
            status: TaskStatus::Completed,
            data: result_data,
            error: None,
            executed_by: self.node_id,
            started_at: active.started_at,
            completed_at: now,
        })
    }

    /// Fail a task with an error message, producing a `TaskResult`.
    ///
    /// Returns `None` if the task is not currently active.
    pub fn fail_task(&mut self, task_id: &TaskId, error: &str) -> Option<TaskResult> {
        let active = self.active_tasks.remove(task_id)?;
        let now = chrono::Utc::now().timestamp_millis();

        Some(TaskResult {
            task_id: task_id.clone(),
            status: TaskStatus::Failed,
            data: None,
            error: Some(error.to_string()),
            executed_by: self.node_id,
            started_at: active.started_at,
            completed_at: now,
        })
    }

    /// Find active tasks that have exceeded their timeout.
    pub fn check_timeouts(&self) -> Vec<TaskId> {
        self.active_tasks
            .values()
            .filter(|task| {
                self.timeout_manager
                    .is_timed_out(task.started_at, task.payload.timeout_secs)
            })
            .map(|task| task.payload.task_id.clone())
            .collect()
    }

    /// Returns the number of currently active tasks.
    pub fn active_count(&self) -> usize {
        self.active_tasks.len()
    }

    /// Returns `true` if the given task is currently being executed.
    pub fn is_executing(&self, task_id: &TaskId) -> bool {
        self.active_tasks.contains_key(task_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::types::{TaskPriority, TaskType};
    use nebula_core::identity::node_id::NodeId;

    fn make_payload(id: &str) -> TaskPayload {
        TaskPayload {
            task_id: TaskId(id.to_string()),
            task_type: TaskType::Ping,
            data: serde_json::json!({}),
            timeout_secs: 90,
            submitted_at: chrono::Utc::now().timestamp_millis(),
            priority: TaskPriority::Normal,
        }
    }

    fn make_executor(max_concurrent: usize) -> TaskExecutor {
        TaskExecutor::new(NodeId::generate(), max_concurrent)
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_executor_is_empty() {
        let executor = make_executor(5);
        assert_eq!(executor.active_count(), 0);
        assert!(executor.can_accept_task());
    }

    // -----------------------------------------------------------------------
    // can_accept_task
    // -----------------------------------------------------------------------

    #[test]
    fn test_can_accept_when_below_limit() {
        let mut executor = make_executor(3);
        executor.accept_task(make_payload("t1")).unwrap();
        executor.accept_task(make_payload("t2")).unwrap();
        assert!(executor.can_accept_task());
    }

    #[test]
    fn test_cannot_accept_at_limit() {
        let mut executor = make_executor(2);
        executor.accept_task(make_payload("t1")).unwrap();
        executor.accept_task(make_payload("t2")).unwrap();
        assert!(!executor.can_accept_task());
    }

    // -----------------------------------------------------------------------
    // accept_task
    // -----------------------------------------------------------------------

    #[test]
    fn test_accept_task_success() {
        let mut executor = make_executor(5);
        let result = executor.accept_task(make_payload("task-1"));
        assert!(result.is_ok());
        assert_eq!(executor.active_count(), 1);
        assert!(executor.is_executing(&TaskId("task-1".to_string())));
    }

    #[test]
    fn test_accept_task_at_capacity_fails() {
        let mut executor = make_executor(1);
        executor.accept_task(make_payload("t1")).unwrap();

        let result = executor.accept_task(make_payload("t2"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at capacity"));
        assert_eq!(executor.active_count(), 1);
    }

    #[test]
    fn test_accept_multiple_tasks() {
        let mut executor = make_executor(10);
        for i in 0..5 {
            executor
                .accept_task(make_payload(&format!("task-{}", i)))
                .unwrap();
        }
        assert_eq!(executor.active_count(), 5);
    }

    // -----------------------------------------------------------------------
    // complete_task
    // -----------------------------------------------------------------------

    #[test]
    fn test_complete_task_success() {
        let mut executor = make_executor(5);
        executor.accept_task(make_payload("task-1")).unwrap();

        let task_id = TaskId("task-1".to_string());
        let result = executor.complete_task(&task_id, Some(serde_json::json!({"ok": true})));

        assert!(result.is_some());
        let task_result = result.unwrap();
        assert_eq!(task_result.task_id, task_id);
        assert_eq!(task_result.status, TaskStatus::Completed);
        assert!(task_result.data.is_some());
        assert!(task_result.error.is_none());
        assert!(task_result.completed_at >= task_result.started_at);
        assert_eq!(executor.active_count(), 0);
    }

    #[test]
    fn test_complete_task_with_no_data() {
        let mut executor = make_executor(5);
        executor.accept_task(make_payload("task-1")).unwrap();

        let task_id = TaskId("task-1".to_string());
        let result = executor.complete_task(&task_id, None);

        assert!(result.is_some());
        assert!(result.unwrap().data.is_none());
    }

    #[test]
    fn test_complete_nonexistent_task() {
        let mut executor = make_executor(5);
        let phantom_id = TaskId("nonexistent".to_string());
        assert!(executor.complete_task(&phantom_id, None).is_none());
    }

    // -----------------------------------------------------------------------
    // fail_task
    // -----------------------------------------------------------------------

    #[test]
    fn test_fail_task_success() {
        let mut executor = make_executor(5);
        executor.accept_task(make_payload("task-1")).unwrap();

        let task_id = TaskId("task-1".to_string());
        let result = executor.fail_task(&task_id, "SIM card not found");

        assert!(result.is_some());
        let task_result = result.unwrap();
        assert_eq!(task_result.task_id, task_id);
        assert_eq!(task_result.status, TaskStatus::Failed);
        assert!(task_result.data.is_none());
        assert_eq!(
            task_result.error,
            Some("SIM card not found".to_string())
        );
        assert_eq!(executor.active_count(), 0);
    }

    #[test]
    fn test_fail_nonexistent_task() {
        let mut executor = make_executor(5);
        let phantom_id = TaskId("nonexistent".to_string());
        assert!(executor.fail_task(&phantom_id, "error").is_none());
    }

    // -----------------------------------------------------------------------
    // check_timeouts
    // -----------------------------------------------------------------------

    #[test]
    fn test_check_timeouts_none_expired() {
        let mut executor = make_executor(5);
        executor.accept_task(make_payload("task-1")).unwrap();

        let timed_out = executor.check_timeouts();
        assert!(timed_out.is_empty());
    }

    #[test]
    fn test_check_timeouts_finds_expired() {
        let mut executor = make_executor(5);
        executor.accept_task(make_payload("task-1")).unwrap();

        // Manually backdate the started_at timestamp
        let task_id = TaskId("task-1".to_string());
        if let Some(active) = executor.active_tasks.get_mut(&task_id) {
            active.started_at = chrono::Utc::now().timestamp_millis() - 200_000;
        }

        let timed_out = executor.check_timeouts();
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], task_id);
    }

    // -----------------------------------------------------------------------
    // is_executing
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_executing_true() {
        let mut executor = make_executor(5);
        executor.accept_task(make_payload("task-1")).unwrap();
        assert!(executor.is_executing(&TaskId("task-1".to_string())));
    }

    #[test]
    fn test_is_executing_false() {
        let executor = make_executor(5);
        assert!(!executor.is_executing(&TaskId("task-1".to_string())));
    }

    #[test]
    fn test_is_executing_false_after_completion() {
        let mut executor = make_executor(5);
        executor.accept_task(make_payload("task-1")).unwrap();

        let task_id = TaskId("task-1".to_string());
        executor.complete_task(&task_id, None);
        assert!(!executor.is_executing(&task_id));
    }

    #[test]
    fn test_is_executing_false_after_failure() {
        let mut executor = make_executor(5);
        executor.accept_task(make_payload("task-1")).unwrap();

        let task_id = TaskId("task-1".to_string());
        executor.fail_task(&task_id, "error");
        assert!(!executor.is_executing(&task_id));
    }

    // -----------------------------------------------------------------------
    // Capacity after completion
    // -----------------------------------------------------------------------

    #[test]
    fn test_capacity_freed_after_completion() {
        let mut executor = make_executor(1);
        executor.accept_task(make_payload("t1")).unwrap();
        assert!(!executor.can_accept_task());

        executor.complete_task(&TaskId("t1".to_string()), None);
        assert!(executor.can_accept_task());

        // Can now accept a new task
        executor.accept_task(make_payload("t2")).unwrap();
        assert_eq!(executor.active_count(), 1);
    }

    #[test]
    fn test_capacity_freed_after_failure() {
        let mut executor = make_executor(1);
        executor.accept_task(make_payload("t1")).unwrap();
        assert!(!executor.can_accept_task());

        executor.fail_task(&TaskId("t1".to_string()), "err");
        assert!(executor.can_accept_task());
    }

    // -----------------------------------------------------------------------
    // Full executor flow
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_executor_flow() {
        let node_id = NodeId::generate();
        let mut executor = TaskExecutor::new(node_id, 5);

        // Accept
        let payload = make_payload("flow-task");
        executor.accept_task(payload).unwrap();
        assert_eq!(executor.active_count(), 1);
        assert!(executor.is_executing(&TaskId("flow-task".to_string())));

        // Complete
        let result = executor
            .complete_task(
                &TaskId("flow-task".to_string()),
                Some(serde_json::json!({"response": "pong"})),
            )
            .unwrap();

        assert_eq!(result.task_id, TaskId("flow-task".to_string()));
        assert_eq!(result.status, TaskStatus::Completed);
        assert_eq!(result.executed_by, node_id);
        assert!(result.data.is_some());
        assert_eq!(executor.active_count(), 0);
    }
}
