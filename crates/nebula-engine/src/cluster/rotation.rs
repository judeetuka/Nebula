use std::time::{Duration, Instant};

use nebula_core::identity::node_id::NodeId;

use crate::cluster::membership::{MemberInfo, NodeMetrics};

/// Weights for the V-formation master scoring algorithm.
const BATTERY_WEIGHT: f64 = 0.35;
const LOAD_WEIGHT: f64 = 0.25;
const UPTIME_WEIGHT: f64 = 0.25;
const TASKS_WEIGHT: f64 = 0.15;

/// Maximum uptime (seconds) used for normalization (24 hours).
const MAX_UPTIME_SECS: f64 = 86400.0;

/// Maximum active tasks used for normalization.
const MAX_ACTIVE_TASKS: f64 = 100.0;

/// Compute a composite master score for the given node metrics.
///
/// Higher scores indicate a node is better suited to be master.
///
/// The score is a weighted sum of:
/// - **Battery** (35%): `battery_level / 100`
/// - **Load** (25%): `1.0 - cpu_load` (lower load is better)
/// - **Uptime** (25%): `1.0 - min(uptime_secs / MAX_UPTIME, 1.0)` (fresher is better)
/// - **Tasks** (15%): `1.0 - min(active_tasks / MAX_TASKS, 1.0)` (fewer tasks is better)
pub fn compute_master_score(metrics: &NodeMetrics) -> f64 {
    let battery_score = f64::from(metrics.battery_level) / 100.0;
    let load_score = 1.0 - f64::from(metrics.cpu_load).clamp(0.0, 1.0);
    let uptime_normalized = (metrics.uptime_secs as f64 / MAX_UPTIME_SECS).min(1.0);
    let uptime_score = 1.0 - uptime_normalized;
    let tasks_normalized = (f64::from(metrics.active_tasks) / MAX_ACTIVE_TASKS).min(1.0);
    let tasks_score = 1.0 - tasks_normalized;

    (BATTERY_WEIGHT * battery_score)
        + (LOAD_WEIGHT * load_score)
        + (UPTIME_WEIGHT * uptime_score)
        + (TASKS_WEIGHT * tasks_score)
}

/// Manages master rotation decisions for V-formation clustering.
///
/// The rotation manager periodically evaluates whether the current master
/// should hand off leadership based on composite scoring, battery floor,
/// and time limits.
pub struct RotationManager {
    /// How often to check for rotation (default: 5 minutes).
    check_interval: Duration,
    /// Minimum score difference to trigger rotation (default: 0.15).
    score_threshold: f64,
    /// Battery level below which rotation is forced (default: 20%).
    battery_floor: u8,
    /// Maximum time a node should serve as master (default: 6 hours).
    time_limit: Duration,
    /// When the current master started serving (set via `start_tracking`).
    master_since: Option<Instant>,
}

impl RotationManager {
    /// Create a rotation manager with default parameters.
    pub fn new() -> Self {
        Self {
            check_interval: Duration::from_secs(300),
            score_threshold: 0.15,
            battery_floor: 20,
            time_limit: Duration::from_secs(6 * 3600),
            master_since: None,
        }
    }

    /// Create a rotation manager with custom parameters.
    pub fn with_params(
        check_interval: Duration,
        score_threshold: f64,
        battery_floor: u8,
        time_limit: Duration,
    ) -> Self {
        Self {
            check_interval,
            score_threshold,
            battery_floor,
            time_limit,
            master_since: None,
        }
    }

    /// Returns the check interval.
    pub fn check_interval(&self) -> Duration {
        self.check_interval
    }

    /// Returns the score threshold.
    pub fn score_threshold(&self) -> f64 {
        self.score_threshold
    }

    /// Returns the battery floor.
    pub fn battery_floor(&self) -> u8 {
        self.battery_floor
    }

    /// Returns the time limit.
    pub fn time_limit(&self) -> Duration {
        self.time_limit
    }

