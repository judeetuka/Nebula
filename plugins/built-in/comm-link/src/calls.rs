//! Call orchestration for the Comm-Link plugin.
//!
//! Provides call triggering (with optional auto-hangup for flash calls),
//! active call termination, call log reading, and phone state queries.
//! All operations route through `common::invoke` to the Android
//! `TelecomManager` / `TelephonyManager` via the platform bridge.

use crate::common;
use crate::sim;

// -------------------------------------------------------------------------
// Actions
// -------------------------------------------------------------------------

/// Trigger an outgoing call.
///
/// Params:
/// - `number` (string, required): phone number to call
/// - `durationMs` (integer, optional): auto-hangup after this many ms
///   (e.g., 2000 for a flash call / missed call pattern)
/// - `simSlot` (integer, optional): SIM slot override
pub fn trigger_call(params: &serde_json::Value) -> Result<String, String> {
    let number = params["number"]
        .as_str()
        .ok_or("missing 'number' parameter")?;

    let duration_ms = params["durationMs"].as_i64().or(params["autoHangupMs"].as_i64());
    let sim_slot = params["simSlot"].as_i64().map(|s| s as i32);
    let resolved_slot = sim::resolve_sim_slot_internal(sim_slot);

    let args = serde_json::json!({
        "number": number,
        "autoHangupMs": duration_ms,
        "simSlot": resolved_slot,
    })
    .to_string();

    common::log(
        common::log_level::INFO,
        &format!(
            "Call trigger: {number} via sim{resolved_slot}{}",
            duration_ms
                .map(|d| format!(", auto-hangup {d}ms"))
                .unwrap_or_default()
        ),
    );

    common::invoke("android:calls:triggerCall", &args)
}

/// End the currently active call.
pub fn end_call(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;
    common::log(common::log_level::INFO, "Call end requested");
    common::invoke("android:calls:endCall", "{}")
}

/// Read entries from the device call log.
///
/// Params:
/// - `type` (string, optional): filter by call type ("incoming", "outgoing", "missed")
/// - `limit` (integer, optional): max entries to return
pub fn read_call_log(params: &serde_json::Value) -> Result<String, String> {
    let args = serde_json::json!({
        "type": params["type"],
        "limit": params["limit"],
    })
    .to_string();

    common::invoke("android:calls:readCallLog", &args)
}

/// Get the current telephony state (idle, ringing, offhook).
pub fn get_phone_state(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;
    common::invoke("android:telephony:getPhoneState", "{}")
}
