use crate::cluster::membership::{ClusterMembership, NodeMetrics};
use crate::cluster::rotation::compute_master_score;
use nebula_core::identity::node_id::NodeId;
use serde::{Deserialize, Serialize};

/// A single entry in the succession line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuccessionEntry {
    pub node_id: String,
    pub score: f32,
    pub rank: u32,
    pub eligible: bool,
}

/// Health thresholds for succession eligibility.
pub struct HealthThresholds {
    pub min_battery: u8,
    pub max_cpu_load: f32,
    pub min_memory_mb: u32,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            min_battery: 20,
            max_cpu_load: 0.80,
            min_memory_mb: 100,
        }
    }
}

/// Manages the succession line for failover.
pub struct SuccessionManager {
    thresholds: HealthThresholds,
}

impl SuccessionManager {
    pub fn new() -> Self {
        Self {
            thresholds: HealthThresholds::default(),
        }
    }
    pub fn with_thresholds(thresholds: HealthThresholds) -> Self {
        Self { thresholds }
    }

    /// Compute the succession line from current cluster membership.
    pub fn compute_succession_line(
        &self,
        membership: &ClusterMembership,
        current_master: &NodeId,
    ) -> Vec<SuccessionEntry> {
        let mut candidates: Vec<(&NodeId, &NodeMetrics, f64)> = membership
            .get_members()
            .iter()
            .filter(|(id, _)| *id != current_master)
            .filter(|(_, info)| self.is_eligible(&info.metrics))
            .map(|(id, info)| {
                let score = compute_master_score(&info.metrics);
                (id, &info.metrics, score)
            })
            .collect();

        candidates.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.1.uptime_secs.cmp(&a.1.uptime_secs))
        });

        candidates
            .iter()
            .enumerate()
            .map(|(i, (id, _, score))| SuccessionEntry {
                node_id: id.to_string(),
                score: *score as f32,
                rank: (i + 1) as u32,
                eligible: true,
            })
            .collect()
    }

    /// Check if a node is eligible for promotion.
    pub fn is_eligible(&self, metrics: &NodeMetrics) -> bool {
        metrics.battery_level > self.thresholds.min_battery
            && metrics.cpu_load < self.thresholds.max_cpu_load
            && metrics.memory_available_mb > self.thresholds.min_memory_mb
    }

    /// Find the designated heir (rank 1).
    pub fn designated_heir(
        &self,
        membership: &ClusterMembership,
        current_master: &NodeId,
    ) -> Option<SuccessionEntry> {
        self.compute_succession_line(membership, current_master)
            .into_iter()
            .next()
    }
}

impl Default for SuccessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Payload wrapping the succession line for storage and MQTT broadcast.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuccessionLinePayload {
    pub entries: Vec<SuccessionEntry>,
    pub computed_at: i64,
    pub computed_by: String,
    pub cluster_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn build_cluster(master_id: NodeId, members: Vec<(NodeId, NodeMetrics)>) -> ClusterMembership {
        let mut m = ClusterMembership::new(master_id, NodeRole::Master);
        for (id, metrics) in members {
            m.add_member(id, NodeRole::Worker, metrics);
        }
        m
    }

    #[test]
    fn test_compute_succession_line_excludes_master() {
        let master = NodeId::generate();
        let worker = NodeId::generate();
        let mut m = ClusterMembership::new(master, NodeRole::Master);
        m.add_member(
            master,
            NodeRole::Master,
            make_metrics(90, 0.1, 4096, 0, 3600),
        );
        m.add_member(
            worker,
            NodeRole::Worker,
            make_metrics(80, 0.2, 2048, 1, 1800),
        );
        let line = SuccessionManager::new().compute_succession_line(&m, &master);
        assert_eq!(line.len(), 1);
        assert_eq!(line[0].node_id, worker.to_string());
    }

    #[test]
    fn test_compute_succession_line_filters_by_health() {
        let master = NodeId::generate();
        let healthy = NodeId::generate();
        let low_bat = NodeId::generate();
        let high_cpu = NodeId::generate();
        let membership = build_cluster(
            master,
            vec![
                (healthy, make_metrics(80, 0.3, 2048, 2, 3600)),
                (low_bat, make_metrics(10, 0.3, 2048, 2, 3600)),
                (high_cpu, make_metrics(80, 0.95, 2048, 2, 3600)),
            ],
        );
        let line = SuccessionManager::new().compute_succession_line(&membership, &master);
        assert_eq!(line.len(), 1);
        assert_eq!(line[0].node_id, healthy.to_string());
    }

    #[test]
    fn test_compute_succession_line_orders_by_score() {
        let master = NodeId::generate();
        let weak = NodeId::generate();
        let strong = NodeId::generate();
        let medium = NodeId::generate();
        let membership = build_cluster(
            master,
            vec![
                (weak, make_metrics(30, 0.7, 512, 10, 3600)),
                (strong, make_metrics(95, 0.1, 4096, 0, 3600)),
                (medium, make_metrics(60, 0.3, 2048, 5, 3600)),
            ],
        );
        let line = SuccessionManager::new().compute_succession_line(&membership, &master);
        assert_eq!(line.len(), 3);
        assert_eq!(line[0].node_id, strong.to_string());
        assert_eq!(line[0].rank, 1);
        assert!(line[0].score >= line[1].score);
    }

    #[test]
    fn test_designated_heir_is_rank_1() {
        let master = NodeId::generate();
        let best = NodeId::generate();
        let ok = NodeId::generate();
        let membership = build_cluster(
            master,
            vec![
                (ok, make_metrics(60, 0.4, 2048, 5, 3600)),
                (best, make_metrics(95, 0.05, 4096, 0, 600)),
            ],
        );
        let heir = SuccessionManager::new()
            .designated_heir(&membership, &master)
            .unwrap();
        assert_eq!(heir.rank, 1);
        assert_eq!(heir.node_id, best.to_string());
    }

    #[test]
    fn test_designated_heir_none_when_all_unhealthy() {
        let master = NodeId::generate();
        let membership = build_cluster(
            master,
            vec![
                (NodeId::generate(), make_metrics(5, 0.3, 2048, 2, 3600)),
                (NodeId::generate(), make_metrics(10, 0.3, 2048, 2, 3600)),
            ],
        );
        assert!(SuccessionManager::new()
            .designated_heir(&membership, &master)
            .is_none());
    }

    #[test]
    fn test_is_eligible_healthy() {
        assert!(SuccessionManager::new().is_eligible(&make_metrics(80, 0.3, 2048, 2, 3600)));
    }
    #[test]
    fn test_is_eligible_low_battery() {
        assert!(!SuccessionManager::new().is_eligible(&make_metrics(15, 0.3, 2048, 2, 3600)));
    }
    #[test]
    fn test_is_eligible_high_cpu() {
        assert!(!SuccessionManager::new().is_eligible(&make_metrics(80, 0.90, 2048, 2, 3600)));
    }

    #[test]
    fn test_succession_line_payload_serialization() {
        let payload = SuccessionLinePayload {
            entries: vec![SuccessionEntry {
                node_id: "a".into(),
                score: 0.95,
                rank: 1,
                eligible: true,
            }],
            computed_at: 1700000000,
            computed_by: "master".into(),
            cluster_id: "c1".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: SuccessionLinePayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, back);
    }
}
