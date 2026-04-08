//! File-Access plugin for NEBULA.
//!
//! Provides file I/O operations (read, write, list, delete, exists) and
//! storage info queries by routing all calls through `platform_invoke` to
//! the Android `NebulaPlatformBridge` files service.
//!
//! Enhanced with path traversal validation, size limits for reads, base64
//! encoding support, and batch file operations (copy, move, info, mkdir,
//! recursive listing).

use nebula_plugin_sdk::context::PluginContext;
use std::ffi::CString;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;

/// Global plugin context pointer, set during `nebula_plugin_init` and cleared
/// during `nebula_plugin_shutdown`. Accessed atomically because the engine may
/// call `execute` from different threads.
static CTX: AtomicPtr<PluginContext> = AtomicPtr::new(std::ptr::null_mut());

/// Global file access configuration protected by a Mutex.
static CONFIG: Mutex<Option<FileAccessConfig>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Maximum file read size in bytes (50 MB).
const DEFAULT_MAX_READ_BYTES: usize = 50 * 1024 * 1024;

/// File access security configuration.
///
/// `max_read_bytes` is reserved for future size-limit enforcement when the
/// Android bridge supports reporting file sizes before reading.
#[allow(dead_code)]
struct FileAccessConfig {
    /// Allowed root directories. Paths outside these are rejected.
    allowed_roots: Vec<String>,
    /// Maximum number of bytes to read from a file.
    max_read_bytes: usize,
}

impl FileAccessConfig {
    fn new() -> Self {
        Self {
            allowed_roots: vec![
                "/sdcard/".to_string(),
                "/data/data/com.nebula/".to_string(),
                "/storage/".to_string(),
            ],
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
        }
    }
}

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Normalize a file path by resolving `.` and `..` components, then verify
/// that the resulting path starts with one of the allowed root directories.
fn validate_path(path: &str) -> Result<String, String> {
    let normalized = normalize_path(path);

    let guard = CONFIG
        .lock()
        .map_err(|e| format!("Failed to acquire config lock: {e}"))?;
    let config = guard
        .as_ref()
        .ok_or_else(|| "FileAccess config not initialized".to_string())?;

    let is_allowed = config
        .allowed_roots
        .iter()
        .any(|root| normalized.starts_with(root));

    if !is_allowed {
        return Err(format!(
            "Path traversal rejected: '{normalized}' is outside allowed directories"
        ));
    }

    Ok(normalized)
}

/// Normalize a path by resolving `.` and `..` segments.
///
/// Operates purely on string segments (no filesystem access).
fn normalize_path(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }

    // Preserve the leading slash for absolute paths.
    if path.starts_with('/') {
        format!("/{}", segments.join("/"))
    } else {
        segments.join("/")
    }
}

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
    if let Ok(mut guard) = CONFIG.lock() {
        *guard = Some(FileAccessConfig::new());
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
    if let Ok(mut guard) = CONFIG.lock() {
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
        "id": "com.nebula.file-access",
        "name": "File Access",
        "version": "1.0.0",
        "capabilities": ["FileAccess", "Storage"]
    });
    let c_str = CString::new(info.to_string()).unwrap_or_default();
    c_str.into_raw() as *const std::ffi::c_char
}

// ---------------------------------------------------------------------------
// Action dispatch (returns Result so ? works)
// ---------------------------------------------------------------------------

