use std::collections::HashMap;
use anyhow::{bail, Result};
use tracing::{debug, info, warn};

pub struct MqttBridge {
    node_to_cluster: HashMap<String, String>,
    cluster_to_nodes: HashMap<String, Vec<String>>,
}

impl MqttBridge {
    pub fn new() -> Self {
        Self { node_to_cluster: HashMap::new(), cluster_to_nodes: HashMap::new() }
    }
    pub fn register_external_node(&mut self, node_id: &str, cluster_id: &str) {
        info!(node_id, cluster_id, "Registering external node for relay");
        self.node_to_cluster.insert(node_id.to_string(), cluster_id.to_string());
        let nodes = self.cluster_to_nodes.entry(cluster_id.to_string()).or_default();
        if !nodes.contains(&node_id.to_string()) { nodes.push(node_id.to_string()); }
    }
    pub fn remove_external_node(&mut self, node_id: &str) {
        if let Some(cluster_id) = self.node_to_cluster.remove(node_id) {
            info!(node_id, cluster_id, "Removing external node from relay");
            if let Some(nodes) = self.cluster_to_nodes.get_mut(&cluster_id) {
                nodes.retain(|n| n != node_id);
                if nodes.is_empty() { self.cluster_to_nodes.remove(&cluster_id); }
            }
        } else { warn!(node_id, "Attempted to remove unknown external node"); }
    }
    pub fn cluster_for_node(&self, node_id: &str) -> Option<&str> {
        self.node_to_cluster.get(node_id).map(|s| s.as_str())
    }
    pub fn nodes_for_cluster(&self, cluster_id: &str) -> Option<&[String]> {
        self.cluster_to_nodes.get(cluster_id).map(|v| v.as_slice())
    }
    pub fn external_node_count(&self) -> usize { self.node_to_cluster.len() }
    pub async fn route_to_master(&self, topic: &str, payload: &[u8]) -> Result<()> {
        let parts: Vec<&str> = topic.splitn(4, '/').collect();
        if parts.len() < 3 || parts[0] != "nebula" { bail!("Invalid topic format for relay routing: {}", topic); }
        let cluster_id = parts[1]; let node_id = parts[2];
        match self.node_to_cluster.get(node_id) {
            Some(rc) if rc == cluster_id => { debug!(cluster_id, node_id, topic, payload_len = payload.len(), "Routing to master"); Ok(()) }
            Some(rc) => bail!("Node {} is registered to cluster {} but message targets cluster {}", node_id, rc, cluster_id),
            None => bail!("Node {} is not registered as an external node", node_id),
        }
    }
    pub async fn route_to_external(&self, node_id: &str, topic: &str, payload: &[u8]) -> Result<()> {
        if !self.node_to_cluster.contains_key(node_id) { bail!("Node {} is not registered as an external node", node_id); }
        debug!(node_id, topic, payload_len = payload.len(), "Routing to external node"); Ok(())
    }
}
impl Default for MqttBridge { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_new_bridge_is_empty() { assert_eq!(MqttBridge::new().external_node_count(), 0); }
    #[test] fn test_register_external_node() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); assert_eq!(b.external_node_count(),1); assert_eq!(b.cluster_for_node("n1"),Some("ca")); }
    #[test] fn test_register_multiple_same_cluster() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); b.register_external_node("n2","ca"); assert_eq!(b.external_node_count(),2); let ns = b.nodes_for_cluster("ca").unwrap(); assert_eq!(ns.len(),2); }
    #[test] fn test_register_different_clusters() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); b.register_external_node("n2","cb"); assert_eq!(b.cluster_for_node("n1"),Some("ca")); assert_eq!(b.cluster_for_node("n2"),Some("cb")); }
    #[test] fn test_remove_external_node() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); b.register_external_node("n2","ca"); b.remove_external_node("n1"); assert_eq!(b.external_node_count(),1); assert!(b.cluster_for_node("n1").is_none()); }
    #[test] fn test_remove_last_cleans_cluster() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); b.remove_external_node("n1"); assert!(b.nodes_for_cluster("ca").is_none()); }
    #[test] fn test_remove_unknown_safe() { let mut b = MqttBridge::new(); b.remove_external_node("x"); assert_eq!(b.external_node_count(),0); }
    #[test] fn test_duplicate_idempotent() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); b.register_external_node("n1","ca"); assert_eq!(b.nodes_for_cluster("ca").unwrap().len(),1); }
    #[tokio::test] async fn test_route_to_master_valid() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); assert!(b.route_to_master("nebula/ca/n1/tel",b"hi").await.is_ok()); }
    #[tokio::test] async fn test_route_to_master_invalid_topic() { let b = MqttBridge::new(); assert!(b.route_to_master("bad/topic",b"hi").await.unwrap_err().to_string().contains("Invalid topic")); }
    #[tokio::test] async fn test_route_to_master_unregistered() { let b = MqttBridge::new(); assert!(b.route_to_master("nebula/ca/unk/d",b"hi").await.unwrap_err().to_string().contains("not registered")); }
    #[tokio::test] async fn test_route_to_master_wrong_cluster() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); assert!(b.route_to_master("nebula/cb/n1/d",b"hi").await.unwrap_err().to_string().contains("registered to cluster")); }
    #[tokio::test] async fn test_route_to_external_valid() { let mut b = MqttBridge::new(); b.register_external_node("n1","ca"); assert!(b.route_to_external("n1","nebula/ca/n1/cmd",b"x").await.is_ok()); }
    #[tokio::test] async fn test_route_to_external_unregistered() { let b = MqttBridge::new(); assert!(b.route_to_external("unk","t",b"d").await.unwrap_err().to_string().contains("not registered")); }
}
