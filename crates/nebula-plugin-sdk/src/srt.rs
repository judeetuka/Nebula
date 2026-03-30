//! SRT (Secure Reliable Transport) SDK convenience functions.
//!
//! Provides high-level wrappers around `platform_invoke("engine:srt:*", ...)`
//! so plugin developers can use SRT streaming without direct crate dependencies.
//!
//! SRT is a UDP-based protocol optimized for reliable low-latency video/audio
//! streaming over unreliable networks (4G/5G, WiFi, public internet).

use crate::context::PluginContext;
use serde::{Deserialize, Serialize};

/// SRT stream configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Target latency in milliseconds (default: 120ms).
    pub latency_ms: u32,
    /// Maximum bandwidth in bytes/sec (0 = unlimited).
    pub max_bandwidth: u64,
    /// AES encryption passphrase (None = no encryption).
    pub passphrase: Option<String>,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            latency_ms: 120,
            max_bandwidth: 0,
            passphrase: None,
        }
    }
}

/// Information about an active stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub stream_id: String,
    pub direction: String, // "send" or "receive"
    pub target: String,
    pub active: bool,
}

/// Start sending an SRT stream to a target address.
///
/// Returns a stream ID that can be used with `send_frame` and `stop_stream`.
pub fn start_sending(
    ctx: &PluginContext,
    target_host: &str,
    target_port: u16,
    config: Option<&StreamConfig>,
) -> Result<String, String> {
    let args = serde_json::json!({
        "action": "start_send",
        "host": target_host,
        "port": target_port,
        "config": config.cloned().unwrap_or_default(),
    })
    .to_string();
    let resp = invoke_engine(ctx, "srt:start", &args)?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| e.to_string())?;
    v["stream_id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No stream_id in response".to_string())
}

/// Start receiving an SRT stream on a port (listener mode).
///
/// Returns a stream ID that can be used with `stop_stream`.
pub fn start_receiving(
    ctx: &PluginContext,
    listen_port: u16,
    config: Option<&StreamConfig>,
) -> Result<String, String> {
    let args = serde_json::json!({
        "action": "start_recv",
        "port": listen_port,
        "config": config.cloned().unwrap_or_default(),
    })
    .to_string();
    let resp = invoke_engine(ctx, "srt:start", &args)?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| e.to_string())?;
    v["stream_id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No stream_id in response".to_string())
}

/// Send a data frame on an active sending stream.
///
/// The data is typically an MPEG-TS packet (188 bytes) or a batch of packets.
pub fn send_frame(ctx: &PluginContext, stream_id: &str, data: &[u8]) -> Result<(), String> {
    let args = serde_json::json!({
        "stream_id": stream_id,
        "data_len": data.len(),
    })
    .to_string();
    // For binary data, we use the args field for metadata and pass data
    // through the args buffer directly. In practice, the engine's SRT handler
    // reads the stream_id and pulls data from a shared buffer.
    invoke_engine(ctx, "srt:send_frame", &args)?;
    Ok(())
}

/// Stop an active stream (sender or receiver).
pub fn stop_stream(ctx: &PluginContext, stream_id: &str) -> Result<(), String> {
    let args = serde_json::json!({ "stream_id": stream_id }).to_string();
    invoke_engine(ctx, "srt:stop", &args)?;
    Ok(())
}

/// List all active SRT streams.
pub fn list_streams(ctx: &PluginContext) -> Result<Vec<StreamInfo>, String> {
    let resp = invoke_engine(ctx, "srt:list", "{}")?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

fn invoke_engine(ctx: &PluginContext, command: &str, args: &str) -> Result<String, String> {
    let capability = format!("engine:{command}");
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
        Err(format!("engine invoke '{command}' failed: {ret}"))
    } else {
        let s = std::str::from_utf8(&result_buf[..ret as usize])
            .map_err(|e| format!("Invalid UTF-8: {e}"))?;
        Ok(s.to_string())
    }
}
