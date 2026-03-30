use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use nebula_core::identity::node_id::NodeId;

/// A single route to a target node.
#[derive(Debug, Clone)]
pub struct Route {
    pub target: NodeId,
    pub method: RouteMethod,
    pub latency_ms: u32,
    pub last_probed: Instant,
}

/// The communication method used to reach a target node.
///
/// Routes are prioritized by type: LAN is preferred over hole-punching,
/// which is preferred over the tunnel relay fallback.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteMethod {
    /// Direct LAN connection via mDNS discovery (~1-5ms).
    LanDirect { addr: SocketAddr },
    /// NAT hole-punched P2P connection (~20-80ms).
    HolePunch { addr: SocketAddr },
    /// Relay through proxy server tunnel (~100-300ms, always works).
    TunnelRelay,
}

impl RouteMethod {
    /// Returns the priority of this route method.
    ///
    /// Lower values indicate higher priority (preferred routes).
    /// - `LanDirect` = 0 (highest)
    /// - `HolePunch` = 1
    /// - `TunnelRelay` = 2 (lowest, fallback)
    pub fn priority(&self) -> u8 {
        match self {
            RouteMethod::LanDirect { .. } => 0,
            RouteMethod::HolePunch { .. } => 1,
            RouteMethod::TunnelRelay => 2,
        }
    }
}

/// Tracks known routes to other nodes in the cluster.
///
/// Routes are maintained per target node and sorted by priority (method type)
/// then by latency. The table supports staleness detection and invalidation
/// for re-probing when network conditions change.
pub struct RoutingTable {
    routes: HashMap<NodeId, Vec<Route>>,
    stale_threshold: Duration,
}

impl RoutingTable {
    /// Create a new routing table with the given staleness threshold.
    ///
    /// Routes whose `last_probed` timestamp exceeds this threshold are
    /// considered stale and eligible for removal via `remove_stale_routes`.
    pub fn new(stale_threshold: Duration) -> Self {
        Self {
            routes: HashMap::new(),
            stale_threshold,
        }
    }

    /// Add or update a route to a target node.
    ///
    /// If a route with the same method already exists for this target, its
    /// latency and probe timestamp are updated. Otherwise, a new route entry
    /// is inserted. Routes are kept sorted by priority (method type) first,
    /// then by latency within the same priority tier.
    pub fn add_route(&mut self, target: NodeId, method: RouteMethod, latency_ms: u32) {
        let entries = self.routes.entry(target).or_default();

        // Check if a route with this method already exists
        if let Some(existing) = entries.iter_mut().find(|r| r.method == method) {
            existing.latency_ms = latency_ms;
            existing.last_probed = Instant::now();
        } else {
            entries.push(Route {
                target,
                method,
                latency_ms,
                last_probed: Instant::now(),
            });
        }

        // Sort by priority first, then by latency within the same priority
        entries.sort_by(|a, b| {
            a.method
                .priority()
                .cmp(&b.method.priority())
                .then_with(|| a.latency_ms.cmp(&b.latency_ms))
        });
    }

    /// Remove a specific route method for a target node.
    ///
    /// Returns `true` if a matching route was found and removed.
    pub fn remove_route(&mut self, target: &NodeId, method: &RouteMethod) -> bool {
        if let Some(entries) = self.routes.get_mut(target) {
            let before = entries.len();
            entries.retain(|r| r.method != *method);
            let removed = entries.len() < before;

            // Clean up empty entries
            if entries.is_empty() {
                self.routes.remove(target);
            }

            removed
        } else {
            false
        }
    }

    /// Returns the best (highest priority, lowest latency) route for a target.
    ///
    /// Since routes are maintained in sorted order, the best route is always
    /// at index 0 if any routes exist.
    pub fn best_route(&self, target: &NodeId) -> Option<&Route> {
        self.routes.get(target).and_then(|entries| entries.first())
    }

    /// Returns all known routes for a target node.
    ///
    /// The returned slice is sorted by priority then latency.
    /// Returns an empty slice if the target has no known routes.
    pub fn all_routes(&self, target: &NodeId) -> &[Route] {
        self.routes
            .get(target)
            .map(|entries| entries.as_slice())
            .unwrap_or(&[])
    }

    /// Mark all routes for a target as stale by resetting their probe timestamp.
    ///
    /// This forces them to be collected by the next `remove_stale_routes` call,
    /// triggering re-probing.
    pub fn invalidate_routes(&mut self, target: &NodeId) {
        if let Some(entries) = self.routes.get_mut(target) {
            // Set last_probed to a time that is guaranteed to be beyond the stale threshold.
            // We subtract stale_threshold + 1 second to ensure the route is stale.
            let stale_time = Instant::now()
                .checked_sub(self.stale_threshold + Duration::from_secs(1))
                .unwrap_or_else(Instant::now);

            for route in entries.iter_mut() {
                route.last_probed = stale_time;
            }
        }
    }

