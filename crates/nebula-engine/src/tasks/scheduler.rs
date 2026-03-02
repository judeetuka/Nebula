use nebula_core::identity::node_id::NodeId;
use nebula_core::identity::roles::NodeRole;

use crate::cluster::membership::{ClusterMembership, NodeMetrics};
use crate::tasks::types::TaskPayload;

/// Minimum battery percentage for a worker to be eligible for task assignment.
const MIN_BATTERY_THRESHOLD: u8 = 15;

/// Select the best available worker for a task.
///
/// Scoring: battery (30%), CPU load (30%), active tasks (30%), network (10% baseline).
/// Only considers nodes with `role == Worker` and `battery_level > 15%`.
///
/// Returns `None` if no eligible workers are found.
pub fn select_worker(
    membership: &ClusterMembership,
    _task: &TaskPayload,
) -> Option<NodeId> {
    membership
        .get_members()
        .values()
        .filter(|info| info.role == NodeRole::Worker)
        .filter(|info| info.metrics.battery_level > MIN_BATTERY_THRESHOLD)
        .max_by(|a, b| {
            compute_worker_score(&a.metrics)
                .partial_cmp(&compute_worker_score(&b.metrics))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|info| info.node_id)
}

/// Compute a worker suitability score (0.0 to 1.0).
///
/// Higher scores indicate a more suitable worker:
/// - **Battery** (30%): `battery_level / 100`
/// - **CPU load** (30%): `1.0 - cpu_load` (lower load is better)
/// - **Active tasks** (30%): `1.0 - min(active_tasks / 50, 1.0)` (fewer tasks is better)
/// - **Network** (10%): baseline 1.0 (always available; real network scoring is future work)
pub fn compute_worker_score(metrics: &NodeMetrics) -> f64 {
    let battery_score = f64::from(metrics.battery_level) / 100.0;
    let load_score = 1.0 - f64::from(metrics.cpu_load).clamp(0.0, 1.0);
    let task_score = 1.0 - (f64::from(metrics.active_tasks) / 50.0).min(1.0);

    (battery_score * 0.30) + (load_score * 0.30) + (task_score * 0.30) + 0.10
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::membership::ClusterMembership;
    use nebula_core::identity::node_id::NodeId;
    use nebula_core::identity::roles::NodeRole;

    fn make_metrics(battery: u8, cpu: f32, mem: u32, tasks: u16, uptime: u64) -> NodeMetrics {
        NodeMetrics {
            battery_level: battery,
            cpu_load: cpu,
            memory_available_mb: mem,
            active_tasks: tasks,
            uptime_secs: uptime,
        }
    }

    fn make_ping_task() -> TaskPayload {
        TaskPayload {
            task_id: crate::tasks::types::TaskId::generate(),
            task_type: crate::tasks::types::TaskType::Ping,
            data: serde_json::json!({}),
            timeout_secs: 90,
            submitted_at: chrono::Utc::now().timestamp_millis(),
            priority: crate::tasks::types::TaskPriority::Normal,
        }
    }

    // -----------------------------------------------------------------------
    // compute_worker_score tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_perfect_worker_score() {
        // Battery: 100%, CPU: 0%, Tasks: 0
        // Score: 0.30*1.0 + 0.30*1.0 + 0.30*1.0 + 0.10 = 1.0
        let metrics = make_metrics(100, 0.0, 4096, 0, 0);
        let score = compute_worker_score(&metrics);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_worst_worker_score() {
        // Battery: 0%, CPU: 100%, Tasks: 50+
        // Score: 0.30*0.0 + 0.30*0.0 + 0.30*0.0 + 0.10 = 0.10
        let metrics = make_metrics(0, 1.0, 0, 50, 0);
        let score = compute_worker_score(&metrics);
        assert!((score - 0.10).abs() < f64::EPSILON);
    }

    #[test]
    fn test_midrange_worker_score() {
        // Battery: 50%, CPU: 50%, Tasks: 25 (half of 50)
        // Score: 0.30*0.5 + 0.30*0.5 + 0.30*0.5 + 0.10 = 0.15+0.15+0.15+0.10 = 0.55
        let metrics = make_metrics(50, 0.5, 2048, 25, 3600);
        let score = compute_worker_score(&metrics);
        assert!((score - 0.55).abs() < 0.001);
    }

    #[test]
    fn test_high_battery_high_load() {
        // Battery: 100%, CPU: 100%, Tasks: 0
        // Score: 0.30*1.0 + 0.30*0.0 + 0.30*1.0 + 0.10 = 0.70
        let metrics = make_metrics(100, 1.0, 4096, 0, 0);
        let score = compute_worker_score(&metrics);
        assert!((score - 0.70).abs() < 0.001);
    }

    #[test]
    fn test_tasks_capped_at_50() {
        // Tasks beyond 50 should cap the task_score at 0.0
        let metrics = make_metrics(100, 0.0, 4096, 200, 0);
        let score = compute_worker_score(&metrics);
        // 0.30*1.0 + 0.30*1.0 + 0.30*0.0 + 0.10 = 0.70
        assert!((score - 0.70).abs() < 0.001);
    }

    #[test]
    fn test_cpu_load_clamped() {
        // CPU load beyond 1.0 should be clamped
        let metrics = NodeMetrics {
            battery_level: 100,
            cpu_load: 1.5, // invalid but handled
            memory_available_mb: 4096,
            active_tasks: 0,
            uptime_secs: 0,
        };
        let score = compute_worker_score(&metrics);
        // Same as CPU = 1.0: 0.30*1.0 + 0.30*0.0 + 0.30*1.0 + 0.10 = 0.70
        assert!((score - 0.70).abs() < 0.001);
    }

    // -----------------------------------------------------------------------
    // select_worker tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_select_worker_no_members() {
        let local_id = NodeId::generate();
        let membership = ClusterMembership::new(local_id, NodeRole::Master);
        let task = make_ping_task();

        assert!(select_worker(&membership, &task).is_none());
    }

    #[test]
    fn test_select_worker_skips_masters() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let master_id = NodeId::generate();
        membership.add_member(master_id, NodeRole::Master, make_metrics(100, 0.0, 4096, 0, 0));

        let task = make_ping_task();
        assert!(select_worker(&membership, &task).is_none());
    }

    #[test]
    fn test_select_worker_skips_low_battery() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        // Worker with battery at 10% -- below 15% threshold
        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, make_metrics(10, 0.2, 2048, 1, 3600));

        let task = make_ping_task();
        assert!(select_worker(&membership, &task).is_none());
    }

    #[test]
    fn test_select_worker_skips_battery_at_threshold() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        // Worker with battery exactly at 15% -- not above threshold
        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, make_metrics(15, 0.2, 2048, 1, 3600));

        let task = make_ping_task();
        assert!(select_worker(&membership, &task).is_none());
    }

    #[test]
    fn test_select_worker_accepts_above_threshold() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, make_metrics(16, 0.2, 2048, 1, 3600));

        let task = make_ping_task();
        assert_eq!(select_worker(&membership, &task), Some(worker_id));
    }

    #[test]
    fn test_select_worker_picks_best() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        // Weak worker: low battery, high load, many tasks
        let weak_id = NodeId::generate();
        membership.add_member(weak_id, NodeRole::Worker, make_metrics(20, 0.9, 512, 40, 0));

        // Strong worker: high battery, low load, few tasks
        let strong_id = NodeId::generate();
        membership.add_member(strong_id, NodeRole::Worker, make_metrics(95, 0.1, 4096, 1, 0));

        // Medium worker
        let medium_id = NodeId::generate();
        membership.add_member(medium_id, NodeRole::Worker, make_metrics(60, 0.4, 2048, 10, 0));

        let task = make_ping_task();
        assert_eq!(select_worker(&membership, &task), Some(strong_id));
    }

    #[test]
    fn test_select_worker_only_considers_workers() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        // A Master with perfect metrics should be ignored
        let master_id = NodeId::generate();
        membership.add_member(master_id, NodeRole::Master, make_metrics(100, 0.0, 8192, 0, 0));

        // A RegionalMaster should also be ignored
        let rm_id = NodeId::generate();
        membership.add_member(rm_id, NodeRole::RegionalMaster, make_metrics(100, 0.0, 8192, 0, 0));

        // Only this worker should be selected
        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, make_metrics(50, 0.5, 2048, 5, 0));

        let task = make_ping_task();
        assert_eq!(select_worker(&membership, &task), Some(worker_id));
    }

    #[test]
    fn test_select_worker_mixed_eligibility() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        // Worker below battery threshold -- ineligible
        let low_id = NodeId::generate();
        membership.add_member(low_id, NodeRole::Worker, make_metrics(5, 0.1, 4096, 0, 0));

        // Worker above battery threshold -- eligible
        let ok_id = NodeId::generate();
        membership.add_member(ok_id, NodeRole::Worker, make_metrics(30, 0.5, 2048, 10, 0));

        let task = make_ping_task();
        assert_eq!(select_worker(&membership, &task), Some(ok_id));
    }
}
