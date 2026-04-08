mod common;

use common::CTX;
use nebula_plugin_sdk::context::PluginContext;
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::sync::atomic::Ordering;

#[derive(Serialize, Deserialize)]
struct StreamStatus {
    state: String,
    stream_id: Option<String>,
    target: Option<String>,
    resolution: String,
    bitrate_kbps: u32,
    latency_ms: u32,
    frames_sent: u64,
    frames_received: u64,
}

#[derive(Serialize, Deserialize)]
struct StreamConfig {
    latency_ms: u32,
    resolution: String,
    bitrate_kbps: u32,
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
    let params: serde_json::Value = serde_json::from_str(args_raw).unwrap_or_default();
    let result = dispatch(action, &params);
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
    let _ = stop_inner("capturing");
    let _ = stop_inner("receiving");
    CTX.store(std::ptr::null_mut(), Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn nebula_plugin_info() -> *const std::ffi::c_char {
    let info = r#"{"id":"screen-stream","name":"Screen Stream","version":"0.1.0","description":"SRT-based screen streaming for puppet mode visualization","author":"HexiCore","actions":["start_capture","stop_capture","start_receiver","stop_receiver","get_status","list_streams","configure"]}"#;
    CString::new(info).unwrap().into_raw()
}

fn dispatch(action: &str, params: &serde_json::Value) -> Result<String, String> {
    match action {
        "start_capture" => start_capture(params),
        "stop_capture" => stop_capture(params),
        "start_receiver" => start_receiver(params),
        "stop_receiver" => stop_receiver(params),
        "get_status" => get_status(params),
        "list_streams" => list_streams(params),
        "configure" => configure(params),
        _ => Err(format!("Unknown action: {action}")),
    }
}

const STATE_STATUS: &str = "screen_stream_status";
const STATE_CONFIG: &str = "screen_stream_config";

fn load_status() -> StreamStatus {
    common::get_state(STATE_STATUS)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(StreamStatus {
            state: "idle".into(),
            stream_id: None,
            target: None,
            resolution: "720p".into(),
            bitrate_kbps: 2000,
            latency_ms: 200,
            frames_sent: 0,
            frames_received: 0,
        })
}

fn save_status(s: &StreamStatus) -> Result<(), String> {
    common::set_state(
        STATE_STATUS,
        &serde_json::to_string(s).map_err(|e| e.to_string())?,
    )
}

fn load_config() -> StreamConfig {
    common::get_state(STATE_CONFIG)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(StreamConfig {
            latency_ms: 200,
            resolution: "720p".into(),
            bitrate_kbps: 2000,
        })
}

fn start_capture(params: &serde_json::Value) -> Result<String, String> {
    let status = load_status();
    if status.state != "idle" {
        return Err(format!("Current state: {}", status.state));
    }
    let host = params["target_host"]
        .as_str()
        .ok_or("Missing target_host")?;
    let port = params["target_port"]
        .as_u64()
        .ok_or("Missing target_port")? as u16;
    let config = load_config();
    common::log(
        common::log_level::INFO,
        &format!("Capture -> {host}:{port}"),
    );
    common::invoke(
        "android:screen:startCapture",
        &serde_json::json!({"resolution": config.resolution, "bitrate_kbps": config.bitrate_kbps})
            .to_string(),
    )?;
    let resp = common::invoke("engine:srt:start", &serde_json::json!({"action":"start_send","host":host,"port":port,"config":{"latency_ms":config.latency_ms}}).to_string())?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| e.to_string())?;
    let sid = v["stream_id"].as_str().unwrap_or("unknown").to_string();
    save_status(&StreamStatus {
        state: "capturing".into(),
        stream_id: Some(sid.clone()),
        target: Some(format!("{host}:{port}")),
        resolution: config.resolution,
        bitrate_kbps: config.bitrate_kbps,
        latency_ms: config.latency_ms,
        frames_sent: 0,
        frames_received: 0,
    })?;
    Ok(serde_json::json!({"status":"capturing","stream_id":sid}).to_string())
}

fn stop_capture(_p: &serde_json::Value) -> Result<String, String> {
    stop_inner("capturing")
}

fn start_receiver(params: &serde_json::Value) -> Result<String, String> {
    let status = load_status();
    if status.state != "idle" {
        return Err(format!("Current state: {}", status.state));
    }
    let port = params["port"].as_u64().ok_or("Missing port")? as u16;
    let config = load_config();
    common::log(common::log_level::INFO, &format!("Receiver on :{port}"));
    let resp = common::invoke("engine:srt:start", &serde_json::json!({"action":"start_recv","port":port,"config":{"latency_ms":config.latency_ms}}).to_string())?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| e.to_string())?;
    let sid = v["stream_id"].as_str().unwrap_or("unknown").to_string();
    save_status(&StreamStatus {
        state: "receiving".into(),
        stream_id: Some(sid.clone()),
        target: Some(format!("0.0.0.0:{port}")),
        resolution: config.resolution,
        bitrate_kbps: config.bitrate_kbps,
        latency_ms: config.latency_ms,
        frames_sent: 0,
        frames_received: 0,
    })?;
    Ok(serde_json::json!({"status":"receiving","stream_id":sid}).to_string())
}

fn stop_receiver(_p: &serde_json::Value) -> Result<String, String> {
    stop_inner("receiving")
}

fn stop_inner(expected: &str) -> Result<String, String> {
    let status = load_status();
    if status.state != expected {
        return Err(format!("Not {expected}"));
    }
    if let Some(ref sid) = status.stream_id {
        common::invoke(
            "engine:srt:stop",
            &serde_json::json!({"stream_id":sid}).to_string(),
        )?;
    }
    if expected == "capturing" {
        let _ = common::invoke("android:screen:stopCapture", "{}");
    }
    save_status(&StreamStatus {
        state: "idle".into(),
        stream_id: None,
        target: None,
        ..status
    })?;
    Ok(serde_json::json!({"status":"stopped"}).to_string())
}

fn get_status(_p: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string(&load_status()).map_err(|e| e.to_string())
}

fn list_streams(_p: &serde_json::Value) -> Result<String, String> {
    common::invoke("engine:srt:list", "{}")
}

fn configure(params: &serde_json::Value) -> Result<String, String> {
    let mut c = load_config();
    if let Some(v) = params["latency_ms"].as_u64() {
        c.latency_ms = v as u32;
    }
    if let Some(v) = params["resolution"].as_str() {
        c.resolution = v.to_string();
    }
    if let Some(v) = params["bitrate_kbps"].as_u64() {
        c.bitrate_kbps = v as u32;
    }
    common::set_state(
        STATE_CONFIG,
        &serde_json::to_string(&c).map_err(|e| e.to_string())?,
    )?;
    serde_json::to_string(&c).map_err(|e| e.to_string())
}

fn write_result(data: &[u8], output: *mut u8, output_len: u32) -> i32 {
    let len = data.len().min(output_len as usize);
    if !output.is_null() && len > 0 {
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), output, len) };
    }
    len as i32
}
