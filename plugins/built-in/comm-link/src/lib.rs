//! Comm-Link plugin for NEBULA.
//!
//! Full DroidRelay-grade communications orchestration: SMS with priority
//! queuing, per-SIM rate limiting, multipart delivery tracking, retry with
//! exponential backoff, scheduled sends, and 24-hour expiration. USSD with
//! both single-step (`TelephonyManager`) and multi-step (`AccessibilityService`)
//! modes. Call triggering with auto-hangup. SIM management with round-robin
//! rotation.
//!
//! All platform interaction routes through `common::invoke` which calls the
//! host engine's `platform_invoke` function pointer. This plugin is entirely
//! self-contained — no engine dependency beyond the SDK.

mod calls;
mod common;
mod sim;
mod sms;
mod ussd;

use nebula_plugin_sdk::context::PluginContext;
use std::ffi::CString;
use std::sync::atomic::Ordering;

// ---------------------------------------------------------------------------
// ABI exports
// ---------------------------------------------------------------------------

/// Initialize the plugin by storing the host-provided context and setting up
/// orchestration state (queue, rate limiter, delivery tracker, retry policy).
///
/// # Safety
///
/// `ctx` must be a valid pointer to a `PluginContext` whose lifetime spans
/// from this call until `nebula_plugin_shutdown` completes. The engine
/// guarantees this by keeping the context alive in `LoadedPlugin`.
#[no_mangle]
pub extern "C" fn nebula_plugin_init(ctx: *const PluginContext) -> i32 {
    common::CTX.store(ctx as *mut PluginContext, Ordering::SeqCst);

    // Initialize orchestration subsystems.
    sms::init();
    sim::init();

    common::log(common::log_level::INFO, "Comm-Link plugin initialized");
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
    let ctx = common::CTX.load(Ordering::SeqCst);
    if ctx.is_null() {
        return -1;
    }

    // SAFETY: `input_ptr` is valid for `input_len` bytes as guaranteed by the
    // engine's calling convention.
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

    let result = dispatch(action, params);

    write_result(result, output_ptr, output_len)
}

/// Shut down the plugin and release the stored context pointer.
///
/// # Safety
///
/// After this call returns, no further calls to `execute` will be made by the
/// engine. The `AtomicPtr` is set to null to prevent use-after-free.
#[no_mangle]
pub extern "C" fn nebula_plugin_shutdown() -> i32 {
    common::log(common::log_level::INFO, "Comm-Link plugin shutting down");
    common::CTX.store(std::ptr::null_mut(), Ordering::SeqCst);
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
        "id": "com.nebula.comm-link",
        "name": "Comm-Link",
        "version": "2.0.0",
        "capabilities": ["Sms", "Ussd", "Telephony"],
        "description": "Full communications orchestration: SMS queuing, USSD sessions, calls, SIM management"
    });
    let c_str = CString::new(info.to_string()).unwrap_or_default();
    // Leak intentionally: the engine expects a pointer valid for the plugin's
    // entire lifetime. Since `nebula_plugin_info` is called once at load time,
    // one allocation is acceptable.
    c_str.into_raw() as *const std::ffi::c_char
}

// ---------------------------------------------------------------------------
// Action dispatch
// ---------------------------------------------------------------------------

/// Route an action string to the appropriate module function.
fn dispatch(action: &str, params: &serde_json::Value) -> Result<String, String> {
    match action {
        // -- SMS orchestration -----------------------------------------------
        "submitSms" => sms::submit_sms(params),
        "sendSmsImmediate" => sms::send_sms_immediate(params),
        "getSmsStatus" => sms::get_sms_status(params),
        "cancelSms" => sms::cancel_sms(params),
        "processQueue" => sms::process_queue(params),
        "expireStale" => sms::expire_stale(params),
        "getQueueStats" => sms::get_queue_stats(params),
        "getReceivedSms" => sms::get_received_sms(params),
        "clearReceivedSms" => sms::clear_received_sms(params),
        "readSmsInbox" => sms::read_sms_inbox(params),

        // -- USSD orchestration ----------------------------------------------
        "executeUssd" => ussd::execute_ussd(params),
        "startUssdSession" => ussd::start_ussd_session(params),
        "sendUssdReply" => ussd::send_ussd_reply(params),
        "cancelUssdSession" => ussd::cancel_ussd_session(params),
        "isSessionActive" => ussd::is_session_active(params),

        // -- Call orchestration ----------------------------------------------
        "triggerCall" => calls::trigger_call(params),
        "endCall" => calls::end_call(params),
        "readCallLog" => calls::read_call_log(params),
        "getPhoneState" => calls::get_phone_state(params),

        // -- SIM management --------------------------------------------------
        "getSimCards" => sim::get_sim_cards(params),
        "getSignalStrength" => sim::get_signal_strength(params),
        "setDefaultSim" => sim::set_default_sim(params),
        "resolveSimSlot" => sim::resolve_sim_slot(params),
        "enableRotation" => sim::enable_rotation(params),

        // -- Legacy aliases (backward compatibility with v1 thin passthrough) -
        "sendSms" => {
            // Map old `sendSms` action to `sendSmsImmediate` with field rename.
            let compat_params = serde_json::json!({
                "to": params["phone"],
                "content": params["message"],
                "simSlot": params["simSlot"],
            });
            sms::send_sms_immediate(&compat_params)
        }
        "setDefaultSimSlot" => {
            // Map old name to new name.
            sim::set_default_sim(params)
        }

        _ => Err(format!("Unknown action: {action}")),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Serialize a `Result<String, String>` into the output buffer as JSON.
///
/// On success, writes the raw JSON string and returns the byte count.
/// On error, writes `{"error": "..."}` and returns the negative byte count.
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
            // SAFETY: same as above.
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

    #[test]
    fn test_dispatch_unknown_action() {
        let result = dispatch("nonexistent_action_xyz", &serde_json::json!({}));
        assert!(result.is_err());
    }
}
