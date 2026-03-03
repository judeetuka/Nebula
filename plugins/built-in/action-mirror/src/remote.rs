//! Remote control module for the Action Mirror plugin.
//!
//! Receives input commands from the admin dashboard and executes them on the
//! device via the accessibility service. Uses `platform_invoke` with the
//! `android:accessibility` capability for element-based interactions.

use nebula_plugin_sdk::context::PluginContext;

/// Execute a remote tap at the given coordinates.
///
/// Uses accessibility service's performClick with coordinate-based targeting.
/// Falls back to element-based clicking if coordinate dispatch is unavailable.
pub fn handle_remote_tap(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let x = params["x"].as_f64().unwrap_or(0.0);
    let y = params["y"].as_f64().unwrap_or(0.0);

    let args = serde_json::json!({
        "action": "TAP",
        "x": x,
        "y": y
    });
    super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
}

/// Execute a remote swipe gesture between two points.
pub fn handle_remote_swipe(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let x1 = params["x1"].as_f64().unwrap_or(0.0);
    let y1 = params["y1"].as_f64().unwrap_or(0.0);
    let x2 = params["x2"].as_f64().unwrap_or(0.0);
    let y2 = params["y2"].as_f64().unwrap_or(0.0);
    let duration_ms = params["duration_ms"].as_u64().unwrap_or(300);

    let args = serde_json::json!({
        "action": "SWIPE",
        "x1": x1,
        "y1": y1,
        "x2": x2,
        "y2": y2,
        "duration_ms": duration_ms
    });
    super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
}

/// Type text into the currently focused input field.
pub fn handle_remote_type(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let text = params["text"].as_str().unwrap_or("");

    let args = serde_json::json!({
        "text": text
    });
    super::invoke(ctx, "android:accessibility:performText", &args.to_string())
}

/// Execute the global BACK action.
pub fn handle_remote_press_back(ctx: *const PluginContext) -> Result<String, String> {
    let args = serde_json::json!({ "action": "GLOBAL_ACTION_BACK" });
    super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
}

/// Execute the global HOME action.
pub fn handle_remote_press_home(ctx: *const PluginContext) -> Result<String, String> {
    let args = serde_json::json!({ "action": "GLOBAL_ACTION_HOME" });
    super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
}

/// Execute a scroll at the given position in the specified direction.
pub fn handle_remote_scroll(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let direction = params["direction"].as_str().unwrap_or("down");
    let x = params["x"].as_f64().unwrap_or(0.0);
    let y = params["y"].as_f64().unwrap_or(0.0);

    let scroll_action = match direction {
        "up" => "ACTION_SCROLL_BACKWARD",
        "down" => "ACTION_SCROLL_FORWARD",
        "left" => "ACTION_SCROLL_LEFT",
        "right" => "ACTION_SCROLL_RIGHT",
        _ => "ACTION_SCROLL_FORWARD",
    };

    let args = serde_json::json!({
        "action": scroll_action,
        "x": x,
        "y": y,
        "direction": direction
    });
    super::invoke(ctx, "android:accessibility:performClick", &args.to_string())
}
