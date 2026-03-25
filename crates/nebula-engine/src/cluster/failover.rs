//! Failover huddle protocol for deterministic master promotion.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cluster::membership::NodeMetrics;
use crate::cluster::rotation::compute_master_score;
use crate::peer::protocol::SuccessionEntry;

/// Configuration for the failover protocol.
pub struct FailoverConfig {
    pub master_timeout: Duration,
    pub grace_period: Duration,
    pub rank_wait: Duration,
    pub min_battery: u8,
    pub max_cpu_load: f32,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            master_timeout: Duration::from_secs(30),
            grace_period: Duration::from_secs(5),
            rank_wait: Duration::from_secs(5),
            min_battery: 20,
            max_cpu_load: 0.80,
        }
    }
}

/// Tracks the failover state for a worker node.
#[derive(Debug, Clone, PartialEq)]
pub enum FailoverState {
    Normal,
    Monitoring {
        last_master_heartbeat: Instant,
    },
    GracePeriod {
        timeout_detected: Instant,
    },
    Huddle {
        started: Instant,
        peer_metrics: HashMap<String, PeerHealthReport>,
    },
    Claiming,
    Acknowledged {
        new_master: String,
    },
    Promoted,
}

/// Health report shared during the huddle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerHealthReport {
    pub node_id: String,
    pub score: f32,
    pub battery_level: u8,
    pub cpu_load: f32,
    pub memory_available_mb: u32,
    pub active_tasks: u16,
    pub join_time: i64,
}

/// The failover coordinator runs on each worker node.
pub struct FailoverCoordinator {
    config: FailoverConfig,
    state: FailoverState,
    local_node_id: String,
    local_metrics: Option<NodeMetrics>,
    local_join_time: i64,
    pub succession_line: Vec<SuccessionEntry>,
}

impl FailoverCoordinator {
    pub fn new(node_id: &str, config: FailoverConfig) -> Self {
        Self {
            config,
            state: FailoverState::Normal,
            local_node_id: node_id.to_string(),
            local_metrics: None,
            local_join_time: 0,
            succession_line: Vec::new(),
        }
    }

    pub fn update_local_metrics(&mut self, metrics: NodeMetrics, join_time: i64) {
        self.local_metrics = Some(metrics);
        self.local_join_time = join_time;
    }

    pub fn update_succession_line(&mut self, line: Vec<SuccessionEntry>) {
        self.succession_line = line;
    }

    pub fn master_heartbeat_received(&mut self) {
        self.state = FailoverState::Normal;
    }

