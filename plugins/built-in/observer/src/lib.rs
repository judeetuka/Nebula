//! Observer plugin for NEBULA.
//!
//! Monitors content changes (ContentObserver) and notification events on the
//! Android device by routing calls through `platform_invoke` to the host
//! engine's Kotlin bridge.
//!
//! Enhanced with debounced event processing, timestamp-based sync tracking,
//! event deduplication via ring buffer, and URI-based event categorization.

use nebula_plugin_sdk::context::PluginContext;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Global plugin context pointer, set during `nebula_plugin_init` and cleared
/// during `nebula_plugin_shutdown`. Accessed atomically because the engine may
/// call `execute` from different threads.
static CTX: AtomicPtr<PluginContext> = AtomicPtr::new(std::ptr::null_mut());

/// Global observer state protected by a Mutex for thread-safe access.
static STATE: Mutex<Option<ObserverState>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Internal state types
// ---------------------------------------------------------------------------

/// Debounce window in milliseconds. Events within this window of the last
/// flush are buffered rather than returned immediately.
const DEBOUNCE_MS: i64 = 500;

/// Maximum number of recent event hashes to track for deduplication.
const DEDUP_RING_SIZE: usize = 100;

/// Time window in milliseconds within which duplicate events are suppressed.
const DEDUP_WINDOW_MS: i64 = 5000;

/// A hash entry for deduplication tracking.
struct EventHash {
    hash: u64,
    timestamp_ms: i64,
}

/// A parsed content change event.
#[derive(Clone, Serialize, Deserialize)]
struct ContentChange {
    uri: String,
    category: String,
    timestamp_ms: i64,
    raw_json: String,
}

/// Internal state for the observer plugin.
struct ObserverState {
    /// Last sync timestamp per event category.
    last_sync_ms: HashMap<String, i64>,
    /// Ring buffer of recent event hashes for deduplication.
    recent_events: VecDeque<EventHash>,
    /// Buffered events awaiting flush (debounce).
    event_buffer: Vec<ContentChange>,
    /// Timestamp of the last event flush.
    last_flush_ms: i64,
    /// Cumulative event counts per category.
    event_counts: HashMap<String, u64>,
}

