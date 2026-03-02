use nebula_core::identity::node_id::NodeId;

use crate::cluster::membership::MemberInfo;
use crate::cluster::rotation::compute_master_score;

/// Describes a sub-cluster region with a regional master and its workers.
#[derive(Debug, Clone)]
pub struct Region {
    pub regional_master: NodeId,
    pub workers: Vec<NodeId>,
}

/// The topology of the cluster, determined by member count and thresholds.
#[derive(Debug, Clone)]
pub enum ClusterTopology {
    /// Single node operating independently.
    Standalone,
    /// Flat topology: one master, multiple workers. Suitable for small clusters.
    Flat {
        master: NodeId,
        workers: Vec<NodeId>,
    },
    /// Hierarchical topology: a super-master coordinating regional masters,
    /// each managing a subset of workers. Used for 100+ node clusters.
    Hierarchical {
        super_master: NodeId,
        regions: Vec<Region>,
    },
}

/// Manages automatic topology decisions based on cluster size.
pub struct HierarchyManager {
    /// Maximum workers a single master can handle before hierarchy is needed.
    max_workers_per_master: usize,
}

impl HierarchyManager {
    /// Create a hierarchy manager with the given worker limit per master.
    pub fn new(max_workers_per_master: usize) -> Self {
        Self {
            max_workers_per_master,
        }
    }

    /// Returns the configured maximum workers per master.
    pub fn max_workers_per_master(&self) -> usize {
        self.max_workers_per_master
    }

    /// Determine the appropriate topology for the given set of members.
    ///
    /// - 0 or 1 member: `Standalone`
    /// - 2 to `max_workers_per_master + 1` members: `Flat` (best-scored node is master)
    /// - More than `max_workers_per_master + 1` members: `Hierarchical` (best-scored
    ///   node is super-master, remaining members are split into regions)
    pub fn determine_topology(&self, members: &[&MemberInfo]) -> ClusterTopology {
        match members.len() {
            0 | 1 => ClusterTopology::Standalone,
            n if n <= self.max_workers_per_master + 1 => {
                self.build_flat_topology(members)
            }
            _ => self.build_hierarchical_topology(members),
        }
    }

    /// Returns `true` if the current member count warrants promoting sub-masters.
    pub fn should_promote_sub_master(&self, member_count: usize) -> bool {
        member_count > self.max_workers_per_master + 1
    }

