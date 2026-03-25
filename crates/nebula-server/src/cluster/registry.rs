use nebula_core::identity::node_id::{ClusterId, NodeId};
use nebula_core::identity::roles::NodeRole;
use std::collections::HashMap;

/// Tracks all connected clusters and their nodes.
pub struct ClusterRegistry {
    clusters: HashMap<ClusterId, ClusterState>,
}

/// State of a single cluster.
pub struct ClusterState {
    pub cluster_id: ClusterId,
    pub connected_nodes: HashMap<NodeId, NodeConnection>,
    pub rotation_state: RotationState,
}

/// Represents a connected node's metadata.
pub struct NodeConnection {
    pub node_id: NodeId,
    pub role: NodeRole,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

/// Tracks the state of a master rotation within a cluster.
#[derive(Debug, Clone, serde::Serialize)]
pub enum RotationState {
    /// No rotation in progress.
    None,
    /// A rotation is actively in progress.
    InProgress {
        old_master: NodeId,
        new_master: NodeId,
        started_at: chrono::DateTime<chrono::Utc>,
    },
}

impl ClusterRegistry {
    pub fn new() -> Self {
        Self {
            clusters: HashMap::new(),
        }
    }

    /// Get or create a cluster, then register a node into it.
    /// Returns the assigned role for the node.
    pub fn register_node(
        &mut self,
        cluster_id: &ClusterId,
        node_id: NodeId,
    ) -> Result<NodeRole, RegistrationError> {
        let cluster = self
            .clusters
            .entry(cluster_id.clone())
            .or_insert_with(|| ClusterState {
                cluster_id: cluster_id.clone(),
                connected_nodes: HashMap::new(),
                rotation_state: RotationState::None,
            });

        // During a rotation, the new master is allowed to re-register.
        // This is the dual-tunnel scenario: the new master connects with a fresh
        // NodeRegistrationHello while already being registered as a worker.
        if cluster.connected_nodes.contains_key(&node_id) {
            if let RotationState::InProgress { new_master, .. } = &cluster.rotation_state {
                if *new_master == node_id {
                    // Accept the dual-tunnel connection for the incoming master.
                    // The node stays registered; we return Master as its new role.
                    return Ok(NodeRole::Master);
                }
            }
            return Err(RegistrationError::NodeAlreadyRegistered);
        }

        if cluster.connected_nodes.len() >= crate::constants::MAX_NODES_PER_CLUSTER {
            return Err(RegistrationError::ClusterFull);
        }

        // First node in a cluster becomes the master; all others are workers
        let role = if cluster.connected_nodes.is_empty() {
            NodeRole::Master
        } else {
            NodeRole::Worker
        };

        let now = chrono::Utc::now();
        cluster.connected_nodes.insert(
            node_id,
            NodeConnection {
                node_id,
                role,
                connected_at: now,
                last_heartbeat: now,
            },
        );

        Ok(role)
    }

    /// Remove a node from a cluster. Returns true if the node was found and removed.
    #[allow(dead_code)]
    pub fn deregister_node(&mut self, cluster_id: &ClusterId, node_id: &NodeId) -> bool {
        if let Some(cluster) = self.clusters.get_mut(cluster_id) {
            let removed = cluster.connected_nodes.remove(node_id).is_some();
            // Clean up empty clusters
            if cluster.connected_nodes.is_empty() {
                self.clusters.remove(cluster_id);
            }
            removed
        } else {
            false
        }
    }

    /// Update the heartbeat timestamp for a node.
    #[allow(dead_code)]
    pub fn update_heartbeat(&mut self, cluster_id: &ClusterId, node_id: &NodeId) -> bool {
        if let Some(cluster) = self.clusters.get_mut(cluster_id) {
            if let Some(conn) = cluster.connected_nodes.get_mut(node_id) {
                conn.last_heartbeat = chrono::Utc::now();
                return true;
            }
        }
        false
    }

