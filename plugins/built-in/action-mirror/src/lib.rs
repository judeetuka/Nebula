//! Action Mirror plugin for NEBULA.
//!
//! Provides screen capture, remote control, action recording, and macro
//! replay capabilities. This plugin enables remote device interaction from
//! the admin dashboard by combining screen capture with accessibility-based
//! input simulation.
//!
//! ## Modules
//!
//! - **capture** -- Screen capture session management
//! - **remote** -- Remote input commands (tap, swipe, type, gestures)
//! - **recorder** -- Action recording into named macros with state persistence
//! - **replay** -- Sequential macro playback with status tracking

pub mod capture;
pub mod recorder;
pub mod remote;
pub mod replay;

use nebula_plugin_sdk::context::PluginContext;
use std::ffi::CString;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;

/// Global plugin context pointer, set during `nebula_plugin_init` and cleared
/// during `nebula_plugin_shutdown`. Accessed atomically because the engine may
/// call `execute` from different threads.
static CTX: AtomicPtr<PluginContext> = AtomicPtr::new(std::ptr::null_mut());

/// Global capture session state.
static CAPTURE: Mutex<Option<capture::CaptureSession>> = Mutex::new(None);

/// Global macro recorder state.
static RECORDER: Mutex<Option<recorder::MacroRecorder>> = Mutex::new(None);

/// Global macro player state.
static PLAYER: Mutex<Option<replay::MacroPlayer>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// ABI exports
// ---------------------------------------------------------------------------

/// Initialize the plugin by storing the host-provided context and setting up
/// internal state for all modules.
///
/// # Safety
///
/// `ctx` must be a valid pointer to a `PluginContext` whose lifetime spans
/// from this call until `nebula_plugin_shutdown` completes. The engine
/// guarantees this by keeping the context alive in `LoadedPlugin`.
#[no_mangle]
pub extern "C" fn nebula_plugin_init(ctx: *const PluginContext) -> i32 {
    CTX.store(ctx as *mut PluginContext, Ordering::SeqCst);

    if let Ok(mut guard) = CAPTURE.lock() {
        *guard = Some(capture::CaptureSession::new());
    }
    if let Ok(mut guard) = RECORDER.lock() {
        *guard = Some(recorder::MacroRecorder::new());
    }
    if let Ok(mut guard) = PLAYER.lock() {
        *guard = Some(replay::MacroPlayer::new());
    }

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

    let result = dispatch(ctx, action, params);
    write_result(result, output_ptr, output_len)
}

/// Shut down the plugin and release all state.
///
/// # Safety
///
/// After this call returns, no further calls to `execute` will be made by the
/// engine. The `AtomicPtr` is set to null to prevent use-after-free if a
/// stale reference somehow persists.
#[no_mangle]
pub extern "C" fn nebula_plugin_shutdown() -> i32 {
    CTX.store(std::ptr::null_mut(), Ordering::SeqCst);

    if let Ok(mut guard) = CAPTURE.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = RECORDER.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = PLAYER.lock() {
        *guard = None;
    }

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
        "id": "com.nebula.action-mirror",
        "name": "Action Mirror",
        "version": "1.0.0",
        "capabilities": ["ScreenControl", "Accessibility"]
    });
    let c_str = CString::new(info.to_string()).unwrap_or_default();
    c_str.into_raw() as *const std::ffi::c_char
}

// ---------------------------------------------------------------------------
// Action dispatch
// ---------------------------------------------------------------------------

