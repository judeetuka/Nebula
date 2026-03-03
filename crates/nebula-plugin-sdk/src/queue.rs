//! Priority message queue for plugin orchestration.
//!
//! Provides a bounded, priority-ordered message queue with status lifecycle
//! tracking. Messages are ordered by priority (High > Normal > Low), then
//! by creation time (oldest first within the same priority).

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

/// Priority level for queued messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessagePriority {
    High,
    Normal,
    Low,
}

impl MessagePriority {
    fn ordinal(&self) -> u8 {
        match self {
            MessagePriority::High => 2,
            MessagePriority::Normal => 1,
            MessagePriority::Low => 0,
        }
    }
}

/// Lifecycle status of a queued message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageStatus {
    Pending,
    Sending,
    Sent,
    Delivered,
    Failed { reason: String, retry_count: u32 },
    Cancelled,
    Expired,
}

impl MessageStatus {
    /// Returns true if the status is terminal (no further transitions expected).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            MessageStatus::Delivered
                | MessageStatus::Failed { .. }
                | MessageStatus::Cancelled
                | MessageStatus::Expired
        )
    }
}

/// A message in the queue with metadata for scheduling, retry, and expiration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub id: String,
    pub payload: String,
    pub priority: MessagePriority,
    pub status: MessageStatus,
    /// Key used for per-target rate limiting (e.g., SIM slot ID).
    pub target_key: String,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Scheduled send time in epoch milliseconds. `None` means immediate.
    pub send_at_ms: Option<i64>,
    /// Expiration time in epoch milliseconds. `None` means never.
    pub expires_at_ms: Option<i64>,
}

/// Wrapper for `BinaryHeap` ordering: higher priority first, then older first.
#[derive(Debug, Clone)]
struct PrioritizedMessage {
    id: String,
    priority: MessagePriority,
    created_at_ms: i64,
}

impl PartialEq for PrioritizedMessage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for PrioritizedMessage {}

impl PartialOrd for PrioritizedMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority ordinal wins.
        // Within the same priority, older (smaller timestamp) wins.
        self.priority
            .ordinal()
            .cmp(&other.priority.ordinal())
            .then_with(|| other.created_at_ms.cmp(&self.created_at_ms))
    }
}

/// Aggregate counts by message status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueStats {
    pub pending: usize,
    pub sending: usize,
    pub sent: usize,
    pub delivered: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub expired: usize,
}

/// A bounded, priority-ordered message queue with full lifecycle tracking.
pub struct MessageQueue {
    heap: BinaryHeap<PrioritizedMessage>,
    by_id: HashMap<String, QueuedMessage>,
    max_size: usize,
}

impl MessageQueue {
    /// Create a new queue with the given maximum capacity.
    pub fn new(max_size: usize) -> Self {
        Self {
            heap: BinaryHeap::new(),
            by_id: HashMap::new(),
            max_size,
        }
    }

    /// Submit a message to the queue.
    ///
    /// Returns `Err` if the queue is at capacity or the message ID already exists.
    pub fn submit(&mut self, message: QueuedMessage) -> Result<(), String> {
        if self.by_id.len() >= self.max_size {
            return Err("queue is at maximum capacity".to_string());
        }
        if self.by_id.contains_key(&message.id) {
            return Err(format!("duplicate message id: {}", message.id));
        }

        let entry = PrioritizedMessage {
            id: message.id.clone(),
            priority: message.priority.clone(),
            created_at_ms: message.created_at_ms,
        };
        self.heap.push(entry);
        self.by_id.insert(message.id.clone(), message);
        Ok(())
    }

    /// Get the next message ready to send.
    ///
    /// A message is ready if its status is `Pending` and its `send_at_ms` is
    /// `None` or at most `now_ms`. Returns a clone of the message without
    /// removing it from the queue.
    pub fn next_ready(&mut self, now_ms: i64) -> Option<QueuedMessage> {
        let mut found: Option<QueuedMessage> = None;
        let mut remaining = Vec::with_capacity(self.heap.len());

        while let Some(entry) = self.heap.pop() {
            if found.is_some() {
                remaining.push(entry);
                continue;
            }

            if let Some(msg) = self.by_id.get(&entry.id) {
                let is_pending = msg.status == MessageStatus::Pending;
                let is_scheduled_ready = match msg.send_at_ms {
                    Some(t) => t <= now_ms,
                    None => true,
                };
                if is_pending && is_scheduled_ready {
                    found = Some(msg.clone());
                }
            }
            remaining.push(entry);
        }

        for entry in remaining {
            self.heap.push(entry);
        }

        found
    }