    /// Remove an entire cluster. Returns true if the cluster existed.
    pub fn remove_cluster(&mut self, cluster_id: &ClusterId) -> bool {
        self.clusters.remove(cluster_id).is_some()
    }

    /// List all cluster IDs.
    pub fn list_clusters(&self) -> Vec<ClusterInfo> {
        self.clusters
            .values()
            .map(|state| ClusterInfo {
                cluster_id: state.cluster_id.clone(),
                node_count: state.connected_nodes.len(),
            })
            .collect()
    }

    /// List all nodes in a given cluster.
    pub fn list_nodes(&self, cluster_id: &ClusterId) -> Option<Vec<NodeInfo>> {
        self.clusters.get(cluster_id).map(|state| {
            state
                .connected_nodes
                .values()
                .map(|conn| NodeInfo {
                    node_id: conn.node_id,
                    role: conn.role,
                    connected_at: conn.connected_at,
                    last_heartbeat: conn.last_heartbeat,
                })
                .collect()
        })
    }

    /// Check if a rotation is in progress for this cluster.
    pub fn is_rotating(&self, cluster_id: &ClusterId) -> bool {
        self.clusters.get(cluster_id).map_or(false, |cluster| {
            matches!(cluster.rotation_state, RotationState::InProgress { .. })
        })
    }

    /// Get the rotation status for a cluster. Returns `None` if the cluster does not exist.
    pub fn rotation_status(&self, cluster_id: &ClusterId) -> Option<&RotationState> {
        self.clusters
            .get(cluster_id)
            .map(|cluster| &cluster.rotation_state)
    }

    /// Start rotation: mark old master as "demoting", new master as "promoting".
    ///
    /// Validates that:
    /// - The cluster exists
    /// - No rotation is already in progress
    /// - The new_master is a registered worker in the cluster
    /// - There is a current master to rotate away from
    pub fn begin_rotation(
        &mut self,
        cluster_id: &ClusterId,
        new_master: &NodeId,
    ) -> Result<(), RegistrationError> {
        let cluster = self
            .clusters
            .get_mut(cluster_id)
            .ok_or(RegistrationError::ClusterNotFound)?;

        if matches!(cluster.rotation_state, RotationState::InProgress { .. }) {
            return Err(RegistrationError::RotationAlreadyInProgress);
        }

        // Find the current master
        let old_master = cluster
            .connected_nodes
            .values()
            .find(|conn| conn.role == NodeRole::Master)
            .map(|conn| conn.node_id)
            .ok_or(RegistrationError::NodeNotFound)?;

        // Validate the new master exists and is a worker
        let new_master_conn = cluster
            .connected_nodes
            .get(new_master)
            .ok_or(RegistrationError::NodeNotFound)?;

        if new_master_conn.role != NodeRole::Worker {
            return Err(RegistrationError::NotAWorker);
        }

        cluster.rotation_state = RotationState::InProgress {
            old_master,
            new_master: *new_master,
            started_at: chrono::Utc::now(),
        };

        Ok(())
    }

    /// Mark a node as the new master (during rotation).
    ///
    /// Swaps roles: new_master becomes `Master`, old_master becomes `Worker`.
    /// Only valid when a rotation is in progress for this cluster.
    pub fn promote_node(
        &mut self,
        cluster_id: &ClusterId,
        node_id: &NodeId,
    ) -> Result<(), RegistrationError> {
        let cluster = self
            .clusters
            .get_mut(cluster_id)
            .ok_or(RegistrationError::ClusterNotFound)?;

        let (old_master, new_master) = match &cluster.rotation_state {
            RotationState::InProgress {
                old_master,
                new_master,
                ..
            } => (*old_master, *new_master),
            RotationState::None => return Err(RegistrationError::NoRotationInProgress),
        };

        if *node_id != new_master {
            return Err(RegistrationError::NodeNotFound);
        }

        // Demote old master to Worker
        if let Some(old) = cluster.connected_nodes.get_mut(&old_master) {
            old.role = NodeRole::Worker;
        }

        // Promote new master to Master
        if let Some(new) = cluster.connected_nodes.get_mut(&new_master) {
            new.role = NodeRole::Master;
        }

        Ok(())
    }

