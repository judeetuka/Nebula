//! Device-Info plugin for NEBULA.
//!
//! Exposes device hardware info, battery status, network state, CPU/RAM
//! metrics, sensor data, WiFi, Bluetooth, screen, and storage information
//! by routing all calls through `platform_invoke` to the Android bridge.
//!
//! Enhanced with intelligent caching: static info is cached forever, dynamic
//! info respects a configurable TTL, and real-time data always hits the platform.

use nebula_plugin_sdk::context::PluginContext;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Global plugin context pointer, set during `nebula_plugin_init` and cleared
/// during `nebula_plugin_shutdown`. Accessed atomically because the engine may
/// call `execute` from different threads.
static CTX: AtomicPtr<PluginContext> = AtomicPtr::new(std::ptr::null_mut());

/// Global device info cache protected by a Mutex for thread-safe access.
static CACHE: Mutex<Option<DeviceInfoCache>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Cache types
// ---------------------------------------------------------------------------

/// Default cache TTL in milliseconds (5 seconds).
const DEFAULT_CACHE_TTL_MS: i64 = 5000;

/// A cached response entry with its creation timestamp.
struct CacheEntry {
    data: String,
    cached_at_ms: i64,
}

/// Cache for device information responses.
struct DeviceInfoCache {
    entries: HashMap<String, CacheEntry>,
    cache_ttl_ms: i64,
}

impl DeviceInfoCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            cache_ttl_ms: DEFAULT_CACHE_TTL_MS,
        }
    }
}

/// Classification of how an action's response should be cached.
enum CachePolicy {
    /// Cache forever (static device properties that never change at runtime).
    Static,
    /// Cache for the configured TTL duration.
    Dynamic,
    /// Never cache; always call the platform.
    RealTime,
}

/// Map an action name to its cache policy.
fn cache_policy_for(action: &str) -> CachePolicy {
    match action {
        "getDeviceInfo" | "getCpuInfo" | "getDeviceSignature" => CachePolicy::Static,
        "getCpuTemperature" => CachePolicy::RealTime,
        _ => CachePolicy::Dynamic,
    }
}

/// Map an action name to its platform capability routing string.
fn capability_for(action: &str) -> Option<&'static str> {
    match action {
        "getDeviceInfo" => Some("android:device:getDeviceInfo"),
        "getBatteryInfo" => Some("android:device:getBatteryInfo"),
        "getNetworkInfo" => Some("android:device:getNetworkInfo"),
        "getCpuInfo" => Some("android:system:getCpuInfo"),
        "getCpuTemperature" => Some("android:system:getCpuTemperature"),
        "getRamInfo" => Some("android:system:getRamInfo"),
        "getSensorList" => Some("android:system:getSensorList"),
        "getWifiInfo" => Some("android:wifi:getWifiInfo"),
        "scanWifiNetworks" => Some("android:wifi:scanWifiNetworks"),
        "isWifiEnabled" => Some("android:wifi:isWifiEnabled"),
        "isBluetoothEnabled" => Some("android:bluetooth:isBluetoothEnabled"),
        "getBluetoothDevices" => Some("android:bluetooth:getBluetoothDevices"),
        "getScreenInfo" => Some("android:screen:getScreenInfo"),
        "getDeviceSignature" => Some("android:device:getDeviceSignature"),
        "getStorageInfo" => Some("android:files:getStorageInfo"),
        _ => None,
    }
}

/// Get the current time in epoch milliseconds.
fn current_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(DeviceInfoCache::new());
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

    let result = match action {
        // All 15 original actions, now with caching.
        "getDeviceInfo"
        | "getBatteryInfo"
        | "getNetworkInfo"
        | "getCpuInfo"
        | "getCpuTemperature"
        | "getRamInfo"
        | "getSensorList"
        | "getWifiInfo"
        | "scanWifiNetworks"
        | "isWifiEnabled"
        | "isBluetoothEnabled"
        | "getBluetoothDevices"
        | "getScreenInfo"
        | "getDeviceSignature"
        | "getStorageInfo" => cached_invoke(ctx, action),

        // New composite report.
        "getFullReport" => handle_get_full_report(ctx),

        // Cache management.
        "setCacheTtl" => {
            let ms = params["ms"].as_i64().unwrap_or(DEFAULT_CACHE_TTL_MS);
            handle_set_cache_ttl(ms)
        }
        "clearCache" => handle_clear_cache(),

        _ => Err(format!("Unknown action: {action}")),
    };

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
    if let Ok(mut guard) = CACHE.lock() {
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
        "id": "com.nebula.device-info",
        "name": "Device Info",
        "version": "1.0.0",
        "capabilities": ["Sensors", "Wifi", "Bluetooth", "Storage"]
    });
    let c_str = CString::new(info.to_string()).unwrap_or_default();
    c_str.into_raw() as *const std::ffi::c_char
}

