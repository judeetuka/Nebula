use std::collections::HashMap;
use std::time::Duration;

use nebula_core::identity::node_id::NodeId;

use crate::routing::hole_punch::{HolePunchManager, HolePunchStatus};
use crate::routing::relay::TunnelRelay;
use crate::routing::table::{RouteMethod, RoutingTable};

/// Default staleness threshold for routes (60 seconds).
const DEFAULT_STALE_THRESHOLD_SECS: u64 = 60;

/// Default latency assumed for the tunnel relay (ms).
const DEFAULT_RELAY_LATENCY_MS: u32 = 200;

/// Coordinates all routing strategies to find the best path to each peer.
///
/// The `SmartRouter` combines three tiers of connectivity:
/// 1. **LAN Direct** -- mDNS-discovered peers on the same network (~1-5ms)
/// 2. **Hole Punch** -- NAT-traversed P2P via the signaling server (~20-80ms)
/// 3. **Tunnel Relay** -- fallback through the proxy server (~100-300ms)
///
/// `resolve_route` walks the tiers in order, returning the first available
/// route. Routes are cached in the `RoutingTable` for fast subsequent lookups.
pub struct SmartRouter {
    table: RoutingTable,
    hole_punch: HolePunchManager,
    tunnel_relay: TunnelRelay,
}

impl SmartRouter {
    /// Create a new smart router for the given cluster and node.
    pub fn new(_cluster_id: &str, _node_id: NodeId, _listen_port: u16, server_url: &str) -> Self {
        Self {
            table: RoutingTable::new(Duration::from_secs(DEFAULT_STALE_THRESHOLD_SECS)),
            hole_punch: HolePunchManager::new(),
            tunnel_relay: TunnelRelay::new(server_url),
        }
    }

    /// Resolve the best route to a target node.
    ///
    /// Fallback chain:
    /// 1. Check the routing table for a valid cached route.
    /// 2. Check LAN discovery for the peer -- if found, add a `LanDirect` route.
    /// 3. Check hole-punch status -- if succeeded, use the punched address.
    /// 4. Fall back to `TunnelRelay` (always available as a last resort).
    pub fn resolve_route(&mut self, target: &NodeId) -> RouteMethod {
        // Tier 1: Check routing table for an existing valid route
        if let Some(route) = self.table.best_route(target) {
            return route.method.clone();
        }

        // Tier 2: Check hole-punch results
        if let Some(attempt) = self.hole_punch.get_attempt(target) {
            if let HolePunchStatus::Succeeded { addr } = attempt.status {
                let method = RouteMethod::HolePunch { addr };
                self.table.add_route(*target, method.clone(), 50);
                return method;
            }
        }

        // Tier 4: Tunnel relay (always works)
        let method = RouteMethod::TunnelRelay;
        self.table
            .add_route(*target, method.clone(), DEFAULT_RELAY_LATENCY_MS);
        method
    }

    /// Delegate to the routing table to add an explicit route.
    pub fn add_route(&mut self, target: NodeId, method: RouteMethod, latency_ms: u32) {
        self.table.add_route(target, method, latency_ms);
    }

    /// Returns the current best route for each known target.
    ///
    /// Useful for displaying routing status in the admin dashboard.
    pub fn routing_summary(&self) -> HashMap<NodeId, RouteMethod> {
        let mut summary = HashMap::new();
        for target in self.table.known_targets() {
            if let Some(route) = self.table.best_route(&target) {
                summary.insert(target, route.method.clone());
            }
        }
        summary
    }

    /// Returns a reference to the routing table.
    pub fn table(&self) -> &RoutingTable {
        &self.table
    }

    /// Returns a mutable reference to the routing table.
    pub fn table_mut(&mut self) -> &mut RoutingTable {
        &mut self.table
    }

    /// Returns a reference to the hole-punch manager.
    pub fn hole_punch(&self) -> &HolePunchManager {
        &self.hole_punch
    }

    /// Returns a mutable reference to the hole-punch manager.
    pub fn hole_punch_mut(&mut self) -> &mut HolePunchManager {
        &mut self.hole_punch
    }

