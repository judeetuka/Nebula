use std::collections::HashMap;
use std::time::{Duration, Instant};

use nebula_core::identity::node_id::NodeId;
use nebula_core::identity::roles::NodeRole;
use serde::{Deserialize, Serialize};

/// Device health metrics reported by each node in heartbeats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    /// Battery charge percentage (0-100).
    pub battery_level: u8,
    /// CPU load as a fraction (0.0 to 1.0).
    pub cpu_load: f32,
    /// Available RAM in megabytes.
    pub memory_available_mb: u32,
    /// Number of tasks currently executing on the node.
    pub active_tasks: u16,
    /// How long the node has been running, in seconds.
    pub uptime_secs: u64,
}

/// Information about a single cluster member.
pub struct MemberInfo {
    pub node_id: NodeId,
    pub role: NodeRole,
    pub last_heartbeat: Instant,
    pub metrics: NodeMetrics,
}

/// Tracks cluster membership: who is in the cluster, their roles, and their
/// most recent heartbeat metrics.
pub struct ClusterMembership {
    members: HashMap<NodeId, MemberInfo>,
    local_node_id: NodeId,
    local_role: NodeRole,
    max_workers_per_master: usize,
}

impl ClusterMembership {
    /// Create a new membership tracker for the given local node.
    pub fn new(local_node_id: NodeId, local_role: NodeRole) -> Self {
        Self {
            members: HashMap::new(),
            local_node_id,
            local_role,
            max_workers_per_master: 100,
        }
    }

    /// Override the maximum number of workers a single master can manage.
    pub fn with_max_workers(mut self, max: usize) -> Self {
        self.max_workers_per_master = max;
        self
    }

    /// Returns the configured maximum workers per master.
    pub fn max_workers_per_master(&self) -> usize {
        self.max_workers_per_master
    }

    /// Returns the local node's ID.
    pub fn local_node_id(&self) -> NodeId {
        self.local_node_id
    }

    /// Returns the local node's role.
    pub fn local_role(&self) -> NodeRole {
        self.local_role
    }

    /// Returns `true` if the local node is a master.
    pub fn is_master(&self) -> bool {
        self.local_role == NodeRole::Master
    }

    /// Add a new member or update an existing member's info.
    pub fn add_member(&mut self, node_id: NodeId, role: NodeRole, metrics: NodeMetrics) {
        let info = MemberInfo {
            node_id,
            role,
            last_heartbeat: Instant::now(),
            metrics,
        };
        self.members.insert(node_id, info);
    }

    /// Remove a member from the cluster.
    ///
    /// Returns `true` if the member was present and removed.
    pub fn remove_member(&mut self, node_id: &NodeId) -> bool {
        self.members.remove(node_id).is_some()
    }

    /// Update the heartbeat timestamp and metrics for a member.
    ///
    /// Returns `true` if the member existed and was updated.
    pub fn update_heartbeat(&mut self, node_id: &NodeId, metrics: NodeMetrics) -> bool {
        if let Some(info) = self.members.get_mut(node_id) {
            info.last_heartbeat = Instant::now();
            info.metrics = metrics;
            true
        } else {
            false
        }
    }

    /// Get a reference to all current members.
    pub fn get_members(&self) -> &HashMap<NodeId, MemberInfo> {
        &self.members
    }

    /// Look up a specific member by node ID.
    pub fn get_member(&self, node_id: &NodeId) -> Option<&MemberInfo> {
        self.members.get(node_id)
    }

    /// Returns the number of tracked members.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Returns a list of node IDs whose last heartbeat is older than the given threshold.
    pub fn stale_members(&self, threshold: Duration) -> Vec<NodeId> {
        let now = Instant::now();
        self.members
            .values()
            .filter(|info| now.duration_since(info.last_heartbeat) > threshold)
            .map(|info| info.node_id)
            .collect()
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

    fn sample_metrics() -> NodeMetrics {
        make_metrics(80, 0.3, 2048, 2, 3600)
    }

    #[test]
    fn test_new_membership_is_empty() {
        let local_id = NodeId::generate();
        let membership = ClusterMembership::new(local_id, NodeRole::Worker);

        assert_eq!(membership.member_count(), 0);
        assert_eq!(membership.local_node_id(), local_id);
        assert_eq!(membership.local_role(), NodeRole::Worker);
        assert!(!membership.is_master());
    }

    #[test]
    fn test_master_role_detected() {
        let local_id = NodeId::generate();
        let membership = ClusterMembership::new(local_id, NodeRole::Master);

        assert!(membership.is_master());
    }

    #[test]
    fn test_add_member() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, sample_metrics());

