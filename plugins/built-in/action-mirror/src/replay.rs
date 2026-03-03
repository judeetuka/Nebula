//! Action replay module for the Action Mirror plugin.
//!
//! Executes previously recorded macros by dispatching each action through
//! the remote control module. Tracks playback state for status queries and
//! supports interruption.

use nebula_plugin_sdk::context::PluginContext;

use crate::recorder::{RecordedAction, RemoteAction};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Playback state tracker.
pub struct MacroPlayer {
    pub playing: bool,
    pub current_step: u32,
    pub total_steps: u32,
}

impl MacroPlayer {
    pub fn new() -> Self {
        Self {
            playing: false,
            current_step: 0,
            total_steps: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Execute a macro from a JSON array of `RecordedAction` steps.
pub fn handle_play_macro(
    ctx: *const PluginContext,
    player: &mut MacroPlayer,
    params: &serde_json::Value,
) -> Result<String, String> {
    let macro_json = params["macro_json"]
        .as_str()
        .ok_or_else(|| "Missing 'macro_json' parameter".to_string())?;

    let actions: Vec<RecordedAction> = serde_json::from_str(macro_json)
        .map_err(|e| format!("Failed to parse macro JSON: {e}"))?;

    execute_actions(ctx, player, &actions)
}

/// Load a macro by name from the state store and execute it.
pub fn handle_play_macro_by_name(
    ctx: *const PluginContext,
    player: &mut MacroPlayer,
    params: &serde_json::Value,
) -> Result<String, String> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| "Missing 'name' parameter".to_string())?;

    let macro_json =
        crate::recorder::handle_get_macro(ctx, &serde_json::json!({ "name": name }))?;

    let actions: Vec<RecordedAction> = serde_json::from_str(&macro_json)
        .map_err(|e| format!("Failed to parse stored macro: {e}"))?;

    execute_actions(ctx, player, &actions)
}

/// Interrupt the current playback.
pub fn handle_stop_playback(player: &mut MacroPlayer) -> Result<String, String> {
    let was_playing = player.playing;
    let stopped_at = player.current_step;
    player.playing = false;

    Ok(serde_json::json!({
        "status": "stopped",
        "was_playing": was_playing,
        "stopped_at_step": stopped_at,
        "total_steps": player.total_steps
    })
    .to_string())
}

/// Return the current playback status.
pub fn handle_get_playback_status(player: &MacroPlayer) -> Result<String, String> {
    Ok(serde_json::json!({
        "playing": player.playing,
        "current_step": player.current_step,
        "total_steps": player.total_steps
    })
    .to_string())
}

// ---------------------------------------------------------------------------
// Execution engine
// ---------------------------------------------------------------------------

/// Execute a sequence of recorded actions, dispatching each through the
/// appropriate remote control handler.
fn execute_actions(
    ctx: *const PluginContext,
    player: &mut MacroPlayer,
    actions: &[RecordedAction],
) -> Result<String, String> {
    player.playing = true;
    player.current_step = 0;
    player.total_steps = actions.len() as u32;

    let mut results = Vec::with_capacity(actions.len());

    for action in actions {
        if !player.playing {
            // Playback was interrupted.
            break;
        }

        player.current_step = action.step + 1;
        let result = dispatch_action(ctx, &action.action);

        results.push(serde_json::json!({
            "step": action.step,
            "success": result.is_ok(),
            "error": result.err()
        }));
    }

    player.playing = false;

    Ok(serde_json::json!({
        "status": "completed",
        "steps_executed": results.len(),
        "total_steps": player.total_steps,
        "results": results
    })
    .to_string())
}

/// Dispatch a single `RemoteAction` to the appropriate platform invoke call.
fn dispatch_action(
    ctx: *const PluginContext,
    action: &RemoteAction,
) -> Result<String, String> {
    match action {
        RemoteAction::Tap { x, y } => {
            let params = serde_json::json!({ "x": x, "y": y });
            crate::remote::handle_remote_tap(ctx, &params)
        }
        RemoteAction::Swipe {
            x1,
            y1,
            x2,
            y2,
            duration_ms,
        } => {
            let params = serde_json::json!({
                "x1": x1, "y1": y1,
                "x2": x2, "y2": y2,
                "duration_ms": duration_ms
            });
            crate::remote::handle_remote_swipe(ctx, &params)
        }
        RemoteAction::Type { text } => {
            let params = serde_json::json!({ "text": text });
            crate::remote::handle_remote_type(ctx, &params)
        }
        RemoteAction::PressBack => crate::remote::handle_remote_press_back(ctx),
        RemoteAction::PressHome => crate::remote::handle_remote_press_home(ctx),
        RemoteAction::Scroll { direction, x, y } => {
            let params = serde_json::json!({
                "direction": direction,
                "x": x,
                "y": y
            });
            crate::remote::handle_remote_scroll(ctx, &params)
        }
        RemoteAction::WaitForElement {
            selector,
            timeout_ms: _,
        } => {
            // WaitForElement is advisory in synchronous context -- we poll once.
            let _screen = super::invoke(ctx, "android:accessibility:getScreenContent", "{}")?;
            Ok(serde_json::json!({
                "action": "WaitForElement",
                "selector": selector,
                "status": "checked"
            })
            .to_string())
        }
        RemoteAction::LaunchApp { package } => {
            let args = serde_json::json!({ "packageName": package });
            super::invoke(ctx, "android:apps:launchApp", &args.to_string())
        }
        RemoteAction::Wait { ms } => {
            // In a synchronous plugin, waits are advisory.
            Ok(serde_json::json!({
                "action": "Wait",
                "ms": ms,
                "status": "skipped_sync"
            })
            .to_string())
        }
    }
}