    /// Remove all routes whose `last_probed` timestamp is older than the
    /// configured stale threshold.
    ///
    /// Returns the number of routes removed.
    pub fn remove_stale_routes(&mut self) -> usize {
        let now = Instant::now();
        let threshold = self.stale_threshold;
        let mut removed_count = 0;

        self.routes.retain(|_, entries| {
            let before = entries.len();
            entries.retain(|r| now.duration_since(r.last_probed) <= threshold);
            removed_count += before - entries.len();
            !entries.is_empty()
        });

        removed_count
    }

    /// Returns `true` if at least one route exists for the target.
    pub fn has_route(&self, target: &NodeId) -> bool {
        self.routes
            .get(target)
            .map(|entries| !entries.is_empty())
            .unwrap_or(false)
    }

    /// Returns all node IDs that have at least one known route.
    pub fn known_targets(&self) -> Vec<NodeId> {
        self.routes.keys().copied().collect()
    }

    /// Update the measured latency for a specific route.
    ///
    /// Also refreshes the probe timestamp. Returns `true` if the route was
    /// found and updated. Re-sorts routes for the target after the update.
    pub fn update_latency(
        &mut self,
        target: &NodeId,
        method: &RouteMethod,
        latency_ms: u32,
    ) -> bool {
        if let Some(entries) = self.routes.get_mut(target) {
            if let Some(route) = entries.iter_mut().find(|r| r.method == *method) {
                route.latency_ms = latency_ms;
                route.last_probed = Instant::now();

                // Re-sort after latency change
                entries.sort_by(|a, b| {
                    a.method
                        .priority()
                        .cmp(&b.method.priority())
                        .then_with(|| a.latency_ms.cmp(&b.latency_ms))
                });

                return true;
            }
        }
        false
    }

    /// Returns the configured stale threshold.
    pub fn stale_threshold(&self) -> Duration {
        self.stale_threshold
    }

    /// Returns the total number of routes across all targets.
    pub fn route_count(&self) -> usize {
        self.routes.values().map(|v| v.len()).sum()
    }

    /// Returns the number of target nodes that have at least one route.
    pub fn target_count(&self) -> usize {
        self.routes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lan_route(port: u16) -> RouteMethod {
        RouteMethod::LanDirect {
            addr: format!("192.168.1.10:{port}").parse().unwrap(),
        }
    }

    fn punch_route(port: u16) -> RouteMethod {
        RouteMethod::HolePunch {
            addr: format!("203.0.113.5:{port}").parse().unwrap(),
        }
    }

    // -----------------------------------------------------------------------
    // RouteMethod priority tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_lan_direct_has_highest_priority() {
        let method = lan_route(8080);
        assert_eq!(method.priority(), 0);
    }

    #[test]
    fn test_hole_punch_has_middle_priority() {
        let method = punch_route(8080);
        assert_eq!(method.priority(), 1);
    }

    #[test]
    fn test_tunnel_relay_has_lowest_priority() {
        let method = RouteMethod::TunnelRelay;
        assert_eq!(method.priority(), 2);
    }

    #[test]
    fn test_priority_ordering_is_correct() {
        let lan = lan_route(8080);
        let punch = punch_route(8080);
        let relay = RouteMethod::TunnelRelay;

        assert!(lan.priority() < punch.priority());
        assert!(punch.priority() < relay.priority());
    }

    // -----------------------------------------------------------------------
    // RoutingTable construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_table_is_empty() {
        let table = RoutingTable::new(Duration::from_secs(60));