    /// Start tracking master tenure from now.
    ///
    /// Should be called when this node becomes master.
    pub fn start_tracking(&mut self) {
        self.master_since = Some(Instant::now());
    }

    /// Reset tracking (e.g., when this node stops being master).
    pub fn stop_tracking(&mut self) {
        self.master_since = None;
    }

    /// Returns how long the current master has been serving, if tracked.
    pub fn master_tenure(&self) -> Option<Duration> {
        self.master_since.map(|since| since.elapsed())
    }

    /// Determine whether rotation should occur and who should be the new master.
    ///
    /// Returns `Some(node_id)` of the best candidate if rotation is warranted,
    /// or `None` if the current master should continue.
    ///
    /// Rotation is triggered if any of these conditions are met:
    /// 1. Current master's battery is below the battery floor
    /// 2. Master has served longer than the time limit
    /// 3. A candidate's score exceeds the master's score by more than `score_threshold`
    pub fn should_rotate(
        &self,
        current_master_metrics: &NodeMetrics,
        all_members: &[&MemberInfo],
    ) -> Option<NodeId> {
        let master_score = compute_master_score(current_master_metrics);

        // Condition 1: Battery floor breach
        let battery_critical = current_master_metrics.battery_level < self.battery_floor;

        // Condition 2: Time limit exceeded
        let time_exceeded = self
            .master_since
            .map(|since| since.elapsed() > self.time_limit)
            .unwrap_or(false);

        // Find the best candidate among all members
        let best_candidate = all_members
            .iter()
            .max_by(|a, b| {
                compute_master_score(&a.metrics)
                    .partial_cmp(&compute_master_score(&b.metrics))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        let best_candidate = match best_candidate {
            Some(c) => c,
            None => return None,
        };

        let candidate_score = compute_master_score(&best_candidate.metrics);

        // Condition 3: Score delta exceeds threshold
        let score_delta_exceeded = (candidate_score - master_score) > self.score_threshold;

        if battery_critical || time_exceeded || score_delta_exceeded {
            Some(best_candidate.node_id)
        } else {
            None
        }
    }
}

impl Default for RotationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::identity::roles::NodeRole;
    use std::time::Instant;

    fn make_metrics(battery: u8, cpu: f32, mem: u32, tasks: u16, uptime: u64) -> NodeMetrics {
        NodeMetrics {
            battery_level: battery,
            cpu_load: cpu,
            memory_available_mb: mem,
            active_tasks: tasks,
            uptime_secs: uptime,
        }
    }

    fn make_member(metrics: NodeMetrics) -> MemberInfo {
        MemberInfo {
            node_id: NodeId::generate(),
            role: NodeRole::Worker,
            last_heartbeat: Instant::now(),
            metrics,
        }
    }

    // -----------------------------------------------------------------------
    // compute_master_score tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_perfect_score() {
        // Battery: 100%, CPU: 0%, Uptime: 0s, Tasks: 0
        // Score: 0.35*1.0 + 0.25*1.0 + 0.25*1.0 + 0.15*1.0 = 1.0
        let metrics = make_metrics(100, 0.0, 4096, 0, 0);
        let score = compute_master_score(&metrics);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_worst_score() {
        // Battery: 0%, CPU: 100%, Uptime: 24h+, Tasks: 100+
        // Score: 0.35*0.0 + 0.25*0.0 + 0.25*0.0 + 0.15*0.0 = 0.0
        let metrics = make_metrics(0, 1.0, 0, 100, 86400);
        let score = compute_master_score(&metrics);
        assert!(score.abs() < f64::EPSILON);
    }

    #[test]
    fn test_half_battery_idle() {
        // Battery: 50%, CPU: 0%, Uptime: 0s, Tasks: 0
        // Score: 0.35*0.5 + 0.25*1.0 + 0.25*1.0 + 0.15*1.0 = 0.175 + 0.25 + 0.25 + 0.15 = 0.825
        let metrics = make_metrics(50, 0.0, 4096, 0, 0);
        let score = compute_master_score(&metrics);
        assert!((score - 0.825).abs() < 0.001);
    }

    #[test]
    fn test_full_battery_full_load() {
        // Battery: 100%, CPU: 100%, Uptime: 0, Tasks: 0
        // Score: 0.35*1.0 + 0.25*0.0 + 0.25*1.0 + 0.15*1.0 = 0.75
        let metrics = make_metrics(100, 1.0, 4096, 0, 0);
        let score = compute_master_score(&metrics);
        assert!((score - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_score_midrange() {
        // Battery: 60%, CPU: 40%, Uptime: 12h (half of 24h), Tasks: 50 (half of 100)
        // Battery: 0.35 * 0.6 = 0.21
        // Load: 0.25 * 0.6 = 0.15
        // Uptime: 0.25 * 0.5 = 0.125
        // Tasks: 0.15 * 0.5 = 0.075
        // Total: 0.56
        let metrics = make_metrics(60, 0.4, 2048, 50, 43200);
        let score = compute_master_score(&metrics);
        assert!((score - 0.56).abs() < 0.001);
    }

    #[test]
    fn test_uptime_capped_at_max() {
        // Uptime well beyond 24h should be capped (score component = 0)
        let metrics = make_metrics(100, 0.0, 4096, 0, 200_000);
        let score = compute_master_score(&metrics);
        // Battery: 0.35, Load: 0.25, Uptime: 0.0 (capped), Tasks: 0.15
        assert!((score - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_tasks_capped_at_max() {
        // Tasks well beyond 100 should be capped (score component = 0)
        let metrics = make_metrics(100, 0.0, 4096, 500, 0);
        let score = compute_master_score(&metrics);
        // Battery: 0.35, Load: 0.25, Uptime: 0.25, Tasks: 0.0 (capped)
        assert!((score - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_score_weights_sum_to_one() {
        let sum = BATTERY_WEIGHT + LOAD_WEIGHT + UPTIME_WEIGHT + TASKS_WEIGHT;
        assert!((sum - 1.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // RotationManager tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_rotation_manager_defaults() {
        let rm = RotationManager::new();

        assert_eq!(rm.check_interval(), Duration::from_secs(300));
        assert!((rm.score_threshold() - 0.15).abs() < f64::EPSILON);
        assert_eq!(rm.battery_floor(), 20);
        assert_eq!(rm.time_limit(), Duration::from_secs(6 * 3600));
        assert!(rm.master_tenure().is_none());
    }

    #[test]
    fn test_rotation_manager_custom_params() {
        let rm = RotationManager::with_params(
            Duration::from_secs(60),
            0.2,
            30,
            Duration::from_secs(3600),
        );

        assert_eq!(rm.check_interval(), Duration::from_secs(60));
        assert!((rm.score_threshold() - 0.2).abs() < f64::EPSILON);
        assert_eq!(rm.battery_floor(), 30);
        assert_eq!(rm.time_limit(), Duration::from_secs(3600));
    }

    #[test]
    fn test_start_tracking() {
        let mut rm = RotationManager::new();
        assert!(rm.master_tenure().is_none());

        rm.start_tracking();
        assert!(rm.master_tenure().is_some());

        let tenure = rm.master_tenure().unwrap();
        // Should be very small (just started)
        assert!(tenure < Duration::from_secs(1));
    }

    #[test]
    fn test_stop_tracking() {
        let mut rm = RotationManager::new();
        rm.start_tracking();
        assert!(rm.master_tenure().is_some());

        rm.stop_tracking();
        assert!(rm.master_tenure().is_none());
    }

    #[test]
    fn test_no_rotation_healthy_master_no_candidates() {
        let rm = RotationManager::new();
        let master_metrics = make_metrics(80, 0.3, 2048, 2, 3600);

        let result = rm.should_rotate(&master_metrics, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_no_rotation_master_better_than_candidates() {
        let rm = RotationManager::new();
        // Master: high battery, low load
        let master_metrics = make_metrics(90, 0.1, 4096, 1, 1800);

        // Candidate: similar or worse
        let candidate = make_member(make_metrics(85, 0.2, 2048, 3, 3600));
        let members: Vec<&MemberInfo> = vec![&candidate];

        let result = rm.should_rotate(&master_metrics, &members);
        assert!(result.is_none());
    }

    #[test]
    fn test_rotation_battery_floor_breach() {
        let rm = RotationManager::new(); // battery_floor = 20

        // Master battery at 15% -- below the 20% floor
        let master_metrics = make_metrics(15, 0.2, 2048, 2, 3600);

        let candidate = make_member(make_metrics(80, 0.3, 2048, 2, 3600));
        let members: Vec<&MemberInfo> = vec![&candidate];

        let result = rm.should_rotate(&master_metrics, &members);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), candidate.node_id);
    }

    #[test]
    fn test_rotation_time_limit_exceeded() {
        let mut rm = RotationManager::with_params(
            Duration::from_secs(300),
            0.15,
            20,
            Duration::from_millis(1), // Very short time limit for testing
        );

        rm.start_tracking();
        // Even a tiny sleep isn't needed -- the Instant elapsed will exceed 1ms
        // by the time we reach should_rotate.
        std::thread::sleep(Duration::from_millis(5));

        let master_metrics = make_metrics(80, 0.3, 2048, 2, 3600);
        let candidate = make_member(make_metrics(80, 0.3, 2048, 2, 3600));
        let members: Vec<&MemberInfo> = vec![&candidate];

        let result = rm.should_rotate(&master_metrics, &members);
        assert!(result.is_some());
    }

    #[test]
    fn test_rotation_score_delta_exceeded() {
        let rm = RotationManager::new(); // score_threshold = 0.15

        // Master: low battery, high load -> low score
        let master_metrics = make_metrics(30, 0.8, 512, 10, 43200);
        let master_score = compute_master_score(&master_metrics);

        // Candidate: high battery, low load -> high score
        let candidate = make_member(make_metrics(95, 0.1, 4096, 0, 600));
        let candidate_score = compute_master_score(&candidate.metrics);

        // Verify the delta actually exceeds threshold
        assert!(candidate_score - master_score > 0.15);

        let members: Vec<&MemberInfo> = vec![&candidate];
        let result = rm.should_rotate(&master_metrics, &members);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), candidate.node_id);
    }

    #[test]
    fn test_rotation_picks_best_candidate() {
        let rm = RotationManager::new();

        // Master is in bad shape
        let master_metrics = make_metrics(10, 0.9, 256, 20, 80000);

        let weak_candidate = make_member(make_metrics(30, 0.7, 512, 15, 60000));
        let strong_candidate = make_member(make_metrics(95, 0.05, 4096, 0, 300));
        let medium_candidate = make_member(make_metrics(60, 0.3, 2048, 5, 7200));

        let members: Vec<&MemberInfo> = vec![&weak_candidate, &strong_candidate, &medium_candidate];
        let result = rm.should_rotate(&master_metrics, &members);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), strong_candidate.node_id);
    }

    #[test]
    fn test_no_rotation_without_time_tracking() {
        let rm = RotationManager::new(); // master_since is None

        // Healthy master, no score delta issue
        let master_metrics = make_metrics(80, 0.3, 2048, 2, 3600);
        let candidate = make_member(make_metrics(82, 0.28, 2048, 2, 3600));
        let members: Vec<&MemberInfo> = vec![&candidate];

        // Time limit check should evaluate to false when master_since is None
        let result = rm.should_rotate(&master_metrics, &members);
        assert!(result.is_none());
    }
}