    /// Build a flat topology: pick the best-scored member as master, rest are workers.
    fn build_flat_topology(&self, members: &[&MemberInfo]) -> ClusterTopology {
        let master = members
            .iter()
            .max_by(|a, b| {
                compute_master_score(&a.metrics)
                    .partial_cmp(&compute_master_score(&b.metrics))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("build_flat_topology called with non-empty members");

        let workers: Vec<NodeId> = members
            .iter()
            .filter(|m| m.node_id != master.node_id)
            .map(|m| m.node_id)
            .collect();

        ClusterTopology::Flat {
            master: master.node_id,
            workers,
        }
    }

    /// Build a hierarchical topology: pick the best as super-master, then partition
    /// remaining members into regions, each led by its best-scored member.
    fn build_hierarchical_topology(&self, members: &[&MemberInfo]) -> ClusterTopology {
        // Sort by score descending to pick super-master and regional masters
        let mut scored: Vec<(&MemberInfo, f64)> = members
            .iter()
            .map(|m| (*m, compute_master_score(&m.metrics)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let super_master = scored[0].0.node_id;
        let remaining: Vec<&MemberInfo> = scored[1..].iter().map(|(m, _)| *m).collect();

        // Calculate how many regions we need
        let region_count = (remaining.len() + self.max_workers_per_master - 1)
            / self.max_workers_per_master;
        let region_count = region_count.max(1);

        let mut regions: Vec<Region> = Vec::with_capacity(region_count);
        let chunk_size = (remaining.len() + region_count - 1) / region_count;

        for chunk in remaining.chunks(chunk_size) {
            // First member of the chunk (highest scored due to sort order) is regional master
            let regional_master = chunk[0].node_id;
            let workers: Vec<NodeId> = chunk[1..].iter().map(|m| m.node_id).collect();

            regions.push(Region {
                regional_master,
                workers,
            });
        }

        ClusterTopology::Hierarchical {
            super_master,
            regions,
        }
    }
}

impl Default for HierarchyManager {
    /// Default: 100 workers per master.
    fn default() -> Self {
        Self {
            max_workers_per_master: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::membership::NodeMetrics;
    use nebula_core::identity::roles::NodeRole;
    use std::time::Instant;

    fn make_member_with_id(node_id: NodeId, battery: u8, cpu: f32) -> MemberInfo {
        MemberInfo {
            node_id,
            role: NodeRole::Worker,
            last_heartbeat: Instant::now(),
            metrics: NodeMetrics {
                battery_level: battery,
                cpu_load: cpu,
                memory_available_mb: 2048,
                active_tasks: 0,
                uptime_secs: 0,
            },
        }
    }

    fn make_member(battery: u8, cpu: f32) -> MemberInfo {
        make_member_with_id(NodeId::generate(), battery, cpu)
    }

    #[test]
    fn test_standalone_with_no_members() {
        let hm = HierarchyManager::default();
        let members: Vec<&MemberInfo> = vec![];

        match hm.determine_topology(&members) {
            ClusterTopology::Standalone => {} // expected
            other => panic!("Expected Standalone, got {:?}", other),
        }
    }

    #[test]
    fn test_standalone_with_one_member() {
        let hm = HierarchyManager::default();
        let member = make_member(80, 0.2);
        let members: Vec<&MemberInfo> = vec![&member];

        match hm.determine_topology(&members) {
            ClusterTopology::Standalone => {} // expected
            other => panic!("Expected Standalone, got {:?}", other),
        }
    }

    #[test]
    fn test_flat_topology_two_members() {
        let hm = HierarchyManager::default();
        let m1 = make_member(90, 0.1); // higher score -> master
        let m2 = make_member(50, 0.5); // lower score -> worker

        let members: Vec<&MemberInfo> = vec![&m1, &m2];

        match hm.determine_topology(&members) {
            ClusterTopology::Flat { master, workers } => {
                assert_eq!(master, m1.node_id);
                assert_eq!(workers.len(), 1);
                assert_eq!(workers[0], m2.node_id);
            }
            other => panic!("Expected Flat, got {:?}", other),
        }
    }

    #[test]
    fn test_flat_topology_at_limit() {
        let hm = HierarchyManager::new(5); // max 5 workers per master

        // 6 members = 5 workers + 1 master = at the Flat limit
        let best = make_member(99, 0.0);
        let others: Vec<MemberInfo> = (0..5).map(|_| make_member(50, 0.5)).collect();

        let mut members: Vec<&MemberInfo> = vec![&best];
        for o in &others {
            members.push(o);
        }

        match hm.determine_topology(&members) {
            ClusterTopology::Flat { master, workers } => {
                assert_eq!(master, best.node_id);
                assert_eq!(workers.len(), 5);
            }
            other => panic!("Expected Flat, got {:?}", other),
        }
    }

    #[test]
    fn test_hierarchical_topology_exceeds_limit() {
        let hm = HierarchyManager::new(3); // max 3 workers per master

        // 5 members: exceeds max_workers_per_master + 1 = 4
        let m1 = make_member(99, 0.0);
        let m2 = make_member(80, 0.1);
        let m3 = make_member(70, 0.2);
        let m4 = make_member(60, 0.3);
        let m5 = make_member(50, 0.4);

        let members: Vec<&MemberInfo> = vec![&m1, &m2, &m3, &m4, &m5];

        match hm.determine_topology(&members) {
            ClusterTopology::Hierarchical { super_master, regions } => {
                assert_eq!(super_master, m1.node_id);
                assert!(!regions.is_empty());
                // All non-super-master members should be accounted for
                let total_in_regions: usize = regions
                    .iter()
                    .map(|r| 1 + r.workers.len()) // regional_master + workers
                    .sum();
                assert_eq!(total_in_regions, 4);
            }
            other => panic!("Expected Hierarchical, got {:?}", other),
        }
    }

    #[test]
    fn test_should_promote_sub_master_false() {
        let hm = HierarchyManager::new(100);

        assert!(!hm.should_promote_sub_master(50));
        assert!(!hm.should_promote_sub_master(100));
        assert!(!hm.should_promote_sub_master(101));
    }

    #[test]
    fn test_should_promote_sub_master_true() {
        let hm = HierarchyManager::new(100);

        assert!(hm.should_promote_sub_master(102));
        assert!(hm.should_promote_sub_master(200));
    }

    #[test]
    fn test_flat_master_is_highest_scored() {
        let hm = HierarchyManager::new(100);

        // Intentionally put the best candidate last
        let weak = make_member(20, 0.9);
        let medium = make_member(50, 0.5);
        let strong = make_member(95, 0.05);

        let members: Vec<&MemberInfo> = vec![&weak, &medium, &strong];

        match hm.determine_topology(&members) {
            ClusterTopology::Flat { master, workers } => {
                assert_eq!(master, strong.node_id);
                assert_eq!(workers.len(), 2);
            }
            other => panic!("Expected Flat, got {:?}", other),
        }
    }

    #[test]
    fn test_hierarchical_super_master_is_highest_scored() {
        let hm = HierarchyManager::new(2);

        let m1 = make_member(40, 0.6);
        let m2 = make_member(99, 0.01); // best
        let m3 = make_member(60, 0.3);
        let m4 = make_member(50, 0.4);

        let members: Vec<&MemberInfo> = vec![&m1, &m2, &m3, &m4];

        match hm.determine_topology(&members) {
            ClusterTopology::Hierarchical { super_master, .. } => {
                assert_eq!(super_master, m2.node_id);
            }
            other => panic!("Expected Hierarchical, got {:?}", other),
        }
    }

    #[test]
    fn test_default_max_workers() {
        let hm = HierarchyManager::default();
        assert_eq!(hm.max_workers_per_master(), 100);
    }

    #[test]
    fn test_hierarchy_regions_have_regional_masters() {
        let hm = HierarchyManager::new(2);

        // 7 members: 1 super-master + 6 remaining, split into regions of ~2-3
        let members_owned: Vec<MemberInfo> = (0..7)
            .map(|i| make_member(90 - i * 5, 0.1 + (i as f32) * 0.05))
            .collect();
        let members: Vec<&MemberInfo> = members_owned.iter().collect();

        match hm.determine_topology(&members) {
            ClusterTopology::Hierarchical { regions, .. } => {
                // Each region should have a regional master
                for region in &regions {
                    // regional_master should be a valid node
                    assert!(members.iter().any(|m| m.node_id == region.regional_master));
                }
            }
            other => panic!("Expected Hierarchical, got {:?}", other),
        }
    }
}