        assert_eq!(table.target_count(), 0);
        assert_eq!(table.route_count(), 0);
        assert_eq!(table.stale_threshold(), Duration::from_secs(60));
    }

    // -----------------------------------------------------------------------
    // add_route tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_single_route() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 3);

        assert!(table.has_route(&target));
        assert_eq!(table.target_count(), 1);
        assert_eq!(table.route_count(), 1);
    }

    #[test]
    fn test_add_multiple_routes_same_target() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 3);
        table.add_route(target, punch_route(9090), 50);
        table.add_route(target, RouteMethod::TunnelRelay, 150);

        assert_eq!(table.target_count(), 1);
        assert_eq!(table.route_count(), 3);

        let routes = table.all_routes(&target);
        assert_eq!(routes.len(), 3);
    }

    #[test]
    fn test_add_routes_for_different_targets() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target_a = NodeId::generate();
        let target_b = NodeId::generate();

        table.add_route(target_a, lan_route(8080), 2);
        table.add_route(target_b, RouteMethod::TunnelRelay, 200);

        assert_eq!(table.target_count(), 2);
        assert_eq!(table.route_count(), 2);
        assert!(table.has_route(&target_a));
        assert!(table.has_route(&target_b));
    }

    #[test]
    fn test_add_route_updates_existing_same_method() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();
        let method = lan_route(8080);

        table.add_route(target, method.clone(), 10);
        table.add_route(target, method.clone(), 5);

        // Should still be one route, not two
        assert_eq!(table.route_count(), 1);
        let best = table.best_route(&target).unwrap();
        assert_eq!(best.latency_ms, 5);
    }

    #[test]
    fn test_routes_sorted_by_priority_then_latency() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        // Add in reverse priority order
        table.add_route(target, RouteMethod::TunnelRelay, 150);
        table.add_route(target, punch_route(9090), 50);
        table.add_route(target, lan_route(8080), 3);

        let routes = table.all_routes(&target);
        assert_eq!(routes[0].method.priority(), 0); // LanDirect
        assert_eq!(routes[1].method.priority(), 1); // HolePunch
        assert_eq!(routes[2].method.priority(), 2); // TunnelRelay
    }

    #[test]
    fn test_routes_sorted_by_latency_within_same_priority() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        // Two LAN routes with different latencies
        let lan_slow = RouteMethod::LanDirect {
            addr: "192.168.1.10:8080".parse().unwrap(),
        };
        let lan_fast = RouteMethod::LanDirect {
            addr: "192.168.1.20:8080".parse().unwrap(),
        };

        table.add_route(target, lan_slow, 10);
        table.add_route(target, lan_fast, 2);

        let routes = table.all_routes(&target);
        assert_eq!(routes[0].latency_ms, 2); // faster first
        assert_eq!(routes[1].latency_ms, 10);
    }

    // -----------------------------------------------------------------------
    // best_route tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_best_route_returns_none_for_unknown_target() {
        let table = RoutingTable::new(Duration::from_secs(60));
        let unknown = NodeId::generate();

        assert!(table.best_route(&unknown).is_none());
    }

    #[test]
    fn test_best_route_prefers_lan_over_punch() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, punch_route(9090), 40);
        table.add_route(target, lan_route(8080), 5);

        let best = table.best_route(&target).unwrap();
        assert_eq!(best.method.priority(), 0); // LanDirect
    }

    #[test]
    fn test_best_route_prefers_punch_over_relay() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, RouteMethod::TunnelRelay, 200);
        table.add_route(target, punch_route(9090), 60);

        let best = table.best_route(&target).unwrap();
        assert_eq!(best.method.priority(), 1); // HolePunch
    }

    #[test]
    fn test_best_route_single_relay() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, RouteMethod::TunnelRelay, 250);

        let best = table.best_route(&target).unwrap();
        assert_eq!(best.method, RouteMethod::TunnelRelay);
        assert_eq!(best.latency_ms, 250);
    }

    // -----------------------------------------------------------------------
    // remove_route tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_route_returns_true_on_success() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();
        let method = lan_route(8080);

        table.add_route(target, method.clone(), 5);

        assert!(table.remove_route(&target, &method));
        assert!(!table.has_route(&target));
        assert_eq!(table.target_count(), 0);
    }

    #[test]
    fn test_remove_route_returns_false_for_unknown() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        assert!(!table.remove_route(&target, &RouteMethod::TunnelRelay));
    }

    #[test]
    fn test_remove_one_of_multiple_routes() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 3);
        table.add_route(target, RouteMethod::TunnelRelay, 200);

        table.remove_route(&target, &RouteMethod::TunnelRelay);

        assert!(table.has_route(&target));
        assert_eq!(table.route_count(), 1);
        assert_eq!(table.best_route(&target).unwrap().method.priority(), 0);
    }

    #[test]
    fn test_remove_nonexistent_method_returns_false() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 3);

        assert!(!table.remove_route(&target, &RouteMethod::TunnelRelay));
        assert_eq!(table.route_count(), 1);
    }

    // -----------------------------------------------------------------------
    // all_routes tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_routes_empty_for_unknown_target() {
        let table = RoutingTable::new(Duration::from_secs(60));
        let unknown = NodeId::generate();

        let routes = table.all_routes(&unknown);
        assert!(routes.is_empty());
    }

    #[test]
    fn test_all_routes_returns_sorted_entries() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, RouteMethod::TunnelRelay, 300);
        table.add_route(target, lan_route(8080), 2);
        table.add_route(target, punch_route(9090), 45);

        let routes = table.all_routes(&target);
        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].method.priority(), 0);
        assert_eq!(routes[1].method.priority(), 1);
        assert_eq!(routes[2].method.priority(), 2);
    }

    // -----------------------------------------------------------------------
    // invalidate_routes tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_invalidate_routes_makes_them_stale() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 3);
        table.add_route(target, RouteMethod::TunnelRelay, 200);

        table.invalidate_routes(&target);

        let removed = table.remove_stale_routes();
        assert_eq!(removed, 2);
        assert!(!table.has_route(&target));
    }

    #[test]
    fn test_invalidate_nonexistent_target_is_noop() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let unknown = NodeId::generate();

        // Should not panic
        table.invalidate_routes(&unknown);
        assert_eq!(table.target_count(), 0);
    }

    // -----------------------------------------------------------------------
    // remove_stale_routes tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_stale_routes_keeps_fresh() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 3);

        let removed = table.remove_stale_routes();
        assert_eq!(removed, 0);
        assert!(table.has_route(&target));
    }

    #[test]
    fn test_remove_stale_routes_cleans_up_entries() {
        let mut table = RoutingTable::new(Duration::from_secs(0));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 3);

        // With a zero threshold, the route is immediately stale
        // (Instant::now elapsed since add is > 0ns)
        std::thread::sleep(Duration::from_millis(1));
        let removed = table.remove_stale_routes();
        assert_eq!(removed, 1);
        assert!(!table.has_route(&target));
    }

    // -----------------------------------------------------------------------
    // has_route tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_has_route_false_for_unknown() {
        let table = RoutingTable::new(Duration::from_secs(60));
        assert!(!table.has_route(&NodeId::generate()));
    }

    #[test]
    fn test_has_route_true_after_add() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, RouteMethod::TunnelRelay, 200);
        assert!(table.has_route(&target));
    }

    // -----------------------------------------------------------------------
    // known_targets tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_known_targets_empty() {
        let table = RoutingTable::new(Duration::from_secs(60));
        assert!(table.known_targets().is_empty());
    }

    #[test]
    fn test_known_targets_returns_all() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let a = NodeId::generate();
        let b = NodeId::generate();
        let c = NodeId::generate();

        table.add_route(a, lan_route(8080), 3);
        table.add_route(b, punch_route(9090), 50);
        table.add_route(c, RouteMethod::TunnelRelay, 200);

        let targets = table.known_targets();
        assert_eq!(targets.len(), 3);
        assert!(targets.contains(&a));
        assert!(targets.contains(&b));
        assert!(targets.contains(&c));
    }

    // -----------------------------------------------------------------------
    // update_latency tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_latency_success() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();
        let method = lan_route(8080);

        table.add_route(target, method.clone(), 10);

        assert!(table.update_latency(&target, &method, 3));

        let best = table.best_route(&target).unwrap();
        assert_eq!(best.latency_ms, 3);
    }

    #[test]
    fn test_update_latency_returns_false_for_unknown_target() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let unknown = NodeId::generate();

        assert!(!table.update_latency(&unknown, &RouteMethod::TunnelRelay, 100));
    }

    #[test]
    fn test_update_latency_returns_false_for_wrong_method() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 5);

        assert!(!table.update_latency(&target, &RouteMethod::TunnelRelay, 200));
    }

    #[test]
    fn test_update_latency_re_sorts_routes() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        let lan_a = RouteMethod::LanDirect {
            addr: "192.168.1.10:8080".parse().unwrap(),
        };
        let lan_b = RouteMethod::LanDirect {
            addr: "192.168.1.20:8080".parse().unwrap(),
        };

        table.add_route(target, lan_a.clone(), 2); // faster initially
        table.add_route(target, lan_b.clone(), 8);

        // Now make lan_a slower
        table.update_latency(&target, &lan_a, 15);

        let routes = table.all_routes(&target);
        assert_eq!(routes[0].latency_ms, 8); // lan_b is now faster
        assert_eq!(routes[1].latency_ms, 15); // lan_a is now slower
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_last_route_cleans_up_target() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();
        let method = lan_route(8080);

        table.add_route(target, method.clone(), 5);
        table.remove_route(&target, &method);

        assert_eq!(table.target_count(), 0);
        assert!(table.known_targets().is_empty());
    }

    #[test]
    fn test_route_stores_correct_target_id() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();

        table.add_route(target, lan_route(8080), 5);

        let best = table.best_route(&target).unwrap();
        assert_eq!(best.target, target);
    }

    #[test]
    fn test_add_route_refreshes_probe_timestamp() {
        let mut table = RoutingTable::new(Duration::from_secs(60));
        let target = NodeId::generate();
        let method = lan_route(8080);

        table.add_route(target, method.clone(), 10);
        let first_probe = table.best_route(&target).unwrap().last_probed;

        std::thread::sleep(Duration::from_millis(2));

        table.add_route(target, method, 8);
        let second_probe = table.best_route(&target).unwrap().last_probed;

        assert!(second_probe >= first_probe);
    }
}
