use std::collections::HashMap;
use std::net::SocketAddr;

use nebula_core::identity::node_id::NodeId;

/// mDNS service name for NEBULA LAN peer discovery.
pub const MDNS_SERVICE_NAME: &str = "_nebula._tcp.local.";

/// Manages mDNS service registration and discovery for LAN peers.
///
/// This is currently a data structure that tracks discovered peers.
/// Actual mDNS broadcast/listen integration (via the `mdns-sd` crate)
/// will be wired in a future phase. For now, peers are added manually
/// via `add_discovered_peer` or through MQTT signaling.
///
/// TXT records carried by the mDNS service:
/// - `cluster_id={id}` -- the cluster this node belongs to
/// - `node_id={id}` -- the node's unique identifier
pub struct LanDiscovery {
    cluster_id: String,
    node_id: NodeId,
    listen_port: u16,
    discovered_peers: HashMap<NodeId, SocketAddr>,
    /// Whether this node has registered its mDNS service.
    registered: bool,
}

impl LanDiscovery {
    /// Create a new LAN discovery manager for the given node and cluster.
    pub fn new(cluster_id: &str, node_id: NodeId, listen_port: u16) -> Self {
        Self {
            cluster_id: cluster_id.to_string(),
            node_id,
            listen_port,
            discovered_peers: HashMap::new(),
            registered: false,
        }
    }

    /// Announce this node on the LAN via mDNS.
    ///
    /// Currently stores the registration state locally. Actual mDNS
    /// broadcast will be integrated when the `mdns-sd` dependency is added.
    pub fn register_service(&mut self) -> Result<(), String> {
        self.registered = true;
        Ok(())
    }

    /// Returns `true` if this node's mDNS service has been registered.
    pub fn is_registered(&self) -> bool {
        self.registered
    }

    /// Returns all peers discovered on the LAN.
    pub fn discovered_peers(&self) -> &HashMap<NodeId, SocketAddr> {
        &self.discovered_peers
    }

    /// Add a discovered peer (from mDNS or manual fallback).
    ///
    /// If the peer was already known, its address is updated.
    pub fn add_discovered_peer(&mut self, node_id: NodeId, addr: SocketAddr) {
        self.discovered_peers.insert(node_id, addr);
    }

    /// Remove a peer that has left the LAN.
    ///
    /// Returns `true` if the peer was present and removed.
    pub fn remove_peer(&mut self, node_id: &NodeId) -> bool {
        self.discovered_peers.remove(node_id).is_some()
    }

    /// Returns the cluster ID this discovery manager is bound to.
    pub fn cluster_id(&self) -> &str {
        &self.cluster_id
    }

    /// Returns this node's ID.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the port this node is listening on.
    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    /// Returns the number of discovered peers.
    pub fn peer_count(&self) -> usize {
        self.discovered_peers.len()
    }

    /// Returns the LAN address for a specific peer, if discovered.
    pub fn peer_addr(&self, node_id: &NodeId) -> Option<&SocketAddr> {
        self.discovered_peers.get(node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_discovery() -> LanDiscovery {
        LanDiscovery::new("test-cluster", NodeId::generate(), 8080)
    }

    fn test_addr(port: u16) -> SocketAddr {
        format!("192.168.1.{}:{}", port % 254 + 1, port)
            .parse()
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_discovery_is_empty() {
        let disc = make_discovery();

        assert_eq!(disc.peer_count(), 0);
        assert!(disc.discovered_peers().is_empty());
        assert!(!disc.is_registered());
    }

    #[test]
    fn test_new_stores_params() {
        let node_id = NodeId::generate();
        let disc = LanDiscovery::new("my-cluster", node_id, 9090);

        assert_eq!(disc.cluster_id(), "my-cluster");
        assert_eq!(disc.node_id(), node_id);
        assert_eq!(disc.listen_port(), 9090);
    }

    // -----------------------------------------------------------------------
    // register_service
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_service_sets_flag() {
        let mut disc = make_discovery();

        disc.register_service().unwrap();
        assert!(disc.is_registered());
    }

    // -----------------------------------------------------------------------
    // add_discovered_peer
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_peer() {
        let mut disc = make_discovery();
        let peer = NodeId::generate();
        let addr = test_addr(8081);

        disc.add_discovered_peer(peer, addr);

        assert_eq!(disc.peer_count(), 1);
        assert_eq!(*disc.peer_addr(&peer).unwrap(), addr);
    }

    #[test]
    fn test_add_multiple_peers() {
        let mut disc = make_discovery();

        let peer_a = NodeId::generate();
        let peer_b = NodeId::generate();
        let peer_c = NodeId::generate();

        disc.add_discovered_peer(peer_a, test_addr(8081));
        disc.add_discovered_peer(peer_b, test_addr(8082));
        disc.add_discovered_peer(peer_c, test_addr(8083));

        assert_eq!(disc.peer_count(), 3);
        assert!(disc.discovered_peers().contains_key(&peer_a));
        assert!(disc.discovered_peers().contains_key(&peer_b));
        assert!(disc.discovered_peers().contains_key(&peer_c));
    }

    #[test]
    fn test_add_same_peer_updates_addr() {
        let mut disc = make_discovery();
        let peer = NodeId::generate();

        disc.add_discovered_peer(peer, test_addr(8081));
        disc.add_discovered_peer(peer, test_addr(9091));

        assert_eq!(disc.peer_count(), 1);
        assert_eq!(*disc.peer_addr(&peer).unwrap(), test_addr(9091));
    }

    // -----------------------------------------------------------------------
    // remove_peer
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_peer_returns_true() {
        let mut disc = make_discovery();
        let peer = NodeId::generate();

        disc.add_discovered_peer(peer, test_addr(8081));
        assert!(disc.remove_peer(&peer));
        assert_eq!(disc.peer_count(), 0);
    }

    #[test]
    fn test_remove_nonexistent_peer_returns_false() {
        let mut disc = make_discovery();
        let unknown = NodeId::generate();

        assert!(!disc.remove_peer(&unknown));
    }

    #[test]
    fn test_remove_peer_leaves_others() {
        let mut disc = make_discovery();
        let a = NodeId::generate();
        let b = NodeId::generate();

        disc.add_discovered_peer(a, test_addr(8081));
        disc.add_discovered_peer(b, test_addr(8082));

        disc.remove_peer(&a);

        assert_eq!(disc.peer_count(), 1);
        assert!(disc.peer_addr(&a).is_none());
        assert!(disc.peer_addr(&b).is_some());
    }

    // -----------------------------------------------------------------------
    // peer_addr
    // -----------------------------------------------------------------------

    #[test]
    fn test_peer_addr_returns_none_for_unknown() {
        let disc = make_discovery();
        assert!(disc.peer_addr(&NodeId::generate()).is_none());
    }

    // -----------------------------------------------------------------------
    // MDNS service name constant
    // -----------------------------------------------------------------------

    #[test]
    fn test_mdns_service_name() {
        assert_eq!(MDNS_SERVICE_NAME, "_nebula._tcp.local.");
    }
}
