#![allow(dead_code)]

//! Shared helpers for the Browser plugin.
//!
//! Provides a global plugin context accessor and convenience wrappers for
//! `platform_invoke`, state management, logging, and timestamps.

use nebula_plugin_sdk::context::PluginContext;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Global plugin context pointer, set during `nebula_plugin_init` and cleared
/// during `nebula_plugin_shutdown`. Accessed atomically because the engine may
/// call `execute` from different threads.
pub static CTX: AtomicPtr<PluginContext> = AtomicPtr::new(std::ptr::null_mut());

/// Log levels following tracing conventions.
#[allow(dead_code)]
pub mod log_level {
    pub const ERROR: u8 = 1;
    pub const WARN: u8 = 2;
    pub const INFO: u8 = 3;
    pub const DEBUG: u8 = 4;
    pub const TRACE: u8 = 5;
}

/// Load the global context pointer, returning an error if the plugin has not
/// been initialized (or has been shut down).
fn with_ctx() -> Result<&'static PluginContext, String> {
    let ptr = CTX.load(Ordering::SeqCst);
    if ptr.is_null() {
        return Err("Plugin not initialized".to_string());
    }
    // SAFETY: `ptr` was set in `nebula_plugin_init` from a valid
    // `*const PluginContext` whose lifetime spans until `nebula_plugin_shutdown`.
    // The engine guarantees this contract.
    Ok(unsafe { &*ptr })
}

/// Call `platform_invoke` on the host engine with the given capability routing
/// string and JSON arguments.
pub fn invoke(capability: &str, args: &str) -> Result<String, String> {
    let ctx = with_ctx()?;
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
        Err(format!("platform_invoke failed: {ret}"))
    } else {
        let result = std::str::from_utf8(&result_buf[..ret as usize])
            .map_err(|e| format!("Invalid UTF-8 in platform response: {e}"))?;
        Ok(result.to_string())
    }
}

/// Read a value from the plugin's persisted key-value store.
pub fn get_state(key: &str) -> Result<Option<String>, String> {
    let ctx = with_ctx()?;
    let mut val_buf = vec![0u8; 4096];
    let ret = (ctx.get_state)(
        ctx.host_data,
        key.as_ptr(),
        key.len(),
        val_buf.as_mut_ptr(),
        val_buf.len(),
    );

    match ret {
        n if n >= 0 => {
            let val = std::str::from_utf8(&val_buf[..n as usize])
                .map_err(|e| format!("Invalid UTF-8 in state value: {e}"))?;
            Ok(Some(val.to_string()))
        }
        -2 => Ok(None),
        _ => Err(format!("get_state failed: {ret}")),
    }
}

/// Write a value into the plugin's persisted key-value store.
pub fn set_state(key: &str, value: &str) -> Result<(), String> {
    let ctx = with_ctx()?;
    let ret = (ctx.set_state)(
        ctx.host_data,
        key.as_ptr(),
        key.len(),
        value.as_ptr(),
        value.len(),
    );

    if ret == 0 {
        Ok(())
    } else {
        Err(format!("set_state failed: {ret}"))
    }
}

/// Delete a key from the plugin's persisted key-value store.
pub fn delete_state(key: &str) -> Result<(), String> {
    let ctx = with_ctx()?;
    let ret = (ctx.delete_state)(ctx.host_data, key.as_ptr(), key.len());

    if ret == 0 {
        Ok(())
    } else {
        Err(format!("delete_state failed: {ret}"))
    }
}

/// Get the current epoch time in milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Emit a log message through the host engine's logging infrastructure.
pub fn log(level: u8, msg: &str) {
    if let Ok(ctx) = with_ctx() {
        let _ = (ctx.log)(ctx.host_data, level, msg.as_ptr(), msg.len());
    }
}
