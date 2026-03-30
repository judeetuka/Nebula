//! Server-mediated failover protocol.
//!
//! Workers detect master MQTT broker disconnect, wait a grace period, then
//! report to the central server via their tunnel connection. The server
//! collects reports, picks the highest-scored worker, and notifies all
//! workers of the new master.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cluster::membership::NodeMetrics;
use crate::cluster::succession::SuccessionEntry;

/// Configuration for the failover protocol.
pub struct FailoverConfig {
    /// Time without master MQTT heartbeat before declaring timeout (default: 30s).
    pub master_timeout: Duration,
    /// Grace period after timeout before reporting to server (default: 5s).
    pub grace_period: Duration,
    /// Minimum battery to be eligible for promotion.
    pub min_battery: u8,
    /// Maximum CPU load to be eligible for promotion.
    pub max_cpu_load: f32,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            master_timeout: Duration::from_secs(30),
            grace_period: Duration::from_secs(5),
            min_battery: 20,
            max_cpu_load: 0.80,
        }
    }
}

/// Failover state for a worker node (server-mediated model).
#[derive(Debug, Clone, PartialEq)]
pub enum FailoverState {
    /// Normal operation, master is alive.
    Normal,
    /// Master MQTT connection lost, monitoring timeout.
    Monitoring { lost_at: Instant },
    /// Timeout confirmed, in grace period before reporting to server.
    GracePeriod { timeout_detected: Instant },
    /// Reported master timeout to server, waiting for server's decision.
    ReportedToServer { reported_at: Instant },
    /// Server notified us that a new master has been elected.
    NewMasterElected { new_master_id: String },
    /// This node was promoted to master by the server.
    PromotedByServer,
}

/// Report sent from a worker to the server when master timeout is detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterTimeoutReport {
    pub reporter_node_id: String,
    pub cluster_id: String,
    pub last_master_heartbeat_secs_ago: u64,
    pub reporter_battery: u8,
    pub reporter_cpu_load: f32,
    pub reporter_memory_mb: u32,
    pub reporter_active_tasks: u16,
    pub reporter_uptime_secs: u64,
}

