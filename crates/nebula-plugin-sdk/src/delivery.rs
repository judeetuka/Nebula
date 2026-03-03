//! Delivery tracking for multipart messages.
//!
//! Tracks per-message delivery lifecycle including multipart SMS where a
//! single logical message may be split into multiple parts, each of which
//! receives independent sent/delivered confirmations.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Delivery status of a tracked message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryStatus {
    Submitted,
    PartiallySent { sent: u32, total: u32 },
    AllSent,
    PartiallyDelivered { delivered: u32, total: u32 },
    Delivered,
    Failed { reason: String },
    TimedOut,
}

/// A single tracked delivery entry.
#[derive(Debug)]
pub struct DeliveryEntry {
    pub message_id: String,
    pub status: DeliveryStatus,
    pub parts_total: u32,
    pub parts_sent: u32,
    pub parts_delivered: u32,
    pub submitted_at: Instant,
    pub sent_at: Option<Instant>,
    pub delivered_at: Option<Instant>,
}

/// Tracks delivery lifecycle for one or more messages.
pub struct DeliveryTracker {
    entries: HashMap<String, DeliveryEntry>,
    timeout: Duration,
}

impl DeliveryTracker {
    /// Create a new tracker with the given timeout for delivery confirmation.
    pub fn new(timeout: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            timeout,
        }
    }

    /// Register a new message for tracking.
    ///
    /// `parts` is the number of parts (e.g., 1 for a single SMS, 3 for a
    /// multipart SMS that was split into 3 segments).
    pub fn register(&mut self, message_id: &str, parts: u32) {
        let entry = DeliveryEntry {
            message_id: message_id.to_string(),
            status: DeliveryStatus::Submitted,
            parts_total: parts,
            parts_sent: 0,
            parts_delivered: 0,
            submitted_at: Instant::now(),
            sent_at: None,
            delivered_at: None,
        };
        self.entries.insert(message_id.to_string(), entry);
    }

    /// Record that one part of the message was sent.
    pub fn mark_part_sent(&mut self, message_id: &str) {
        if let Some(entry) = self.entries.get_mut(message_id) {
            // Do not modify terminal states.
            if matches!(
                entry.status,
                DeliveryStatus::Failed { .. } | DeliveryStatus::TimedOut
            ) {
                return;
            }

            entry.parts_sent += 1;
            if entry.parts_sent >= entry.parts_total {
                entry.status = DeliveryStatus::AllSent;
                entry.sent_at = Some(Instant::now());
            } else {
                entry.status = DeliveryStatus::PartiallySent {
                    sent: entry.parts_sent,
                    total: entry.parts_total,
                };
            }
        }
    }

    /// Record that one part of the message was delivered.
    pub fn mark_part_delivered(&mut self, message_id: &str) {
        if let Some(entry) = self.entries.get_mut(message_id) {
            if matches!(
                entry.status,
                DeliveryStatus::Failed { .. } | DeliveryStatus::TimedOut
            ) {
                return;
            }

            entry.parts_delivered += 1;
            if entry.parts_delivered >= entry.parts_total {
                entry.status = DeliveryStatus::Delivered;
                entry.delivered_at = Some(Instant::now());
            } else {
                entry.status = DeliveryStatus::PartiallyDelivered {
                    delivered: entry.parts_delivered,
                    total: entry.parts_total,
                };
            }
        }
    }

    /// Mark a message as failed with a reason.
    pub fn mark_failed(&mut self, message_id: &str, reason: &str) {
        if let Some(entry) = self.entries.get_mut(message_id) {
            entry.status = DeliveryStatus::Failed {
                reason: reason.to_string(),
            };
        }
    }

    /// Check all entries for timeouts. Returns the IDs of messages that have
    /// timed out and transitions their status to `TimedOut`.
    pub fn check_timeouts(&mut self) -> Vec<String> {
        let mut timed_out = Vec::new();
        for entry in self.entries.values_mut() {
            // Only timeout non-terminal entries.
            if matches!(
                entry.status,
                DeliveryStatus::Delivered
                    | DeliveryStatus::Failed { .. }
                    | DeliveryStatus::TimedOut
            ) {
                continue;
            }

            if entry.submitted_at.elapsed() >= self.timeout {
                entry.status = DeliveryStatus::TimedOut;
                timed_out.push(entry.message_id.clone());
            }
        }
        timed_out
    }

    /// Get the current delivery status of a message.
    pub fn get_status(&self, message_id: &str) -> Option<&DeliveryStatus> {
        self.entries.get(message_id).map(|e| &e.status)
    }

    /// Returns `true` if the message has reached a terminal state:
    /// `Delivered`, `Failed`, or `TimedOut`.
    pub fn is_complete(&self, message_id: &str) -> bool {
        self.entries
            .get(message_id)
            .map(|e| {
                matches!(
                    e.status,
                    DeliveryStatus::Delivered
                        | DeliveryStatus::Failed { .. }
                        | DeliveryStatus::TimedOut
                )
            })
            .unwrap_or(false)
    }

    /// Remove a message from tracking entirely.
    pub fn remove(&mut self, message_id: &str) {
        self.entries.remove(message_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_sets_submitted() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 1);
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::Submitted)
        );
    }

    #[test]
    fn test_single_part_sent_to_all_sent() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 1);
        tracker.mark_part_sent("msg1");
        assert_eq!(tracker.get_status("msg1"), Some(&DeliveryStatus::AllSent));
    }

    #[test]
    fn test_single_part_delivered() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 1);
        tracker.mark_part_sent("msg1");
        tracker.mark_part_delivered("msg1");
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::Delivered)
        );
    }

    #[test]
    fn test_multipart_sent_progression() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 3);

        tracker.mark_part_sent("msg1");
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::PartiallySent {
                sent: 1,
                total: 3
            })
        );

        tracker.mark_part_sent("msg1");
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::PartiallySent {
                sent: 2,
                total: 3
            })
        );

        tracker.mark_part_sent("msg1");
        assert_eq!(tracker.get_status("msg1"), Some(&DeliveryStatus::AllSent));
    }

    #[test]
    fn test_multipart_delivered_progression() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 2);

        tracker.mark_part_sent("msg1");
        tracker.mark_part_sent("msg1");

        tracker.mark_part_delivered("msg1");
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::PartiallyDelivered {
                delivered: 1,
                total: 2
            })
        );

        tracker.mark_part_delivered("msg1");
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::Delivered)
        );
    }

    #[test]
    fn test_mark_failed() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 1);
        tracker.mark_failed("msg1", "network error");
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::Failed {
                reason: "network error".to_string()
            })
        );
    }

    #[test]
    fn test_failed_ignores_further_sent_marks() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 2);
        tracker.mark_failed("msg1", "error");

        // Further marks should be ignored.
        tracker.mark_part_sent("msg1");
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::Failed {
                reason: "error".to_string()
            })
        );
    }

    #[test]
    fn test_timeout_detection() {
        let mut tracker = DeliveryTracker::new(Duration::from_millis(10));
        tracker.register("msg1", 1);
        std::thread::sleep(Duration::from_millis(15));

        let timed_out = tracker.check_timeouts();
        assert_eq!(timed_out, vec!["msg1"]);
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::TimedOut)
        );
    }

    #[test]
    fn test_timeout_does_not_affect_delivered() {
        let mut tracker = DeliveryTracker::new(Duration::from_millis(10));
        tracker.register("msg1", 1);
        tracker.mark_part_sent("msg1");
        tracker.mark_part_delivered("msg1");
        std::thread::sleep(Duration::from_millis(15));

        let timed_out = tracker.check_timeouts();
        assert!(timed_out.is_empty());
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::Delivered)
        );
    }

    #[test]
    fn test_is_complete() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 1);
        assert!(!tracker.is_complete("msg1"));

        tracker.mark_part_sent("msg1");
        assert!(!tracker.is_complete("msg1"));

        tracker.mark_part_delivered("msg1");
        assert!(tracker.is_complete("msg1"));
    }

    #[test]
    fn test_is_complete_for_failed() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 1);
        tracker.mark_failed("msg1", "error");
        assert!(tracker.is_complete("msg1"));
    }

    #[test]
    fn test_is_complete_for_timed_out() {
        let mut tracker = DeliveryTracker::new(Duration::from_millis(1));
        tracker.register("msg1", 1);
        std::thread::sleep(Duration::from_millis(5));
        tracker.check_timeouts();
        assert!(tracker.is_complete("msg1"));
    }

    #[test]
    fn test_is_complete_nonexistent() {
        let tracker = DeliveryTracker::new(Duration::from_secs(300));
        assert!(!tracker.is_complete("ghost"));
    }

    #[test]
    fn test_remove() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 1);
        tracker.remove("msg1");
        assert!(tracker.get_status("msg1").is_none());
    }

    #[test]
    fn test_remove_nonexistent_is_noop() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        // Should not panic.
        tracker.remove("ghost");
    }

    #[test]
    fn test_get_status_nonexistent() {
        let tracker = DeliveryTracker::new(Duration::from_secs(300));
        assert!(tracker.get_status("ghost").is_none());
    }

    #[test]
    fn test_multipart_completion_sets_timestamps() {
        let mut tracker = DeliveryTracker::new(Duration::from_secs(300));
        tracker.register("msg1", 2);

        tracker.mark_part_sent("msg1");
        tracker.mark_part_sent("msg1");

        // sent_at should now be set.
        let entry = tracker.entries.get("msg1").unwrap();
        assert!(entry.sent_at.is_some());

        tracker.mark_part_delivered("msg1");
        tracker.mark_part_delivered("msg1");

        let entry = tracker.entries.get("msg1").unwrap();
        assert!(entry.delivered_at.is_some());
    }

    #[test]
    fn test_timed_out_ignores_further_marks() {
        let mut tracker = DeliveryTracker::new(Duration::from_millis(1));
        tracker.register("msg1", 2);
        std::thread::sleep(Duration::from_millis(5));
        tracker.check_timeouts();

        tracker.mark_part_sent("msg1");
        assert_eq!(
            tracker.get_status("msg1"),
            Some(&DeliveryStatus::TimedOut)
        );
    }
}
