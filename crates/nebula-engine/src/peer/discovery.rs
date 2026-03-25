//! Service discovery for the peer-to-peer mesh.
//!
//! Tracks which nodes provide which services (roles/capabilities) so that
//! any node in the mesh can locate the best peer for a given task.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// A service that a node can provide.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeRole {
    Master,
    Worker,
    Database,
    FileHost,
    Gateway,
    Relay,
    Custom(String),
}

/// Capabilities announced by a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub node_id: String,
    pub roles: Vec<NodeRole>,
    pub peer_address: String,
    pub peer_port: u16,
    pub capacity: f32,
    pub version: String,
}

/// In-memory registry that tracks what each node in the mesh offers.
pub struct ServiceRegistry {
    entries: Arc<RwLock<HashMap<String, NodeCapabilities>>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn announce(&self, capabilities: NodeCapabilities) {
        self.entries
            .write()
            .await
            .insert(capabilities.node_id.clone(), capabilities);
    }

    pub async fn remove(&self, node_id: &str) {
        self.entries.write().await.remove(node_id);
    }

    pub async fn find_by_role(&self, role: &NodeRole) -> Vec<NodeCapabilities> {
        self.entries
            .read()
            .await
            .values()
            .filter(|cap| cap.roles.contains(role))
            .cloned()
            .collect()
    }

    pub async fn best_for_role(&self, role: &NodeRole) -> Option<NodeCapabilities> {
        self.entries
            .read()
            .await
            .values()
            .filter(|cap| cap.roles.contains(role))
            .max_by(|a, b| a.capacity.partial_cmp(&b.capacity).unwrap_or(std::cmp::Ordering::Equal))
            .cloned()
    }

    pub async fn all_nodes(&self) -> Vec<NodeCapabilities> {
        self.entries.read().await.values().cloned().collect()
    }

    pub async fn get(&self, node_id: &str) -> Option<NodeCapabilities> {
        self.entries.read().await.get(node_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(id: &str, roles: Vec<NodeRole>, capacity: f32) -> NodeCapabilities {
        NodeCapabilities {
            node_id: id.to_string(),
            roles,
            peer_address: "127.0.0.1".to_string(),
            peer_port: 9000,
            capacity,
            version: "0.1.0".to_string(),
        }
    }

    #[tokio::test]
    async fn test_announce_and_get() {
        let reg = ServiceRegistry::new();
        reg.announce(cap("node-1", vec![NodeRole::Worker], 0.8)).await;
        let found = reg.get("node-1").await.unwrap();
        assert_eq!(found.node_id, "node-1");
        assert_eq!(found.capacity, 0.8);
    }

    #[tokio::test]
    async fn test_announce_overwrites() {
        let reg = ServiceRegistry::new();
        reg.announce(cap("node-1", vec![NodeRole::Worker], 0.5)).await;
        reg.announce(cap("node-1", vec![NodeRole::Worker], 0.9)).await;
        let found = reg.get("node-1").await.unwrap();
        assert_eq!(found.capacity, 0.9);
    }

    #[tokio::test]
    async fn test_remove() {
        let reg = ServiceRegistry::new();
        reg.announce(cap("node-1", vec![NodeRole::Worker], 0.5)).await;
        reg.remove("node-1").await;
        assert!(reg.get("node-1").await.is_none());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_is_noop() {
        let reg = ServiceRegistry::new();
        reg.remove("ghost").await;
        assert_eq!(reg.all_nodes().await.len(), 0);
    }

    #[tokio::test]
    async fn test_find_by_role() {
        let reg = ServiceRegistry::new();
        reg.announce(cap("a", vec![NodeRole::Worker, NodeRole::Database], 0.6)).await;
        reg.announce(cap("b", vec![NodeRole::Database], 0.9)).await;
        reg.announce(cap("c", vec![NodeRole::Gateway], 0.7)).await;
        let dbs = reg.find_by_role(&NodeRole::Database).await;
        assert_eq!(dbs.len(), 2);
        let ids: Vec<&str> = dbs.iter().map(|c| c.node_id.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
    }

    #[tokio::test]
    async fn test_find_by_role_empty() {
        let reg = ServiceRegistry::new();
        reg.announce(cap("a", vec![NodeRole::Worker], 0.6)).await;
        assert!(reg.find_by_role(&NodeRole::Relay).await.is_empty());
    }

    #[tokio::test]
    async fn test_best_for_role() {
        let reg = ServiceRegistry::new();
        reg.announce(cap("low", vec![NodeRole::FileHost], 0.2)).await;
        reg.announce(cap("high", vec![NodeRole::FileHost], 0.95)).await;
        reg.announce(cap("mid", vec![NodeRole::FileHost], 0.5)).await;
        let best = reg.best_for_role(&NodeRole::FileHost).await.unwrap();
        assert_eq!(best.node_id, "high");
        assert_eq!(best.capacity, 0.95);
    }

    #[tokio::test]
    async fn test_best_for_role_none() {
        let reg = ServiceRegistry::new();
        assert!(reg.best_for_role(&NodeRole::Master).await.is_none());
    }

    #[tokio::test]
    async fn test_all_nodes() {
        let reg = ServiceRegistry::new();
        reg.announce(cap("a", vec![NodeRole::Worker], 0.5)).await;
        reg.announce(cap("b", vec![NodeRole::Gateway], 0.7)).await;
        assert_eq!(reg.all_nodes().await.len(), 2);
    }

    #[tokio::test]
    async fn test_custom_role() {
        let reg = ServiceRegistry::new();
        let custom = NodeRole::Custom("ml-inference".to_string());
        reg.announce(cap("gpu-node", vec![custom.clone()], 0.99)).await;
        let found = reg.find_by_role(&custom).await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id, "gpu-node");
    }

    #[tokio::test]
    async fn test_node_capabilities_serde_roundtrip() {
        let c = cap("x", vec![NodeRole::Relay, NodeRole::Custom("foo".into())], 0.42);
        let json = serde_json::to_string(&c).unwrap();
        let decoded: NodeCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.node_id, "x");
        assert_eq!(decoded.roles.len(), 2);
        assert!((decoded.capacity - 0.42).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_node_role_serde_roundtrip() {
        let roles = vec![
            NodeRole::Master, NodeRole::Worker, NodeRole::Database,
            NodeRole::FileHost, NodeRole::Gateway, NodeRole::Relay,
            NodeRole::Custom("special".into()),
        ];
        let json = serde_json::to_string(&roles).unwrap();
        let decoded: Vec<NodeRole> = serde_json::from_str(&json).unwrap();
        assert_eq!(roles, decoded);
    }
}
