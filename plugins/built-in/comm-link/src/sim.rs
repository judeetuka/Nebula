//! SIM card management for the Comm-Link plugin.
//!
//! Handles SIM slot resolution (round-robin rotation or explicit selection),
//! signal strength queries, and default SIM persistence. Used by both `sms`
//! and `ussd` modules to determine which SIM slot to use before invoking
//! platform capabilities.

use crate::common;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Mutex;

/// Default SIM slot used when no explicit slot is requested and rotation is
/// disabled. Persisted across restarts via the host state store.
static DEFAULT_SIM: AtomicI32 = AtomicI32::new(0);

/// Tracks which SIM was last used for round-robin rotation.
static LAST_USED_SIM: AtomicI32 = AtomicI32::new(0);

/// When enabled, `resolve_sim_slot` cycles through available SIMs instead of
/// always returning `DEFAULT_SIM`.
static ROTATION_ENABLED: AtomicBool = AtomicBool::new(false);

/// Known SIM count (updated when `get_sim_cards` is called). Used by the
/// rotation logic to wrap around. Defaults to 1 (single SIM).
static SIM_COUNT: AtomicI32 = AtomicI32::new(1);

/// Mutex guard for SIM count updates during slot enumeration.
static SIM_COUNT_LOCK: Mutex<()> = Mutex::new(());

/// Restore persisted SIM preferences from the host state store.
pub fn init() {
    if let Ok(Some(val)) = common::get_state("sim:default_slot") {
        if let Ok(slot) = val.parse::<i32>() {
            DEFAULT_SIM.store(slot, Ordering::SeqCst);
        }
    }
    if let Ok(Some(val)) = common::get_state("sim:rotation_enabled") {
        ROTATION_ENABLED.store(val == "true", Ordering::SeqCst);
    }
    if let Ok(Some(val)) = common::get_state("sim:last_used") {
        if let Ok(slot) = val.parse::<i32>() {
            LAST_USED_SIM.store(slot, Ordering::SeqCst);
        }
    }
}

// -------------------------------------------------------------------------
// Actions
// -------------------------------------------------------------------------

/// Query the platform for available SIM cards and update the internal SIM
/// count used by rotation logic.
pub fn get_sim_cards(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;
    let result = common::invoke("android:sim:getSimCards", "{}")?;

    // Try to extract the SIM count from the platform response.
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result) {
        if let Some(arr) = parsed.as_array() {
            let _lock = SIM_COUNT_LOCK.lock().map_err(|e| e.to_string())?;
            SIM_COUNT.store(arr.len().max(1) as i32, Ordering::SeqCst);
        }
    }

    Ok(result)
}

/// Query the signal strength for all SIM slots.
pub fn get_signal_strength(params: &serde_json::Value) -> Result<String, String> {
    let _ = params;
    common::invoke("android:sim:getSignalStrength", "{}")
}

/// Set the default SIM slot and persist it.
pub fn set_default_sim(params: &serde_json::Value) -> Result<String, String> {
    let slot = params["slot"]
        .as_i64()
        .ok_or_else(|| "missing 'slot' parameter".to_string())? as i32;

    DEFAULT_SIM.store(slot, Ordering::SeqCst);
    let _ = common::set_state("sim:default_slot", &slot.to_string());

    common::log(
        common::log_level::INFO,
        &format!("Default SIM slot set to {slot}"),
    );

    Ok(serde_json::json!({"defaultSim": slot}).to_string())
}

/// Resolve which SIM slot to use.
///
/// Priority:
/// 1. If a specific slot is provided in the request, use it.
/// 2. If rotation is enabled, advance to the next SIM.
/// 3. Otherwise, use the default SIM.
///
/// This function is called internally by `sms` and `ussd` modules, as well as
/// exposed as an action for diagnostic purposes.
pub fn resolve_sim_slot(params: &serde_json::Value) -> Result<String, String> {
    let slot = resolve_sim_slot_internal(params["slot"].as_i64().map(|s| s as i32));
    Ok(serde_json::json!({"resolvedSlot": slot}).to_string())
}

/// Internal resolution logic used by other modules.
pub fn resolve_sim_slot_internal(requested: Option<i32>) -> i32 {
    if let Some(slot) = requested {
        return slot;
    }

    if ROTATION_ENABLED.load(Ordering::SeqCst) {
        let count = SIM_COUNT.load(Ordering::SeqCst).max(1);
        let last = LAST_USED_SIM.load(Ordering::SeqCst);
        let next = (last + 1) % count;
        LAST_USED_SIM.store(next, Ordering::SeqCst);
        let _ = common::set_state("sim:last_used", &next.to_string());
        return next;
    }

    DEFAULT_SIM.load(Ordering::SeqCst)
}

/// Enable or disable round-robin SIM rotation.
pub fn enable_rotation(params: &serde_json::Value) -> Result<String, String> {
    let enabled = params["enabled"]
        .as_bool()
        .ok_or_else(|| "missing 'enabled' parameter".to_string())?;

    ROTATION_ENABLED.store(enabled, Ordering::SeqCst);
    let _ = common::set_state("sim:rotation_enabled", if enabled { "true" } else { "false" });

    common::log(
        common::log_level::INFO,
        &format!("SIM rotation {}", if enabled { "enabled" } else { "disabled" }),
    );

    Ok(serde_json::json!({"rotationEnabled": enabled}).to_string())
}
