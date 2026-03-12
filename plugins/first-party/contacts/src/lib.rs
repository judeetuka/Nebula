//! Contacts plugin for NEBULA.
//!
//! Read/write contacts and calendar events via the Android platform bridge.
//! Supports contact search, phone number lookup, offline caching via plugin
//! state, and bulk export. Calendar operations gracefully degrade if the
//! platform bridge does not support them.

mod common;
mod contacts;

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
    common::log(common::log_level::INFO, "Contacts plugin initialized");
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
    common::log(common::log_level::INFO, "Contacts plugin shutting down");
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
        "id": "com.nebula.contacts",
        "name": "Contacts",
        "version": "1.0.0",
        "capabilities": ["Contacts", "Calendar"],
        "description": "Contacts and calendar read/write with offline caching and bulk export",
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
        "readContacts" => contacts::read_contacts(params),
        "addContact" => contacts::add_contact(params),
        "deleteContact" => contacts::delete_contact(params),
        "searchContacts" => contacts::search_contacts(params),
        "getContactByPhone" => contacts::get_contact_by_phone(params),
        "readCalendarEvents" => contacts::read_calendar_events(params),
        "addCalendarEvent" => contacts::add_calendar_event(params),
        "syncContactsToState" => contacts::sync_contacts_to_state(params),
        "getContactCount" => contacts::get_contact_count(params),
        "exportContacts" => contacts::export_contacts(params),
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