impl ObserverState {
    fn new() -> Self {
        Self {
            last_sync_ms: HashMap::new(),
            recent_events: VecDeque::with_capacity(DEDUP_RING_SIZE),
            event_buffer: Vec::new(),
            last_flush_ms: 0,
            event_counts: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ABI exports
// ---------------------------------------------------------------------------

/// Initialize the plugin by storing the host-provided context.
///
/// # Safety
///
/// `ctx` must be a valid pointer to a `PluginContext` whose lifetime spans
/// from this call until `nebula_plugin_shutdown` completes. The engine
/// guarantees this by keeping the context alive in `LoadedPlugin`.
#[no_mangle]
pub extern "C" fn nebula_plugin_init(ctx: *const PluginContext) -> i32 {
    CTX.store(ctx as *mut PluginContext, Ordering::SeqCst);
    if let Ok(mut guard) = STATE.lock() {
        *guard = Some(ObserverState::new());
    }
    0
}

/// Execute an action dispatched to this plugin.
///
/// Input is a JSON object: `{"action": "...", "params": {...}}`
/// Output is written to the caller-provided buffer as a JSON string.
///
/// Returns the number of bytes written on success (positive), or the negative
/// byte count of an error JSON on failure. Returns -1 for fatal errors.
///
/// # Safety
///
/// - `input_ptr` must be valid for `input_len` bytes of UTF-8 data.
/// - `output_ptr` must be valid for `output_len` bytes of writable memory.
/// - Both buffers must remain valid for the duration of this synchronous call.
///   The engine guarantees this by allocating them on the stack or heap before
///   calling `execute`.
#[no_mangle]
pub extern "C" fn nebula_plugin_execute(
    input_ptr: *const u8,
    input_len: usize,
    output_ptr: *mut u8,
    output_len: usize,
) -> i32 {
    let ctx = CTX.load(Ordering::SeqCst);
    if ctx.is_null() {
        return -1;
    }

    // SAFETY: `input_ptr` is valid for `input_len` bytes as guaranteed by the
    // engine's calling convention. The slice borrows the data for this
    // synchronous call only.
    let input = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };
    let input_str = match std::str::from_utf8(input) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let request: serde_json::Value = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(_) => return -1,
    };

    let action = request["action"].as_str().unwrap_or("");
    let params = &request["params"];

    let result = match action {
        "startObserving" => invoke(ctx, "android:observer:startContentObserving", "{}"),
        "stopObserving" => invoke(ctx, "android:observer:stopContentObserving", "{}"),
        "getChanges" => handle_get_changes(ctx, None),
        "getChangesSince" => {
            let ts = params["timestamp_ms"].as_i64().unwrap_or(0);
            handle_get_changes(ctx, Some(ts))
        }
        "getActiveNotifications" => {
            invoke(ctx, "android:notification:getActiveNotifications", "{}")
        }
        "dismissNotification" => {
            let args = serde_json::json!({ "key": params["key"] });
            invoke(
                ctx,
                "android:notification:dismissNotification",
                &args.to_string(),
            )
        }
        "isNotificationListenerEnabled" => {
            invoke(ctx, "android:notification:isNotificationListenerEnabled", "{}")
        }
        "getEventStats" => handle_get_event_stats(),
        "clearEventBuffer" => handle_clear_event_buffer(),
        _ => Err(format!("Unknown action: {action}")),
    };

    write_result(result, output_ptr, output_len)
}

/// Shut down the plugin and release the stored context pointer.
///
/// # Safety
///
/// After this call returns, no further calls to `execute` will be made by the
/// engine. The `AtomicPtr` is set to null to prevent use-after-free if a
/// stale reference somehow persists.
#[no_mangle]
pub extern "C" fn nebula_plugin_shutdown() -> i32 {
    CTX.store(std::ptr::null_mut(), Ordering::SeqCst);
    if let Ok(mut guard) = STATE.lock() {
        *guard = None;
    }
    0
}

/// Return a null-terminated JSON string describing this plugin's manifest.
///
/// # Safety
///
/// The returned pointer is valid for the lifetime of the process because it
/// points to a leaked `CString`. The engine must not free or write to it.
#[no_mangle]
pub extern "C" fn nebula_plugin_info() -> *const std::ffi::c_char {
    let info = serde_json::json!({
        "id": "com.nebula.observer",
        "name": "Observer",
        "version": "1.0.0",
        "capabilities": ["Notification"]
    });
    let c_str = CString::new(info.to_string()).unwrap_or_default();
    c_str.into_raw() as *const std::ffi::c_char
}

// ---------------------------------------------------------------------------
// Orchestration logic
// ---------------------------------------------------------------------------

/// Get the current time in epoch milliseconds.
fn current_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Categorize a content URI into a known event category.
fn categorize_uri(uri: &str) -> &'static str {
    if uri.contains("content://sms") {
        "sms"
    } else if uri.contains("content://call_log") {
        "call_log"
    } else if uri.contains("content://contacts") || uri.contains("content://com.android.contacts")
    {
        "contacts"
    } else if uri.contains("content://media") {
        "media"
    } else if uri.contains("content://calendar") || uri.contains("content://com.android.calendar")
    {
        "calendar"
    } else if uri.contains("content://settings") {
        "settings"
    } else {
        "unknown"
    }
}

/// Compute a hash for dedup purposes from URI and timestamp.
fn compute_event_hash(uri: &str, timestamp_ms: i64) -> u64 {
    let mut hasher = DefaultHasher::new();
    uri.hash(&mut hasher);
    timestamp_ms.hash(&mut hasher);
    hasher.finish()
}