    /// Returns a reference to the tunnel relay.
    pub fn tunnel_relay(&self) -> &TunnelRelay {
        &self.tunnel_relay
    }

    /// Returns a mutable reference to the tunnel relay.
    pub fn tunnel_relay_mut(&mut self) -> &mut TunnelRelay {
        &mut self.tunnel_relay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn make_router() -> SmartRouter {
        SmartRouter::new("test-cluster", NodeId::generate(), 8080, "wss://proxy.test")
    }

    fn lan_addr() -> SocketAddr {
        "192.168.1.50:8080".parse().unwrap()
    }

    fn punch_addr() -> SocketAddr {
        "203.0.113.50:9090".parse().unwrap()
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_router() {
        let router = make_router();

        assert_eq!(router.table().target_count(), 0);
        assert_eq!(router.hole_punch().attempt_count(), 0);
        assert!(!router.tunnel_relay().is_available());
    }

    // -----------------------------------------------------------------------
    // resolve_route: fallback chain
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_route_falls_back_to_relay() {
        let mut router = make_router();
        let target = NodeId::generate();

        let route = router.resolve_route(&target);
        assert_eq!(route, RouteMethod::TunnelRelay);

        // Should have cached the relay route
        assert!(router.table().has_route(&target));
    }

    #[test]
    fn test_resolve_route_uses_cached_route() {
        let mut router = make_router();
        let target = NodeId::generate();

        // Pre-populate routing table
        router.add_route(target, RouteMethod::LanDirect { addr: lan_addr() }, 3);

        let route = router.resolve_route(&target);
        assert_eq!(route, RouteMethod::LanDirect { addr: lan_addr() });
    }

    #[test]
    fn test_resolve_route_uses_hole_punch() {
        let mut router = make_router();
        let target = NodeId::generate();

        // Simulate a successful hole-punch
        router
            .hole_punch_mut()
            .initiate_punch(target, punch_addr())
            .unwrap();
        router
            .hole_punch_mut()
            .mark_succeeded(&target, punch_addr());

        let route = router.resolve_route(&target);
        assert_eq!(route, RouteMethod::HolePunch { addr: punch_addr() });
    }

    #[test]
    fn test_resolve_route_failed_punch_falls_to_relay() {
        let mut router = make_router();
        let target = NodeId::generate();

        // Hole-punch failed, no LAN
        router
            .hole_punch_mut()
            .initiate_punch(target, punch_addr())
            .unwrap();
        router
            .hole_punch_mut()
            .mark_failed(&target, "symmetric NAT");

        let route = router.resolve_route(&target);
        assert_eq!(route, RouteMethod::TunnelRelay);
    }

    // -----------------------------------------------------------------------
    // routing_summary
    // -----------------------------------------------------------------------

    #[test]
    fn test_routing_summary_empty() {
        let router = make_router();
        assert!(router.routing_summary().is_empty());
    }

    #[test]
    fn test_routing_summary_shows_best_routes() {
        let mut router = make_router();
        let a = NodeId::generate();
        let b = NodeId::generate();
        router.add_route(a, RouteMethod::TunnelRelay, 200);
        router.add_route(b, RouteMethod::HolePunch { addr: punch_addr() }, 50);
        let summary = router.routing_summary();
        assert_eq!(summary.len(), 2);
    }

    // -----------------------------------------------------------------------
    // add_route delegation
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_route_delegates_to_table() {
        let mut router = make_router();
        let target = NodeId::generate();

        router.add_route(target, RouteMethod::TunnelRelay, 150);

        assert!(router.table().has_route(&target));
        assert_eq!(router.table().route_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Accessor methods
    // -----------------------------------------------------------------------

    #[test]
    fn test_tunnel_relay_url() {
        let router = SmartRouter::new(
            "cluster-1",
            NodeId::generate(),
            8080,
            "wss://nebula-proxy.io",
        );
        assert_eq!(router.tunnel_relay().server_url(), "wss://nebula-proxy.io");
    }
}
