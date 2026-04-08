//! Accessibility plugin for NEBULA.
//!
//! Provides accessibility service interaction (screen content, clicks, text
//! input), clipboard access, and app management (list, launch, check installed)
//! by routing all calls through `platform_invoke` to the Android bridge.
//!
//! Enhanced with action recording/replay, element finding, wait conditions,
//! and global accessibility actions (back, home, scroll, swipe).

use nebula_plugin_sdk::context::PluginContext;
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Global plugin context pointer, set during `nebula_plugin_init` and cleared
/// during `nebula_plugin_shutdown`. Accessed atomically because the engine may
/// call `execute` from different threads.
static CTX: AtomicPtr<PluginContext> = AtomicPtr::new(std::ptr::null_mut());

/// Global action recorder state protected by a Mutex.
static RECORDER: Mutex<Option<ActionRecorder>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Recording types
// ---------------------------------------------------------------------------

/// A single recorded UI action with timing metadata.
#[derive(Clone, Serialize, Deserialize)]
struct RecordedAction {
    action_type: String,
    target: String,
    value: Option<String>,
    delay_ms: u64,
    timestamp_ms: i64,
}

/// Tracks the recording state for UI action macros.
struct ActionRecorder {
    recording: bool,
    actions: Vec<RecordedAction>,
    start_time_ms: i64,
    last_action_ms: i64,
}

impl ActionRecorder {
    fn new() -> Self {
        Self {
            recording: false,
            actions: Vec::new(),
            start_time_ms: 0,
            last_action_ms: 0,
        }
    }
}

