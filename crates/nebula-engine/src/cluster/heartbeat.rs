use std::time::Duration;

use nebula_core::identity::node_id::NodeId;

use crate::cluster::membership::{ClusterMembership, NodeMetrics};

/// Manages heartbeat timing and staleness detection.
pub struct HeartbeatManager {
    /// How often heartbeats should be sent.
    interval: Duration,
    /// How long before a missing heartbeat marks a node as stale.
    stale_threshold: Duration,
}

impl HeartbeatManager {
    /// Create a heartbeat manager with the given interval and staleness threshold.
    pub fn new(interval: Duration, stale_threshold: Duration) -> Self {
        Self {
            interval,
            stale_threshold,
        }
    }

    /// Returns the heartbeat send interval.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Returns the staleness threshold.
    pub fn stale_threshold(&self) -> Duration {
        self.stale_threshold
    }

    /// Create a serialized heartbeat payload for the given node.
    ///
    /// Returns JSON bytes suitable for publishing on an MQTT heartbeat topic.
    pub fn create_heartbeat_payload(
        node_id: &NodeId,
        metrics: &NodeMetrics,
    ) -> Result<Vec<u8>, serde_json::Error> {
        let payload = serde_json::json!({
            "node_id": node_id.to_string(),
            "battery_level": metrics.battery_level,
            "cpu_load": metrics.cpu_load,
            "memory_available_mb": metrics.memory_available_mb,
            "active_tasks": metrics.active_tasks,
            "uptime_secs": metrics.uptime_secs,
            "timestamp": chrono::Utc::now().timestamp(),
        });
        serde_json::to_vec(&payload)
    }

    /// Check which members are stale in the given membership tracker.
    ///
    /// Returns the node IDs of all members whose last heartbeat exceeds
    /// this manager's staleness threshold.
    pub fn check_stale_members(&self, membership: &ClusterMembership) -> Vec<NodeId> {
        membership.stale_members(self.stale_threshold)
    }
}

impl Default for HeartbeatManager {
    /// Default: 10-second interval, 30-second staleness threshold.
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            stale_threshold: Duration::from_secs(30),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::identity::roles::NodeRole;

    fn sample_metrics() -> NodeMetrics {
        NodeMetrics {
            battery_level: 75,
            cpu_load: 0.4,
            memory_available_mb: 1500,
            active_tasks: 3,
            uptime_secs: 5400,
        }
    }

    #[test]
    fn test_new_heartbeat_manager() {
        let hm = HeartbeatManager::new(
            Duration::from_secs(5),
            Duration::from_secs(15),
        );

        assert_eq!(hm.interval(), Duration::from_secs(5));
        assert_eq!(hm.stale_threshold(), Duration::from_secs(15));
    }

    #[test]
    fn test_default_heartbeat_manager() {
        let hm = HeartbeatManager::default();

        assert_eq!(hm.interval(), Duration::from_secs(10));
        assert_eq!(hm.stale_threshold(), Duration::from_secs(30));
    }

    #[test]
    fn test_create_heartbeat_payload_is_valid_json() {
        let node_id = NodeId::generate();
        let metrics = sample_metrics();

        let payload = HeartbeatManager::create_heartbeat_payload(&node_id, &metrics).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();

        assert_eq!(parsed["node_id"].as_str().unwrap(), node_id.to_string());
        assert_eq!(parsed["battery_level"].as_u64().unwrap(), 75);
        assert!((parsed["cpu_load"].as_f64().unwrap() - 0.4).abs() < 0.01);
        assert_eq!(parsed["memory_available_mb"].as_u64().unwrap(), 1500);
        assert_eq!(parsed["active_tasks"].as_u64().unwrap(), 3);
        assert_eq!(parsed["uptime_secs"].as_u64().unwrap(), 5400);
        assert!(parsed["timestamp"].as_i64().is_some());
    }

    #[test]
    fn test_create_heartbeat_payload_has_timestamp() {
        let node_id = NodeId::generate();
        let metrics = sample_metrics();

        let before = chrono::Utc::now().timestamp();
        let payload = HeartbeatManager::create_heartbeat_payload(&node_id, &metrics).unwrap();
        let after = chrono::Utc::now().timestamp();

        let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        let ts = parsed["timestamp"].as_i64().unwrap();

        assert!(ts >= before);
        assert!(ts <= after);
    }

    #[test]
    fn test_check_stale_members_fresh() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        membership.add_member(NodeId::generate(), NodeRole::Worker, sample_metrics());
        membership.add_member(NodeId::generate(), NodeRole::Worker, sample_metrics());

        let hm = HeartbeatManager::default();
        let stale = hm.check_stale_members(&membership);

        assert!(stale.is_empty());
    }

    #[test]
    fn test_check_stale_members_all_fresh_with_custom_threshold() {
        let local_id = NodeId::generate();
        let mut membership = ClusterMembership::new(local_id, NodeRole::Master);

        membership.add_member(NodeId::generate(), NodeRole::Worker, sample_metrics());

        let hm = HeartbeatManager::new(Duration::from_secs(1), Duration::from_secs(60));
        let stale = hm.check_stale_members(&membership);

        assert!(stale.is_empty());
    }

    #[test]
    fn test_create_heartbeat_payload_boundary_battery_zero() {
        let node_id = NodeId::generate();
        let metrics = NodeMetrics {
            battery_level: 0,
            cpu_load: 1.0,
            memory_available_mb: 0,
            active_tasks: 0,
            uptime_secs: 0,
        };

        let payload = HeartbeatManager::create_heartbeat_payload(&node_id, &metrics).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();

        assert_eq!(parsed["battery_level"].as_u64().unwrap(), 0);
        assert!((parsed["cpu_load"].as_f64().unwrap() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_create_heartbeat_payload_boundary_battery_full() {
        let node_id = NodeId::generate();
        let metrics = NodeMetrics {
            battery_level: 100,
            cpu_load: 0.0,
            memory_available_mb: 8192,
            active_tasks: u16::MAX,
            uptime_secs: u64::MAX,
        };

        let payload = HeartbeatManager::create_heartbeat_payload(&node_id, &metrics).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();

        assert_eq!(parsed["battery_level"].as_u64().unwrap(), 100);
        assert_eq!(parsed["active_tasks"].as_u64().unwrap(), u64::from(u16::MAX));
    }
}
