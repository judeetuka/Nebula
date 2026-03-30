//! WhatsApp SDK convenience functions.
//!
//! Provides high-level wrappers around `platform_invoke("plugin:whatsapp:*", ...)`
//! so any plugin can send WhatsApp messages without direct crate dependencies.
//!
//! The WhatsApp plugin must be installed and connected (paired via QR code)
//! on the node for these functions to work. If the plugin is not installed
//! or not connected, all functions return an error.

use crate::context::PluginContext;
use serde::{Deserialize, Serialize};

/// WhatsApp connection status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub paired: bool,
    pub phone_number: Option<String>,
}

/// A WhatsApp contact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub jid: String,
    pub name: Option<String>,
    pub phone: Option<String>,
    pub is_group: bool,
}

/// A WhatsApp group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub jid: String,
    pub name: String,
    pub participant_count: u32,
}

/// Result of sending a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    pub sent: bool,
    pub message_id: Option<String>,
    pub error: Option<String>,
}

/// Get the current WhatsApp connection status.
pub fn get_status(ctx: &PluginContext) -> Result<ConnectionStatus, String> {
    let resp = invoke_plugin(ctx, "get_status", "{}")?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

/// Send a text message to a phone number or JID.
///
/// # Arguments
/// * `to` - Phone number (e.g., "+1234567890") or JID (e.g., "1234567890@s.whatsapp.net")
/// * `message` - Text message content
pub fn send_message(ctx: &PluginContext, to: &str, message: &str) -> Result<SendResult, String> {
    let args = serde_json::json!({ "to": to, "message": message }).to_string();
    let resp = invoke_plugin(ctx, "send_message", &args)?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

/// Send a media message (image, video, audio, document).
///
/// # Arguments
/// * `to` - Phone number or JID
/// * `media_type` - "image", "video", "audio", or "document"
/// * `file_path` - Local path to the media file
/// * `caption` - Optional caption for the media
pub fn send_media(
    ctx: &PluginContext,
    to: &str,
    media_type: &str,
    file_path: &str,
    caption: Option<&str>,
) -> Result<SendResult, String> {
    let args = serde_json::json!({
        "to": to,
        "type": media_type,
        "file_path": file_path,
        "caption": caption,
    })
    .to_string();
    let resp = invoke_plugin(ctx, "send_media", &args)?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

/// Send a location message.
pub fn send_location(
    ctx: &PluginContext,
    to: &str,
    latitude: f64,
    longitude: f64,
    name: Option<&str>,
) -> Result<SendResult, String> {
    let args = serde_json::json!({
        "to": to,
        "latitude": latitude,
        "longitude": longitude,
        "name": name,
    })
    .to_string();
    let resp = invoke_plugin(ctx, "send_location", &args)?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

/// Send an emoji reaction to a message.
pub fn send_reaction(
    ctx: &PluginContext,
    message_id: &str,
    emoji: &str,
) -> Result<SendResult, String> {
    let args = serde_json::json!({
        "message_id": message_id,
        "emoji": emoji,
    })
    .to_string();
    let resp = invoke_plugin(ctx, "send_reaction", &args)?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

/// Get the contact list.
pub fn get_contacts(ctx: &PluginContext) -> Result<Vec<Contact>, String> {
    let resp = invoke_plugin(ctx, "get_contacts", "{}")?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| e.to_string())?;
    let contacts = v["contacts"].clone();
    serde_json::from_value(contacts).map_err(|e| format!("parse error: {e}"))
}

/// Get the group list.
pub fn get_groups(ctx: &PluginContext) -> Result<Vec<Group>, String> {
    let resp = invoke_plugin(ctx, "get_groups", "{}")?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| e.to_string())?;
    let groups = v["groups"].clone();
    serde_json::from_value(groups).map_err(|e| format!("parse error: {e}"))
}

/// Check if a phone number is registered on WhatsApp.
pub fn is_on_whatsapp(ctx: &PluginContext, phone: &str) -> Result<bool, String> {
    let args = serde_json::json!({ "phone": phone }).to_string();
    let resp = invoke_plugin(ctx, "is_on_whatsapp", &args)?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| e.to_string())?;
    Ok(v["registered"].as_bool().unwrap_or(false))
}

// ---------------------------------------------------------------------------
// Internal helper -- routes through plugin:whatsapp:action
// ---------------------------------------------------------------------------

fn invoke_plugin(ctx: &PluginContext, action: &str, args: &str) -> Result<String, String> {
    let capability = format!("plugin:whatsapp:{action}");
    let method = "";
    let mut result_buf = vec![0u8; 65536];
    let ret = (ctx.platform_invoke)(
        ctx.host_data,
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
        Err(format!("WhatsApp plugin invoke '{action}' failed: {ret}"))
    } else {
        let s = std::str::from_utf8(&result_buf[..ret as usize])
            .map_err(|e| format!("Invalid UTF-8: {e}"))?;
        Ok(s.to_string())
    }
}
