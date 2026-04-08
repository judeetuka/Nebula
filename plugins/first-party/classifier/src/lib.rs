//! Classifier plugin for NEBULA.
//!
//! Rule-based text classification engine for detecting transaction messages,
//! spam, OTPs, and other categories. Includes extraction utilities for
//! currency amounts, phone numbers, and transaction references.
//!
//! The default engine uses weighted keyword/pattern matching. Custom rules
//! can be added and are persisted via plugin state.

mod common;
mod engine;

use nebula_plugin_sdk::context::PluginContext;
use std::ffi::CString;
use std::sync::atomic::Ordering;

// ---------------------------------------------------------------------------
// ABI exports
// ---------------------------------------------------------------------------

/// Initialize the plugin by storing the host-provided context.
///
/// # Safety
///
/// `ctx` must be a valid pointer to a `PluginContext` whose lifetime spans
/// from this call until `nebula_plugin_shutdown` completes.
#[no_mangle]
pub extern "C" fn nebula_plugin_init(ctx: *const PluginContext) -> i32 {
    common::CTX.store(ctx as *mut PluginContext, Ordering::SeqCst);
    common::log(common::log_level::INFO, "Classifier plugin initialized");
    0
}

/// Execute an action dispatched to this plugin.
///
/// # Safety
///
/// - `input_ptr` must be valid for `input_len` bytes of UTF-8 data.
/// - `output_ptr` must be valid for `output_len` bytes of writable memory.
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
/// After this call returns, no further calls to `execute` will be made.
#[no_mangle]
pub extern "C" fn nebula_plugin_shutdown() -> i32 {
    common::log(common::log_level::INFO, "Classifier plugin shutting down");
    common::CTX.store(std::ptr::null_mut(), Ordering::SeqCst);
    0
}

/// Return a null-terminated JSON string describing this plugin's manifest.
///
/// # Safety
///
/// The returned pointer is valid for the lifetime of the process (leaked).
#[no_mangle]
pub extern "C" fn nebula_plugin_info() -> *const std::ffi::c_char {
    let info = serde_json::json!({
        "id": "com.nebula.classifier",
        "name": "Classifier",
        "version": "1.0.0",
        "capabilities": [],
        "description": "Rule-based text classification for transaction detection, spam filtering, and content categorization",
        "depends_on": []
    });
    let c_str = CString::new(info.to_string()).unwrap_or_default();
    c_str.into_raw() as *const std::ffi::c_char
}

// ---------------------------------------------------------------------------
// Action dispatch
// ---------------------------------------------------------------------------

fn dispatch(action: &str, params: &serde_json::Value) -> Result<String, String> {
    match action {
        "classifyText" => engine::classify_text(params),
        "classifySms" => engine::classify_sms(params),
        "classifyEmail" => engine::classify_email(params),
        "extractAmount" => engine::extract_amount(params),
        "extractPhoneNumber" => engine::extract_phone_number(params),
        "extractReference" => engine::extract_reference(params),
        "addCustomRule" => engine::add_custom_rule(params),
        "getCategories" => engine::get_categories(params),
        _ => Err(format!("Unknown action: {action}")),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn write_result(result: Result<String, String>, output_ptr: *mut u8, output_len: usize) -> i32 {
    match result {
        Ok(json) => {
            let bytes = json.as_bytes();
            let copy_len = bytes.len().min(output_len);
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), output_ptr, copy_len);
            }
            copy_len as i32
        }
        Err(e) => {
            let err_json = serde_json::json!({"error": e}).to_string();
            let bytes = err_json.as_bytes();
            let copy_len = bytes.len().min(output_len);
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