    /// Check if master has timed out. Returns true on Monitoring -> GracePeriod transition.
    pub fn check_master_timeout(&mut self) -> bool {
        match self.state {
            FailoverState::Normal => {
                self.state = FailoverState::Monitoring {
                    last_master_heartbeat: Instant::now(),
                };
                false
            }
            FailoverState::Monitoring {
                last_master_heartbeat,
            } => {
                if last_master_heartbeat.elapsed() >= self.config.master_timeout {
                    self.state = FailoverState::GracePeriod {
                        timeout_detected: Instant::now(),
                    };
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if grace period expired. Returns true on GracePeriod -> Huddle transition.
    pub fn check_grace_period(&mut self) -> bool {
        if let FailoverState::GracePeriod { timeout_detected } = self.state {
            if timeout_detected.elapsed() >= self.config.grace_period {
                self.state = FailoverState::Huddle {
                    started: Instant::now(),
                    peer_metrics: HashMap::new(),
                };
                return true;
            }
        }
        false
    }

    pub fn record_peer_health(&mut self, report: PeerHealthReport) {
        if let FailoverState::Huddle {
            ref mut peer_metrics,
            ..
        } = self.state
        {
            peer_metrics.insert(report.node_id.clone(), report);
        }
    }

    /// Determine if this node should claim promotion (deterministic algorithm).
    pub fn should_claim_promotion(&self) -> bool {
        let peer_metrics = match &self.state {
            FailoverState::Huddle { peer_metrics, .. } => peer_metrics,
            _ => return false,
        };
        let local_metrics = match &self.local_metrics {
            Some(m) => m,
            None => return false,
        };

        let local_score = compute_master_score(local_metrics) as f32;
        let local_report = PeerHealthReport {
            node_id: self.local_node_id.clone(),
            score: local_score,
            battery_level: local_metrics.battery_level,
            cpu_load: local_metrics.cpu_load,
            memory_available_mb: local_metrics.memory_available_mb,
            active_tasks: local_metrics.active_tasks,
            join_time: self.local_join_time,
        };

        let mut all: Vec<&PeerHealthReport> = peer_metrics.values().collect();
        all.push(&local_report);

        let eligible: Vec<&PeerHealthReport> = all
            .into_iter()
            .filter(|r| {
                r.battery_level >= self.config.min_battery && r.cpu_load <= self.config.max_cpu_load
            })
            .collect();

        if eligible.is_empty() {
            return false;
        }

        let winner = eligible
            .iter()
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.join_time.cmp(&a.join_time))
            })
            .unwrap();

        winner.node_id == self.local_node_id
    }

    /// Calculate wait time before claiming based on succession rank.
    pub fn promotion_wait_time(&self) -> Duration {
        let rank = self
            .succession_line
            .iter()
            .find(|e| e.node_id == self.local_node_id)
            .map(|e| e.rank)
            .unwrap_or_else(|| {
                self.succession_line
                    .iter()
                    .map(|e| e.rank)
                    .max()
                    .unwrap_or(0)
                    + 1
            });
        self.config.grace_period + self.config.rank_wait * rank.saturating_sub(1)
    }

    pub fn begin_claiming(&mut self) {
        self.state = FailoverState::Claiming;
    }
    pub fn acknowledge_claim(&mut self, new_master_id: &str) {
        self.state = FailoverState::Acknowledged {
            new_master: new_master_id.to_string(),
        };
    }
    pub fn promotion_complete(&mut self) {
        self.state = FailoverState::Promoted;
    }
    pub fn state(&self) -> &FailoverState {
        &self.state
    }
    pub fn reset(&mut self) {
        self.state = FailoverState::Normal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(battery: u8, cpu: f32, mem: u32, tasks: u16, uptime: u64) -> NodeMetrics {
        NodeMetrics {
            battery_level: battery,
            cpu_load: cpu,
            memory_available_mb: mem,
            active_tasks: tasks,
            uptime_secs: uptime,
        }
    }

    #[test]
    fn test_new_coordinator_starts_in_normal() {
        assert_eq!(
            *FailoverCoordinator::new("a", FailoverConfig::default()).state(),
            FailoverState::Normal
        );
    }

    #[test]
    fn test_master_heartbeat_received_resets_to_normal() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.check_master_timeout();
        assert!(matches!(*c.state(), FailoverState::Monitoring { .. }));
        c.master_heartbeat_received();
        assert_eq!(*c.state(), FailoverState::Normal);
    }

    #[test]
    fn test_check_master_timeout_triggers_after_threshold() {
        let mut c = FailoverCoordinator::new(
            "a",
            FailoverConfig {
                master_timeout: Duration::from_millis(1),
                ..Default::default()
            },
        );
        assert!(!c.check_master_timeout());
        std::thread::sleep(Duration::from_millis(5));
        assert!(c.check_master_timeout());
        assert!(matches!(*c.state(), FailoverState::GracePeriod { .. }));
    }

    #[test]
    fn test_check_master_timeout_does_not_trigger_early() {
        let mut c = FailoverCoordinator::new(
            "a",
            FailoverConfig {
                master_timeout: Duration::from_secs(60),
                ..Default::default()
            },
        );
        assert!(!c.check_master_timeout());
        assert!(!c.check_master_timeout());
        assert!(matches!(*c.state(), FailoverState::Monitoring { .. }));
    }

    #[test]
    fn test_check_grace_period_triggers() {
        let mut c = FailoverCoordinator::new(
            "a",
            FailoverConfig {
                master_timeout: Duration::from_millis(1),
                grace_period: Duration::from_millis(1),
                ..Default::default()
            },
        );
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        assert!(c.check_grace_period());
        assert!(matches!(*c.state(), FailoverState::Huddle { .. }));
    }

    #[test]
    fn test_record_peer_health() {
        let mut c = FailoverCoordinator::new(
            "a",
            FailoverConfig {
                master_timeout: Duration::from_millis(1),
                grace_period: Duration::from_millis(1),
                ..Default::default()
            },
        );
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_grace_period();
        c.record_peer_health(PeerHealthReport {
            node_id: "b".into(),
            score: 0.9,
            battery_level: 90,
            cpu_load: 0.1,
            memory_available_mb: 4096,
            active_tasks: 0,
            join_time: 100,
        });
        if let FailoverState::Huddle { peer_metrics, .. } = c.state() {
            assert_eq!(peer_metrics.len(), 1);
        } else {
            panic!("Expected Huddle");
        }
    }

    #[test]
    fn test_should_claim_promotion_highest_score_wins() {
        let mut c = FailoverCoordinator::new(
            "a",
            FailoverConfig {
                master_timeout: Duration::from_millis(1),
                grace_period: Duration::from_millis(1),
                ..Default::default()
            },
        );
        c.update_local_metrics(make_metrics(95, 0.1, 4096, 0, 300), 100);
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_grace_period();
        c.record_peer_health(PeerHealthReport {
            node_id: "b".into(),
            score: compute_master_score(&make_metrics(50, 0.5, 1024, 10, 7200)) as f32,
            battery_level: 50,
            cpu_load: 0.5,
            memory_available_mb: 1024,
            active_tasks: 10,
            join_time: 50,
        });
        assert!(c.should_claim_promotion());
    }

    #[test]
    fn test_should_claim_promotion_seniority_tiebreaker() {
        let mut c = FailoverCoordinator::new(
            "a",
            FailoverConfig {
                master_timeout: Duration::from_millis(1),
                grace_period: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let metrics = make_metrics(80, 0.2, 2048, 2, 3600);
        c.update_local_metrics(metrics.clone(), 200);
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_grace_period();
        let score = compute_master_score(&metrics) as f32;
        c.record_peer_health(PeerHealthReport {
            node_id: "b".into(),
            score,
            battery_level: 80,
            cpu_load: 0.2,
            memory_available_mb: 2048,
            active_tasks: 2,
            join_time: 100,
        });
        assert!(!c.should_claim_promotion()); // b joined earlier
    }

    #[test]
    fn test_should_claim_promotion_ineligible_skipped() {
        let mut c = FailoverCoordinator::new(
            "a",
            FailoverConfig {
                master_timeout: Duration::from_millis(1),
                grace_period: Duration::from_millis(1),
                ..Default::default()
            },
        );
        c.update_local_metrics(make_metrics(60, 0.3, 2048, 2, 3600), 100);
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        c.check_grace_period();
        c.record_peer_health(PeerHealthReport {
            node_id: "b".into(),
            score: 0.99,
            battery_level: 10,
            cpu_load: 0.1,
            memory_available_mb: 8192,
            active_tasks: 0,
            join_time: 50,
        });
        assert!(c.should_claim_promotion());
    }

    #[test]
    fn test_promotion_wait_time_rank_based() {
        let mut c = FailoverCoordinator::new(
            "a",
            FailoverConfig {
                grace_period: Duration::from_secs(5),
                rank_wait: Duration::from_secs(5),
                ..Default::default()
            },
        );
        c.update_succession_line(vec![
            SuccessionEntry {
                node_id: "b".into(),
                score: 0.9,
                rank: 1,
                eligible: true,
            },
            SuccessionEntry {
                node_id: "a".into(),
                score: 0.8,
                rank: 2,
                eligible: true,
            },
        ]);
        assert_eq!(c.promotion_wait_time(), Duration::from_secs(10));
    }

    #[test]
    fn test_begin_claiming() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.begin_claiming();
        assert_eq!(*c.state(), FailoverState::Claiming);
    }
    #[test]
    fn test_acknowledge_claim() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.acknowledge_claim("b");
        assert_eq!(
            *c.state(),
            FailoverState::Acknowledged {
                new_master: "b".into()
            }
        );
    }
    #[test]
    fn test_promotion_complete() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.promotion_complete();
        assert_eq!(*c.state(), FailoverState::Promoted);
    }
    #[test]
    fn test_reset() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.begin_claiming();
        c.reset();
        assert_eq!(*c.state(), FailoverState::Normal);
    }
    #[test]
    fn test_update_succession_line() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.update_succession_line(vec![SuccessionEntry {
            node_id: "x".into(),
            score: 0.95,
            rank: 1,
            eligible: true,
        }]);
        assert_eq!(c.succession_line.len(), 1);
    }
    #[test]
    fn test_failover_config_default() {
        let c = FailoverConfig::default();
        assert_eq!(c.master_timeout, Duration::from_secs(30));
        assert_eq!(c.grace_period, Duration::from_secs(5));
        assert_eq!(c.min_battery, 20);
    }
}
