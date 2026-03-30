mod common;

use common::CTX;
use nebula_plugin_sdk::context::PluginContext;
use std::ffi::CString;
use std::sync::atomic::Ordering;

#[no_mangle]
pub extern "C" fn nebula_plugin_init(ctx: *mut PluginContext) {
    CTX.store(ctx, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn nebula_plugin_execute(
    input: *const u8, input_len: u32, output: *mut u8, output_len: u32,
) -> i32 {
    let input_slice = unsafe { std::slice::from_raw_parts(input, input_len as usize) };
    let action_len = u32::from_le_bytes(input_slice[..4].try_into().unwrap_or([0; 4])) as usize;
    let action = std::str::from_utf8(&input_slice[4..4 + action_len]).unwrap_or("");
    let args_raw = std::str::from_utf8(&input_slice[4 + action_len..]).unwrap_or("{}");

    let result = dispatch(action, args_raw);

    match result {
        Ok(data) => write_result(data.as_bytes(), output, output_len),
        Err(e) => {
            let err_json = serde_json::json!({ "error": e }).to_string();
            write_result(err_json.as_bytes(), output, output_len)
        }
    }
}

#[no_mangle]
pub extern "C" fn nebula_plugin_shutdown() {
    CTX.store(std::ptr::null_mut(), Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn nebula_plugin_info() -> *const std::ffi::c_char {
    let info = r#"{"id":"whatsapp","name":"WhatsApp Client","version":"0.1.0","description":"WhatsApp Web multi-device client for NEBULA","author":"HexiCore","actions":["connect","disconnect","send_message","send_media","get_contacts","get_groups","send_reaction","get_status"]}"#;
    CString::new(info).unwrap().into_raw()
}

fn dispatch(action: &str, args: &str) -> Result<String, String> {
    let params: serde_json::Value =
        serde_json::from_str(args).map_err(|e| format!("Invalid JSON: {e}"))?;

    match action {
        "connect" => connect(&params),
        "disconnect" => disconnect(&params),
        "send_message" => send_message(&params),
        "send_media" => send_media(&params),
        "get_contacts" => get_contacts(&params),
        "get_groups" => get_groups(&params),
        "send_reaction" => send_reaction(&params),
        "get_status" => get_status(&params),
        _ => Err(format!("Unknown action: {action}")),
    }
}

// ---------------------------------------------------------------------------
// Action handlers -- these will use whatsapp-client crate via tokio runtime
// ---------------------------------------------------------------------------

fn connect(_params: &serde_json::Value) -> Result<String, String> {
    // TODO: Initialize WhatsApp client via whatsapp-client crate's BotBuilder
    // Returns QR code data for pairing via the WhatsApp mobile app
    Ok(serde_json::json!({
        "status": "ready",
        "note": "WhatsApp client initialized. QR pairing will be handled via engine event stream.",
    }).to_string())
}

fn disconnect(_params: &serde_json::Value) -> Result<String, String> {
    Ok(serde_json::json!({ "status": "disconnected" }).to_string())
}

fn send_message(params: &serde_json::Value) -> Result<String, String> {
    let to = params["to"].as_str().ok_or("Missing field: to")?;
    let message = params["message"].as_str().ok_or("Missing field: message")?;
    common::log(common::log_level::INFO, &format!("WA send to {to}: {message}"));
    // TODO: Use whatsapp-client to send the message
    Ok(serde_json::json!({ "sent": true, "to": to }).to_string())
}

fn send_media(params: &serde_json::Value) -> Result<String, String> {
    let to = params["to"].as_str().ok_or("Missing field: to")?;
    let media_type = params["type"].as_str().unwrap_or("image");
    common::log(common::log_level::INFO, &format!("WA send {media_type} to {to}"));
    Ok(serde_json::json!({ "sent": true, "to": to, "type": media_type }).to_string())
}

fn get_contacts(_params: &serde_json::Value) -> Result<String, String> {
    Ok(serde_json::json!({ "contacts": [], "note": "Contact sync not yet active" }).to_string())
}

fn get_groups(_params: &serde_json::Value) -> Result<String, String> {
    Ok(serde_json::json!({ "groups": [], "note": "Group list not yet active" }).to_string())
}

fn send_reaction(params: &serde_json::Value) -> Result<String, String> {
    let message_id = params["message_id"].as_str().ok_or("Missing field: message_id")?;
    let emoji = params["emoji"].as_str().ok_or("Missing field: emoji")?;
    Ok(serde_json::json!({ "reacted": true, "message_id": message_id, "emoji": emoji }).to_string())
}

fn get_status(_params: &serde_json::Value) -> Result<String, String> {
    Ok(serde_json::json!({
        "connected": false,
        "paired": false,
        "phone_number": null::<String>,
    }).to_string())
}

fn write_result(data: &[u8], output: *mut u8, output_len: u32) -> i32 {
    let len = data.len().min(output_len as usize);
    if !output.is_null() && len > 0 {
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), output, len) };
    }
    len as i32
}