/// Check if a hash is a duplicate within the dedup window.
fn is_duplicate(state: &ObserverState, hash: u64, now_ms: i64) -> bool {
    state.recent_events.iter().any(|entry| {
        entry.hash == hash && (now_ms - entry.timestamp_ms) < DEDUP_WINDOW_MS
    })
}

/// Add a hash to the ring buffer, evicting the oldest entry if at capacity.
fn record_hash(state: &mut ObserverState, hash: u64, timestamp_ms: i64) {
    if state.recent_events.len() >= DEDUP_RING_SIZE {
        state.recent_events.pop_front();
    }
    state.recent_events.push_back(EventHash {
        hash,
        timestamp_ms,
    });
}

/// Parse raw content changes JSON from the Android bridge into typed events.
fn parse_content_changes(raw_json: &str) -> Vec<ContentChange> {
    let now = current_time_ms();
    // The bridge returns a JSON array of change objects, each with a "uri" field.
    // If it returns a single object, treat it as a one-element array.
    let value: serde_json::Value = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let items = match &value {
        serde_json::Value::Array(arr) => arr.clone(),
        serde_json::Value::Object(_) => vec![value],
        _ => return Vec::new(),
    };

    items
        .into_iter()
        .map(|item| {
            let uri = item["uri"].as_str().unwrap_or("").to_string();
            let category = categorize_uri(&uri).to_string();
            let ts = item["timestamp"].as_i64().unwrap_or(now);
            ContentChange {
                uri,
                category,
                timestamp_ms: ts,
                raw_json: item.to_string(),
            }
        })
        .collect()
}

/// Fetch content changes from the platform, apply debouncing, dedup, and
/// category filtering. If `since_ms` is provided, only return events with
/// timestamps after that value.
fn handle_get_changes(
    ctx: *const PluginContext,
    since_ms: Option<i64>,
) -> Result<String, String> {
    let raw = invoke(ctx, "android:observer:getContentChanges", "{}")?;
    let now = current_time_ms();
    let changes = parse_content_changes(&raw);

    let mut guard = STATE
        .lock()
        .map_err(|e| format!("Failed to acquire state lock: {e}"))?;
    let state = guard
        .as_mut()
        .ok_or_else(|| "Observer state not initialized".to_string())?;

    // Determine the filter timestamp: explicit since_ms or per-category last_sync.
    let filter_ts = since_ms.unwrap_or(0);

    // Debounce check: if less than DEBOUNCE_MS since last flush, buffer events.
    let should_flush = (now - state.last_flush_ms) >= DEBOUNCE_MS;

    // Process new changes: dedup, categorize, filter.
    for change in &changes {
        let hash = compute_event_hash(&change.uri, change.timestamp_ms);
        if is_duplicate(state, hash, now) {
            continue;
        }
        record_hash(state, hash, now);

        // Update per-category event count.
        *state.event_counts.entry(change.category.clone()).or_insert(0) += 1;

        // Only include events after the filter timestamp.
        let category_last = state
            .last_sync_ms
            .get(&change.category)
            .copied()
            .unwrap_or(0);
        let effective_filter = filter_ts.max(category_last);

        if change.timestamp_ms > effective_filter {
            state.event_buffer.push(change.clone());
        }
    }

    if !should_flush {
        // Return empty result during debounce window.
        return Ok(serde_json::json!({ "events": [], "buffered": state.event_buffer.len() }).to_string());
    }

    // Flush: drain the buffer.
    let flushed: Vec<ContentChange> = state.event_buffer.drain(..).collect();
    state.last_flush_ms = now;

    // Update per-category last sync timestamps.
    for event in &flushed {
        let entry = state
            .last_sync_ms
            .entry(event.category.clone())
            .or_insert(0);
        if event.timestamp_ms > *entry {
            *entry = event.timestamp_ms;
        }
    }

    let result = serde_json::json!({
        "events": flushed,
        "count": flushed.len(),
        "timestamp_ms": now
    });
    Ok(result.to_string())
}

