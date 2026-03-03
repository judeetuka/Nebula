//! Action recording module for the Action Mirror plugin.
//!
//! Records sequences of remote actions into named macros that can be persisted
//! to the plugin's key-value state store and replayed later.

use nebula_plugin_sdk::context::PluginContext;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single step in a recorded macro.
#[derive(Clone, Serialize, Deserialize)]
pub struct RecordedAction {
    pub step: u32,
    pub action: RemoteAction,
    pub delay_after_ms: u64,
    pub screenshot_before: bool,
}

/// The set of remote actions that can be recorded and replayed.
#[derive(Clone, Serialize, Deserialize)]
pub enum RemoteAction {
    Tap { x: f32, y: f32 },
    Swipe {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        duration_ms: u64,
    },
    Type { text: String },
    PressBack,
    PressHome,
    Scroll {
        direction: String,
        x: f32,
        y: f32,
    },
    WaitForElement {
        selector: String,
        timeout_ms: u64,
    },
    LaunchApp { package: String },
    Wait { ms: u64 },
}

/// Macro recorder state.
pub struct MacroRecorder {
    pub recording: bool,
    pub actions: Vec<RecordedAction>,
    pub start_time: i64,
    pub next_step: u32,
    last_action_ms: i64,
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self {
            recording: false,
            actions: Vec::new(),
            start_time: 0,
            next_step: 0,
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

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Begin recording actions.
pub fn handle_start_recording(recorder: &mut MacroRecorder) -> Result<String, String> {
    recorder.recording = true;
    recorder.actions.clear();
    recorder.start_time = current_time_ms();
    recorder.last_action_ms = recorder.start_time;
    recorder.next_step = 0;

    Ok(serde_json::json!({
        "status": "recording",
        "start_time_ms": recorder.start_time
    })
    .to_string())
}

/// Add a single action to the current recording.
pub fn handle_record_action(
    recorder: &mut MacroRecorder,
    params: &serde_json::Value,
) -> Result<String, String> {
    if !recorder.recording {
        return Err("Not currently recording".to_string());
    }

    let action: RemoteAction = serde_json::from_value(params["action"].clone())
        .map_err(|e| format!("Failed to parse action: {e}"))?;

    let now = current_time_ms();
    let delay = if recorder.last_action_ms > 0 {
        (now - recorder.last_action_ms).max(0) as u64
    } else {
        0
    };

    let screenshot_before = params["screenshot_before"].as_bool().unwrap_or(false);

    let step = recorder.next_step;
    recorder.next_step += 1;
    recorder.last_action_ms = now;

    recorder.actions.push(RecordedAction {
        step,
        action,
        delay_after_ms: delay,
        screenshot_before,
    });

    Ok(serde_json::json!({
        "status": "recorded",
        "step": step,
        "total_actions": recorder.actions.len()
    })
    .to_string())
}

/// Stop recording and return the full macro JSON.
pub fn handle_stop_recording(recorder: &mut MacroRecorder) -> Result<String, String> {
    recorder.recording = false;
    let duration_ms = current_time_ms() - recorder.start_time;
    let actions = recorder.actions.clone();

    let result = serde_json::json!({
        "status": "stopped",
        "action_count": actions.len(),
        "duration_ms": duration_ms,
        "macro": actions
    });
    Ok(result.to_string())
}

/// Retrieve a named macro from the plugin's key-value state store.
pub fn handle_get_macro(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| "Missing 'name' parameter".to_string())?;

    let key = format!("macro:{name}");
    read_state(ctx, &key)
}

/// Save a macro to the plugin's key-value state store.
pub fn handle_save_macro(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| "Missing 'name' parameter".to_string())?;
    let macro_json = params["macro_json"]
        .as_str()
        .ok_or_else(|| "Missing 'macro_json' parameter".to_string())?;

    let key = format!("macro:{name}");
    write_state(ctx, &key, macro_json)?;

