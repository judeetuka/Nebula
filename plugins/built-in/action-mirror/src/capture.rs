//! Screen capture orchestration for the Action Mirror plugin.
//!
//! Manages screen capture sessions by routing commands through `platform_invoke`
//! to the Android `capture` service group. Tracks session state (resolution,
//! FPS, frame count) locally.

use nebula_plugin_sdk::context::PluginContext;

/// Screen capture session state.
pub struct CaptureSession {
    pub active: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
    pub frame_count: u64,
}

impl CaptureSession {
    pub fn new() -> Self {
        Self {
            active: false,
            width: 0,
            height: 0,
            fps: 0,
            bitrate: 0,
            frame_count: 0,
        }
    }
}

/// Start a screen capture session with the given parameters.
pub fn handle_start_capture(
    ctx: *const PluginContext,
    session: &mut CaptureSession,
    params: &serde_json::Value,
) -> Result<String, String> {
    let width = params["width"].as_u64().unwrap_or(720) as u32;
    let height = params["height"].as_u64().unwrap_or(1280) as u32;
    let fps = params["fps"].as_u64().unwrap_or(30) as u32;
    let bitrate = params["bitrate"].as_u64().unwrap_or(2_000_000) as u32;

    let args = serde_json::json!({
        "width": width,
        "height": height,
        "fps": fps,
        "bitrate": bitrate
    });

    let result = super::invoke(ctx, "android:capture:startScreenCapture", &args.to_string())?;

    session.active = true;
    session.width = width;
    session.height = height;
    session.fps = fps;
    session.bitrate = bitrate;
    session.frame_count = 0;

    Ok(serde_json::json!({
        "status": "started",
        "width": width,
        "height": height,
        "fps": fps,
        "bitrate": bitrate,
        "platform_response": result
    })
    .to_string())
}

/// Stop the active screen capture session.
pub fn handle_stop_capture(
    ctx: *const PluginContext,
    session: &mut CaptureSession,
) -> Result<String, String> {
    let result = super::invoke(ctx, "android:capture:stopScreenCapture", "{}")?;

    let frame_count = session.frame_count;
    session.active = false;
    session.frame_count = 0;

    Ok(serde_json::json!({
        "status": "stopped",
        "total_frames": frame_count,
        "platform_response": result
    })
    .to_string())
}

/// Get the latest captured frame (base64-encoded H.264 data).
pub fn handle_get_frame(
    ctx: *const PluginContext,
    session: &mut CaptureSession,
) -> Result<String, String> {
    let result = super::invoke(ctx, "android:capture:getScreenFrame", "{}")?;
    session.frame_count += 1;
    Ok(result)
}

/// Get the current capture configuration (resolution, FPS, SPS/PPS info).
pub fn handle_get_capture_config(ctx: *const PluginContext) -> Result<String, String> {
    super::invoke(ctx, "android:capture:getScreenCaptureConfig", "{}")
}

/// Check whether screen capture is currently active.
pub fn handle_is_capturing(
    ctx: *const PluginContext,
    session: &CaptureSession,
) -> Result<String, String> {
    let platform_result = super::invoke(ctx, "android:capture:isScreenCaptureActive", "{}")?;

    Ok(serde_json::json!({
        "active": session.active,
        "frame_count": session.frame_count,
        "width": session.width,
        "height": session.height,
        "fps": session.fps,
        "platform_response": platform_result
    })
    .to_string())
}
