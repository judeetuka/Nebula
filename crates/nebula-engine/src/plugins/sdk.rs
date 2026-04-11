use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use super::ipc::{InvokeRouter, InvokeTarget};

// Re-export PluginContext from the SDK crate.
pub use nebula_plugin_sdk::context::PluginContext;

// ---------------------------------------------------------------------------
// Global engine handle for C-ABI callbacks
// ---------------------------------------------------------------------------

/// Shared handle to engine subsystems, registered once at startup.
pub struct EngineHandle {
    /// Publish a message to an MQTT topic. Returns 0 on success, -1 on error.
    pub mqtt_publish: Arc<dyn Fn(String, Vec<u8>) -> i32 + Send + Sync>,
    /// Subscribe to an MQTT topic. Returns 0 on success, -1 on error.
    pub mqtt_subscribe: Arc<dyn Fn(String) -> i32 + Send + Sync>,
    /// Invoke another plugin's execute function.
    pub plugin_invoke:
        Arc<dyn Fn(String, String, Vec<u8>) -> Result<Vec<u8>, String> + Send + Sync>,
    /// Invoke an engine system command.
    pub engine_invoke: Arc<dyn Fn(String, Vec<u8>) -> Result<Vec<u8>, String> + Send + Sync>,
    /// Report task progress (0-100). Returns 0 on success, -1 on error.
    pub task_progress: Arc<dyn Fn(String, u8) -> i32 + Send + Sync>,
    /// Report task completion with result data. Returns 0 on success, -1 on error.
    pub task_complete: Arc<dyn Fn(String, Vec<u8>) -> i32 + Send + Sync>,
    /// Report task failure with error message. Returns 0 on success, -1 on error.
    pub task_failed: Arc<dyn Fn(String, String) -> i32 + Send + Sync>,
}

static ENGINE_HANDLE: OnceLock<EngineHandle> = OnceLock::new();

/// Register the global engine handle. Should be called once at startup.
/// Subsequent calls are ignored with a warning (safe for hot-reload).
pub fn register_engine_handle(handle: EngineHandle) {
    if ENGINE_HANDLE.set(handle).is_err() {
        tracing::warn!("EngineHandle already registered — ignoring duplicate registration");
    }
}

/// Host-side data associated with a loaded plugin.
pub struct HostData {
    pub plugin_id: String,
    pub state_store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

// ---------------------------------------------------------------------------
// extern "C" callback implementations
// ---------------------------------------------------------------------------

unsafe fn host_data_ref<'a>(host: *mut std::ffi::c_void) -> Option<&'a HostData> {
    if host.is_null() {
        return None;
    }
    Some(&*(host as *const HostData))
}

unsafe fn slice_from_raw<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if ptr.is_null() || len == 0 {
        return Some(&[]);
    }
    Some(std::slice::from_raw_parts(ptr, len))
}

