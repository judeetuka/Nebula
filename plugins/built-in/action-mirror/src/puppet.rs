//! Puppet Mode for the Action Mirror plugin.
//!
//! Implements real-time action replication from a master device (conductor) to
//! all worker devices (puppets) in the cluster. The master user interacts with
//! their device normally, and every action (tap, swipe, type, scroll, etc.) is
//! captured by the AccessibilityService, converted to relative coordinates, and
//! broadcast via MQTT to all workers for simultaneous replay.
//!
//! ## Architecture
//!
//! - **Conductor** (master): captures accessibility events, normalises
//!   coordinates to 0.0-1.0 range, publishes `PuppetAction` over MQTT.
//! - **Puppet** (worker): subscribes to the MQTT topic, converts relative
//!   coordinates back to local absolute pixels, and replays each action
//!   through the local AccessibilityService.
//!
//! ## MQTT Topics
//!
//! - `nebula/{cluster}/puppet/actions` -- action broadcast from conductor
//! - `nebula/{cluster}/puppet/status`  -- worker status reports
//! - `nebula/{cluster}/puppet/control` -- start/stop commands

use nebula_plugin_sdk::context::PluginContext;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A captured user action with relative coordinates for cross-device
/// compatibility. Coordinates are normalised to 0.0-1.0 so that a tap at
/// (540, 1200) on a 1080x2400 screen becomes (0.5, 0.5) and maps correctly
/// to any other resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuppetAction {
    /// Monotonically increasing sequence number for ordering.
    pub seq: u64,
    /// Epoch milliseconds when the action occurred on the master.
    pub timestamp_ms: i64,
    /// The action to replay.
    pub action_type: PuppetActionType,
    /// Package name of the foreground app on the master, if available.
    pub app_package: Option<String>,
}

/// The set of user actions that can be captured and replayed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PuppetActionType {
    /// Tap at relative coordinates (0.0-1.0).
    Tap { x: f64, y: f64 },
    /// Long press at relative coordinates.
    LongPress { x: f64, y: f64 },
    /// Swipe from (x1,y1) to (x2,y2) over `duration_ms` milliseconds.
    Swipe {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        duration_ms: u64,
    },
    /// Type text into the currently focused input field.
    TypeText { text: String },
    /// Press the back button.
    PressBack,
    /// Press the home button.
    PressHome,
    /// Scroll in the given direction at relative position.
    Scroll {
        direction: ScrollDirection,
        x: f64,
        y: f64,
    },
    /// Launch an application by package name.
    LaunchApp { package: String },
    /// Click an element identified by its text content.
    ClickByText { text: String },
    /// Click an element identified by its content description.
    ClickByDescription { description: String },
}

/// Cardinal scroll directions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

// ---------------------------------------------------------------------------
// Conductor state (runs on master)
// ---------------------------------------------------------------------------

/// Whether the conductor is actively capturing and broadcasting actions.
static CONDUCTOR_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Next sequence number for outgoing actions.
static ACTION_SEQ: AtomicU64 = AtomicU64::new(0);

/// Master screen dimensions for pixel-to-relative coordinate conversion.
static SCREEN_DIMS: Mutex<Option<(u32, u32)>> = Mutex::new(None);

/// Cluster ID the conductor is broadcasting to.
static CONDUCTOR_CLUSTER: Mutex<Option<String>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Puppet state (runs on workers)
// ---------------------------------------------------------------------------

/// Whether the puppet is actively receiving and replaying actions.
static PUPPET_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Worker screen dimensions for relative-to-pixel coordinate conversion.
static PUPPET_SCREEN_DIMS: Mutex<Option<(u32, u32)>> = Mutex::new(None);

/// Buffer for received actions awaiting replay.
static ACTION_BUFFER: Mutex<Option<VecDeque<PuppetAction>>> = Mutex::new(None);

/// Highest sequence number successfully replayed.
static LAST_SEQ: AtomicU64 = AtomicU64::new(0);

/// Total number of actions successfully replayed.
static ACTIONS_REPLAYED: AtomicU64 = AtomicU64::new(0);

/// Total number of actions that failed replay.
static ACTIONS_FAILED: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Coordinate conversion
// ---------------------------------------------------------------------------

/// Convert absolute pixel coordinates to relative (0.0-1.0).
fn to_relative(x_px: u32, y_px: u32, screen_w: u32, screen_h: u32) -> (f64, f64) {
    (
        x_px as f64 / screen_w as f64,
        y_px as f64 / screen_h as f64,
    )
}