/// Notification from server to workers about the failover result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionNotice {
    pub cluster_id: String,
    pub new_master_id: String,
    pub new_master_mqtt_host: Option<String>,
    pub new_master_mqtt_port: Option<u16>,
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

    /// Update local health metrics.
    pub fn update_local_metrics(&mut self, metrics: NodeMetrics, join_time: i64) {
        self.local_metrics = Some(metrics);
        self.local_join_time = join_time;
    }

    /// Update the stored succession line (from master's MQTT broadcast).
    pub fn update_succession_line(&mut self, line: Vec<SuccessionEntry>) {
        self.succession_line = line;
    }

    /// Record that a master heartbeat was received (resets to Normal).
    pub fn master_heartbeat_received(&mut self) {
        self.state = FailoverState::Normal;
    }

    /// Called when the MQTT connection to the master broker is lost.
    pub fn mqtt_connection_lost(&mut self) {
        if matches!(self.state, FailoverState::Normal) {
            self.state = FailoverState::Monitoring {
                lost_at: Instant::now(),
            };
        }
    }

    /// Check if master timeout threshold has been reached.
    /// Returns true on Monitoring -> GracePeriod transition.
    pub fn check_master_timeout(&mut self) -> bool {
        match self.state {
            FailoverState::Normal => {
                // Auto-transition to monitoring (for backward compat with tests)
                self.state = FailoverState::Monitoring {
                    lost_at: Instant::now(),
                };
                false
            }
            FailoverState::Monitoring { lost_at } => {
                if lost_at.elapsed() >= self.config.master_timeout {
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

    /// Check if grace period has expired.
    /// Returns true when ready to report to server.
    pub fn check_grace_period(&mut self) -> bool {
        if let FailoverState::GracePeriod { timeout_detected } = self.state {
            if timeout_detected.elapsed() >= self.config.grace_period {
                self.state = FailoverState::ReportedToServer {
                    reported_at: Instant::now(),
                };
                return true;
            }
        }
        false
    }

    /// Build the timeout report to send to the server.
    pub fn build_timeout_report(&self, cluster_id: &str) -> Option<MasterTimeoutReport> {
        let metrics = self.local_metrics.as_ref()?;
        Some(MasterTimeoutReport {
            reporter_node_id: self.local_node_id.clone(),
            cluster_id: cluster_id.to_string(),
            last_master_heartbeat_secs_ago: match self.state {
                FailoverState::Monitoring { lost_at } => lost_at.elapsed().as_secs(),
                FailoverState::GracePeriod { timeout_detected } => {
                    timeout_detected.elapsed().as_secs() + self.config.master_timeout.as_secs()
                }
                FailoverState::ReportedToServer { reported_at } => {
                    reported_at.elapsed().as_secs()
                        + self.config.grace_period.as_secs()
                        + self.config.master_timeout.as_secs()
                }
                _ => 0,
            },
            reporter_battery: metrics.battery_level,
            reporter_cpu_load: metrics.cpu_load,
            reporter_memory_mb: metrics.memory_available_mb,
            reporter_active_tasks: metrics.active_tasks,
            reporter_uptime_secs: metrics.uptime_secs,
        })
    }

    /// Handle server's promotion notice.
    pub fn handle_promotion_notice(&mut self, notice: &PromotionNotice) {
        if notice.new_master_id == self.local_node_id {
            self.state = FailoverState::PromotedByServer;
        } else {
            self.state = FailoverState::NewMasterElected {
                new_master_id: notice.new_master_id.clone(),
            };
        }
    }

    /// Get current failover state.
    pub fn state(&self) -> &FailoverState {
        &self.state
    }

    /// Reset to normal operation.
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
        let c = FailoverCoordinator::new("a", FailoverConfig::default());
        assert_eq!(*c.state(), FailoverState::Normal);
    }

    #[test]
    fn test_master_heartbeat_resets_to_normal() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.mqtt_connection_lost();
        assert!(matches!(*c.state(), FailoverState::Monitoring { .. }));
        c.master_heartbeat_received();
        assert_eq!(*c.state(), FailoverState::Normal);
    }

    #[test]
    fn test_mqtt_connection_lost_transitions_to_monitoring() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.mqtt_connection_lost();
        assert!(matches!(*c.state(), FailoverState::Monitoring { .. }));
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
        c.mqtt_connection_lost();
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
        c.mqtt_connection_lost();
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
        c.mqtt_connection_lost();
        std::thread::sleep(Duration::from_millis(5));
        c.check_master_timeout();
        std::thread::sleep(Duration::from_millis(5));
        assert!(c.check_grace_period());
        assert!(matches!(*c.state(), FailoverState::ReportedToServer { .. }));
    }

    #[test]
    fn test_build_timeout_report() {
        let mut c = FailoverCoordinator::new("node-a", FailoverConfig::default());
        c.update_local_metrics(make_metrics(85, 0.3, 2048, 2, 3600), 100);
        c.mqtt_connection_lost();
        let report = c.build_timeout_report("cluster-1").unwrap();
        assert_eq!(report.reporter_node_id, "node-a");
        assert_eq!(report.cluster_id, "cluster-1");
        assert_eq!(report.reporter_battery, 85);
    }

    #[test]
    fn test_build_timeout_report_none_without_metrics() {
        let c = FailoverCoordinator::new("a", FailoverConfig::default());
        assert!(c.build_timeout_report("c1").is_none());
    }

    #[test]
    fn test_handle_promotion_notice_this_node() {
        let mut c = FailoverCoordinator::new("node-a", FailoverConfig::default());
        c.handle_promotion_notice(&PromotionNotice {
            cluster_id: "c1".into(),
            new_master_id: "node-a".into(),
            new_master_mqtt_host: None,
            new_master_mqtt_port: None,
        });
        assert_eq!(*c.state(), FailoverState::PromotedByServer);
    }

    #[test]
    fn test_handle_promotion_notice_other_node() {
        let mut c = FailoverCoordinator::new("node-a", FailoverConfig::default());
        c.handle_promotion_notice(&PromotionNotice {
            cluster_id: "c1".into(),
            new_master_id: "node-b".into(),
            new_master_mqtt_host: Some("10.0.0.2".into()),
            new_master_mqtt_port: Some(1883),
        });
        assert_eq!(
            *c.state(),
            FailoverState::NewMasterElected {
                new_master_id: "node-b".into()
            }
        );
    }

    #[test]
    fn test_reset_returns_to_normal() {
        let mut c = FailoverCoordinator::new("a", FailoverConfig::default());
        c.mqtt_connection_lost();
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
        let cfg = FailoverConfig::default();
        assert_eq!(cfg.master_timeout, Duration::from_secs(30));
        assert_eq!(cfg.grace_period, Duration::from_secs(5));
        assert_eq!(cfg.min_battery, 20);
    }

    #[test]
    fn test_master_timeout_report_serialization() {
        let report = MasterTimeoutReport {
            reporter_node_id: "n1".into(),
            cluster_id: "c1".into(),
            last_master_heartbeat_secs_ago: 35,
            reporter_battery: 80,
            reporter_cpu_load: 0.3,
            reporter_memory_mb: 2048,
            reporter_active_tasks: 2,
            reporter_uptime_secs: 3600,
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: MasterTimeoutReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.reporter_node_id, "n1");
    }

    #[test]
    fn test_promotion_notice_serialization() {
        let notice = PromotionNotice {
            cluster_id: "c1".into(),
            new_master_id: "n2".into(),
            new_master_mqtt_host: Some("10.0.0.2".into()),
            new_master_mqtt_port: Some(1883),
        };
        let json = serde_json::to_string(&notice).unwrap();
        let back: PromotionNotice = serde_json::from_str(&json).unwrap();
        assert_eq!(back.new_master_id, "n2");
    }
}