    /// Complete rotation: finalize the role swap and reset rotation state.
    pub fn complete_rotation(&mut self, cluster_id: &ClusterId) -> Result<(), RegistrationError> {
        let cluster = self
            .clusters
            .get_mut(cluster_id)
            .ok_or(RegistrationError::ClusterNotFound)?;

        match &cluster.rotation_state {
            RotationState::InProgress { .. } => {
                cluster.rotation_state = RotationState::None;
                Ok(())
            }
            RotationState::None => Err(RegistrationError::NoRotationInProgress),
        }
    }
}

/// Summary info for a cluster, used by the API layer.
#[derive(serde::Serialize)]
pub struct ClusterInfo {
    pub cluster_id: ClusterId,
    pub node_count: usize,
}

/// Summary info for a node, used by the API layer.
#[derive(serde::Serialize)]
pub struct NodeInfo {
    pub node_id: NodeId,
    pub role: NodeRole,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

/// Errors that can occur during node registration and rotation.
#[derive(Debug)]
pub enum RegistrationError {
    NodeAlreadyRegistered,
    ClusterFull,
    ClusterNotFound,
    NodeNotFound,
    NotAWorker,
    RotationAlreadyInProgress,
    NoRotationInProgress,
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationError::NodeAlreadyRegistered => write!(f, "Node already registered"),
            RegistrationError::ClusterFull => write!(f, "Cluster is full"),
            RegistrationError::ClusterNotFound => write!(f, "Cluster not found"),
            RegistrationError::NodeNotFound => write!(f, "Node not found"),
            RegistrationError::NotAWorker => {
                write!(f, "Target node is not a worker")
            }
            RegistrationError::RotationAlreadyInProgress => {
                write!(f, "Rotation already in progress")
            }
            RegistrationError::NoRotationInProgress => {
                write!(f, "No rotation in progress")
            }
        }
    }
}