/// Route an action to the appropriate module handler.
fn dispatch(
    ctx: *const PluginContext,
    action: &str,
    params: &serde_json::Value,
) -> Result<String, String> {
    match action {
        // --- Capture module ---
        "startCapture" => {
            let mut guard = CAPTURE
                .lock()
                .map_err(|e| format!("Failed to acquire capture lock: {e}"))?;
            let session = guard
                .as_mut()
                .ok_or_else(|| "Capture session not initialized".to_string())?;
            capture::handle_start_capture(ctx, session, params)
        }
        "stopCapture" => {
            let mut guard = CAPTURE
                .lock()
                .map_err(|e| format!("Failed to acquire capture lock: {e}"))?;
            let session = guard
                .as_mut()
                .ok_or_else(|| "Capture session not initialized".to_string())?;
            capture::handle_stop_capture(ctx, session)
        }
        "getFrame" => {
            let mut guard = CAPTURE
                .lock()
                .map_err(|e| format!("Failed to acquire capture lock: {e}"))?;
            let session = guard
                .as_mut()
                .ok_or_else(|| "Capture session not initialized".to_string())?;
            capture::handle_get_frame(ctx, session)
        }
        "getCaptureConfig" => capture::handle_get_capture_config(ctx),
        "isCapturing" => {
            let guard = CAPTURE
                .lock()
                .map_err(|e| format!("Failed to acquire capture lock: {e}"))?;
            let session = guard
                .as_ref()
                .ok_or_else(|| "Capture session not initialized".to_string())?;
            capture::handle_is_capturing(ctx, session)
        }

        // --- Remote control module ---
        "remoteTap" => remote::handle_remote_tap(ctx, params),
        "remoteSwipe" => remote::handle_remote_swipe(ctx, params),
        "remoteType" => remote::handle_remote_type(ctx, params),
        "remotePressBack" => remote::handle_remote_press_back(ctx),
        "remotePressHome" => remote::handle_remote_press_home(ctx),
        "remoteScroll" => remote::handle_remote_scroll(ctx, params),

        // --- Recorder module ---
        "startRecording" => {
            let mut guard = RECORDER
                .lock()
                .map_err(|e| format!("Failed to acquire recorder lock: {e}"))?;
            let rec = guard
                .as_mut()
                .ok_or_else(|| "Recorder not initialized".to_string())?;
            recorder::handle_start_recording(rec)
        }
        "recordAction" => {
            let mut guard = RECORDER
                .lock()
                .map_err(|e| format!("Failed to acquire recorder lock: {e}"))?;
            let rec = guard
                .as_mut()
                .ok_or_else(|| "Recorder not initialized".to_string())?;
            recorder::handle_record_action(rec, params)
        }
        "stopRecording" => {
            let mut guard = RECORDER
                .lock()
                .map_err(|e| format!("Failed to acquire recorder lock: {e}"))?;
            let rec = guard
                .as_mut()
                .ok_or_else(|| "Recorder not initialized".to_string())?;
            recorder::handle_stop_recording(rec)
        }
        "getMacro" => recorder::handle_get_macro(ctx, params),
        "saveMacro" => recorder::handle_save_macro(ctx, params),
        "listMacros" => recorder::handle_list_macros(ctx),
        "deleteMacro" => recorder::handle_delete_macro(ctx, params),

        // --- Replay module ---
        "playMacro" => {
            let mut guard = PLAYER
                .lock()
                .map_err(|e| format!("Failed to acquire player lock: {e}"))?;
            let player = guard
                .as_mut()
                .ok_or_else(|| "Player not initialized".to_string())?;
            replay::handle_play_macro(ctx, player, params)
        }
        "playMacroByName" => {
            let mut guard = PLAYER
                .lock()
                .map_err(|e| format!("Failed to acquire player lock: {e}"))?;
            let player = guard
                .as_mut()
                .ok_or_else(|| "Player not initialized".to_string())?;
            replay::handle_play_macro_by_name(ctx, player, params)
        }
        "stopPlayback" => {
            let mut guard = PLAYER
                .lock()
                .map_err(|e| format!("Failed to acquire player lock: {e}"))?;
            let player = guard
                .as_mut()
                .ok_or_else(|| "Player not initialized".to_string())?;
            replay::handle_stop_playback(player)
        }
        "getPlaybackStatus" => {
            let guard = PLAYER
                .lock()
                .map_err(|e| format!("Failed to acquire player lock: {e}"))?;
            let player = guard
                .as_ref()
                .ok_or_else(|| "Player not initialized".to_string())?;
            replay::handle_get_playback_status(player)
        }

        _ => Err(format!("Unknown action: {action}")),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Call `platform_invoke` on the host engine with the given capability routing
/// string and JSON arguments.
pub(crate) fn invoke(
    ctx: *const PluginContext,
    capability: &str,
    args: &str,
) -> Result<String, String> {
    // SAFETY: `ctx` was set in `nebula_plugin_init` and the engine guarantees
    // it remains valid until `nebula_plugin_shutdown`. We verified `ctx` is
    // non-null at the top of `execute`.
    let ctx_ref = unsafe { &*ctx };

    let method = "";
    let mut result_buf = vec![0u8; 65536];
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
            // SAFETY: same as above -- `output_ptr` is valid for `output_len` bytes.
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), output_ptr, copy_len);
            }
            -(copy_len as i32)
        }
    }
}
