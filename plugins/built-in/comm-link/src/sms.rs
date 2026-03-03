//! SMS orchestration for the Comm-Link plugin.
//!
//! Provides DroidRelay-grade SMS handling: priority queuing, per-SIM rate
//! limiting (100 ms minimum interval), multipart delivery tracking, automatic
//! retry with exponential backoff, scheduled sends, and 24-hour expiration.
//!
//! All actual SMS transmission is routed through `common::invoke` to the
//! platform bridge — this module only manages the orchestration logic.

use crate::common;
use crate::sim;
use nebula_plugin_sdk::delivery::DeliveryTracker;
use nebula_plugin_sdk::queue::{MessagePriority, MessageQueue, MessageStatus, QueuedMessage};
use nebula_plugin_sdk::ratelimit::RateLimiter;
use nebula_plugin_sdk::retry::RetryPolicy;
use std::sync::Mutex;
use std::time::Duration;

// -------------------------------------------------------------------------
// Orchestration state (initialized in `init()`, guarded by Mutex)
// -------------------------------------------------------------------------

static SMS_QUEUE: Mutex<Option<MessageQueue>> = Mutex::new(None);
static RATE_LIMITER: Mutex<Option<RateLimiter>> = Mutex::new(None);
static DELIVERY_TRACKER: Mutex<Option<DeliveryTracker>> = Mutex::new(None);
static RETRY_POLICY: Mutex<Option<RetryPolicy>> = Mutex::new(None);

/// Maximum number of messages in the queue.
const QUEUE_CAPACITY: usize = 1000;

/// Per-SIM minimum interval between SMS sends (DroidRelay default: 100 ms).
const RATE_LIMIT_INTERVAL: Duration = Duration::from_millis(100);

/// Delivery confirmation timeout (5 minutes).
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(300);

/// Maximum retry attempts per message.
const MAX_RETRIES: u32 = 3;

/// Stale message age for expiration (24 hours).
const EXPIRATION_AGE_MS: i64 = 24 * 60 * 60 * 1000;

/// Stale-sending recovery threshold (5 minutes).
const STALE_SENDING_THRESHOLD_MS: i64 = 5 * 60 * 1000;

/// Maximum concurrent messages in `Sending` state.
const MAX_CONCURRENT_SENDING: usize = 4;

/// Initialize SMS orchestration state.
pub fn init() {
    *SMS_QUEUE.lock().unwrap() = Some(MessageQueue::new(QUEUE_CAPACITY));
    *RATE_LIMITER.lock().unwrap() = Some(RateLimiter::new(RATE_LIMIT_INTERVAL));
    *DELIVERY_TRACKER.lock().unwrap() = Some(DeliveryTracker::new(DELIVERY_TIMEOUT));
    *RETRY_POLICY.lock().unwrap() = Some(
        RetryPolicy::new()
            .with_max_retries(MAX_RETRIES)
            .with_initial_delay(1000)
            .with_max_delay(30_000)
            .with_backoff_multiplier(2.0),
    );

    common::log(common::log_level::INFO, "SMS orchestration initialized");
}

// -------------------------------------------------------------------------
// Actions
// -------------------------------------------------------------------------

/// Submit an SMS to the priority queue for asynchronous sending.
///
/// Params:
/// - `to` (string, required): destination phone number
/// - `content` (string, required): message body
/// - `simSlot` (integer, optional): SIM slot override
/// - `priority` (string, optional): "High", "Normal" (default), or "Low"
/// - `sendAt` (integer, optional): scheduled send time in epoch ms
///
/// Returns the generated `messageId`.
pub fn submit_sms(params: &serde_json::Value) -> Result<String, String> {
    let to = params["to"]
        .as_str()
        .ok_or("missing 'to' parameter")?;
    let content = params["content"]
        .as_str()
        .ok_or("missing 'content' parameter")?;

    let sim_slot = params["simSlot"].as_i64().map(|s| s as i32);
    let resolved_slot = sim::resolve_sim_slot_internal(sim_slot);

    let priority = match params["priority"].as_str() {
        Some("High") => MessagePriority::High,
        Some("Low") => MessagePriority::Low,
        _ => MessagePriority::Normal,
    };

    let send_at_ms = params["sendAt"].as_i64();

    let now = common::now_ms();
    let message_id = common::generate_message_id();

    let payload = serde_json::json!({
        "to": to,
        "content": content,
        "simSlot": resolved_slot,
    })
    .to_string();

    let msg = QueuedMessage {
        id: message_id.clone(),
        payload,
        priority,
        status: MessageStatus::Pending,
        target_key: format!("sim{resolved_slot}"),
        retry_count: 0,
        max_retries: MAX_RETRIES,
        created_at_ms: now,
        updated_at_ms: now,
        send_at_ms,
        expires_at_ms: Some(now + EXPIRATION_AGE_MS),
    };

    let mut queue = SMS_QUEUE.lock().map_err(|e| e.to_string())?;
    let q = queue.as_mut().ok_or("SMS queue not initialized")?;
    q.submit(msg)?;

    common::log(
        common::log_level::DEBUG,
        &format!("SMS queued: {message_id} -> {to} via sim{resolved_slot}"),
    );

    Ok(serde_json::json!({"messageId": message_id}).to_string())
}

