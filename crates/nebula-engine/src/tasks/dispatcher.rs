use std::collections::HashMap;

use nebula_core::identity::node_id::NodeId;

use crate::cluster::membership::ClusterMembership;
use crate::tasks::scheduler::select_worker;
use crate::tasks::timeout::TimeoutManager;
use crate::tasks::types::{TaskId, TaskPayload, TaskResult, TaskStatus};

/// A task that has been dispatched to a worker node and is awaiting completion.
pub struct DispatchedTask {
    pub payload: TaskPayload,
    pub assigned_to: NodeId,
    pub dispatched_at: i64,
}

/// Manages task dispatch on the master node.
///
/// Tracks pending tasks (queue), dispatched tasks (in-flight), and completed
/// results. Uses the scheduler to select the best worker for each task.
pub struct TaskDispatcher {
    pending: Vec<TaskPayload>,
    dispatched: HashMap<TaskId, DispatchedTask>,
    completed: HashMap<TaskId, TaskResult>,
    max_pending: usize,
    timeout_manager: TimeoutManager,
}

impl TaskDispatcher {
    /// Create a new dispatcher with the given maximum pending queue size.
    pub fn new(max_pending: usize) -> Self {
        Self {
            pending: Vec::new(),
            dispatched: HashMap::new(),
            completed: HashMap::new(),
            max_pending,
            timeout_manager: TimeoutManager::default(),
        }
    }

    /// Submit a task to the pending queue.
    ///
    /// Returns the `TaskId` on success, or an error if the queue is full.
    pub fn submit_task(&mut self, payload: TaskPayload) -> Result<TaskId, String> {
        if self.pending.len() >= self.max_pending {
            return Err(format!(
                "Pending queue is full ({}/{})",
                self.pending.len(),
                self.max_pending
            ));
        }

        let task_id = payload.task_id.clone();
        self.pending.push(payload);
        Ok(task_id)
    }

    /// Pick the next highest-priority pending task, select the best worker,
    /// and move it to the dispatched map.
    ///
    /// Returns `Some((payload, node_id))` if a task was dispatched, or `None`
    /// if no tasks are pending or no eligible workers are available.
    pub fn dispatch_next(
        &mut self,
        membership: &ClusterMembership,
    ) -> Option<(TaskPayload, NodeId)> {
        if self.pending.is_empty() {
            return None;
        }

        // Find the index of the highest-priority task.
        // Among equal priorities, the first one in the queue (oldest) wins.
        let best_idx = self
            .pending
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| p.priority)
            .map(|(idx, _)| idx)?;

        // Attempt to find a worker for this task.
        let worker = select_worker(membership, &self.pending[best_idx])?;

        let payload = self.pending.remove(best_idx);
        let now = chrono::Utc::now().timestamp_millis();

        let dispatched = DispatchedTask {
            payload: payload.clone(),
            assigned_to: worker,
            dispatched_at: now,
        };

        self.dispatched.insert(payload.task_id.clone(), dispatched);