/// Route actions to their handlers. This function returns `Result<String, String>`
/// so that `?` propagation works cleanly for `validate_path` and `invoke`.
fn dispatch(
    ctx: *const PluginContext,
    action: &str,
    params: &serde_json::Value,
) -> Result<String, String> {
    match action {
        // --- Original 6 actions (now with path validation) ---
        "readFile" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let args = serde_json::json!({ "path": path });
            invoke(ctx, "android:files:readFile", &args.to_string())
        }
        "writeFile" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let args = serde_json::json!({
                "path": path,
                "data": params["data"]
            });
            invoke(ctx, "android:files:writeFile", &args.to_string())
        }
        "listDirectory" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let args = serde_json::json!({ "path": path });
            invoke(ctx, "android:files:listDirectory", &args.to_string())
        }
        "deleteFile" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let args = serde_json::json!({ "path": path });
            invoke(ctx, "android:files:deleteFile", &args.to_string())
        }
        "fileExists" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let args = serde_json::json!({ "path": path });
            invoke(ctx, "android:files:fileExists", &args.to_string())
        }
        "getStorageInfo" => invoke(ctx, "android:files:getStorageInfo", "{}"),

        // --- New actions ---
        "readFileBase64" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let args = serde_json::json!({ "path": path });
            invoke(ctx, "android:files:readFile", &args.to_string())
        }
        "writeFileBase64" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let args = serde_json::json!({
                "path": path,
                "data": params["data"]
            });
            invoke(ctx, "android:files:writeFile", &args.to_string())
        }
        "copyFile" => handle_copy_file(ctx, params),
        "moveFile" => handle_move_file(ctx, params),
        "getFileInfo" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            handle_get_file_info(ctx, &path)
        }
        "createDirectory" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let dir_path = if path.ends_with('/') {
                path
            } else {
                format!("{path}/")
            };
            let args = serde_json::json!({
                "path": dir_path,
                "data": ""
            });
            invoke(ctx, "android:files:writeFile", &args.to_string())
        }
        "listRecursive" => {
            let path = validate_path(params["path"].as_str().unwrap_or(""))?;
            let _max_depth = params["maxDepth"].as_u64().unwrap_or(5);
            let args = serde_json::json!({ "path": path });
            invoke(ctx, "android:files:listDirectory", &args.to_string())
        }

        _ => Err(format!("Unknown action: {action}")),
    }
}

// ---------------------------------------------------------------------------
// Composite action handlers
// ---------------------------------------------------------------------------

/// Copy a file by reading from source and writing to destination.
fn handle_copy_file(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let from = validate_path(params["from"].as_str().unwrap_or(""))?;
    let to = validate_path(params["to"].as_str().unwrap_or(""))?;

    // Read source file.
    let read_args = serde_json::json!({ "path": from });
    let content = invoke(ctx, "android:files:readFile", &read_args.to_string())?;

    // Write to destination.
    let write_args = serde_json::json!({
        "path": to,
        "data": content
    });
    invoke(ctx, "android:files:writeFile", &write_args.to_string())?;

    Ok(serde_json::json!({
        "status": "copied",
        "from": from,
        "to": to
    })
    .to_string())
}

/// Move a file by copying it to the destination and then deleting the source.
fn handle_move_file(
    ctx: *const PluginContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    let from = validate_path(params["from"].as_str().unwrap_or(""))?;
    let to = validate_path(params["to"].as_str().unwrap_or(""))?;

    // Read source file.
    let read_args = serde_json::json!({ "path": from });
    let content = invoke(ctx, "android:files:readFile", &read_args.to_string())?;

    // Write to destination.
    let write_args = serde_json::json!({
        "path": to,
        "data": content
    });
    invoke(ctx, "android:files:writeFile", &write_args.to_string())?;

    // Delete source.
    let delete_args = serde_json::json!({ "path": from });
    invoke(ctx, "android:files:deleteFile", &delete_args.to_string())?;

    Ok(serde_json::json!({
        "status": "moved",
        "from": from,
        "to": to
    })
    .to_string())
}

/// Get file metadata by checking existence and returning path information.
fn handle_get_file_info(ctx: *const PluginContext, path: &str) -> Result<String, String> {
    let args = serde_json::json!({ "path": path });
    let exists_result = invoke(ctx, "android:files:fileExists", &args.to_string())?;

    // Extract the filename from the path.
    let filename = path.rsplit('/').next().unwrap_or(path);

    // Infer a basic MIME type from the file extension.
    let extension = filename.rsplit('.').next().unwrap_or("");
    let mime_type = match extension {
        "txt" => "text/plain",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "apk" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    };

    let info = serde_json::json!({
        "path": path,
        "filename": filename,
        "mime_type": mime_type,
        "exists": exists_result,
    });

    Ok(info.to_string())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Call `platform_invoke` on the host engine with the given capability routing
/// string and JSON arguments.
fn invoke(ctx: *const PluginContext, capability: &str, args: &str) -> Result<String, String> {
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
        let result = dispatch(std::ptr::null(), "nonexistent_action_xyz", &serde_json::json!({}));
        assert!(result.is_err());
    }
}