/// Return cumulative event counts by category.
fn handle_get_event_stats() -> Result<String, String> {
    let guard = STATE
        .lock()
        .map_err(|e| format!("Failed to acquire state lock: {e}"))?;
    let state = guard
        .as_ref()
        .ok_or_else(|| "Observer state not initialized".to_string())?;

    let result = serde_json::json!({
        "counts": state.event_counts,
        "last_sync": state.last_sync_ms,
        "buffer_size": state.event_buffer.len(),
        "dedup_ring_size": state.recent_events.len()
    });
    Ok(result.to_string())
}

/// Clear the internal event buffer, dedup ring, and event counts.
fn handle_clear_event_buffer() -> Result<String, String> {
    let mut guard = STATE
        .lock()
        .map_err(|e| format!("Failed to acquire state lock: {e}"))?;
    let state = guard
        .as_mut()
        .ok_or_else(|| "Observer state not initialized".to_string())?;

    state.event_buffer.clear();
    state.recent_events.clear();
    state.event_counts.clear();
    state.last_sync_ms.clear();
    state.last_flush_ms = 0;

    Ok(serde_json::json!({"status": "cleared"}).to_string())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Call `platform_invoke` on the host engine with the given capability routing
/// string and JSON arguments.
fn invoke(ctx: *const PluginContext, capability: &str, args: &str) -> Result<String, String> {
    // SAFETY: `ctx` was set in `nebula_plugin_init` and the engine guarantees
    // it remains valid until `nebula_plugin_shutdown`. We verified `ctx` is
    // non-null at the top of `execute`.
    let ctx_ref = unsafe { &*ctx };

    let method = "";
    let mut result_buf = vec![0u8; 65536];
    let ret = (ctx_ref.platform_invoke)(
        ctx_ref.host_data,
        capability.as_ptr(),
        capability.len(),
        method.as_ptr(),
        method.len(),
        args.as_ptr(),
        args.len(),
        result_buf.as_mut_ptr(),
        result_buf.len(),
    );

    if ret < 0 {
        Err(format!("platform_invoke failed: {ret}"))
    } else {
        let result = std::str::from_utf8(&result_buf[..ret as usize])
            .map_err(|e| format!("Invalid UTF-8 in platform response: {e}"))?;
        Ok(result.to_string())
    }
}

/// Serialize a `Result<String, String>` into the output buffer as JSON.
fn write_result(result: Result<String, String>, output_ptr: *mut u8, output_len: usize) -> i32 {
    match result {
        Ok(json) => {
            let bytes = json.as_bytes();
            let copy_len = bytes.len().min(output_len);
            // SAFETY: `output_ptr` is valid for `output_len` bytes as
            // guaranteed by the engine. We copy at most `output_len` bytes.
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), output_ptr, copy_len);
            }
            copy_len as i32
        }
        Err(e) => {
            let err_json = serde_json::json!({"error": e}).to_string();
            let bytes = err_json.as_bytes();
            let copy_len = bytes.len().min(output_len);
            // SAFETY: same as above -- `output_ptr` is valid for `output_len` bytes.
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), output_ptr, copy_len);
            }
            -(copy_len as i32)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_info_returns_valid_json() {
        let ptr = nebula_plugin_info();
        assert!(!ptr.is_null());
        let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
        let json: serde_json::Value = serde_json::from_str(cstr.to_str().unwrap()).unwrap();
        assert!(json["id"].is_string());
        assert!(json["name"].is_string());
        assert!(json["version"].is_string());
        assert!(json["capabilities"].is_array());
    }

    #[test]
    fn test_write_result_within_buffer() {
        let mut buf = [0u8; 64];
        let n = write_result(Ok("hello".to_string()), buf.as_mut_ptr(), 64);
        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_write_result_truncates() {
        let mut buf = [0u8; 5];
        let n = write_result(Ok("this is a long message".to_string()), buf.as_mut_ptr(), 5);
        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"this ");
    }
}
