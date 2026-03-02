use flutter_rust_bridge::frb;

use crate::api::node_api::with_engine_read;
use crate::tasks::types::{TaskId, TaskPayload};

/// Submit a task for dispatch to a worker node.
///
/// Accepts a JSON-encoded `TaskPayload` and adds it to the dispatcher's
/// pending queue. Returns the task ID as a string on success.
///
/// # JSON format
/// ```json
/// {
///   "task_id": "auto-generated-if-empty",
///   "task_type": { "Ping": null },
///   "data": {},
///   "timeout_secs": 90,
///   "submitted_at": 1700000000000,
///   "priority": "Normal"
/// }
/// ```
#[frb(sync)]
pub fn submit_task(task_json: String) -> Result<String, String> {
    with_engine_read(|engine| {
        let payload: TaskPayload =
            serde_json::from_str(&task_json).map_err(|e| format!("Invalid task JSON: {}", e))?;

        let mut dispatcher = engine
            .task_dispatcher()
            .write()
            .map_err(|e| format!("Dispatcher lock poisoned: {}", e))?;

        let task_id = dispatcher
            .submit_task(payload)
            .map_err(|e| format!("Failed to submit task: {}", e))?;

        Ok(task_id.to_string())
    })
}

/// Get the current status of a task.
///
/// Returns a JSON string describing the task's status:
/// `"Pending"`, `{"Dispatched": {"to_node": "..."}}`, `"Completed"`, etc.
/// Returns an error if the task ID is not found.
#[frb(sync)]
pub fn get_task_status(task_id: String) -> Result<String, String> {
    with_engine_read(|engine| {
        let dispatcher = engine
            .task_dispatcher()
            .read()
            .map_err(|e| format!("Dispatcher lock poisoned: {}", e))?;

        let tid = TaskId(task_id.clone());
        let status = dispatcher
            .get_task_status(&tid)
            .ok_or_else(|| format!("Task not found: {}", task_id))?;

        serde_json::to_string(&status).map_err(|e| format!("Failed to serialize status: {}", e))
    })
}

/// Get the result of a completed task.
///
/// Returns a JSON-encoded `TaskResult` on success, or an error if the task
/// has not completed yet or does not exist.
#[frb(sync)]
pub fn get_task_result(task_id: String) -> Result<String, String> {
    with_engine_read(|engine| {
        let dispatcher = engine
            .task_dispatcher()
            .read()
            .map_err(|e| format!("Dispatcher lock poisoned: {}", e))?;

        let tid = TaskId(task_id.clone());
        let result = dispatcher
            .get_result(&tid)
            .ok_or_else(|| format!("No result for task: {}", task_id))?;

        serde_json::to_string(result).map_err(|e| format!("Failed to serialize result: {}", e))
    })
}

/// Get dispatcher statistics as a JSON string.
///
/// Returns:
/// ```json
/// {
///   "pending_count": 5,
///   "dispatched_count": 2,
///   "completed_count": 10
/// }
/// ```
#[frb(sync)]
pub fn get_dispatcher_stats() -> Result<String, String> {
    with_engine_read(|engine| {
        let dispatcher = engine
            .task_dispatcher()
            .read()
            .map_err(|e| format!("Dispatcher lock poisoned: {}", e))?;

        let stats = serde_json::json!({
            "pending_count": dispatcher.pending_count(),
            "dispatched_count": dispatcher.dispatched_count(),
            "completed_count": dispatcher.completed_count(),
        });

        serde_json::to_string(&stats).map_err(|e| format!("Failed to serialize stats: {}", e))
    })
}