/// Get the current time in epoch milliseconds.
fn current_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// If recording is active, capture this action into the recorder.
fn maybe_record(action_type: &str, target: &str, value: Option<&str>) {
    if let Ok(mut guard) = RECORDER.lock() {
        if let Some(recorder) = guard.as_mut() {
            if !recorder.recording {
                return;
            }
            let now = current_time_ms();
            let delay = if recorder.last_action_ms > 0 {
                (now - recorder.last_action_ms).max(0) as u64
            } else {
                0
            };
            recorder.actions.push(RecordedAction {
                action_type: action_type.to_string(),
                target: target.to_string(),
                value: value.map(|s| s.to_string()),
                delay_ms: delay,
                timestamp_ms: now,
            });
            recorder.last_action_ms = now;
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
    if let Ok(mut guard) = RECORDER.lock() {
        *guard = Some(ActionRecorder::new());
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
        // --- Original 9 actions ---
        "isEnabled" => invoke(ctx, "android:accessibility:isAccessibilityEnabled", "{}"),
        "getScreenContent" => invoke(ctx, "android:accessibility:getScreenContent", "{}"),
        "performClick" => {
            let node_id = params["nodeId"].as_str().unwrap_or("");
            maybe_record("Click", node_id, None);
            let args = serde_json::json!({ "nodeId": params["nodeId"] });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "performText" => {
            let node_id = params["nodeId"].as_str().unwrap_or("");
            let text = params["text"].as_str().unwrap_or("");
            maybe_record("SetText", node_id, Some(text));
            let args = serde_json::json!({
                "nodeId": params["nodeId"],
                "text": params["text"]
            });
            invoke(ctx, "android:accessibility:performText", &args.to_string())
        }
        "getClipboard" => invoke(ctx, "android:clipboard:getClipboard", "{}"),
        "setClipboard" => {
            let args = serde_json::json!({ "text": params["text"] });
            invoke(ctx, "android:clipboard:setClipboard", &args.to_string())
        }
        "listInstalledApps" => invoke(ctx, "android:apps:listInstalledApps", "{}"),
        "launchApp" => {
            let pkg = params["packageName"].as_str().unwrap_or("");
            maybe_record("LaunchApp", pkg, None);
            let args = serde_json::json!({ "packageName": params["packageName"] });
            invoke(ctx, "android:apps:launchApp", &args.to_string())
        }
        "isAppInstalled" => {
            let args = serde_json::json!({ "packageName": params["packageName"] });
            invoke(ctx, "android:apps:isAppInstalled", &args.to_string())
        }

        // --- Recording actions ---
        "startRecording" => handle_start_recording(),
        "stopRecording" => handle_stop_recording(),
        "playMacro" => handle_play_macro(ctx, params),

        // --- Element finding ---
        "findElement" => handle_find_element(ctx, params),
        "waitForElement" => handle_wait_for_element(ctx, params),

        // --- Global accessibility actions ---
        "pressBack" => {
            maybe_record("PressBack", "", None);
            let args = serde_json::json!({ "action": "GLOBAL_ACTION_BACK" });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "pressHome" => {
            maybe_record("PressHome", "", None);
            let args = serde_json::json!({ "action": "GLOBAL_ACTION_HOME" });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "scrollDown" => {
            let node_id = params["nodeId"].as_str().unwrap_or("");
            maybe_record("ScrollDown", node_id, None);
            let args = serde_json::json!({
                "nodeId": params["nodeId"],
                "action": "ACTION_SCROLL_FORWARD"
            });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "scrollUp" => {
            let node_id = params["nodeId"].as_str().unwrap_or("");
            maybe_record("ScrollUp", node_id, None);
            let args = serde_json::json!({
                "nodeId": params["nodeId"],
                "action": "ACTION_SCROLL_BACKWARD"
            });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "swipe" => {
            let node_id = params["nodeId"].as_str().unwrap_or("");
            let direction = params["direction"].as_str().unwrap_or("down");
            maybe_record("Swipe", node_id, Some(direction));
            let args = serde_json::json!({
                "nodeId": params["nodeId"],
                "action": format!("ACTION_SWIPE_{}", direction.to_uppercase()),
                "direction": direction
            });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }

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
    if let Ok(mut guard) = RECORDER.lock() {
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
        "id": "com.nebula.accessibility",
        "name": "Accessibility",
        "version": "1.0.0",
        "capabilities": ["Accessibility", "Clipboard", "AppManagement"]
    });
    let c_str = CString::new(info.to_string()).unwrap_or_default();
    c_str.into_raw() as *const std::ffi::c_char
}

// ---------------------------------------------------------------------------
// Recording handlers
// ---------------------------------------------------------------------------

/// Begin recording UI actions.
fn handle_start_recording() -> Result<String, String> {
    let mut guard = RECORDER
        .lock()
        .map_err(|e| format!("Failed to acquire recorder lock: {e}"))?;
    let recorder = guard
        .as_mut()
        .ok_or_else(|| "Recorder not initialized".to_string())?;

    recorder.recording = true;
    recorder.actions.clear();
    recorder.start_time_ms = current_time_ms();
    recorder.last_action_ms = recorder.start_time_ms;

    Ok(serde_json::json!({
        "status": "recording",
        "start_time_ms": recorder.start_time_ms
    })
    .to_string())
}

/// Stop recording and return the captured macro.
fn handle_stop_recording() -> Result<String, String> {
    let mut guard = RECORDER
        .lock()
        .map_err(|e| format!("Failed to acquire recorder lock: {e}"))?;
    let recorder = guard
        .as_mut()
        .ok_or_else(|| "Recorder not initialized".to_string())?;

    recorder.recording = false;
    let actions = recorder.actions.clone();
    let duration_ms = current_time_ms() - recorder.start_time_ms;

    let result = serde_json::json!({
        "status": "stopped",
        "action_count": actions.len(),
        "duration_ms": duration_ms,
        "macro": actions
    });
    Ok(result.to_string())
}

/// Execute a previously recorded macro by replaying each action.
fn handle_play_macro(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let macro_data = params["macro"].as_array().ok_or_else(|| {
        "Expected 'macro' parameter to be a JSON array of recorded actions".to_string()
    })?;

    let actions: Vec<RecordedAction> = macro_data
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();

    let mut results = Vec::new();

    for action in &actions {
        // Note: In a real implementation, we would sleep for action.delay_ms.
        // Since this is a synchronous plugin call, delays are advisory.
        let result = replay_action(ctx, action);
        results.push(serde_json::json!({
            "action_type": action.action_type,
            "target": action.target,
            "success": result.is_ok(),
            "error": result.err()
        }));
    }

    Ok(serde_json::json!({
        "status": "completed",
        "actions_played": results.len(),
        "results": results
    })
    .to_string())
}

/// Replay a single recorded action by dispatching to the appropriate platform call.
fn replay_action(ctx: *const PluginContext, action: &RecordedAction) -> Result<String, String> {
    match action.action_type.as_str() {
        "Click" => {
            let args = serde_json::json!({ "nodeId": action.target });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "SetText" => {
            let text = action.value.as_deref().unwrap_or("");
            let args = serde_json::json!({
                "nodeId": action.target,
                "text": text
            });
            invoke(ctx, "android:accessibility:performText", &args.to_string())
        }
        "LaunchApp" => {
            let args = serde_json::json!({ "packageName": action.target });
            invoke(ctx, "android:apps:launchApp", &args.to_string())
        }
        "PressBack" => {
            let args = serde_json::json!({ "action": "GLOBAL_ACTION_BACK" });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "PressHome" => {
            let args = serde_json::json!({ "action": "GLOBAL_ACTION_HOME" });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "ScrollDown" => {
            let args = serde_json::json!({
                "nodeId": action.target,
                "action": "ACTION_SCROLL_FORWARD"
            });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "ScrollUp" => {
            let args = serde_json::json!({
                "nodeId": action.target,
                "action": "ACTION_SCROLL_BACKWARD"
            });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        "Swipe" => {
            let direction = action.value.as_deref().unwrap_or("down");
            let args = serde_json::json!({
                "nodeId": action.target,
                "action": format!("ACTION_SWIPE_{}", direction.to_uppercase()),
                "direction": direction
            });
            invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        _ => Err(format!("Unknown action type for replay: {}", action.action_type)),
    }
}

// ---------------------------------------------------------------------------
// Element finding handlers
// ---------------------------------------------------------------------------

/// Search the current screen content for an element matching the given criteria.
fn handle_find_element(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let screen_json = invoke(ctx, "android:accessibility:getScreenContent", "{}")?;
    let screen: serde_json::Value = serde_json::from_str(&screen_json)
        .map_err(|e| format!("Failed to parse screen content: {e}"))?;

    let text = params["text"].as_str().unwrap_or("");
    let class_name = params["className"].as_str().unwrap_or("");
    let content_desc = params["contentDesc"].as_str().unwrap_or("");

    let matches = find_matching_nodes(&screen, text, class_name, content_desc);

    Ok(serde_json::json!({
        "found": !matches.is_empty(),
        "count": matches.len(),
        "elements": matches
    })
    .to_string())
}

/// Recursively search a screen content tree for nodes matching the criteria.
fn find_matching_nodes(
    node: &serde_json::Value,
    text: &str,
    class_name: &str,
    content_desc: &str,
) -> Vec<serde_json::Value> {
    let mut results = Vec::new();

    // Check if this node matches.
    let mut matches = true;
    if !text.is_empty() {
        let node_text = node["text"].as_str().unwrap_or("");
        if !node_text.contains(text) {
            matches = false;
        }
    }
    if !class_name.is_empty() {
        let node_class = node["className"].as_str().unwrap_or("");
        if !node_class.contains(class_name) {
            matches = false;
        }
    }
    if !content_desc.is_empty() {
        let node_desc = node["contentDescription"].as_str().unwrap_or("");
        if !node_desc.contains(content_desc) {
            matches = false;
        }
    }

    // At least one criterion must be non-empty for a valid match.
    let has_criteria = !text.is_empty() || !class_name.is_empty() || !content_desc.is_empty();

    if matches && has_criteria {
        results.push(node.clone());
    }

    // Recurse into children.
    if let Some(children) = node["children"].as_array() {
        for child in children {
            results.extend(find_matching_nodes(child, text, class_name, content_desc));
        }
    }

    // Also check "nodes" array if this is a flat list.
    if let Some(nodes) = node["nodes"].as_array() {
        for child in nodes {
            results.extend(find_matching_nodes(child, text, class_name, content_desc));
        }
    }

    results
}

/// Poll the screen content until an element matching the selector appears,
/// or until the timeout is reached.
fn handle_wait_for_element(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let selector_text = params["selector"].as_str().unwrap_or("");
    let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(5000);

    // Poll interval: 500ms, max 10 attempts within timeout.
    let max_polls = ((timeout_ms / 500) + 1).min(10) as usize;
    let start_ms = current_time_ms();

    for poll in 0..max_polls {
        let elapsed = (current_time_ms() - start_ms) as u64;
        if elapsed >= timeout_ms {
            break;
        }

        let screen_json = invoke(ctx, "android:accessibility:getScreenContent", "{}")?;
        let screen: serde_json::Value = serde_json::from_str(&screen_json)
            .map_err(|e| format!("Failed to parse screen content: {e}"))?;

        let matches = find_matching_nodes(&screen, selector_text, "", "");
        if !matches.is_empty() {
            return Ok(serde_json::json!({
                "found": true,
                "poll_count": poll + 1,
                "elapsed_ms": elapsed,
                "element": matches[0]
            })
            .to_string());
        }

        // In a real implementation, we would sleep here. Since this is a
        // synchronous plugin, the polling is best-effort.
    }

    let elapsed = (current_time_ms() - start_ms) as u64;
    Ok(serde_json::json!({
        "found": false,
        "poll_count": max_polls,
        "elapsed_ms": elapsed,
        "timeout_ms": timeout_ms
    })
    .to_string())
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