    /// Transition a message from `Pending` to `Sending`.
    pub fn mark_sending(&mut self, id: &str) {
        if let Some(msg) = self.by_id.get_mut(id) {
            if msg.status == MessageStatus::Pending {
                msg.status = MessageStatus::Sending;
            }
        }
    }

    /// Transition a message from `Sending` to `Sent`.
    pub fn mark_sent(&mut self, id: &str) {
        if let Some(msg) = self.by_id.get_mut(id) {
            if msg.status == MessageStatus::Sending {
                msg.status = MessageStatus::Sent;
            }
        }
    }

    /// Transition a message from `Sent` to `Delivered`.
    pub fn mark_delivered(&mut self, id: &str) {
        if let Some(msg) = self.by_id.get_mut(id) {
            if msg.status == MessageStatus::Sent {
                msg.status = MessageStatus::Delivered;
            }
        }
    }

    /// Mark a message as failed. Increments `retry_count`; if below
    /// `max_retries`, resets to `Pending` for automatic retry.
    pub fn mark_failed(&mut self, id: &str, reason: &str) {
        if let Some(msg) = self.by_id.get_mut(id) {
            msg.retry_count += 1;
            if msg.retry_count < msg.max_retries {
                msg.status = MessageStatus::Pending;
            } else {
                msg.status = MessageStatus::Failed {
                    reason: reason.to_string(),
                    retry_count: msg.retry_count,
                };
            }
        }
    }

    /// Cancel a message. Only succeeds if the message is currently `Pending`.
    /// Returns `true` if the message was cancelled.
    pub fn mark_cancelled(&mut self, id: &str) -> bool {
        if let Some(msg) = self.by_id.get_mut(id) {
            if msg.status == MessageStatus::Pending {
                msg.status = MessageStatus::Cancelled;
                return true;
            }
        }
        false
    }

    /// Expire all `Pending` messages whose `created_at_ms` is older than
    /// `now_ms - max_age_ms`.
    pub fn expire_stale(&mut self, now_ms: i64, max_age_ms: i64) {
        let cutoff = now_ms - max_age_ms;
        for msg in self.by_id.values_mut() {
            if msg.status == MessageStatus::Pending && msg.created_at_ms < cutoff {
                msg.status = MessageStatus::Expired;
            }
        }
    }

    /// Recover messages stuck in `Sending` for longer than `stale_threshold_ms`.
    ///
    /// If a stuck message still has retries left, it goes back to `Pending`.
    /// Otherwise it transitions to `Failed`.
    pub fn recover_stale_sending(&mut self, now_ms: i64, stale_threshold_ms: i64) {
        let cutoff = now_ms - stale_threshold_ms;
        for msg in self.by_id.values_mut() {
            if msg.status == MessageStatus::Sending && msg.updated_at_ms < cutoff {
                msg.retry_count += 1;
                if msg.retry_count < msg.max_retries {
                    msg.status = MessageStatus::Pending;
                } else {
                    msg.status = MessageStatus::Failed {
                        reason: "stale sending recovery: retries exhausted".to_string(),
                        retry_count: msg.retry_count,
                    };
                }
            }
        }
    }

    /// Get the current status of a message.
    pub fn get_status(&self, id: &str) -> Option<&MessageStatus> {
        self.by_id.get(id).map(|m| &m.status)
    }

    /// Count of messages currently in `Pending` status.
    pub fn pending_count(&self) -> usize {
        self.by_id
            .values()
            .filter(|m| m.status == MessageStatus::Pending)
            .count()
    }

    /// Total number of non-terminal messages in the queue.
    pub fn queue_depth(&self) -> usize {
        self.by_id
            .values()
            .filter(|m| !m.status.is_terminal())
            .count()
    }

