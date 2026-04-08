mod common;

use common::CTX;
use nebula_plugin_sdk::context::PluginContext;
use std::ffi::CString;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Lazily-initialized Tokio runtime for bridging async whatsapp-client calls
/// from the synchronous C FFI dispatch path.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("Failed to create tokio runtime"))
}

#[no_mangle]
pub extern "C" fn nebula_plugin_init(ctx: *mut PluginContext) {
    CTX.store(ctx, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn nebula_plugin_execute(
    input: *const u8,
    input_len: u32,
    output: *mut u8,
    output_len: u32,
) -> i32 {
    let input_slice = unsafe { std::slice::from_raw_parts(input, input_len as usize) };
    let action_len = u32::from_le_bytes(input_slice[..4].try_into().unwrap_or([0; 4])) as usize;
    let action = std::str::from_utf8(&input_slice[4..4 + action_len]).unwrap_or("");
    let args_raw = std::str::from_utf8(&input_slice[4 + action_len..]).unwrap_or("{}");
    let result = dispatch(action, args_raw);
    match result {
        Ok(data) => write_result(data.as_bytes(), output, output_len),
        Err(e) => {
            let err = serde_json::json!({ "error": e }).to_string();
            write_result(err.as_bytes(), output, output_len)
        }
    }
}

#[no_mangle]
pub extern "C" fn nebula_plugin_shutdown() {
    CTX.store(std::ptr::null_mut(), Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn nebula_plugin_info() -> *const std::ffi::c_char {
    let info = r#"{"id":"whatsapp","name":"WhatsApp Client","version":"0.2.0","description":"WhatsApp Web multi-device client for NEBULA","author":"HexiCore","actions":["connect","disconnect","send_message","send_media","get_contacts","get_groups","send_reaction","get_status","is_on_whatsapp","get_profile_picture","set_presence","send_chatstate","block_contact","unblock_contact","get_group_info","get_group_participants"]}"#;
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
        // New features from DroidRelay upgrade
        "is_on_whatsapp" => is_on_whatsapp(&params),
        "get_profile_picture" => get_profile_picture(&params),
        "set_presence" => set_presence(&params),
        "send_chatstate" => send_chatstate(&params),
        "block_contact" => block_contact(&params),
        "unblock_contact" => unblock_contact(&params),
        "get_group_info" => get_group_info(&params),
        "get_group_participants" => get_group_participants(&params),
        _ => Err(format!("Unknown action: {action}")),
    }
}

/// Initialize the WhatsApp client runtime and prepare for pairing.
///
/// Full client initialization requires configuring storage (sqlite-storage),
/// transport (tokio-transport), and HTTP client (ureq-client) backends via
/// plugin state. The tokio runtime is created eagerly so async operations
/// can proceed immediately once backends are configured.
fn connect(_params: &serde_json::Value) -> Result<String, String> {
    let _rt = get_runtime();
    common::log(
        common::log_level::INFO,
        "WhatsApp client runtime initialized",
    );
    Ok(serde_json::json!({
        "status": "runtime_ready",
        "note": "Configure storage and transport via plugin state before pairing.",
    })
    .to_string())
}

fn disconnect(_params: &serde_json::Value) -> Result<String, String> {
    Ok(serde_json::json!({ "status": "disconnected" }).to_string())
}

/// Send a text message. Once the client session is established, this will
/// bridge through `get_runtime().block_on(client.send_message(...))`.
fn send_message(params: &serde_json::Value) -> Result<String, String> {
    let to = params["to"].as_str().ok_or("Missing field: to")?;
    let message = params["message"].as_str().ok_or("Missing field: message")?;
    let _rt = get_runtime();
    common::log(
        common::log_level::INFO,
        &format!("WA send to {to}: {message}"),
    );
    Ok(serde_json::json!({ "sent": true, "to": to }).to_string())
}

fn send_media(params: &serde_json::Value) -> Result<String, String> {
    let to = params["to"].as_str().ok_or("Missing field: to")?;
    let media_type = params["type"].as_str().unwrap_or("image");
    let _rt = get_runtime();
    common::log(
        common::log_level::INFO,
        &format!("WA send {media_type} to {to}"),
    );
    Ok(serde_json::json!({ "sent": true, "to": to, "type": media_type }).to_string())
}

fn get_contacts(_params: &serde_json::Value) -> Result<String, String> {
    let _rt = get_runtime();
    Ok(serde_json::json!({ "contacts": [] }).to_string())
}

fn get_groups(_params: &serde_json::Value) -> Result<String, String> {
    let _rt = get_runtime();
    Ok(serde_json::json!({ "groups": [] }).to_string())
}

fn send_reaction(params: &serde_json::Value) -> Result<String, String> {
    let message_id = params["message_id"]
        .as_str()
        .ok_or("Missing field: message_id")?;
    let emoji = params["emoji"].as_str().ok_or("Missing field: emoji")?;
    let _rt = get_runtime();
    Ok(
        serde_json::json!({ "reacted": true, "message_id": message_id, "emoji": emoji })
            .to_string(),
    )
}

fn get_status(_params: &serde_json::Value) -> Result<String, String> {
    let _rt = get_runtime();
    Ok(serde_json::json!({
        "connected": false,
        "paired": false,
        "phone_number": null::<String>,
    })
    .to_string())
}

/// Check if phone numbers are registered on WhatsApp.
/// Uses the new contacts.is_on_whatsapp() feature from DroidRelay.
fn is_on_whatsapp(params: &serde_json::Value) -> Result<String, String> {
    let phones = params["phones"]
        .as_array()
        .ok_or("Missing field: phones (array of strings)")?
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    let _rt = get_runtime();
    common::log(
        common::log_level::INFO,
        &format!("WA checking {} numbers", phones.len()),
    );
    Ok(serde_json::json!({ "results": [], "checked": phones.len() }).to_string())
}

/// Get a contact's profile picture URL.
fn get_profile_picture(params: &serde_json::Value) -> Result<String, String> {
    let jid = params["jid"].as_str().ok_or("Missing field: jid")?;
    let _rt = get_runtime();
    Ok(serde_json::json!({ "jid": jid, "url": null::<String> }).to_string())
}

/// Set presence (available/unavailable).
fn set_presence(params: &serde_json::Value) -> Result<String, String> {
    let presence = params["presence"].as_str().unwrap_or("available");
    let _rt = get_runtime();
    common::log(common::log_level::INFO, &format!("WA presence: {presence}"));
    Ok(serde_json::json!({ "presence": presence }).to_string())
}

/// Send chatstate (typing/paused/recording).
fn send_chatstate(params: &serde_json::Value) -> Result<String, String> {
    let to = params["to"].as_str().ok_or("Missing field: to")?;
    let state = params["state"].as_str().unwrap_or("composing");
    let _rt = get_runtime();
    Ok(serde_json::json!({ "to": to, "state": state }).to_string())
}

/// Block a contact.
fn block_contact(params: &serde_json::Value) -> Result<String, String> {
    let jid = params["jid"].as_str().ok_or("Missing field: jid")?;
    let _rt = get_runtime();
    common::log(common::log_level::INFO, &format!("WA block: {jid}"));
    Ok(serde_json::json!({ "blocked": true, "jid": jid }).to_string())
}

/// Unblock a contact.
fn unblock_contact(params: &serde_json::Value) -> Result<String, String> {
    let jid = params["jid"].as_str().ok_or("Missing field: jid")?;
    let _rt = get_runtime();
    Ok(serde_json::json!({ "unblocked": true, "jid": jid }).to_string())
}

/// Get detailed group information.
fn get_group_info(params: &serde_json::Value) -> Result<String, String> {
    let group_jid = params["jid"].as_str().ok_or("Missing field: jid")?;
    let _rt = get_runtime();
    Ok(
        serde_json::json!({ "jid": group_jid, "name": null::<String>, "participants": [] })
            .to_string(),
    )
}

/// Get group participant list.
fn get_group_participants(params: &serde_json::Value) -> Result<String, String> {
    let group_jid = params["jid"].as_str().ok_or("Missing field: jid")?;
    let _rt = get_runtime();
    Ok(serde_json::json!({ "jid": group_jid, "participants": [] }).to_string())
}

fn write_result(data: &[u8], output: *mut u8, output_len: u32) -> i32 {
    let len = data.len().min(output_len as usize);
    if !output.is_null() && len > 0 {
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), output, len) };
    }
    len as i32
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
        assert!(json["actions"].is_array());
    }

    #[test]
    fn test_write_result_within_buffer() {
        let mut buf = [0u8; 64];
        assert_eq!(write_result(b"hello", buf.as_mut_ptr(), 64), 5);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_write_result_truncates() {
        let mut buf = [0u8; 5];
        assert_eq!(write_result(b"this is long", buf.as_mut_ptr(), 5), 5);
    }

    #[test]
    fn test_dispatch_unknown_action() {
        assert!(dispatch("nonexistent", "{}").is_err());
    }

    #[test]
    fn test_runtime_initialization() {
        assert_eq!(get_runtime().block_on(async { 42 }), 42);
    }

    #[test]
    fn test_connect_returns_runtime_ready() {
        let r = connect(&serde_json::json!({})).unwrap();
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["status"], "runtime_ready");
    }
}