/// Convert relative (0.0-1.0) coordinates to absolute pixels.
fn to_absolute(x_rel: f64, y_rel: f64, screen_w: u32, screen_h: u32) -> (u32, u32) {
    (
        (x_rel * screen_w as f64) as u32,
        (y_rel * screen_h as f64) as u32,
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the current time as epoch milliseconds.
fn current_time_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Publish a payload to an MQTT topic via the plugin context.
fn mqtt_publish(ctx: *const PluginContext, topic: &str, payload: &str) -> Result<(), String> {
    // SAFETY: `ctx` was validated non-null by the caller in `nebula_plugin_execute`.
    let ctx_ref = unsafe { &*ctx };

    let ret = (ctx_ref.publish)(
        ctx_ref.host_data,
        topic.as_ptr(),
        topic.len(),
        payload.as_ptr(),
        payload.len(),
    );

    if ret < 0 {
        return Err(format!("MQTT publish failed: {ret}"));
    }
    Ok(())
}

/// Subscribe to an MQTT topic via the plugin context.
fn mqtt_subscribe(ctx: *const PluginContext, topic: &str) -> Result<(), String> {
    // SAFETY: `ctx` was validated non-null by the caller in `nebula_plugin_execute`.
    let ctx_ref = unsafe { &*ctx };

    let ret = (ctx_ref.subscribe)(
        ctx_ref.host_data,
        topic.as_ptr(),
        topic.len(),
    );

    if ret < 0 {
        return Err(format!("MQTT subscribe failed: {ret}"));
    }
    Ok(())
}

/// Emit a log message at the given level via the plugin context.
fn log_msg(ctx: *const PluginContext, level: u8, msg: &str) {
    // SAFETY: `ctx` was validated non-null by the caller in `nebula_plugin_execute`.
    let ctx_ref = unsafe { &*ctx };

    let _ = (ctx_ref.log)(
        ctx_ref.host_data,
        level,
        msg.as_ptr(),
        msg.len(),
    );
}

// ---------------------------------------------------------------------------
// Conductor handlers (master side)
// ---------------------------------------------------------------------------

/// Start conductor mode -- the master begins capturing and broadcasting
/// user actions to all workers in the cluster.
///
/// Expected params:
/// ```json
/// {
///   "cluster_id": "my-cluster",
///   "screen_width": 1080,   // optional, auto-detected if omitted
///   "screen_height": 2400   // optional, auto-detected if omitted
/// }
/// ```
pub fn start_conductor(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let cluster_id = params["cluster_id"]
        .as_str()
        .ok_or_else(|| "Missing 'cluster_id' parameter".to_string())?;

    // Try to get screen dimensions from params, fall back to platform query.
    let (width, height) = match (
        params["screen_width"].as_u64(),
        params["screen_height"].as_u64(),
    ) {
        (Some(w), Some(h)) => (w as u32, h as u32),
        _ => {
            // Query the device for screen info.
            let screen_info =
                super::invoke(ctx, "android:screen:getScreenInfo", "{}")?;
            let info: serde_json::Value = serde_json::from_str(&screen_info)
                .map_err(|e| format!("Failed to parse screen info: {e}"))?;
            let w = info["width"].as_u64().unwrap_or(1080) as u32;
            let h = info["height"].as_u64().unwrap_or(1920) as u32;
            (w, h)
        }
    };

    // Store screen dimensions.
    {
        let mut guard = SCREEN_DIMS
            .lock()
            .map_err(|e| format!("Failed to acquire screen dims lock: {e}"))?;
        *guard = Some((width, height));
    }

    // Store cluster ID.
    {
        let mut guard = CONDUCTOR_CLUSTER
            .lock()
            .map_err(|e| format!("Failed to acquire cluster lock: {e}"))?;
        *guard = Some(cluster_id.to_string());
    }

    // Reset sequence counter.
    ACTION_SEQ.store(0, Ordering::SeqCst);

    // Activate conductor.
    CONDUCTOR_ACTIVE.store(true, Ordering::SeqCst);

    log_msg(ctx, 3, &format!(
        "Puppet conductor started for cluster '{cluster_id}' (screen: {width}x{height})"
    ));

    Ok(serde_json::json!({
        "status": "conductor_started",
        "cluster_id": cluster_id,
        "screen_width": width,
        "screen_height": height
    })
    .to_string())
}

/// Called by the engine/Kotlin side when the master's AccessibilityService
/// detects a user action. Converts absolute pixel coordinates to relative
/// (0.0-1.0) and broadcasts the action to all workers via MQTT.
///
/// Expected params:
/// ```json
/// {
///   "action_type": "Tap",
///   "x": 540,
///   "y": 1200,
///   "text": "hello",
///   "package": "com.example.app",
///   "description": "Send button",
///   "duration_ms": 300,
///   "x2": 540,
///   "y2": 600,
///   "direction": "Down",
///   "app_package": "com.example.app"
/// }
/// ```
pub fn capture_action(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    if !CONDUCTOR_ACTIVE.load(Ordering::SeqCst) {
        return Err("Conductor is not active".to_string());
    }

    let (screen_w, screen_h) = {
        let guard = SCREEN_DIMS
            .lock()
            .map_err(|e| format!("Failed to acquire screen dims lock: {e}"))?;
        guard.ok_or_else(|| "Screen dimensions not set".to_string())?
    };

    let cluster_id = {
        let guard = CONDUCTOR_CLUSTER
            .lock()
            .map_err(|e| format!("Failed to acquire cluster lock: {e}"))?;
        guard
            .as_ref()
            .ok_or_else(|| "Cluster ID not set".to_string())?
            .clone()
    };

    let action_type_str = params["action_type"]
        .as_str()
        .ok_or_else(|| "Missing 'action_type' parameter".to_string())?;

    let action_type = match action_type_str {
        "Tap" => {
            let x_px = params["x"].as_u64().unwrap_or(0) as u32;
            let y_px = params["y"].as_u64().unwrap_or(0) as u32;
            let (x, y) = to_relative(x_px, y_px, screen_w, screen_h);
            PuppetActionType::Tap { x, y }
        }
        "LongPress" => {
            let x_px = params["x"].as_u64().unwrap_or(0) as u32;
            let y_px = params["y"].as_u64().unwrap_or(0) as u32;
            let (x, y) = to_relative(x_px, y_px, screen_w, screen_h);
            PuppetActionType::LongPress { x, y }
        }
        "Swipe" => {
            let x1_px = params["x"].as_u64().unwrap_or(0) as u32;
            let y1_px = params["y"].as_u64().unwrap_or(0) as u32;
            let x2_px = params["x2"].as_u64().unwrap_or(0) as u32;
            let y2_px = params["y2"].as_u64().unwrap_or(0) as u32;
            let (x1, y1) = to_relative(x1_px, y1_px, screen_w, screen_h);
            let (x2, y2) = to_relative(x2_px, y2_px, screen_w, screen_h);
            let duration_ms = params["duration_ms"].as_u64().unwrap_or(300);
            PuppetActionType::Swipe {
                x1,
                y1,
                x2,
                y2,
                duration_ms,
            }
        }
        "TypeText" => {
            let text = params["text"]
                .as_str()
                .unwrap_or("")
                .to_string();
            PuppetActionType::TypeText { text }
        }
        "PressBack" => PuppetActionType::PressBack,
        "PressHome" => PuppetActionType::PressHome,
        "Scroll" => {
            let x_px = params["x"].as_u64().unwrap_or(0) as u32;
            let y_px = params["y"].as_u64().unwrap_or(0) as u32;
            let (x, y) = to_relative(x_px, y_px, screen_w, screen_h);
            let direction = match params["direction"].as_str().unwrap_or("Down") {
                "Up" => ScrollDirection::Up,
                "Down" => ScrollDirection::Down,
                "Left" => ScrollDirection::Left,
                "Right" => ScrollDirection::Right,
                _ => ScrollDirection::Down,
            };
            PuppetActionType::Scroll { direction, x, y }
        }
        "LaunchApp" => {
            let package = params["package"]
                .as_str()
                .unwrap_or("")
                .to_string();
            PuppetActionType::LaunchApp { package }
        }
        "ClickByText" => {
            let text = params["text"]
                .as_str()
                .unwrap_or("")
                .to_string();
            PuppetActionType::ClickByText { text }
        }
        "ClickByDescription" => {
            let description = params["description"]
                .as_str()
                .unwrap_or("")
                .to_string();
            PuppetActionType::ClickByDescription { description }
        }
        _ => return Err(format!("Unknown action type: {action_type_str}")),
    };

    let seq = ACTION_SEQ.fetch_add(1, Ordering::SeqCst);
    let app_package = params["app_package"].as_str().map(String::from);

    let puppet_action = PuppetAction {
        seq,
        timestamp_ms: current_time_ms(),
        action_type,
        app_package,
    };

    let action_json = serde_json::to_string(&puppet_action)
        .map_err(|e| format!("Failed to serialize puppet action: {e}"))?;

    let topic = format!("nebula/{cluster_id}/puppet/actions");
    mqtt_publish(ctx, &topic, &action_json)?;

    Ok(serde_json::json!({
        "status": "captured",
        "seq": seq,
        "action": action_json
    })
    .to_string())
}

/// Stop conductor mode and notify workers.
///
/// Expected params:
/// ```json
/// { "cluster_id": "my-cluster" }
/// ```
/// If `cluster_id` is omitted, uses the stored cluster ID from `start_conductor`.
pub fn stop_conductor(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let cluster_id = match params["cluster_id"].as_str() {
        Some(id) => id.to_string(),
        None => {
            let guard = CONDUCTOR_CLUSTER
                .lock()
                .map_err(|e| format!("Failed to acquire cluster lock: {e}"))?;
            guard
                .as_ref()
                .ok_or_else(|| "No active conductor session".to_string())?
                .clone()
        }
    };

    let final_seq = ACTION_SEQ.load(Ordering::SeqCst);
    CONDUCTOR_ACTIVE.store(false, Ordering::SeqCst);

    // Notify workers that the conductor has stopped.
    let control_msg = serde_json::json!({
        "command": "conductor_stopped",
        "final_seq": final_seq
    });
    let topic = format!("nebula/{cluster_id}/puppet/control");
    mqtt_publish(ctx, &topic, &control_msg.to_string())?;

    // Clear stored state.
    {
        let mut guard = SCREEN_DIMS
            .lock()
            .map_err(|e| format!("Failed to acquire screen dims lock: {e}"))?;
        *guard = None;
    }
    {
        let mut guard = CONDUCTOR_CLUSTER
            .lock()
            .map_err(|e| format!("Failed to acquire cluster lock: {e}"))?;
        *guard = None;
    }

    log_msg(ctx, 3, &format!(
        "Puppet conductor stopped for cluster '{cluster_id}' (total actions: {final_seq})"
    ));

    Ok(serde_json::json!({
        "status": "conductor_stopped",
        "cluster_id": cluster_id,
        "total_actions_broadcast": final_seq
    })
    .to_string())
}

/// Check whether the conductor is active and return its current state.
pub fn is_conductor_active(
    _ctx: *const PluginContext,
    _params: &serde_json::Value,
) -> Result<String, String> {
    let active = CONDUCTOR_ACTIVE.load(Ordering::SeqCst);
    let seq = ACTION_SEQ.load(Ordering::SeqCst);

    let (width, height) = {
        let guard = SCREEN_DIMS
            .lock()
            .map_err(|e| format!("Failed to acquire screen dims lock: {e}"))?;
        guard.unwrap_or((0, 0))
    };

    let cluster_id = {
        let guard = CONDUCTOR_CLUSTER
            .lock()
            .map_err(|e| format!("Failed to acquire cluster lock: {e}"))?;
        guard.clone().unwrap_or_default()
    };

    Ok(serde_json::json!({
        "active": active,
        "seq": seq,
        "screen_width": width,
        "screen_height": height,
        "cluster_id": cluster_id
    })
    .to_string())
}

// ---------------------------------------------------------------------------
// Puppet handlers (worker side)
// ---------------------------------------------------------------------------

/// Start puppet mode -- the worker begins receiving and replaying actions
/// broadcast by the conductor.
///
/// Expected params:
/// ```json
/// {
///   "cluster_id": "my-cluster",
///   "screen_width": 720,    // optional, auto-detected if omitted
///   "screen_height": 1600   // optional, auto-detected if omitted
/// }
/// ```
pub fn start_puppet(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let cluster_id = params["cluster_id"]
        .as_str()
        .ok_or_else(|| "Missing 'cluster_id' parameter".to_string())?;

    // Get worker's screen dimensions.
    let (width, height) = match (
        params["screen_width"].as_u64(),
        params["screen_height"].as_u64(),
    ) {
        (Some(w), Some(h)) => (w as u32, h as u32),
        _ => {
            let screen_info =
                super::invoke(ctx, "android:screen:getScreenInfo", "{}")?;
            let info: serde_json::Value = serde_json::from_str(&screen_info)
                .map_err(|e| format!("Failed to parse screen info: {e}"))?;
            let w = info["width"].as_u64().unwrap_or(1080) as u32;
            let h = info["height"].as_u64().unwrap_or(1920) as u32;
            (w, h)
        }
    };

    // Store worker screen dimensions.
    {
        let mut guard = PUPPET_SCREEN_DIMS
            .lock()
            .map_err(|e| format!("Failed to acquire puppet screen dims lock: {e}"))?;
        *guard = Some((width, height));
    }

    // Initialize the action buffer.
    {
        let mut guard = ACTION_BUFFER
            .lock()
            .map_err(|e| format!("Failed to acquire action buffer lock: {e}"))?;
        *guard = Some(VecDeque::new());
    }

    // Reset counters.
    LAST_SEQ.store(0, Ordering::SeqCst);
    ACTIONS_REPLAYED.store(0, Ordering::SeqCst);
    ACTIONS_FAILED.store(0, Ordering::SeqCst);

    // Subscribe to action broadcast and control topics.
    let actions_topic = format!("nebula/{cluster_id}/puppet/actions");
    let control_topic = format!("nebula/{cluster_id}/puppet/control");
    mqtt_subscribe(ctx, &actions_topic)?;
    mqtt_subscribe(ctx, &control_topic)?;

    // Activate puppet.
    PUPPET_ACTIVE.store(true, Ordering::SeqCst);

    log_msg(ctx, 3, &format!(
        "Puppet mode started for cluster '{cluster_id}' (screen: {width}x{height})"
    ));

    Ok(serde_json::json!({
        "status": "puppet_started",
        "cluster_id": cluster_id,
        "screen_width": width,
        "screen_height": height
    })
    .to_string())
}

/// Called when an MQTT message arrives on the `puppet/actions` topic.
/// The engine routes received MQTT messages to the plugin's `execute()`,
/// which dispatches to this handler.
///
/// Expected params: a serialized `PuppetAction` JSON object, typically
/// delivered as the `params` field of the execute envelope.
pub fn receive_action(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    if !PUPPET_ACTIVE.load(Ordering::SeqCst) {
        return Err("Puppet mode is not active".to_string());
    }

    let action: PuppetAction = serde_json::from_value(params.clone())
        .map_err(|e| format!("Failed to parse PuppetAction: {e}"))?;

    let (screen_w, screen_h) = {
        let guard = PUPPET_SCREEN_DIMS
            .lock()
            .map_err(|e| format!("Failed to acquire puppet screen dims lock: {e}"))?;
        guard.ok_or_else(|| "Worker screen dimensions not set".to_string())?
    };

    let seq = action.seq;
    let result = replay_action(ctx, &action, screen_w, screen_h);

    match &result {
        Ok(_) => {
            ACTIONS_REPLAYED.fetch_add(1, Ordering::SeqCst);
            LAST_SEQ.store(seq, Ordering::SeqCst);
        }
        Err(_) => {
            ACTIONS_FAILED.fetch_add(1, Ordering::SeqCst);
        }
    }

    result
}

/// Stop puppet mode and clear state.
pub fn stop_puppet(
    ctx: *const PluginContext,
    _params: &serde_json::Value,
) -> Result<String, String> {
    let replayed = ACTIONS_REPLAYED.load(Ordering::SeqCst);
    let failed = ACTIONS_FAILED.load(Ordering::SeqCst);

    PUPPET_ACTIVE.store(false, Ordering::SeqCst);

    // Clear the action buffer.
    {
        let mut guard = ACTION_BUFFER
            .lock()
            .map_err(|e| format!("Failed to acquire action buffer lock: {e}"))?;
        *guard = None;
    }

    // Clear screen dimensions.
    {
        let mut guard = PUPPET_SCREEN_DIMS
            .lock()
            .map_err(|e| format!("Failed to acquire puppet screen dims lock: {e}"))?;
        *guard = None;
    }

    log_msg(ctx, 3, &format!(
        "Puppet mode stopped (replayed: {replayed}, failed: {failed})"
    ));

    Ok(serde_json::json!({
        "status": "puppet_stopped",
        "actions_replayed": replayed,
        "actions_failed": failed
    })
    .to_string())
}

/// Get puppet mode status and statistics.
pub fn get_puppet_status(
    _ctx: *const PluginContext,
    _params: &serde_json::Value,
) -> Result<String, String> {
    let active = PUPPET_ACTIVE.load(Ordering::SeqCst);
    let replayed = ACTIONS_REPLAYED.load(Ordering::SeqCst);
    let failed = ACTIONS_FAILED.load(Ordering::SeqCst);
    let last_seq = LAST_SEQ.load(Ordering::SeqCst);

    let buffer_size = {
        let guard = ACTION_BUFFER
            .lock()
            .map_err(|e| format!("Failed to acquire action buffer lock: {e}"))?;
        guard.as_ref().map_or(0, VecDeque::len)
    };

    Ok(serde_json::json!({
        "active": active,
        "actions_replayed": replayed,
        "actions_failed": failed,
        "buffer_size": buffer_size,
        "last_seq": last_seq
    })
    .to_string())
}

/// Check whether puppet mode is active.
pub fn is_puppet_active(
    _ctx: *const PluginContext,
    _params: &serde_json::Value,
) -> Result<String, String> {
    Ok(serde_json::json!({
        "active": PUPPET_ACTIVE.load(Ordering::SeqCst)
    })
    .to_string())
}

// ---------------------------------------------------------------------------
// Action replay engine
// ---------------------------------------------------------------------------

/// Replay a single `PuppetAction` on the local device by converting relative
/// coordinates to absolute pixels and dispatching through `platform_invoke`.
fn replay_action(
    ctx: *const PluginContext,
    action: &PuppetAction,
    screen_w: u32,
    screen_h: u32,
) -> Result<String, String> {
    match &action.action_type {
        PuppetActionType::Tap { x, y } => {
            let (abs_x, abs_y) = to_absolute(*x, *y, screen_w, screen_h);
            let args = serde_json::json!({
                "action": "TAP",
                "x": abs_x,
                "y": abs_y
            });
            super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        PuppetActionType::LongPress { x, y } => {
            let (abs_x, abs_y) = to_absolute(*x, *y, screen_w, screen_h);
            let args = serde_json::json!({
                "action": "LONG_PRESS",
                "x": abs_x,
                "y": abs_y
            });
            super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        PuppetActionType::Swipe {
            x1,
            y1,
            x2,
            y2,
            duration_ms,
        } => {
            let (abs_x1, abs_y1) = to_absolute(*x1, *y1, screen_w, screen_h);
            let (abs_x2, abs_y2) = to_absolute(*x2, *y2, screen_w, screen_h);
            let args = serde_json::json!({
                "action": "SWIPE",
                "x1": abs_x1,
                "y1": abs_y1,
                "x2": abs_x2,
                "y2": abs_y2,
                "duration_ms": duration_ms
            });
            super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        PuppetActionType::TypeText { text } => {
            let args = serde_json::json!({ "text": text });
            super::invoke(ctx, "android:accessibility:performText", &args.to_string())
        }
        PuppetActionType::PressBack => {
            let args = serde_json::json!({ "action": "GLOBAL_ACTION_BACK" });
            super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        PuppetActionType::PressHome => {
            let args = serde_json::json!({ "action": "GLOBAL_ACTION_HOME" });
            super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        PuppetActionType::Scroll { direction, x, y } => {
            let (abs_x, abs_y) = to_absolute(*x, *y, screen_w, screen_h);
            let scroll_action = match direction {
                ScrollDirection::Up => "ACTION_SCROLL_BACKWARD",
                ScrollDirection::Down => "ACTION_SCROLL_FORWARD",
                ScrollDirection::Left => "ACTION_SCROLL_LEFT",
                ScrollDirection::Right => "ACTION_SCROLL_RIGHT",
            };
            let dir_str = match direction {
                ScrollDirection::Up => "up",
                ScrollDirection::Down => "down",
                ScrollDirection::Left => "left",
                ScrollDirection::Right => "right",
            };
            let args = serde_json::json!({
                "action": scroll_action,
                "x": abs_x,
                "y": abs_y,
                "direction": dir_str
            });
            super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        PuppetActionType::LaunchApp { package } => {
            let args = serde_json::json!({ "packageName": package });
            super::invoke(ctx, "android:apps:launchApp", &args.to_string())
        }
        PuppetActionType::ClickByText { text } => {
            let args = serde_json::json!({
                "action": "CLICK_BY_TEXT",
                "text": text
            });
            super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
        PuppetActionType::ClickByDescription { description } => {
            let args = serde_json::json!({
                "action": "CLICK_BY_DESCRIPTION",
                "contentDescription": description
            });
            super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
        }
    }
}