    /// Aggregate counts by status.
    pub fn stats(&self) -> QueueStats {
        let mut stats = QueueStats {
            pending: 0,
            sending: 0,
            sent: 0,
            delivered: 0,
            failed: 0,
            cancelled: 0,
            expired: 0,
        };
        for msg in self.by_id.values() {
            match &msg.status {
                MessageStatus::Pending => stats.pending += 1,
                MessageStatus::Sending => stats.sending += 1,
                MessageStatus::Sent => stats.sent += 1,
                MessageStatus::Delivered => stats.delivered += 1,
                MessageStatus::Failed { .. } => stats.failed += 1,
                MessageStatus::Cancelled => stats.cancelled += 1,
                MessageStatus::Expired => stats.expired += 1,
            }
        }
        stats
    }

    /// Remove and return all messages in a terminal state
    /// (`Delivered`, `Failed`, `Cancelled`, `Expired`).
    pub fn drain_delivered(&mut self) -> Vec<QueuedMessage> {
        let terminal_ids: Vec<String> = self
            .by_id
            .iter()
            .filter(|(_, m)| m.status.is_terminal())
            .map(|(id, _)| id.clone())
            .collect();

        let mut drained = Vec::with_capacity(terminal_ids.len());
        for id in &terminal_ids {
            if let Some(msg) = self.by_id.remove(id) {
                drained.push(msg);
            }
        }

        // Rebuild the heap without the drained entries.
        let old_heap: Vec<PrioritizedMessage> = std::mem::take(&mut self.heap).into_vec();
        for entry in old_heap {
            if self.by_id.contains_key(&entry.id) {
                self.heap.push(entry);
            }
        }

        drained
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str, priority: MessagePriority, created_at_ms: i64) -> QueuedMessage {
        QueuedMessage {
            id: id.to_string(),
            payload: format!(r#"{{"data":"{}"}}"#, id),
            priority,
            status: MessageStatus::Pending,
            target_key: "sim0".to_string(),
            retry_count: 0,
            max_retries: 3,
            created_at_ms,
            updated_at_ms: created_at_ms,
            send_at_ms: None,
            expires_at_ms: None,
        }
    }

    #[test]
    fn test_submit_and_pending_count() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("m1", MessagePriority::Normal, 1000))
            .unwrap();
        q.submit(make_msg("m2", MessagePriority::Normal, 2000))
            .unwrap();
        assert_eq!(q.pending_count(), 2);
    }

    #[test]
    fn test_submit_rejects_when_full() {
        let mut q = MessageQueue::new(1);
        q.submit(make_msg("m1", MessagePriority::Normal, 1000))
            .unwrap();
        let result = q.submit(make_msg("m2", MessagePriority::Normal, 2000));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("maximum capacity"));
    }

    #[test]
    fn test_submit_rejects_duplicate_id() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("m1", MessagePriority::Normal, 1000))
            .unwrap();
        let result = q.submit(make_msg("m1", MessagePriority::High, 2000));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("duplicate"));
    }

    #[test]
    fn test_priority_ordering_high_before_normal_before_low() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("low", MessagePriority::Low, 1000))
            .unwrap();
        q.submit(make_msg("normal", MessagePriority::Normal, 1000))
            .unwrap();
        q.submit(make_msg("high", MessagePriority::High, 1000))
            .unwrap();

        let now = 5000;
        let first = q.next_ready(now).unwrap();
        assert_eq!(first.id, "high");

        q.mark_sending("high");
        let second = q.next_ready(now).unwrap();
        assert_eq!(second.id, "normal");

        q.mark_sending("normal");
        let third = q.next_ready(now).unwrap();
        assert_eq!(third.id, "low");
    }

    #[test]
    fn test_fifo_within_same_priority() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("older", MessagePriority::Normal, 1000))
            .unwrap();
        q.submit(make_msg("newer", MessagePriority::Normal, 2000))
            .unwrap();

        let first = q.next_ready(5000).unwrap();
        assert_eq!(first.id, "older");

        q.mark_sending("older");
        let second = q.next_ready(5000).unwrap();
        assert_eq!(second.id, "newer");
    }

    #[test]
    fn test_next_ready_respects_scheduled_send() {
        let mut q = MessageQueue::new(10);
        let mut scheduled = make_msg("scheduled", MessagePriority::High, 1000);
        scheduled.send_at_ms = Some(5000);

        let immediate = make_msg("immediate", MessagePriority::Normal, 2000);

        q.submit(scheduled).unwrap();
        q.submit(immediate).unwrap();

        // At time 3000, the High message is not yet ready.
        let first = q.next_ready(3000).unwrap();
        assert_eq!(first.id, "immediate");

        // At time 5000, the High message is ready and takes priority.
        q.mark_sending("immediate");
        let second = q.next_ready(5000).unwrap();
        assert_eq!(second.id, "scheduled");
    }

    #[test]
    fn test_next_ready_returns_none_on_empty_queue() {
        let mut q = MessageQueue::new(10);
        assert!(q.next_ready(1000).is_none());
    }

    #[test]
    fn test_status_transitions_happy_path() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("m1", MessagePriority::Normal, 1000))
            .unwrap();

        assert_eq!(q.get_status("m1"), Some(&MessageStatus::Pending));

        q.mark_sending("m1");
        assert_eq!(q.get_status("m1"), Some(&MessageStatus::Sending));

        q.mark_sent("m1");
        assert_eq!(q.get_status("m1"), Some(&MessageStatus::Sent));

        q.mark_delivered("m1");
        assert_eq!(q.get_status("m1"), Some(&MessageStatus::Delivered));
    }

    #[test]
    fn test_mark_failed_retries_then_fails() {
        let mut q = MessageQueue::new(10);
        let mut msg = make_msg("m1", MessagePriority::Normal, 1000);
        msg.max_retries = 2;
        q.submit(msg).unwrap();

        q.mark_sending("m1");
        q.mark_failed("m1", "timeout");
        // Should be back to Pending (retry 1/2).
        assert_eq!(q.get_status("m1"), Some(&MessageStatus::Pending));

        q.mark_sending("m1");
        q.mark_failed("m1", "timeout again");
        // Should be Failed (retry 2/2 = exhausted).
        assert_eq!(
            q.get_status("m1"),
            Some(&MessageStatus::Failed {
                reason: "timeout again".to_string(),
                retry_count: 2,
            })
        );
    }

    #[test]
    fn test_cancel_only_pending() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("m1", MessagePriority::Normal, 1000))
            .unwrap();
        q.submit(make_msg("m2", MessagePriority::Normal, 2000))
            .unwrap();

        // Cancel pending works.
        assert!(q.mark_cancelled("m1"));
        assert_eq!(q.get_status("m1"), Some(&MessageStatus::Cancelled));

        // Cannot cancel a Sending message.
        q.mark_sending("m2");
        assert!(!q.mark_cancelled("m2"));
        assert_eq!(q.get_status("m2"), Some(&MessageStatus::Sending));
    }

    #[test]
    fn test_cancel_nonexistent_returns_false() {
        let mut q = MessageQueue::new(10);
        assert!(!q.mark_cancelled("ghost"));
    }

    #[test]
    fn test_expire_stale() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("old", MessagePriority::Normal, 1000))
            .unwrap();
        q.submit(make_msg("new", MessagePriority::Normal, 9000))
            .unwrap();

        // Max age 5000ms, now=10000 => cutoff=5000. "old" (created 1000) < 5000 => Expired.
        q.expire_stale(10000, 5000);

        assert_eq!(q.get_status("old"), Some(&MessageStatus::Expired));
        assert_eq!(q.get_status("new"), Some(&MessageStatus::Pending));
    }

    #[test]
    fn test_recover_stale_sending() {
        let mut q = MessageQueue::new(10);
        let mut msg = make_msg("m1", MessagePriority::Normal, 1000);
        msg.updated_at_ms = 1000;
        msg.max_retries = 3;
        q.submit(msg).unwrap();
        q.mark_sending("m1");

        // Threshold 2000ms, now=5000 => cutoff=3000. updated_at=1000 < 3000 => stale.
        q.recover_stale_sending(5000, 2000);
        assert_eq!(q.get_status("m1"), Some(&MessageStatus::Pending));
    }

    #[test]
    fn test_recover_stale_sending_exhausts_retries() {
        let mut q = MessageQueue::new(10);
        let mut msg = make_msg("m1", MessagePriority::Normal, 1000);
        msg.updated_at_ms = 1000;
        msg.max_retries = 1;
        q.submit(msg).unwrap();
        q.mark_sending("m1");

        q.recover_stale_sending(5000, 2000);
        match q.get_status("m1") {
            Some(MessageStatus::Failed { retry_count, .. }) => {
                assert_eq!(*retry_count, 1);
            }
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn test_queue_depth_excludes_terminal() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("m1", MessagePriority::Normal, 1000))
            .unwrap();
        q.submit(make_msg("m2", MessagePriority::Normal, 2000))
            .unwrap();
        q.submit(make_msg("m3", MessagePriority::Normal, 3000))
            .unwrap();

        assert_eq!(q.queue_depth(), 3);

        q.mark_cancelled("m1");
        assert_eq!(q.queue_depth(), 2);

        q.mark_sending("m2");
        q.mark_sent("m2");
        q.mark_delivered("m2");
        assert_eq!(q.queue_depth(), 1);
    }

    #[test]
    fn test_stats() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("m1", MessagePriority::Normal, 1000))
            .unwrap();
        q.submit(make_msg("m2", MessagePriority::Normal, 2000))
            .unwrap();
        q.submit(make_msg("m3", MessagePriority::Normal, 3000))
            .unwrap();

        q.mark_sending("m2");
        q.mark_cancelled("m3");

        let s = q.stats();
        assert_eq!(s.pending, 1);
        assert_eq!(s.sending, 1);
        assert_eq!(s.cancelled, 1);
        assert_eq!(s.sent, 0);
        assert_eq!(s.delivered, 0);
        assert_eq!(s.failed, 0);
        assert_eq!(s.expired, 0);
    }

    #[test]
    fn test_drain_delivered() {
        let mut q = MessageQueue::new(10);
        q.submit(make_msg("m1", MessagePriority::Normal, 1000))
            .unwrap();
        q.submit(make_msg("m2", MessagePriority::Normal, 2000))
            .unwrap();
        // m3 is recent enough to survive the expire_stale call.
        q.submit(make_msg("m3", MessagePriority::Normal, 90000))
            .unwrap();
        // m4 is old and will be expired.
        q.submit(make_msg("m4", MessagePriority::Normal, 500))
            .unwrap();

        // m1 -> Delivered
        q.mark_sending("m1");
        q.mark_sent("m1");
        q.mark_delivered("m1");
        // m2 -> Cancelled
        q.mark_cancelled("m2");
        // m3 -> Pending (stays — created at 90000, above cutoff)
        // m4 -> Expired (created at 500 < cutoff 5000)
        q.expire_stale(10000, 5000);

        let drained = q.drain_delivered();
        let mut drained_ids: Vec<String> = drained.iter().map(|m| m.id.clone()).collect();
        drained_ids.sort();

        assert_eq!(drained_ids, vec!["m1", "m2", "m4"]);
        assert_eq!(q.pending_count(), 1);
        assert!(q.get_status("m1").is_none());
        assert!(q.get_status("m3").is_some());
    }

    #[test]
    fn test_get_status_nonexistent() {
        let q = MessageQueue::new(10);
        assert!(q.get_status("ghost").is_none());
    }

    #[test]
    fn test_message_priority_serialization() {
        let priorities = vec![
            MessagePriority::High,
            MessagePriority::Normal,
            MessagePriority::Low,
        ];
        for p in &priorities {
            let json = serde_json::to_string(p).unwrap();
            let deser: MessagePriority = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, deser);
        }
    }

    #[test]
    fn test_message_status_serialization() {
        let statuses = vec![
            MessageStatus::Pending,
            MessageStatus::Sending,
            MessageStatus::Sent,
            MessageStatus::Delivered,
            MessageStatus::Failed {
                reason: "timeout".to_string(),
                retry_count: 2,
            },
            MessageStatus::Cancelled,
            MessageStatus::Expired,
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let deser: MessageStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, deser);
        }
    }

    #[test]
    fn test_queued_message_serialization() {
        let msg = make_msg("m1", MessagePriority::High, 1000);
        let json = serde_json::to_string(&msg).unwrap();
        let deser: QueuedMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.id, "m1");
        assert_eq!(deser.priority, MessagePriority::High);
        assert_eq!(deser.created_at_ms, 1000);
    }

    #[test]
    fn test_is_terminal() {
        assert!(!MessageStatus::Pending.is_terminal());
        assert!(!MessageStatus::Sending.is_terminal());
        assert!(!MessageStatus::Sent.is_terminal());
        assert!(MessageStatus::Delivered.is_terminal());
        assert!(MessageStatus::Failed {
            reason: "x".to_string(),
            retry_count: 1
        }
        .is_terminal());
        assert!(MessageStatus::Cancelled.is_terminal());
        assert!(MessageStatus::Expired.is_terminal());
    }
}