impl std::error::Error for RegistrationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cluster_id() -> ClusterId {
        ClusterId("test-cluster".to_string())
    }

    fn test_node_id() -> NodeId {
        NodeId::generate()
    }

    // ── Original registration tests ─────────────────────────────────────────

    #[test]
    fn test_register_first_node_becomes_master() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let node_id = test_node_id();

        let role = registry.register_node(&cluster_id, node_id).unwrap();
        assert_eq!(role, NodeRole::Master);
    }

    #[test]
    fn test_register_second_node_becomes_worker() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();

        let node1 = test_node_id();
        let node2 = test_node_id();

        let role1 = registry.register_node(&cluster_id, node1).unwrap();
        let role2 = registry.register_node(&cluster_id, node2).unwrap();

        assert_eq!(role1, NodeRole::Master);
        assert_eq!(role2, NodeRole::Worker);
    }

    #[test]
    fn test_register_duplicate_node_fails() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let node_id = test_node_id();

        registry.register_node(&cluster_id, node_id).unwrap();
        let result = registry.register_node(&cluster_id, node_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_deregister_node() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let node_id = test_node_id();

        registry.register_node(&cluster_id, node_id).unwrap();
        assert!(registry.deregister_node(&cluster_id, &node_id));

        // Cluster should be removed since it's empty
        assert!(registry.list_clusters().is_empty());
    }

    #[test]
    fn test_list_clusters() {
        let mut registry = ClusterRegistry::new();

        let cluster1 = ClusterId("cluster-1".to_string());
        let cluster2 = ClusterId("cluster-2".to_string());

        registry.register_node(&cluster1, test_node_id()).unwrap();
        registry.register_node(&cluster1, test_node_id()).unwrap();
        registry.register_node(&cluster2, test_node_id()).unwrap();

        let clusters = registry.list_clusters();
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_list_nodes_in_cluster() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();

        let node1 = test_node_id();
        let node2 = test_node_id();

        registry.register_node(&cluster_id, node1).unwrap();
        registry.register_node(&cluster_id, node2).unwrap();

        let nodes = registry.list_nodes(&cluster_id).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_list_nodes_unknown_cluster_returns_none() {
        let registry = ClusterRegistry::new();
        let cluster_id = ClusterId("nonexistent".to_string());
        assert!(registry.list_nodes(&cluster_id).is_none());
    }

    #[test]
    fn test_update_heartbeat() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let node_id = test_node_id();

        registry.register_node(&cluster_id, node_id).unwrap();

        // Get initial heartbeat
        let nodes = registry.list_nodes(&cluster_id).unwrap();
        let initial_heartbeat = nodes[0].last_heartbeat;

        // Small delay to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        assert!(registry.update_heartbeat(&cluster_id, &node_id));

        let nodes = registry.list_nodes(&cluster_id).unwrap();
        assert!(nodes[0].last_heartbeat >= initial_heartbeat);
    }

    // ── Rotation tests ──────────────────────────────────────────────────────

    #[test]
    fn test_begin_rotation_succeeds() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker).unwrap();

        let result = registry.begin_rotation(&cluster_id, &worker);
        assert!(result.is_ok());
        assert!(registry.is_rotating(&cluster_id));

        // Verify rotation state has the correct old/new master
        if let Some(RotationState::InProgress {
            old_master,
            new_master,
            ..
        }) = registry.rotation_status(&cluster_id)
        {
            assert_eq!(*old_master, master);
            assert_eq!(*new_master, worker);
        } else {
            panic!("Expected RotationState::InProgress");
        }
    }

    #[test]
    fn test_begin_rotation_cluster_not_found() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = ClusterId("nonexistent".to_string());
        let node = test_node_id();

        let result = registry.begin_rotation(&cluster_id, &node);
        assert!(matches!(result, Err(RegistrationError::ClusterNotFound)));
    }

    #[test]
    fn test_begin_rotation_already_in_progress() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker1 = test_node_id();
        let worker2 = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker1).unwrap();
        registry.register_node(&cluster_id, worker2).unwrap();

        registry.begin_rotation(&cluster_id, &worker1).unwrap();
        let result = registry.begin_rotation(&cluster_id, &worker2);
        assert!(matches!(
            result,
            Err(RegistrationError::RotationAlreadyInProgress)
        ));
    }

    #[test]
    fn test_begin_rotation_target_not_a_worker() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker).unwrap();

        // Try to rotate to the master (already a master, not a worker)
        let result = registry.begin_rotation(&cluster_id, &master);
        assert!(matches!(result, Err(RegistrationError::NotAWorker)));
    }

    #[test]
    fn test_begin_rotation_target_not_found() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let phantom_node = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();

        let result = registry.begin_rotation(&cluster_id, &phantom_node);
        assert!(matches!(result, Err(RegistrationError::NodeNotFound)));
    }

    #[test]
    fn test_promote_node_succeeds() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker).unwrap();
        registry.begin_rotation(&cluster_id, &worker).unwrap();

        let result = registry.promote_node(&cluster_id, &worker);
        assert!(result.is_ok());

        // Verify roles have swapped
        let nodes = registry.list_nodes(&cluster_id).unwrap();
        let old_master_info = nodes.iter().find(|n| n.node_id == master).unwrap();
        let new_master_info = nodes.iter().find(|n| n.node_id == worker).unwrap();

        assert_eq!(old_master_info.role, NodeRole::Worker);
        assert_eq!(new_master_info.role, NodeRole::Master);
    }

    #[test]
    fn test_promote_node_no_rotation() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker).unwrap();

        let result = registry.promote_node(&cluster_id, &worker);
        assert!(matches!(
            result,
            Err(RegistrationError::NoRotationInProgress)
        ));
    }

    #[test]
    fn test_promote_wrong_node_fails() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker1 = test_node_id();
        let worker2 = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker1).unwrap();
        registry.register_node(&cluster_id, worker2).unwrap();

        registry.begin_rotation(&cluster_id, &worker1).unwrap();

        // Try to promote worker2, but rotation targets worker1
        let result = registry.promote_node(&cluster_id, &worker2);
        assert!(matches!(result, Err(RegistrationError::NodeNotFound)));
    }

    #[test]
    fn test_complete_rotation_succeeds() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker).unwrap();
        registry.begin_rotation(&cluster_id, &worker).unwrap();
        registry.promote_node(&cluster_id, &worker).unwrap();

        let result = registry.complete_rotation(&cluster_id);
        assert!(result.is_ok());
        assert!(!registry.is_rotating(&cluster_id));
    }

    #[test]
    fn test_complete_rotation_no_rotation() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();

        let result = registry.complete_rotation(&cluster_id);
        assert!(matches!(
            result,
            Err(RegistrationError::NoRotationInProgress)
        ));
    }

    #[test]
    fn test_full_rotation_lifecycle() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker = test_node_id();

        // Setup: master + worker
        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker).unwrap();
        assert!(!registry.is_rotating(&cluster_id));

        // Phase 1: begin rotation
        registry.begin_rotation(&cluster_id, &worker).unwrap();
        assert!(registry.is_rotating(&cluster_id));

        // Phase 2: new master re-registers (dual tunnel)
        let role = registry.register_node(&cluster_id, worker).unwrap();
        assert_eq!(role, NodeRole::Master);

        // Phase 3: promote the new master (swap roles)
        registry.promote_node(&cluster_id, &worker).unwrap();

        // Verify intermediate state: rotation still in progress, but roles swapped
        assert!(registry.is_rotating(&cluster_id));
        let nodes = registry.list_nodes(&cluster_id).unwrap();
        let old = nodes.iter().find(|n| n.node_id == master).unwrap();
        let new = nodes.iter().find(|n| n.node_id == worker).unwrap();
        assert_eq!(old.role, NodeRole::Worker);
        assert_eq!(new.role, NodeRole::Master);

        // Phase 4: complete rotation
        registry.complete_rotation(&cluster_id).unwrap();
        assert!(!registry.is_rotating(&cluster_id));
    }

    #[test]
    fn test_is_rotating_nonexistent_cluster() {
        let registry = ClusterRegistry::new();
        let cluster_id = ClusterId("nonexistent".to_string());
        assert!(!registry.is_rotating(&cluster_id));
    }

    #[test]
    fn test_rotation_status_none_by_default() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let node = test_node_id();

        registry.register_node(&cluster_id, node).unwrap();

        let status = registry.rotation_status(&cluster_id);
        assert!(matches!(status, Some(RotationState::None)));
    }

    #[test]
    fn test_rotation_status_nonexistent_cluster() {
        let registry = ClusterRegistry::new();
        let cluster_id = ClusterId("nonexistent".to_string());
        assert!(registry.rotation_status(&cluster_id).is_none());
    }

    #[test]
    fn test_duplicate_register_during_rotation_for_non_target_fails() {
        let mut registry = ClusterRegistry::new();
        let cluster_id = test_cluster_id();
        let master = test_node_id();
        let worker1 = test_node_id();
        let worker2 = test_node_id();

        registry.register_node(&cluster_id, master).unwrap();
        registry.register_node(&cluster_id, worker1).unwrap();
        registry.register_node(&cluster_id, worker2).unwrap();

        // Begin rotation targeting worker1
        registry.begin_rotation(&cluster_id, &worker1).unwrap();

        // worker2 trying to re-register should still fail (only new_master is allowed)
        let result = registry.register_node(&cluster_id, worker2);
        assert!(matches!(
            result,
            Err(RegistrationError::NodeAlreadyRegistered)
        ));
    }
}
