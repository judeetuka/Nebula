//! USSD orchestration for the Comm-Link plugin.
//!
//! Supports two modes of USSD interaction:
//!
//! 1. **Single-step** (`executeUssd`): Fire-and-forget via `TelephonyManager`.
//!    Sends a USSD code and returns the carrier response directly. No session
//!    state is maintained.
//!
//! 2. **Multi-step** (`startUssdSession` / `sendUssdReply` / `cancelUssdSession`):
//!    Uses the Android `AccessibilityService` path to maintain an interactive
//!    USSD session. The plugin tracks session state to prevent overlapping
//!    sessions and ensure clean teardown.
//!
//! Both modes use `sim::resolve_sim_slot_internal` to determine the SIM slot.

use crate::common;
use crate::sim;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// Whether a multi-step USSD session is currently active.
static SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The SIM slot used by the active session (-1 = none).
static SESSION_SIM_SLOT: AtomicI32 = AtomicI32::new(-1);

// -------------------------------------------------------------------------
// Actions
// -------------------------------------------------------------------------

/// Execute a single-step USSD code and return the carrier response.
///
/// Params:
/// - `code` (string, required): USSD code (e.g., "*123#")
/// - `simSlot` (integer, optional): SIM slot override
///
/// Uses `TelephonyManager.sendUssdRequest()` under the hood. Does not
/// interfere with multi-step sessions.
pub fn execute_ussd(params: &serde_json::Value) -> Result<String, String> {
    let code = params["code"]
        .as_str()
        .ok_or("missing 'code' parameter")?;

    let sim_slot = params["simSlot"].as_i64().map(|s| s as i32);
    let resolved_slot = sim::resolve_sim_slot_internal(sim_slot);

    let args = serde_json::json!({
        "code": code,
        "simSlot": resolved_slot,
    })
    .to_string();

    common::log(
        common::log_level::DEBUG,
        &format!("USSD execute: {code} via sim{resolved_slot}"),
    );

    common::invoke("android:telephony:executeUssd", &args)
}

/// Start a multi-step USSD session.
///
/// Params:
/// - `code` (string, required): initial USSD code
/// - `simSlot` (integer, optional): SIM slot override
///
/// Returns the first carrier response. The session remains active until
/// `cancelUssdSession` is called or the carrier terminates it.
///
/// Fails if a session is already active.
pub fn start_ussd_session(params: &serde_json::Value) -> Result<String, String> {
    if SESSION_ACTIVE.load(Ordering::SeqCst) {
        return Err("USSD session already active".to_string());
    }

    let code = params["code"]
        .as_str()
        .ok_or("missing 'code' parameter")?;

    let sim_slot = params["simSlot"].as_i64().map(|s| s as i32);
    let resolved_slot = sim::resolve_sim_slot_internal(sim_slot);

    let args = serde_json::json!({
        "code": code,
        "simSlot": resolved_slot,
    })
    .to_string();

    common::log(
        common::log_level::INFO,
        &format!("USSD session starting: {code} via sim{resolved_slot}"),
    );

    let result = common::invoke("android:ussd:startUssdSession", &args)?;

    SESSION_ACTIVE.store(true, Ordering::SeqCst);
    SESSION_SIM_SLOT.store(resolved_slot, Ordering::SeqCst);

    Ok(result)
}

/// Send a reply within an active multi-step USSD session.
///
/// Params:
/// - `text` (string, required): reply text (e.g., "1" for menu option 1)
///
/// Fails if no session is currently active.
pub fn send_ussd_reply(params: &serde_json::Value) -> Result<String, String> {
    if !SESSION_ACTIVE.load(Ordering::SeqCst) {
        return Err("No active USSD session".to_string());
    }

    let text = params["text"]
        .as_str()
        .ok_or("missing 'text' parameter")?;

    let args = serde_json::json!({ "text": text }).to_string();

    common::log(
        common::log_level::DEBUG,
        &format!("USSD reply: {text}"),
    );

    let result = common::invoke("android:ussd:sendUssdReply", &args);

    // If the platform indicates the session ended (invoke failure or
    // explicit termination in the response), clean up state.
    if result.is_err() {
        cleanup_session();
    }

    result
}

/// Cancel the active multi-step USSD session.
///
/// Safe to call even if no session is active (no-op).
pub fn cancel_ussd_session(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;

    if !SESSION_ACTIVE.load(Ordering::SeqCst) {
        return Ok(serde_json::json!({"active": false}).to_string());
    }

    common::log(common::log_level::INFO, "USSD session cancelling");

    let result = common::invoke("android:ussd:cancelUssdSession", "{}");

    cleanup_session();

    match result {
        Ok(r) => Ok(r),
        Err(_) => {
            // Session cleanup succeeded locally even if platform call failed.
            Ok(serde_json::json!({"cancelled": true}).to_string())
        }
    }
}

/// Check whether a multi-step USSD session is currently active.
pub fn is_session_active(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;

    let active = SESSION_ACTIVE.load(Ordering::SeqCst);
    let sim_slot = if active {
        SESSION_SIM_SLOT.load(Ordering::SeqCst)
    } else {
        -1
    };

    Ok(serde_json::json!({
        "active": active,
        "simSlot": sim_slot,
    })
    .to_string())
}

// -------------------------------------------------------------------------
// Internal helpers
// -------------------------------------------------------------------------

/// Reset session state atomics.
fn cleanup_session() {
    SESSION_ACTIVE.store(false, Ordering::SeqCst);
    SESSION_SIM_SLOT.store(-1, Ordering::SeqCst);
}