    // Update the macro list.
    let list_json = read_state(ctx, "macro_list").unwrap_or_else(|_| "[]".to_string());
    let mut list: Vec<String> =
        serde_json::from_str(&list_json).unwrap_or_default();
    if !list.contains(&name.to_string()) {
        list.push(name.to_string());
        let updated = serde_json::to_string(&list)
            .map_err(|e| format!("Failed to serialize macro list: {e}"))?;
        write_state(ctx, "macro_list", &updated)?;
    }

    Ok(serde_json::json!({
        "status": "saved",
        "name": name
    })
    .to_string())
}

/// List all saved macros.
pub fn handle_list_macros(ctx: *const PluginContext) -> Result<String, String> {
    let list_json = read_state(ctx, "macro_list").unwrap_or_else(|_| "[]".to_string());
    let list: Vec<String> =
        serde_json::from_str(&list_json).unwrap_or_default();

    Ok(serde_json::json!({
        "macros": list,
        "count": list.len()
    })
    .to_string())
}

/// Delete a named macro from the state store.
pub fn handle_delete_macro(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| "Missing 'name' parameter".to_string())?;

    let key = format!("macro:{name}");
    delete_state(ctx, &key)?;

    // Update the macro list.
    let list_json = read_state(ctx, "macro_list").unwrap_or_else(|_| "[]".to_string());
    let mut list: Vec<String> =
        serde_json::from_str(&list_json).unwrap_or_default();
    list.retain(|n| n != name);
    let updated = serde_json::to_string(&list)
        .map_err(|e| format!("Failed to serialize macro list: {e}"))?;
    write_state(ctx, "macro_list", &updated)?;

    Ok(serde_json::json!({
        "status": "deleted",
        "name": name
    })
    .to_string())
}

// ---------------------------------------------------------------------------
// State store helpers
// ---------------------------------------------------------------------------

/// Read a value from the plugin's key-value state store.
fn read_state(ctx: *const PluginContext, key: &str) -> Result<String, String> {
    // SAFETY: `ctx` was validated non-null by the caller in `nebula_plugin_execute`.
    let ctx_ref = unsafe { &*ctx };

    let mut val_buf = vec![0u8; 65536];
    let ret = (ctx_ref.get_state)(
        ctx_ref.host_data,
        key.as_ptr(),
        key.len(),
        val_buf.as_mut_ptr(),
        val_buf.len(),
    );

    if ret == -2 {
        return Err(format!("Key not found: {key}"));
    }
    if ret < 0 {
        return Err(format!("get_state failed: {ret}"));
    }

    let result = std::str::from_utf8(&val_buf[..ret as usize])
        .map_err(|e| format!("Invalid UTF-8 in state value: {e}"))?;
    Ok(result.to_string())
}

/// Write a value to the plugin's key-value state store.
fn write_state(ctx: *const PluginContext, key: &str, value: &str) -> Result<(), String> {
    // SAFETY: `ctx` was validated non-null by the caller in `nebula_plugin_execute`.
    let ctx_ref = unsafe { &*ctx };

    let ret = (ctx_ref.set_state)(
        ctx_ref.host_data,
        key.as_ptr(),
        key.len(),
        value.as_ptr(),
        value.len(),
    );

    if ret < 0 {
        return Err(format!("set_state failed: {ret}"));
    }
    Ok(())
}

/// Delete a key from the plugin's key-value state store.
fn delete_state(ctx: *const PluginContext, key: &str) -> Result<(), String> {
    // SAFETY: `ctx` was validated non-null by the caller in `nebula_plugin_execute`.
    let ctx_ref = unsafe { &*ctx };

    let ret = (ctx_ref.delete_state)(
        ctx_ref.host_data,
        key.as_ptr(),
        key.len(),
    );

    if ret < 0 {
        return Err(format!("delete_state failed: {ret}"));
    }
    Ok(())
}
