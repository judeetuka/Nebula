//! Comm-Link plugin for NEBULA.
//!
//! Provides SMS, USSD, telephony, call management, and SIM card operations
//! by routing all calls through the host engine's `platform_invoke` to the
//! Android `NebulaPlatformBridge`.

use nebula_plugin_sdk::context::PluginContext;
use std::ffi::CString;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Global plugin context pointer, set during `nebula_plugin_init` and cleared
/// during `nebula_plugin_shutdown`. Accessed atomically because the engine may
/// call `execute` from different threads.
static CTX: AtomicPtr<PluginContext> = AtomicPtr::new(std::ptr::null_mut());

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
        "sendSms" => {
            let args = serde_json::json!({
                "phone": params["phone"],
                "message": params["message"]
            });
            invoke(ctx, "android:telephony:sendSms", &args.to_string())
        }
        "readSmsInbox" => {
            let args = serde_json::json!({ "limit": params["limit"] });
            invoke(ctx, "android:telephony:readSmsInbox", &args.to_string())
        }
        "getReceivedSms" => {
            invoke(ctx, "android:sms:getReceivedSms", "{}")
        }
        "clearReceivedSms" => {
            invoke(ctx, "android:sms:clearReceivedSms", "{}")
        }
        "executeUssd" => {
            let args = serde_json::json!({ "code": params["code"] });
            invoke(ctx, "android:telephony:executeUssd", &args.to_string())
        }
        "startUssdSession" => {
            let args = serde_json::json!({
                "code": params["code"],
                "simSlot": params["simSlot"]
            });
            invoke(ctx, "android:ussd:startUssdSession", &args.to_string())
        }
        "sendUssdReply" => {
            let args = serde_json::json!({ "text": params["text"] });
            invoke(ctx, "android:ussd:sendUssdReply", &args.to_string())
        }
        "cancelUssdSession" => {
            invoke(ctx, "android:ussd:cancelUssdSession", "{}")
        }
        "triggerCall" => {
            let args = serde_json::json!({
                "number": params["number"],
                "autoHangupMs": params["autoHangupMs"],
                "simSlot": params["simSlot"]
            });
            invoke(ctx, "android:calls:triggerCall", &args.to_string())
        }
        "endCall" => {
            invoke(ctx, "android:calls:endCall", "{}")
        }
        "readCallLog" => {
            let args = serde_json::json!({
                "type": params["type"],
                "limit": params["limit"]
            });
            invoke(ctx, "android:calls:readCallLog", &args.to_string())
        }
        "getPhoneState" => {
            invoke(ctx, "android:telephony:getPhoneState", "{}")
        }
        "getSimCards" => {
            invoke(ctx, "android:sim:getSimCards", "{}")
        }
        "getSignalStrength" => {
            invoke(ctx, "android:sim:getSignalStrength", "{}")
        }
        "setDefaultSimSlot" => {
            let args = serde_json::json!({ "slot": params["slot"] });
            invoke(ctx, "android:sim:setDefaultSimSlot", &args.to_string())
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
    0
}

/// Return a null-terminated JSON string describing this plugin's manifest.
///
/// The returned pointer is to a static `CString` that lives for the duration
/// of the process. The engine reads this to discover the plugin's identity
/// and required capabilities without needing a separate manifest file.
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
        "version": "1.0.0",
        "capabilities": ["Sms", "Ussd", "Telephony"]
    });
    let c_str = CString::new(info.to_string()).unwrap_or_default();
    // Leak intentionally: the engine expects a pointer valid for the plugin's
    // entire lifetime. Since `nebula_plugin_info` is called once at load time,
    // one allocation is acceptable.
    c_str.into_raw() as *const std::ffi::c_char
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Call `platform_invoke` on the host engine with the given capability routing
/// string and JSON arguments.
///
/// The `capability` string follows the `"android:<service>:<method>"` routing
/// convention documented in `PluginContext`. The engine's `InvokeRouter`
/// extracts the service and method from this string.
fn invoke(ctx: *const PluginContext, capability: &str, args: &str) -> Result<String, String> {
    // SAFETY: `ctx` was set in `nebula_plugin_init` and the engine guarantees
    // it remains valid until `nebula_plugin_shutdown`. We verified `ctx` is
    // non-null at the top of `execute`.
    let ctx_ref = unsafe { &*ctx };

    // The `method` parameter in `platform_invoke` is ignored for android:
    // targets (the engine extracts the method from the capability string).
    // We pass an empty slice for consistency.
    let method = "";

    let mut result_buf = vec![0u8; 65536]; // 64 KiB buffer
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
            // SAFETY: same as above — `output_ptr` is valid for `output_len` bytes.
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), output_ptr, copy_len);
            }
            -(copy_len as i32)
        }
    }
}
