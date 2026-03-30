//! Node service role management.

use std::collections::HashMap;
use std::sync::Arc;
use anyhow::{bail, Result};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// A service role that a node can provide to the cluster.
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

/// Capabilities announced by a node to the cluster via MQTT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub node_id: String,
    pub roles: Vec<NodeRole>,
    pub peer_address: String,
    pub peer_port: u16,
    pub capacity: f32,
    pub version: String,
}

/// Trait for implementing a service that responds to data requests.
pub trait ServiceHandler: Send + Sync {
    fn handle_request(&self, action: &str, payload: &[u8]) -> Result<Vec<u8>>;
    fn role(&self) -> NodeRole;
    fn supported_actions(&self) -> Vec<String>;
}

/// Manages the node's active roles and their service handlers.
pub struct RoleManager {
    node_id: String,
    roles: Arc<RwLock<Vec<NodeRole>>>,
    handlers: Arc<RwLock<HashMap<NodeRole, Box<dyn ServiceHandler>>>>,
}

impl RoleManager {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            roles: Arc::new(RwLock::new(Vec::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn assign_role(&self, role: NodeRole) -> Result<()> {
        let mut roles = self.roles.write().await;
        if !roles.contains(&role) { roles.push(role); }
        Ok(())
    }

    pub async fn remove_role(&self, role: &NodeRole) -> Result<()> {
        self.roles.write().await.retain(|r| r != role);
        self.handlers.write().await.remove(role);
        Ok(())
    }

    pub async fn register_handler(&self, handler: Box<dyn ServiceHandler>) -> Result<()> {
        let role = handler.role();
        self.handlers.write().await.insert(role, handler);
        Ok(())
    }

    pub async fn current_roles(&self) -> Vec<NodeRole> { self.roles.read().await.clone() }

    pub async fn build_capabilities(&self, address: &str, port: u16) -> NodeCapabilities {
        NodeCapabilities {
            node_id: self.node_id.clone(),
            roles: self.roles.read().await.clone(),
            peer_address: address.to_string(),
            peer_port: port,
            capacity: 1.0,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub async fn handle_request(&self, role: &NodeRole, action: &str, payload: &[u8]) -> Result<Vec<u8>> {
        let handlers = self.handlers.read().await;
        match handlers.get(role) {
            Some(handler) => handler.handle_request(action, payload),
            None => bail!("no handler registered for role {:?}", role),
        }
    }

    pub async fn has_role(&self, role: &NodeRole) -> bool {
        self.roles.read().await.contains(role)
    }
}

/// Built-in echo service handler for testing.
pub struct EchoServiceHandler;
impl ServiceHandler for EchoServiceHandler {
    fn handle_request(&self, action: &str, payload: &[u8]) -> Result<Vec<u8>> {
        Ok([action.as_bytes(), b":", payload].concat())
    }
    fn role(&self) -> NodeRole { NodeRole::Custom("echo".to_string()) }
    fn supported_actions(&self) -> Vec<String> { vec!["echo".to_string(), "ping".to_string()] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_assign_and_get_roles() {
        let mgr = RoleManager::new("node-1");
        mgr.assign_role(NodeRole::Database).await.unwrap();
        mgr.assign_role(NodeRole::Gateway).await.unwrap();
        assert_eq!(mgr.current_roles().await.len(), 2);
        mgr.assign_role(NodeRole::Database).await.unwrap(); // idempotent
        assert_eq!(mgr.current_roles().await.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_role() {
        let mgr = RoleManager::new("node-1");
        mgr.assign_role(NodeRole::FileHost).await.unwrap();
        mgr.assign_role(NodeRole::Relay).await.unwrap();
        mgr.remove_role(&NodeRole::FileHost).await.unwrap();
        assert_eq!(mgr.current_roles().await.len(), 1);
        assert!(!mgr.has_role(&NodeRole::FileHost).await);
    }

    #[tokio::test]
    async fn test_register_and_dispatch_handler() {
        let mgr = RoleManager::new("node-1");
        mgr.register_handler(Box::new(EchoServiceHandler)).await.unwrap();
        let result = mgr.handle_request(&NodeRole::Custom("echo".into()), "echo", b"hello").await.unwrap();
        assert_eq!(result, b"echo:hello");
    }

    #[tokio::test]
    async fn test_dispatch_to_unknown_role_fails() {
        let mgr = RoleManager::new("node-1");
        assert!(mgr.handle_request(&NodeRole::Database, "query", b"").await.is_err());
    }

    #[tokio::test]
    async fn test_has_role() {
        let mgr = RoleManager::new("node-1");
        assert!(!mgr.has_role(&NodeRole::Worker).await);
        mgr.assign_role(NodeRole::Worker).await.unwrap();
        assert!(mgr.has_role(&NodeRole::Worker).await);
    }

    #[tokio::test]
    async fn test_build_capabilities() {
        let mgr = RoleManager::new("node-42");
        mgr.assign_role(NodeRole::Database).await.unwrap();
        let caps = mgr.build_capabilities("10.0.0.1", 9090).await;
        assert_eq!(caps.node_id, "node-42");
        assert_eq!(caps.peer_port, 9090);
        assert!(caps.roles.contains(&NodeRole::Database));
    }

    #[tokio::test]
    async fn test_echo_handler() {
        let h = EchoServiceHandler;
        assert_eq!(h.role(), NodeRole::Custom("echo".into()));
        assert_eq!(h.handle_request("ping", b"").unwrap(), b"ping:");
        assert_eq!(h.handle_request("echo", b"world").unwrap(), b"echo:world");
    }

    #[tokio::test]
    async fn test_multiple_roles() {
        let mgr = RoleManager::new("multi");
        for r in [NodeRole::Master, NodeRole::Worker, NodeRole::Database, NodeRole::FileHost, NodeRole::Gateway] {
            mgr.assign_role(r).await.unwrap();
        }
        assert_eq!(mgr.current_roles().await.len(), 5);
        mgr.remove_role(&NodeRole::Gateway).await.unwrap();
        assert_eq!(mgr.current_roles().await.len(), 4);
    }
}