/// Process the SMS queue: dequeue ready messages, check rate limits, send.
///
/// Called periodically by the engine via the `processQueue` action. Processes
/// up to `MAX_CONCURRENT_SENDING` messages per invocation.
pub fn process_queue(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;
    let now = common::now_ms();
    let mut sent_count = 0u32;
    let mut skipped_count = 0u32;

    let mut queue = SMS_QUEUE.lock().map_err(|e| e.to_string())?;
    let q = queue.as_mut().ok_or("SMS queue not initialized")?;
    let mut limiter = RATE_LIMITER.lock().map_err(|e| e.to_string())?;
    let rl = limiter.as_mut().ok_or("Rate limiter not initialized")?;
    let mut tracker = DELIVERY_TRACKER.lock().map_err(|e| e.to_string())?;
    let dt = tracker.as_mut().ok_or("Delivery tracker not initialized")?;

    // Check how many are already sending.
    let stats = q.stats();
    let mut active_sending = stats.sending;

    // Process ready messages up to the concurrency cap.
    while active_sending < MAX_CONCURRENT_SENDING {
        let ready = match q.next_ready(now) {
            Some(msg) => msg,
            None => break,
        };

        // Per-SIM rate limiting.
        let wait = rl.acquire(&ready.target_key);
        if wait > Duration::ZERO {
            skipped_count += 1;
            // Message stays Pending; will be picked up next cycle.
            break;
        }

        q.mark_sending(&ready.id);
        active_sending += 1;

        // Parse payload to extract send parameters.
        let payload: serde_json::Value = serde_json::from_str(&ready.payload)
            .map_err(|e| format!("corrupt payload for {}: {e}", ready.id))?;

        let send_args = serde_json::json!({
            "phone": payload["to"],
            "message": payload["content"],
            "simSlot": payload["simSlot"],
        })
        .to_string();

        match common::invoke("android:telephony:sendSms", &send_args) {
            Ok(result) => {
                q.mark_sent(&ready.id);
                sent_count += 1;

                // Determine part count from platform response (default 1).
                let parts = serde_json::from_str::<serde_json::Value>(&result)
                    .ok()
                    .and_then(|v| v["parts"].as_u64())
                    .unwrap_or(1) as u32;

                dt.register(&ready.id, parts);

                // If the platform already confirmed sent for all parts, mark them.
                for _ in 0..parts {
                    dt.mark_part_sent(&ready.id);
                }

                common::log(
                    common::log_level::DEBUG,
                    &format!("SMS sent: {} ({parts} parts)", ready.id),
                );
            }
            Err(e) => {
                common::log(
                    common::log_level::WARN,
                    &format!("SMS send failed for {}: {e}", ready.id),
                );
                q.mark_failed(&ready.id, &e);
            }
        }
    }

    // Check delivery timeouts.
    let timed_out = dt.check_timeouts();
    for id in &timed_out {
        common::log(
            common::log_level::WARN,
            &format!("SMS delivery timed out: {id}"),
        );
    }

    Ok(serde_json::json!({
        "sent": sent_count,
        "skipped": skipped_count,
        "timedOut": timed_out.len(),
    })
    .to_string())
}

/// Send an SMS immediately, bypassing the queue.
///
/// Params:
/// - `to` (string, required): destination phone number
/// - `content` (string, required): message body
/// - `simSlot` (integer, optional): SIM slot override
///
/// Still applies per-SIM rate limiting and registers delivery tracking.
pub fn send_sms_immediate(params: &serde_json::Value) -> Result<String, String> {
    let to = params["to"]
        .as_str()
        .ok_or("missing 'to' parameter")?;
    let content = params["content"]
        .as_str()
        .ok_or("missing 'content' parameter")?;

    let sim_slot = params["simSlot"].as_i64().map(|s| s as i32);
    let resolved_slot = sim::resolve_sim_slot_internal(sim_slot);

    let target_key = format!("sim{resolved_slot}");

    // Rate limit check.
    {
        let mut limiter = RATE_LIMITER.lock().map_err(|e| e.to_string())?;
        let rl = limiter.as_mut().ok_or("Rate limiter not initialized")?;
        let wait = rl.acquire(&target_key);
        if wait > Duration::ZERO {
            return Err(format!(
                "Rate limited: wait {}ms for {target_key}",
                wait.as_millis()
            ));
        }
    }

    let send_args = serde_json::json!({
        "phone": to,
        "message": content,
        "simSlot": resolved_slot,
    })
    .to_string();

    let result = common::invoke("android:telephony:sendSms", &send_args)?;

    // Register delivery tracking.
    let message_id = common::generate_message_id();
    let parts = serde_json::from_str::<serde_json::Value>(&result)
        .ok()
        .and_then(|v| v["parts"].as_u64())
        .unwrap_or(1) as u32;

    {
        let mut tracker = DELIVERY_TRACKER.lock().map_err(|e| e.to_string())?;
        let dt = tracker.as_mut().ok_or("Delivery tracker not initialized")?;
        dt.register(&message_id, parts);
        for _ in 0..parts {
            dt.mark_part_sent(&message_id);
        }
    }

    common::log(
        common::log_level::DEBUG,
        &format!("SMS immediate send: {message_id} -> {to} via sim{resolved_slot}"),
    );

    Ok(serde_json::json!({
        "messageId": message_id,
        "result": result,
    })
    .to_string())
}