        Some((payload, worker))
    }

    /// Mark a dispatched task as running on the given node.
    pub fn mark_running(&mut self, task_id: &TaskId, node_id: NodeId) {
        if let Some(task) = self.dispatched.get_mut(task_id) {
            task.assigned_to = node_id;
        }
    }

    /// Record a completed task result.
    ///
    /// Moves the task from dispatched to completed.
    pub fn mark_completed(&mut self, result: TaskResult) {
        self.dispatched.remove(&result.task_id);
        self.completed.insert(result.task_id.clone(), result);
    }

    /// Mark a dispatched task as timed out.
    ///
    /// Creates a `TaskResult` with `TaskStatus::TimedOut` and moves it to completed.
    pub fn mark_timed_out(&mut self, task_id: &TaskId) {
        if let Some(dispatched) = self.dispatched.remove(task_id) {
            let now = chrono::Utc::now().timestamp_millis();
            let result = TaskResult {
                task_id: task_id.clone(),
                status: TaskStatus::TimedOut,
                data: None,
                error: Some("Task execution timed out".to_string()),
                executed_by: dispatched.assigned_to,
                started_at: dispatched.dispatched_at,
                completed_at: now,
            };
            self.completed.insert(task_id.clone(), result);
        }
    }

    /// Find all dispatched tasks that have exceeded their timeout.
    pub fn check_timeouts(&self) -> Vec<TaskId> {
        self.dispatched
            .values()
            .filter(|task| {
                self.timeout_manager
                    .is_timed_out(task.dispatched_at, task.payload.timeout_secs)
            })
            .map(|task| task.payload.task_id.clone())
            .collect()
    }

    /// Get the current status of a task by looking across all maps.
    pub fn get_task_status(&self, task_id: &TaskId) -> Option<TaskStatus> {
        // Check completed first
        if let Some(result) = self.completed.get(task_id) {
            return Some(result.status.clone());
        }

        // Check dispatched
        if let Some(dispatched) = self.dispatched.get(task_id) {
            return Some(TaskStatus::Dispatched {
                to_node: dispatched.assigned_to,
            });
        }

        // Check pending
        if self.pending.iter().any(|p| p.task_id == *task_id) {
            return Some(TaskStatus::Pending);
        }

        None
    }

    /// Get a completed task result by ID.
    pub fn get_result(&self, task_id: &TaskId) -> Option<&TaskResult> {
        self.completed.get(task_id)
    }

    /// Returns the number of tasks waiting in the pending queue.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Returns the number of tasks currently dispatched and in flight.
    pub fn dispatched_count(&self) -> usize {
        self.dispatched.len()
    }

    /// Returns the number of completed tasks.
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::types::{TaskPriority, TaskType};
    use nebula_core::identity::node_id::NodeId;
    use nebula_core::identity::roles::NodeRole;

    fn make_metrics(battery: u8, cpu: f32, mem: u32, tasks: u16, uptime: u64) -> crate::cluster::membership::NodeMetrics {
        crate::cluster::membership::NodeMetrics {
            battery_level: battery,
            cpu_load: cpu,
            memory_available_mb: mem,
            active_tasks: tasks,
            uptime_secs: uptime,
        }
    }

    fn make_payload(priority: TaskPriority) -> TaskPayload {
        TaskPayload {
            task_id: TaskId::generate(),
            task_type: TaskType::Ping,
            data: serde_json::json!({}),
            timeout_secs: 90,
            submitted_at: chrono::Utc::now().timestamp_millis(),
            priority,
        }
    }

    fn make_payload_with_id(id: &str, priority: TaskPriority) -> TaskPayload {
        TaskPayload {
            task_id: TaskId(id.to_string()),
            task_type: TaskType::Ping,
            data: serde_json::json!({}),
            timeout_secs: 90,
            submitted_at: chrono::Utc::now().timestamp_millis(),
            priority,
        }
    }

    fn membership_with_worker() -> (ClusterMembership, NodeId) {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, make_metrics(80, 0.3, 2048, 2, 3600));

        (membership, worker_id)
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_dispatcher_is_empty() {
        let dispatcher = TaskDispatcher::new(100);
        assert_eq!(dispatcher.pending_count(), 0);
        assert_eq!(dispatcher.dispatched_count(), 0);
        assert_eq!(dispatcher.completed_count(), 0);
    }

    // -----------------------------------------------------------------------
    // submit_task
    // -----------------------------------------------------------------------

    #[test]
    fn test_submit_task_success() {
        let mut dispatcher = TaskDispatcher::new(100);
        let payload = make_payload(TaskPriority::Normal);
        let expected_id = payload.task_id.clone();

        let result = dispatcher.submit_task(payload);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_id);
        assert_eq!(dispatcher.pending_count(), 1);
    }

    #[test]
    fn test_submit_multiple_tasks() {
        let mut dispatcher = TaskDispatcher::new(100);

        for _ in 0..10 {
            dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();
        }

        assert_eq!(dispatcher.pending_count(), 10);
    }

    #[test]
    fn test_submit_task_queue_overflow() {
        let mut dispatcher = TaskDispatcher::new(2);

        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();
        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();

        let result = dispatcher.submit_task(make_payload(TaskPriority::Normal));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Pending queue is full"));
        assert_eq!(dispatcher.pending_count(), 2);
    }

    // -----------------------------------------------------------------------
    // dispatch_next
    // -----------------------------------------------------------------------

    #[test]
    fn test_dispatch_next_empty_queue() {
        let mut dispatcher = TaskDispatcher::new(100);
        let (membership, _) = membership_with_worker();

        assert!(dispatcher.dispatch_next(&membership).is_none());
    }

    #[test]
    fn test_dispatch_next_no_workers() {
        let mut dispatcher = TaskDispatcher::new(100);
        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();

        let local_id = NodeId::generate();
        let membership = ClusterMembership::new(local_id, NodeRole::Master);

        assert!(dispatcher.dispatch_next(&membership).is_none());
        // Task should still be pending
        assert_eq!(dispatcher.pending_count(), 1);
    }

    #[test]
    fn test_dispatch_next_success() {
        let mut dispatcher = TaskDispatcher::new(100);
        let payload = make_payload(TaskPriority::Normal);
        let task_id = payload.task_id.clone();
        dispatcher.submit_task(payload).unwrap();

        let (membership, worker_id) = membership_with_worker();
        let result = dispatcher.dispatch_next(&membership);

        assert!(result.is_some());
        let (dispatched_payload, dispatched_to) = result.unwrap();
        assert_eq!(dispatched_payload.task_id, task_id);
        assert_eq!(dispatched_to, worker_id);
        assert_eq!(dispatcher.pending_count(), 0);
        assert_eq!(dispatcher.dispatched_count(), 1);
    }

    #[test]
    fn test_dispatch_next_picks_highest_priority() {
        let mut dispatcher = TaskDispatcher::new(100);

        let low = make_payload_with_id("low", TaskPriority::Low);
        let high = make_payload_with_id("high", TaskPriority::High);
        let normal = make_payload_with_id("normal", TaskPriority::Normal);

        dispatcher.submit_task(low).unwrap();
        dispatcher.submit_task(high).unwrap();
        dispatcher.submit_task(normal).unwrap();

        let (membership, _) = membership_with_worker();
        let (payload, _) = dispatcher.dispatch_next(&membership).unwrap();
        assert_eq!(payload.task_id, TaskId("high".to_string()));

        let (payload, _) = dispatcher.dispatch_next(&membership).unwrap();
        assert_eq!(payload.task_id, TaskId("normal".to_string()));

        let (payload, _) = dispatcher.dispatch_next(&membership).unwrap();
        assert_eq!(payload.task_id, TaskId("low".to_string()));

        assert_eq!(dispatcher.pending_count(), 0);
        assert_eq!(dispatcher.dispatched_count(), 3);
    }

    // -----------------------------------------------------------------------
    // mark_running
    // -----------------------------------------------------------------------

    #[test]
    fn test_mark_running() {
        let mut dispatcher = TaskDispatcher::new(100);
        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();

        let (membership, _worker_id) = membership_with_worker();
        let (payload, _) = dispatcher.dispatch_next(&membership).unwrap();

        let other_node = NodeId::generate();
        dispatcher.mark_running(&payload.task_id, other_node);

        // Status should still show Dispatched (mark_running updates internal tracking)
        let status = dispatcher.get_task_status(&payload.task_id);
        assert!(status.is_some());
        match status.unwrap() {
            TaskStatus::Dispatched { to_node } => assert_eq!(to_node, other_node),
            other => panic!("Expected Dispatched, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // mark_completed
    // -----------------------------------------------------------------------

    #[test]
    fn test_mark_completed() {
        let mut dispatcher = TaskDispatcher::new(100);
        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();

        let (membership, worker_id) = membership_with_worker();
        let (payload, _) = dispatcher.dispatch_next(&membership).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let result = TaskResult {
            task_id: payload.task_id.clone(),
            status: TaskStatus::Completed,
            data: Some(serde_json::json!({"pong": true})),
            error: None,
            executed_by: worker_id,
            started_at: now - 1000,
            completed_at: now,
        };

        dispatcher.mark_completed(result);

        assert_eq!(dispatcher.dispatched_count(), 0);
        assert_eq!(dispatcher.completed_count(), 1);

        let status = dispatcher.get_task_status(&payload.task_id);
        assert_eq!(status, Some(TaskStatus::Completed));
    }

    // -----------------------------------------------------------------------
    // mark_timed_out
    // -----------------------------------------------------------------------

    #[test]
    fn test_mark_timed_out() {
        let mut dispatcher = TaskDispatcher::new(100);
        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();

        let (membership, _) = membership_with_worker();
        let (payload, _) = dispatcher.dispatch_next(&membership).unwrap();

        dispatcher.mark_timed_out(&payload.task_id);

        assert_eq!(dispatcher.dispatched_count(), 0);
        assert_eq!(dispatcher.completed_count(), 1);

        let status = dispatcher.get_task_status(&payload.task_id);
        assert_eq!(status, Some(TaskStatus::TimedOut));

        let result = dispatcher.get_result(&payload.task_id).unwrap();
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("timed out"));
    }

    #[test]
    fn test_mark_timed_out_nonexistent_is_noop() {
        let mut dispatcher = TaskDispatcher::new(100);
        let phantom_id = TaskId("nonexistent".to_string());
        dispatcher.mark_timed_out(&phantom_id);

        assert_eq!(dispatcher.completed_count(), 0);
    }

    // -----------------------------------------------------------------------
    // check_timeouts
    // -----------------------------------------------------------------------

    #[test]
    fn test_check_timeouts_none_expired() {
        let mut dispatcher = TaskDispatcher::new(100);
        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();

        let (membership, _) = membership_with_worker();
        dispatcher.dispatch_next(&membership);

        // Just dispatched, should not be timed out
        let timed_out = dispatcher.check_timeouts();
        assert!(timed_out.is_empty());
    }

    #[test]
    fn test_check_timeouts_finds_expired() {
        let mut dispatcher = TaskDispatcher::new(100);

        // Create a task that was "submitted" long ago and has a short timeout
        let mut payload = make_payload(TaskPriority::Normal);
        payload.timeout_secs = 60; // min clamped to 60s
        let task_id = payload.task_id.clone();
        dispatcher.submit_task(payload).unwrap();

        let (membership, _) = membership_with_worker();
        dispatcher.dispatch_next(&membership);

        // Manually backdate the dispatched_at timestamp
        if let Some(dispatched) = dispatcher.dispatched.get_mut(&task_id) {
            dispatched.dispatched_at = chrono::Utc::now().timestamp_millis() - 200_000;
        }

        let timed_out = dispatcher.check_timeouts();
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], task_id);
    }

    // -----------------------------------------------------------------------
    // get_task_status
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_task_status_pending() {
        let mut dispatcher = TaskDispatcher::new(100);
        let payload = make_payload(TaskPriority::Normal);
        let task_id = payload.task_id.clone();
        dispatcher.submit_task(payload).unwrap();

        assert_eq!(dispatcher.get_task_status(&task_id), Some(TaskStatus::Pending));
    }

    #[test]
    fn test_get_task_status_dispatched() {
        let mut dispatcher = TaskDispatcher::new(100);
        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();

        let (membership, worker_id) = membership_with_worker();
        let (payload, _) = dispatcher.dispatch_next(&membership).unwrap();

        let status = dispatcher.get_task_status(&payload.task_id).unwrap();
        match status {
            TaskStatus::Dispatched { to_node } => assert_eq!(to_node, worker_id),
            other => panic!("Expected Dispatched, got {:?}", other),
        }
    }

    #[test]
    fn test_get_task_status_nonexistent() {
        let dispatcher = TaskDispatcher::new(100);
        let phantom_id = TaskId("nonexistent".to_string());
        assert!(dispatcher.get_task_status(&phantom_id).is_none());
    }

    // -----------------------------------------------------------------------
    // get_result
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_result_exists() {
        let mut dispatcher = TaskDispatcher::new(100);
        dispatcher.submit_task(make_payload(TaskPriority::Normal)).unwrap();

        let (membership, worker_id) = membership_with_worker();
        let (payload, _) = dispatcher.dispatch_next(&membership).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let result = TaskResult {
            task_id: payload.task_id.clone(),
            status: TaskStatus::Completed,
            data: Some(serde_json::json!({"ok": true})),
            error: None,
            executed_by: worker_id,
            started_at: now - 500,
            completed_at: now,
        };

        dispatcher.mark_completed(result);

        let fetched = dispatcher.get_result(&payload.task_id);
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().status, TaskStatus::Completed);
    }

    #[test]
    fn test_get_result_nonexistent() {
        let dispatcher = TaskDispatcher::new(100);
        let phantom_id = TaskId("nonexistent".to_string());
        assert!(dispatcher.get_result(&phantom_id).is_none());
    }

    // -----------------------------------------------------------------------
    // Full dispatch flow
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_dispatch_flow() {
        let mut dispatcher = TaskDispatcher::new(100);

        // Step 1: Submit
        let payload = make_payload(TaskPriority::High);
        let task_id = payload.task_id.clone();
        dispatcher.submit_task(payload).unwrap();
        assert_eq!(dispatcher.get_task_status(&task_id), Some(TaskStatus::Pending));

        // Step 2: Dispatch
        let (membership, worker_id) = membership_with_worker();
        let (dispatched_payload, selected_worker) = dispatcher.dispatch_next(&membership).unwrap();
        assert_eq!(dispatched_payload.task_id, task_id);
        assert_eq!(selected_worker, worker_id);

        // Step 3: Mark running
        dispatcher.mark_running(&task_id, worker_id);

        // Step 4: Complete
        let now = chrono::Utc::now().timestamp_millis();
        let result = TaskResult {
            task_id: task_id.clone(),
            status: TaskStatus::Completed,
            data: Some(serde_json::json!({"result": "done"})),
            error: None,
            executed_by: worker_id,
            started_at: now - 2000,
            completed_at: now,
        };
        dispatcher.mark_completed(result);

        // Step 5: Verify
        assert_eq!(dispatcher.pending_count(), 0);
        assert_eq!(dispatcher.dispatched_count(), 0);
        assert_eq!(dispatcher.completed_count(), 1);
        assert_eq!(dispatcher.get_task_status(&task_id), Some(TaskStatus::Completed));

        let final_result = dispatcher.get_result(&task_id).unwrap();
        assert_eq!(final_result.executed_by, worker_id);
        assert!(final_result.data.is_some());
    }
}
