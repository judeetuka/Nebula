use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use nebula_core::identity::node_id::NodeId;

/// Tracks the lifecycle of a single NAT hole-punch attempt.
pub struct HolePunchAttempt {
    pub target: NodeId,
    pub target_external_addr: Option<SocketAddr>,
    pub started_at: Instant,
    pub status: HolePunchStatus,
}

/// The current status of a hole-punch attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum HolePunchStatus {
    /// Waiting for the target's external address from the signaling server.
    Pending,
    /// Actively sending probes to the target's external address.
    Punching,
    /// Connection established through the punched hole.
    Succeeded { addr: SocketAddr },
    /// The attempt failed.
    Failed { reason: String },
}

/// Manages STUN-like NAT hole-punching via the proxy server.
///
/// The proxy server acts as the signaling server, exchanging external
/// addresses between peers so they can attempt direct P2P connections
/// through their NAT gateways.
pub struct HolePunchManager {
    /// Our external address as seen by the proxy server.
    external_addr: Option<SocketAddr>,
    /// Pending and completed hole-punch attempts, keyed by target node.
    pending: HashMap<NodeId, HolePunchAttempt>,
}

impl HolePunchManager {
    /// Create a new hole-punch manager with no known external address.
    pub fn new() -> Self {
        Self {
            external_addr: None,
            pending: HashMap::new(),
        }
    }

    /// Set this node's external address as observed by the proxy server.
    ///
    /// This address is shared with peers during the signaling phase so
    /// they know where to send their hole-punch probes.
    pub fn set_external_addr(&mut self, addr: SocketAddr) {
        self.external_addr = Some(addr);
    }

    /// Returns this node's external address, if known.
    pub fn external_addr(&self) -> Option<SocketAddr> {
        self.external_addr
    }

    /// Initiate a hole-punch attempt with a target node.
    ///
    /// The target's external address (learned from the signaling server)
    /// is used to direct the probe packets. Returns an error if a
    /// punch to this target is already in progress.
    pub fn initiate_punch(
        &mut self,
        target: NodeId,
        target_external_addr: SocketAddr,
    ) -> Result<(), String> {
        if self.pending.contains_key(&target) {
            return Err(format!(
                "Hole-punch already in progress for target {}",
                target
            ));
        }

        self.pending.insert(
            target,
            HolePunchAttempt {
                target,
                target_external_addr: Some(target_external_addr),
                started_at: Instant::now(),
                status: HolePunchStatus::Punching,
            },
        );

        Ok(())
    }

    /// Returns the attempt state for a target, if one exists.
    pub fn get_attempt(&self, target: &NodeId) -> Option<&HolePunchAttempt> {
        self.pending.get(target)
    }

    /// Mark a hole-punch attempt as succeeded.
    ///
    /// The `addr` is the confirmed direct address through the punched NAT hole.
    /// Returns `true` if the attempt existed and was updated.
    pub fn mark_succeeded(&mut self, target: &NodeId, addr: SocketAddr) -> bool {
        if let Some(attempt) = self.pending.get_mut(target) {
            attempt.status = HolePunchStatus::Succeeded { addr };
            true
        } else {
            false
        }
    }

    /// Mark a hole-punch attempt as failed.
    ///
    /// Returns `true` if the attempt existed and was updated.
    pub fn mark_failed(&mut self, target: &NodeId, reason: &str) -> bool {
        if let Some(attempt) = self.pending.get_mut(target) {
            attempt.status = HolePunchStatus::Failed {
                reason: reason.to_string(),
            };
            true
        } else {
            false
        }
    }

    /// Remove hole-punch attempts that have been pending longer than the timeout.
    ///
    /// Only removes attempts in `Pending` or `Punching` status. Succeeded and
    /// failed attempts are left for the caller to inspect and clean up.
    /// Returns the number of attempts removed.
    pub fn cleanup_stale(&mut self, timeout: Duration) -> usize {
        let now = Instant::now();
        let mut removed = 0;

        self.pending.retain(|_, attempt| {
            let is_active = matches!(
                attempt.status,
                HolePunchStatus::Pending | HolePunchStatus::Punching
            );
            let is_stale = is_active && now.duration_since(attempt.started_at) > timeout;

            if is_stale {
                removed += 1;
                false
            } else {
                true
            }
        });

        removed
    }