extern "C" fn cb_get_state(
    host: *mut std::ffi::c_void,
    key_ptr: *const u8,
    key_len: usize,
    val_buf: *mut u8,
    val_buf_len: usize,
) -> i32 {
    let (data, key_bytes) = match unsafe { (host_data_ref(host), slice_from_raw(key_ptr, key_len)) }
    {
        (Some(d), Some(k)) => (d, k),
        _ => return -1,
    };
    let key = match std::str::from_utf8(key_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let store = match data.state_store.read() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match store.get(key) {
        Some(value) => {
            let copy_len = value.len().min(val_buf_len);
            if !val_buf.is_null() && copy_len > 0 {
                unsafe {
                    std::ptr::copy_nonoverlapping(value.as_ptr(), val_buf, copy_len);
                }
            }
            copy_len as i32
        }
        None => -2,
    }
}

extern "C" fn cb_set_state(
    host: *mut std::ffi::c_void,
    key_ptr: *const u8,
    key_len: usize,
    val_ptr: *const u8,
    val_len: usize,
) -> i32 {
    let (data, key_bytes, val_bytes) = match unsafe {
        (
            host_data_ref(host),
            slice_from_raw(key_ptr, key_len),
            slice_from_raw(val_ptr, val_len),
        )
    } {
        (Some(d), Some(k), Some(v)) => (d, k, v),
        _ => return -1,
    };
    let key = match std::str::from_utf8(key_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let mut store = match data.state_store.write() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    store.insert(key.to_string(), val_bytes.to_vec());
    0
}

extern "C" fn cb_delete_state(
    host: *mut std::ffi::c_void,
    key_ptr: *const u8,
    key_len: usize,
) -> i32 {
    let (data, key_bytes) = match unsafe { (host_data_ref(host), slice_from_raw(key_ptr, key_len)) }
    {
        (Some(d), Some(k)) => (d, k),
        _ => return -1,
    };
    let key = match std::str::from_utf8(key_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let mut store = match data.state_store.write() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    store.remove(key);
    0
}

extern "C" fn cb_log(
    host: *mut std::ffi::c_void,
    level: u8,
    msg_ptr: *const u8,
    msg_len: usize,
) -> i32 {
    let (data, msg_bytes) = match unsafe { (host_data_ref(host), slice_from_raw(msg_ptr, msg_len)) }
    {
        (Some(d), Some(m)) => (d, m),
        _ => return -1,
    };
    let msg = match std::str::from_utf8(msg_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match level {
        1 => tracing::error!(plugin_id = %data.plugin_id, "{}", msg),
        2 => tracing::warn!(plugin_id = %data.plugin_id, "{}", msg),
        3 => tracing::info!(plugin_id = %data.plugin_id, "{}", msg),
        4 => tracing::debug!(plugin_id = %data.plugin_id, "{}", msg),
        _ => tracing::trace!(plugin_id = %data.plugin_id, "{}", msg),
    }
    0
}

extern "C" fn cb_publish(
    _host: *mut std::ffi::c_void,
    topic_ptr: *const u8,
    topic_len: usize,
    payload_ptr: *const u8,
    payload_len: usize,
) -> i32 {
    let Some(handle) = ENGINE_HANDLE.get() else {
        return -1;
    };
    let topic_bytes = match unsafe { slice_from_raw(topic_ptr, topic_len) } {
        Some(s) => s,
        None => return -1,
    };
    let payload_bytes = match unsafe { slice_from_raw(payload_ptr, payload_len) } {
        Some(s) => s,
        None => return -1,
    };
    let topic = match std::str::from_utf8(topic_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    (handle.mqtt_publish)(topic.to_string(), payload_bytes.to_vec())
}

extern "C" fn cb_subscribe(
    _host: *mut std::ffi::c_void,
    topic_ptr: *const u8,
    topic_len: usize,
) -> i32 {
    let Some(handle) = ENGINE_HANDLE.get() else {
        return -1;
    };
    let topic_bytes = match unsafe { slice_from_raw(topic_ptr, topic_len) } {
        Some(s) => s,
        None => return -1,
    };
    let topic = match std::str::from_utf8(topic_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    (handle.mqtt_subscribe)(topic.to_string())
}

extern "C" fn cb_report_task_progress(
    _host: *mut std::ffi::c_void,
    task_id_ptr: *const u8,
    task_id_len: usize,
    progress: u8,
) -> i32 {
    let Some(handle) = ENGINE_HANDLE.get() else {
        return -1;
    };
    let task_id_bytes = match unsafe { slice_from_raw(task_id_ptr, task_id_len) } {
        Some(s) => s,
        None => return -1,
    };
    let task_id = match std::str::from_utf8(task_id_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    (handle.task_progress)(task_id.to_string(), progress)
}

extern "C" fn cb_report_task_complete(
    _host: *mut std::ffi::c_void,
    task_id_ptr: *const u8,
    task_id_len: usize,
    result_ptr: *const u8,
    result_len: usize,
) -> i32 {
    let Some(handle) = ENGINE_HANDLE.get() else {
        return -1;
    };
    let task_id_bytes = match unsafe { slice_from_raw(task_id_ptr, task_id_len) } {
        Some(s) => s,
        None => return -1,
    };
    let result_bytes = match unsafe { slice_from_raw(result_ptr, result_len) } {
        Some(s) => s,
        None => return -1,
    };
    let task_id = match std::str::from_utf8(task_id_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    (handle.task_complete)(task_id.to_string(), result_bytes.to_vec())
}

extern "C" fn cb_report_task_failed(
    _host: *mut std::ffi::c_void,
    task_id_ptr: *const u8,
    task_id_len: usize,
    error_ptr: *const u8,
    error_len: usize,
) -> i32 {
    let Some(handle) = ENGINE_HANDLE.get() else {
        return -1;
    };
    let task_id_bytes = match unsafe { slice_from_raw(task_id_ptr, task_id_len) } {
        Some(s) => s,
        None => return -1,
    };
    let error_bytes = match unsafe { slice_from_raw(error_ptr, error_len) } {
        Some(s) => s,
        None => return -1,
    };
    let task_id = match std::str::from_utf8(task_id_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let error_msg = match std::str::from_utf8(error_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    (handle.task_failed)(task_id.to_string(), error_msg.to_string())
}

extern "C" fn cb_platform_invoke(
    _host: *mut std::ffi::c_void,
    capability_ptr: *const u8,
    capability_len: usize,
    method_ptr: *const u8,
    method_len: usize,
    args_ptr: *const u8,
    args_len: usize,
    result_buf: *mut u8,
    result_buf_len: usize,
) -> i32 {
    let capability_bytes = match unsafe { slice_from_raw(capability_ptr, capability_len) } {
        Some(s) => s,
        None => return -1,
    };
    let method_bytes = match unsafe { slice_from_raw(method_ptr, method_len) } {
        Some(s) => s,
        None => return -1,
    };
    let args_bytes = match unsafe { slice_from_raw(args_ptr, args_len) } {
        Some(s) => s,
        None => return -1,
    };
    let capability = match std::str::from_utf8(capability_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let method = match std::str::from_utf8(method_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let args = match std::str::from_utf8(args_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let target = InvokeRouter::parse_target(capability);

    let result = match target {
        InvokeTarget::Android { service, method: _ } => {
            let android_method = match InvokeRouter::parse_target(capability) {
                InvokeTarget::Android { method: m, .. } => m,
                _ => return -1,
            };
            crate::platform::invoke_android(&service, &android_method, args)
        }
        InvokeTarget::Plugin { plugin_id, action } => {
            let _ = method;
            let Some(handle) = ENGINE_HANDLE.get() else {
                return -1;
            };
            match (handle.plugin_invoke)(plugin_id, action, args_bytes.to_vec()) {
                Ok(result) => {
                    let copy_len = result.len().min(result_buf_len);
                    if !result_buf.is_null() && copy_len > 0 {
                        unsafe {
                            std::ptr::copy_nonoverlapping(result.as_ptr(), result_buf, copy_len);
                        }
                    }
                    return copy_len as i32;
                }
                Err(_) => return -1,
            }
        }
        InvokeTarget::Engine { command } => {
            let _ = method;
            let Some(handle) = ENGINE_HANDLE.get() else {
                return -1;
            };
            match (handle.engine_invoke)(command, args_bytes.to_vec()) {
                Ok(result) => {
                    let copy_len = result.len().min(result_buf_len);
                    if !result_buf.is_null() && copy_len > 0 {
                        unsafe {
                            std::ptr::copy_nonoverlapping(result.as_ptr(), result_buf, copy_len);
                        }
                    }
                    return copy_len as i32;
                }
                Err(_) => return -1,
            }
        }
        InvokeTarget::Unknown => Err(format!("Unknown invoke target: {capability}")),
    };

    match result {
        Ok(response) => {
            let bytes = response.as_bytes();
            let copy_len = bytes.len().min(result_buf_len);
            if !result_buf.is_null() && copy_len > 0 {
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), result_buf, copy_len);
                }
            }
            copy_len as i32
        }
        Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------
// Context factory
// ---------------------------------------------------------------------------

pub fn create_plugin_context(
    plugin_id: &str,
    state_store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
) -> PluginContext {
    let host = Box::new(HostData {
        plugin_id: plugin_id.to_string(),
        state_store,
    });
    PluginContext {
        host_data: Box::into_raw(host) as *mut std::ffi::c_void,
        get_state: cb_get_state,
        set_state: cb_set_state,
        delete_state: cb_delete_state,
        log: cb_log,
        publish: cb_publish,
        subscribe: cb_subscribe,
        report_task_progress: cb_report_task_progress,
        report_task_complete: cb_report_task_complete,
        report_task_failed: cb_report_task_failed,
        platform_invoke: cb_platform_invoke,
    }
}

pub unsafe fn drop_host_data(ctx: &mut PluginContext) {
    if !ctx.host_data.is_null() {
        let _ = Box::from_raw(ctx.host_data as *mut HostData);
        ctx.host_data = std::ptr::null_mut();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state_store() -> Arc<RwLock<HashMap<String, Vec<u8>>>> {
        Arc::new(RwLock::new(HashMap::new()))
    }

    fn ensure_test_engine_handle() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            register_engine_handle(EngineHandle {
                mqtt_publish: Arc::new(|topic, payload| {
                    if topic == "test/ok" && !payload.is_empty() {
                        0
                    } else {
                        -1
                    }
                }),
                mqtt_subscribe: Arc::new(|topic| if topic == "test/ok" { 0 } else { -1 }),
                plugin_invoke: Arc::new(|plugin_id, action, payload| {
                    if plugin_id == "echo" && action == "ping" {
                        let mut out = b"pong:".to_vec();
                        out.extend_from_slice(&payload);
                        Ok(out)
                    } else {
                        Err(format!("Unknown plugin: {plugin_id}:{action}"))
                    }
                }),
                engine_invoke: Arc::new(|command, _payload| {
                    if command == "status" {
                        Ok(b"ok".to_vec())
                    } else {
                        Err(format!("Unknown command: {command}"))
                    }
                }),
                task_progress: Arc::new(|_task_id, _progress| 0),
                task_complete: Arc::new(|_task_id, _result| 0),
                task_failed: Arc::new(|_task_id, _error| 0),
            });
        });
    }

    #[test]
    fn test_create_plugin_context_returns_non_null_host_data() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        assert!(!ctx.host_data.is_null());
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_set_and_get_state_via_callbacks() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store.clone());
        let key = b"my_key";
        let value = b"hello world";
        let result = (ctx.set_state)(
            ctx.host_data,
            key.as_ptr(),
            key.len(),
            value.as_ptr(),
            value.len(),
        );
        assert_eq!(result, 0);
        let mut buf = [0u8; 64];
        let result = (ctx.get_state)(
            ctx.host_data,
            key.as_ptr(),
            key.len(),
            buf.as_mut_ptr(),
            buf.len(),
        );
        assert_eq!(result, value.len() as i32);
        assert_eq!(&buf[..value.len()], value);
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_get_state_key_not_found() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let key = b"nonexistent";
        let mut buf = [0u8; 64];
        let result = (ctx.get_state)(
            ctx.host_data,
            key.as_ptr(),
            key.len(),
            buf.as_mut_ptr(),
            buf.len(),
        );
        assert_eq!(result, -2);
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_publish_without_engine_handle_returns_negative() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let topic = b"unknown/topic";
        let payload = b"hello";
        let result = (ctx.publish)(
            ctx.host_data,
            topic.as_ptr(),
            topic.len(),
            payload.as_ptr(),
            payload.len(),
        );
        assert_eq!(result, -1);
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_subscribe_without_engine_handle_returns_negative() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let topic = b"unknown/topic";
        let result = (ctx.subscribe)(ctx.host_data, topic.as_ptr(), topic.len());
        assert_eq!(result, -1);
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_platform_invoke_returns_negative_for_plugin_target() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let cap = b"plugin:classifier:classify";
        let method = b"classify";
        let args = b"{}";
        let mut result_buf = [0u8; 64];
        let result = (ctx.platform_invoke)(
            ctx.host_data,
            cap.as_ptr(),
            cap.len(),
            method.as_ptr(),
            method.len(),
            args.as_ptr(),
            args.len(),
            result_buf.as_mut_ptr(),
            result_buf.len(),
        );
        assert_eq!(result, -1);
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_platform_invoke_returns_negative_for_engine_target() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let cap = b"engine:device_info";
        let method = b"device_info";
        let args = b"{}";
        let mut result_buf = [0u8; 64];
        let result = (ctx.platform_invoke)(
            ctx.host_data,
            cap.as_ptr(),
            cap.len(),
            method.as_ptr(),
            method.len(),
            args.as_ptr(),
            args.len(),
            result_buf.as_mut_ptr(),
            result_buf.len(),
        );
        assert_eq!(result, -1);
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_get_state_with_null_host_returns_error() {
        let key = b"key";
        let mut buf = [0u8; 32];
        let result = cb_get_state(
            std::ptr::null_mut(),
            key.as_ptr(),
            key.len(),
            buf.as_mut_ptr(),
            buf.len(),
        );
        assert_eq!(result, -1);
    }

    #[test]
    fn test_drop_host_data_clears_pointer() {
        let store = make_state_store();
        let mut ctx = create_plugin_context("test-plugin", store);
        assert!(!ctx.host_data.is_null());
        unsafe { drop_host_data(&mut ctx) };
        assert!(ctx.host_data.is_null());
    }

    // EngineHandle wiring tests
    #[test]
    fn test_engine_handle_publish_routes_through_closure() {
        ensure_test_engine_handle();
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let topic = b"test/ok";
        let payload = b"data";
        let result = (ctx.publish)(
            ctx.host_data,
            topic.as_ptr(),
            topic.len(),
            payload.as_ptr(),
            payload.len(),
        );
        assert_eq!(result, 0);
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_engine_handle_subscribe_routes_through_closure() {
        ensure_test_engine_handle();
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let topic = b"test/ok";
        let result = (ctx.subscribe)(ctx.host_data, topic.as_ptr(), topic.len());
        assert_eq!(result, 0);
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_engine_handle_plugin_invoke_routes_through_closure() {
        ensure_test_engine_handle();
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let cap = b"plugin:echo:ping";
        let method = b"ping";
        let args = b"hello";
        let mut result_buf = [0u8; 256];
        let n = (ctx.platform_invoke)(
            ctx.host_data,
            cap.as_ptr(),
            cap.len(),
            method.as_ptr(),
            method.len(),
            args.as_ptr(),
            args.len(),
            result_buf.as_mut_ptr(),
            result_buf.len(),
        );
        assert_eq!(n, 10);
        assert_eq!(&result_buf[..10], b"pong:hello");
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_engine_handle_engine_invoke_routes_through_closure() {
        ensure_test_engine_handle();
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        let cap = b"engine:status";
        let method = b"status";
        let args = b"{}";
        let mut result_buf = [0u8; 256];
        let n = (ctx.platform_invoke)(
            ctx.host_data,
            cap.as_ptr(),
            cap.len(),
            method.as_ptr(),
            method.len(),
            args.as_ptr(),
            args.len(),
            result_buf.as_mut_ptr(),
            result_buf.len(),
        );
        assert_eq!(n, 2);
        assert_eq!(&result_buf[..2], b"ok");
        let mut ctx = ctx;
        unsafe { drop_host_data(&mut ctx) };
    }
}