// ---------------------------------------------------------------------------
// Caching logic
// ---------------------------------------------------------------------------

/// Invoke a platform action with cache awareness. Checks the cache first
/// based on the action's cache policy before making a platform call.
fn cached_invoke(ctx: *const PluginContext, action: &str) -> Result<String, String> {
    let capability = capability_for(action)
        .ok_or_else(|| format!("No capability mapping for action: {action}"))?;
    let policy = cache_policy_for(action);
    let now = current_time_ms();

    // Check cache first (except for RealTime).
    if !matches!(policy, CachePolicy::RealTime) {
        if let Ok(guard) = CACHE.lock() {
            if let Some(cache) = guard.as_ref() {
                if let Some(entry) = cache.entries.get(action) {
                    let is_valid = match policy {
                        CachePolicy::Static => true,
                        CachePolicy::Dynamic => (now - entry.cached_at_ms) < cache.cache_ttl_ms,
                        CachePolicy::RealTime => false,
                    };
                    if is_valid {
                        return Ok(entry.data.clone());
                    }
                }
            }
        }
    }

    // Cache miss or real-time: call the platform.
    let result = invoke(ctx, capability, "{}")?;

    // Store in cache (except for RealTime).
    if !matches!(policy, CachePolicy::RealTime) {
        if let Ok(mut guard) = CACHE.lock() {
            if let Some(cache) = guard.as_mut() {
                cache.entries.insert(
                    action.to_string(),
                    CacheEntry {
                        data: result.clone(),
                        cached_at_ms: now,
                    },
                );
            }
        }
    }

    Ok(result)
}

/// Build a composite report by calling all 15 device info actions.
fn handle_get_full_report(ctx: *const PluginContext) -> Result<String, String> {
    let actions = [
        "getDeviceInfo",
        "getBatteryInfo",
        "getNetworkInfo",
        "getCpuInfo",
        "getCpuTemperature",
        "getRamInfo",
        "getSensorList",
        "getWifiInfo",
        "scanWifiNetworks",
        "isWifiEnabled",
        "isBluetoothEnabled",
        "getBluetoothDevices",
        "getScreenInfo",
        "getDeviceSignature",
        "getStorageInfo",
    ];

    let mut report = serde_json::Map::new();

    for action in &actions {
        let value = match cached_invoke(ctx, action) {
            Ok(json_str) => {
                // Try to parse as JSON value; if it fails, store as raw string.
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::String(json_str))
            }
            Err(e) => serde_json::json!({"error": e}),
        };
        report.insert(action.to_string(), value);
    }

    report.insert(
        "timestamp_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(current_time_ms())),
    );

    Ok(serde_json::Value::Object(report).to_string())
}

/// Update the cache TTL duration.
fn handle_set_cache_ttl(ms: i64) -> Result<String, String> {
    let mut guard = CACHE
        .lock()
        .map_err(|e| format!("Failed to acquire cache lock: {e}"))?;
    let cache = guard
        .as_mut()
        .ok_or_else(|| "Cache not initialized".to_string())?;

    cache.cache_ttl_ms = ms;
    Ok(serde_json::json!({"cache_ttl_ms": ms}).to_string())
}

/// Invalidate all cached data.
fn handle_clear_cache() -> Result<String, String> {
    let mut guard = CACHE
        .lock()
        .map_err(|e| format!("Failed to acquire cache lock: {e}"))?;
    let cache = guard
        .as_mut()
        .ok_or_else(|| "Cache not initialized".to_string())?;

    let count = cache.entries.len();
    cache.entries.clear();
    Ok(serde_json::json!({"cleared": count}).to_string())
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