    /// Returns the number of tracked attempts (all statuses).
    pub fn attempt_count(&self) -> usize {
        self.pending.len()
    }

    /// Remove a completed attempt (succeeded or failed) from tracking.
    ///
    /// Returns `true` if the attempt existed and was removed.
    pub fn remove_attempt(&mut self, target: &NodeId) -> bool {
        self.pending.remove(target).is_some()
    }
}

impl Default for HolePunchManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn external_addr(port: u16) -> SocketAddr {
        format!("203.0.113.1:{port}").parse().unwrap()
    }

    fn target_addr(port: u16) -> SocketAddr {
        format!("198.51.100.1:{port}").parse().unwrap()
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_manager_is_empty() {
        let mgr = HolePunchManager::new();

        assert!(mgr.external_addr().is_none());
        assert_eq!(mgr.attempt_count(), 0);
    }

    #[test]
    fn test_default_is_same_as_new() {
        let mgr = HolePunchManager::default();

        assert!(mgr.external_addr().is_none());
        assert_eq!(mgr.attempt_count(), 0);
    }

    // -----------------------------------------------------------------------
    // set_external_addr
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_addr() {
        let mut mgr = HolePunchManager::new();
        let addr = external_addr(5000);

        mgr.set_external_addr(addr);
        assert_eq!(mgr.external_addr(), Some(addr));
    }

    #[test]
    fn test_set_external_addr_overwrites() {
        let mut mgr = HolePunchManager::new();

        mgr.set_external_addr(external_addr(5000));
        mgr.set_external_addr(external_addr(6000));

        assert_eq!(mgr.external_addr(), Some(external_addr(6000)));
    }

    // -----------------------------------------------------------------------
    // initiate_punch
    // -----------------------------------------------------------------------

    #[test]
    fn test_initiate_punch_success() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();

        mgr.initiate_punch(target, target_addr(7000)).unwrap();

        assert_eq!(mgr.attempt_count(), 1);
        let attempt = mgr.get_attempt(&target).unwrap();
        assert_eq!(attempt.target, target);
        assert_eq!(attempt.target_external_addr, Some(target_addr(7000)));
        assert_eq!(attempt.status, HolePunchStatus::Punching);
    }

    #[test]
    fn test_initiate_punch_duplicate_fails() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();

        mgr.initiate_punch(target, target_addr(7000)).unwrap();

        let result = mgr.initiate_punch(target, target_addr(7001));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already in progress"));
    }

    #[test]
    fn test_initiate_multiple_targets() {
        let mut mgr = HolePunchManager::new();
        let a = NodeId::generate();
        let b = NodeId::generate();

        mgr.initiate_punch(a, target_addr(7000)).unwrap();
        mgr.initiate_punch(b, target_addr(7001)).unwrap();

        assert_eq!(mgr.attempt_count(), 2);
    }

    // -----------------------------------------------------------------------
    // get_attempt
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_attempt_returns_none_for_unknown() {
        let mgr = HolePunchManager::new();
        assert!(mgr.get_attempt(&NodeId::generate()).is_none());
    }

    // -----------------------------------------------------------------------
    // mark_succeeded
    // -----------------------------------------------------------------------

    #[test]
    fn test_mark_succeeded() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();
        let punched_addr = target_addr(7777);

        mgr.initiate_punch(target, target_addr(7000)).unwrap();

        assert!(mgr.mark_succeeded(&target, punched_addr));

        let attempt = mgr.get_attempt(&target).unwrap();
        assert_eq!(
            attempt.status,
            HolePunchStatus::Succeeded { addr: punched_addr }
        );
    }

    #[test]
    fn test_mark_succeeded_unknown_returns_false() {
        let mut mgr = HolePunchManager::new();
        assert!(!mgr.mark_succeeded(&NodeId::generate(), target_addr(7000)));
    }

    // -----------------------------------------------------------------------
    // mark_failed
    // -----------------------------------------------------------------------

    #[test]
    fn test_mark_failed() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();

        mgr.initiate_punch(target, target_addr(7000)).unwrap();

        assert!(mgr.mark_failed(&target, "timeout"));

        let attempt = mgr.get_attempt(&target).unwrap();
        assert_eq!(
            attempt.status,
            HolePunchStatus::Failed {
                reason: "timeout".to_string()
            }
        );
    }

    #[test]
    fn test_mark_failed_unknown_returns_false() {
        let mut mgr = HolePunchManager::new();
        assert!(!mgr.mark_failed(&NodeId::generate(), "timeout"));
    }

    // -----------------------------------------------------------------------
    // cleanup_stale
    // -----------------------------------------------------------------------

    #[test]
    fn test_cleanup_stale_removes_old_active_attempts() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();

        mgr.initiate_punch(target, target_addr(7000)).unwrap();

        // With a zero timeout, the attempt is immediately stale
        std::thread::sleep(Duration::from_millis(2));
        let removed = mgr.cleanup_stale(Duration::from_secs(0));

        assert_eq!(removed, 1);
        assert_eq!(mgr.attempt_count(), 0);
    }

    #[test]
    fn test_cleanup_stale_keeps_fresh_attempts() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();

        mgr.initiate_punch(target, target_addr(7000)).unwrap();

        let removed = mgr.cleanup_stale(Duration::from_secs(60));
        assert_eq!(removed, 0);
        assert_eq!(mgr.attempt_count(), 1);
    }

    #[test]
    fn test_cleanup_stale_preserves_succeeded() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();

        mgr.initiate_punch(target, target_addr(7000)).unwrap();
        mgr.mark_succeeded(&target, target_addr(7777));

        std::thread::sleep(Duration::from_millis(2));
        let removed = mgr.cleanup_stale(Duration::from_secs(0));

        // Succeeded attempts are not removed by cleanup
        assert_eq!(removed, 0);
        assert_eq!(mgr.attempt_count(), 1);
    }

    #[test]
    fn test_cleanup_stale_preserves_failed() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();

        mgr.initiate_punch(target, target_addr(7000)).unwrap();
        mgr.mark_failed(&target, "NAT symmetric");

        std::thread::sleep(Duration::from_millis(2));
        let removed = mgr.cleanup_stale(Duration::from_secs(0));

        // Failed attempts are not removed by cleanup
        assert_eq!(removed, 0);
        assert_eq!(mgr.attempt_count(), 1);
    }

    // -----------------------------------------------------------------------
    // remove_attempt
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_attempt() {
        let mut mgr = HolePunchManager::new();
        let target = NodeId::generate();

        mgr.initiate_punch(target, target_addr(7000)).unwrap();
        assert!(mgr.remove_attempt(&target));
        assert_eq!(mgr.attempt_count(), 0);
    }

    #[test]
    fn test_remove_attempt_unknown_returns_false() {
        let mut mgr = HolePunchManager::new();
        assert!(!mgr.remove_attempt(&NodeId::generate()));
    }

    // -----------------------------------------------------------------------
    // HolePunchStatus equality
    // -----------------------------------------------------------------------

    #[test]
    fn test_status_pending_eq() {
        assert_eq!(HolePunchStatus::Pending, HolePunchStatus::Pending);
    }

    #[test]
    fn test_status_punching_eq() {
        assert_eq!(HolePunchStatus::Punching, HolePunchStatus::Punching);
    }

    #[test]
    fn test_status_succeeded_eq() {
        let addr = target_addr(7777);
        assert_eq!(
            HolePunchStatus::Succeeded { addr },
            HolePunchStatus::Succeeded { addr }
        );
    }

    #[test]
    fn test_status_failed_eq() {
        assert_eq!(
            HolePunchStatus::Failed {
                reason: "x".to_string()
            },
            HolePunchStatus::Failed {
                reason: "x".to_string()
            }
        );
    }

    #[test]
    fn test_status_different_variants_not_eq() {
        assert_ne!(HolePunchStatus::Pending, HolePunchStatus::Punching);
    }
}