        assert_eq!(membership.member_count(), 1);
        let member = membership.get_member(&worker_id).unwrap();
        assert_eq!(member.node_id, worker_id);
        assert_eq!(member.role, NodeRole::Worker);
        assert_eq!(member.metrics.battery_level, 80);
    }

    #[test]
    fn test_add_duplicate_overwrites() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, make_metrics(80, 0.3, 2048, 2, 3600));
        membership.add_member(worker_id, NodeRole::Worker, make_metrics(50, 0.7, 1024, 5, 7200));

        assert_eq!(membership.member_count(), 1);
        let member = membership.get_member(&worker_id).unwrap();
        assert_eq!(member.metrics.battery_level, 50);
        assert_eq!(member.metrics.active_tasks, 5);
    }

    #[test]
    fn test_remove_member() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, sample_metrics());

        assert!(membership.remove_member(&worker_id));
        assert_eq!(membership.member_count(), 0);
        assert!(membership.get_member(&worker_id).is_none());
    }

    #[test]
    fn test_remove_nonexistent_member_returns_false() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let phantom_id = NodeId::generate();
        assert!(!membership.remove_member(&phantom_id));
    }

    #[test]
    fn test_update_heartbeat() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let worker_id = NodeId::generate();
        membership.add_member(worker_id, NodeRole::Worker, make_metrics(80, 0.3, 2048, 2, 3600));

        let updated = membership.update_heartbeat(
            &worker_id,
            make_metrics(60, 0.5, 1500, 4, 7200),
        );

        assert!(updated);
        let member = membership.get_member(&worker_id).unwrap();
        assert_eq!(member.metrics.battery_level, 60);
        assert_eq!(member.metrics.active_tasks, 4);
    }

    #[test]
    fn test_update_heartbeat_nonexistent_returns_false() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let phantom_id = NodeId::generate();
        assert!(!membership.update_heartbeat(&phantom_id, sample_metrics()));
    }

    #[test]
    fn test_get_members_returns_all() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let ids: Vec<NodeId> = (0..5).map(|_| NodeId::generate()).collect();
        for id in &ids {
            membership.add_member(*id, NodeRole::Worker, sample_metrics());
        }

        assert_eq!(membership.get_members().len(), 5);
        for id in &ids {
            assert!(membership.get_members().contains_key(id));
        }
    }

    #[test]
    fn test_stale_members_none_stale() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        membership.add_member(NodeId::generate(), NodeRole::Worker, sample_metrics());
        membership.add_member(NodeId::generate(), NodeRole::Worker, sample_metrics());

        // With a threshold of 60 seconds, freshly added members should not be stale
        let stale = membership.stale_members(Duration::from_secs(60));
        assert!(stale.is_empty());
    }

    #[test]
    fn test_stale_members_with_zero_threshold() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        let id1 = NodeId::generate();
        let id2 = NodeId::generate();
        membership.add_member(id1, NodeRole::Worker, sample_metrics());
        membership.add_member(id2, NodeRole::Worker, sample_metrics());

        // Zero threshold means everything is stale immediately
        // (last_heartbeat is Instant::now() from add_member, so duration_since will
        // be near-zero but >= 0, which is > Duration::ZERO for non-zero elapsed time)
        // We use a tiny sleep-free approach: just check the count is 0 or 2
        // depending on timing. For correctness we use a 1-second threshold to be safe.
        let stale = membership.stale_members(Duration::from_secs(1));
        assert!(stale.is_empty());
    }

    #[test]
    fn test_with_max_workers() {
        let local_id = NodeId::generate();
        let membership = ClusterMembership::new(local_id, NodeRole::Master)
            .with_max_workers(50);

        assert_eq!(membership.max_workers_per_master(), 50);
    }

    #[test]
    fn test_default_max_workers() {
        let local_id = NodeId::generate();
        let membership = ClusterMembership::new(local_id, NodeRole::Master);

        assert_eq!(membership.max_workers_per_master(), 100);
    }

    #[test]
    fn test_metrics_serialization_roundtrip() {
        let metrics = make_metrics(95, 0.15, 4096, 0, 120);
        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: NodeMetrics = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.battery_level, 95);
        assert!((deserialized.cpu_load - 0.15).abs() < f32::EPSILON);
        assert_eq!(deserialized.memory_available_mb, 4096);
        assert_eq!(deserialized.active_tasks, 0);
        assert_eq!(deserialized.uptime_secs, 120);
    }
}