/// Get the status of a queued or tracked SMS message.
///
/// Params:
/// - `messageId` (string, required)
///
/// Returns the queue status and delivery status (if tracked).
pub fn get_sms_status(params: &serde_json::Value) -> Result<String, String> {
    let message_id = params["messageId"]
        .as_str()
        .ok_or("missing 'messageId' parameter")?;

    let queue_status = {
        let queue = SMS_QUEUE.lock().map_err(|e| e.to_string())?;
        let q = queue.as_ref().ok_or("SMS queue not initialized")?;
        q.get_status(message_id).map(|s| format!("{s:?}"))
    };

    let delivery_status = {
        let tracker = DELIVERY_TRACKER.lock().map_err(|e| e.to_string())?;
        let dt = tracker.as_ref().ok_or("Delivery tracker not initialized")?;
        dt.get_status(message_id).map(|s| format!("{s:?}"))
    };

    Ok(serde_json::json!({
        "messageId": message_id,
        "queueStatus": queue_status,
        "deliveryStatus": delivery_status,
    })
    .to_string())
}

/// Cancel a pending SMS in the queue.
///
/// Params:
/// - `messageId` (string, required)
///
/// Only succeeds if the message is still in `Pending` status.
pub fn cancel_sms(params: &serde_json::Value) -> Result<String, String> {
    let message_id = params["messageId"]
        .as_str()
        .ok_or("missing 'messageId' parameter")?;

    let mut queue = SMS_QUEUE.lock().map_err(|e| e.to_string())?;
    let q = queue.as_mut().ok_or("SMS queue not initialized")?;
    let cancelled = q.mark_cancelled(message_id);

    if cancelled {
        common::log(
            common::log_level::INFO,
            &format!("SMS cancelled: {message_id}"),
        );
    }

    Ok(serde_json::json!({
        "messageId": message_id,
        "cancelled": cancelled,
    })
    .to_string())
}

/// Expire stale messages and recover stuck-in-sending messages.
///
/// Should be called periodically by the engine for housekeeping.
pub fn expire_stale(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;
    let now = common::now_ms();

    let mut queue = SMS_QUEUE.lock().map_err(|e| e.to_string())?;
    let q = queue.as_mut().ok_or("SMS queue not initialized")?;

    q.expire_stale(now, EXPIRATION_AGE_MS);
    q.recover_stale_sending(now, STALE_SENDING_THRESHOLD_MS);

    // Drain terminal messages to free capacity.
    let drained = q.drain_delivered();

    // Clean up delivery tracker entries for drained messages.
    {
        let mut tracker = DELIVERY_TRACKER.lock().map_err(|e| e.to_string())?;
        if let Some(dt) = tracker.as_mut() {
            for msg in &drained {
                dt.remove(&msg.id);
            }
        }
    }

    common::log(
        common::log_level::DEBUG,
        &format!("SMS housekeeping: drained {} terminal messages", drained.len()),
    );

    Ok(serde_json::json!({
        "drained": drained.len(),
    })
    .to_string())
}

/// Get aggregate queue statistics.
pub fn get_queue_stats(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;

    let queue = SMS_QUEUE.lock().map_err(|e| e.to_string())?;
    let q = queue.as_ref().ok_or("SMS queue not initialized")?;
    let stats = q.stats();

    Ok(serde_json::json!({
        "pending": stats.pending,
        "sending": stats.sending,
        "sent": stats.sent,
        "delivered": stats.delivered,
        "failed": stats.failed,
        "cancelled": stats.cancelled,
        "expired": stats.expired,
        "depth": q.queue_depth(),
    })
    .to_string())
}

/// Retrieve received SMS messages from the platform buffer.
///
/// This is a passthrough to the platform bridge. The Android side maintains a
/// buffer of incoming SMS messages received via `BroadcastReceiver`.
pub fn get_received_sms(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;
    common::invoke("android:sms:getReceivedSms", "{}")
}

/// Clear the received SMS buffer on the platform side.
pub fn clear_received_sms(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;
    common::invoke("android:sms:clearReceivedSms", "{}")
}

/// Read SMS messages from the device inbox (ContentResolver query).
///
/// Params:
/// - `limit` (integer, optional): max messages to return (default: platform decides)
pub fn read_sms_inbox(params: &serde_json::Value) -> Result<String, String> {
    let args = serde_json::json!({ "limit": params["limit"] }).to_string();
    common::invoke("android:telephony:readSmsInbox", &args)
}
