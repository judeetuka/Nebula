use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Host-side data associated with a loaded plugin.
///
/// A `Box<HostData>` is allocated for each plugin and passed to the plugin
/// as an opaque `*mut c_void`. The `extern "C"` callback functions cast it
/// back to `&HostData` to service the plugin's requests.
pub struct HostData {
    /// Which plugin this context belongs to.
    pub plugin_id: String,
    /// Per-plugin key-value store. Shared with the registry so that state
    /// survives hot-reloads.
    pub state_store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

/// The context structure passed to plugins via `nebula_plugin_init`.
///
/// Plugins call back into the host through the function pointers in this
/// struct. The `host_data` pointer is an opaque handle that must be passed
/// as the first argument to every callback — it lets the host identify the
/// calling plugin and route the request to the correct internal state.
///
/// # ABI stability
///
/// This struct is `#[repr(C)]` so that its layout is predictable across
/// compiler versions. All function pointer signatures use only C-compatible
/// types (raw pointers, `usize`, `u8`, `i32`).
///
/// # Safety contract
///
/// * The host guarantees that `host_data` remains valid for the lifetime of
///   the plugin (from init to shutdown).
/// * Plugins must not free or mutate `host_data`.
/// * Pointer/length pairs (`key_ptr`/`key_len`, etc.) must point to valid
///   memory for the stated length. The host validates them defensively.
#[repr(C)]
pub struct PluginContext {
    /// Opaque pointer to host-managed data for this plugin.
    pub host_data: *mut std::ffi::c_void,

    // -- State management ---------------------------------------------------
    /// Read a value from the plugin's key-value store.
    /// Returns the number of bytes written to `val_buf`, or -1 on error,
    /// or -2 if the key was not found.
    pub get_state: extern "C" fn(
        host: *mut std::ffi::c_void,
        key_ptr: *const u8,
        key_len: usize,
        val_buf: *mut u8,
        val_buf_len: usize,
    ) -> i32,

    /// Write a value into the plugin's key-value store.
    /// Returns 0 on success, -1 on error.
    pub set_state: extern "C" fn(
        host: *mut std::ffi::c_void,
        key_ptr: *const u8,
        key_len: usize,
        val_ptr: *const u8,
        val_len: usize,
    ) -> i32,

    /// Remove a key from the plugin's key-value store.
    /// Returns 0 on success (even if the key did not exist), -1 on error.
    pub delete_state: extern "C" fn(
        host: *mut std::ffi::c_void,
        key_ptr: *const u8,
        key_len: usize,
    ) -> i32,

    // -- Logging ------------------------------------------------------------
    /// Emit a log message. `level` follows tracing conventions:
    /// 1 = error, 2 = warn, 3 = info, 4 = debug, 5 = trace.
    /// Returns 0 on success, -1 on error.
    pub log: extern "C" fn(
        host: *mut std::ffi::c_void,
        level: u8,
        msg_ptr: *const u8,
        msg_len: usize,
    ) -> i32,

    // -- MQTT messaging -----------------------------------------------------
    /// Publish a message to an MQTT topic.
    /// Returns 0 on success, -1 on error (stub: always returns -1 for now).
    pub publish: extern "C" fn(
        host: *mut std::ffi::c_void,
        topic_ptr: *const u8,
        topic_len: usize,
        payload_ptr: *const u8,
        payload_len: usize,
    ) -> i32,

    /// Subscribe to an MQTT topic.
    /// Returns 0 on success, -1 on error (stub: always returns -1 for now).
    pub subscribe: extern "C" fn(
        host: *mut std::ffi::c_void,
        topic_ptr: *const u8,
        topic_len: usize,
    ) -> i32,

    // -- Task management ----------------------------------------------------
    /// Report incremental progress on a task (0-100).
    /// Returns 0 on success, -1 on error (stub).
    pub report_task_progress: extern "C" fn(
        host: *mut std::ffi::c_void,
        task_id_ptr: *const u8,
        task_id_len: usize,
        progress: u8,
    ) -> i32,

    /// Report that a task completed successfully.
    /// Returns 0 on success, -1 on error (stub).
    pub report_task_complete: extern "C" fn(
        host: *mut std::ffi::c_void,
        task_id_ptr: *const u8,
        task_id_len: usize,
        result_ptr: *const u8,
        result_len: usize,
    ) -> i32,

    /// Report that a task failed.
    /// Returns 0 on success, -1 on error (stub).
    pub report_task_failed: extern "C" fn(
        host: *mut std::ffi::c_void,
        task_id_ptr: *const u8,
        task_id_len: usize,
        error_ptr: *const u8,
        error_len: usize,
    ) -> i32,

    // -- Platform capabilities ----------------------------------------------
    /// Invoke a platform-specific capability (Android API bridge).
    /// Returns bytes written to `result_buf`, or -1 if not implemented.
    pub platform_invoke: extern "C" fn(
        host: *mut std::ffi::c_void,
        capability_ptr: *const u8,
        capability_len: usize,
        method_ptr: *const u8,
        method_len: usize,
        args_ptr: *const u8,
        args_len: usize,
        result_buf: *mut u8,
        result_buf_len: usize,
    ) -> i32,
}

// SAFETY: `PluginContext` contains a `*mut c_void` that points to a
// heap-allocated `HostData`. This pointer is:
//   1. Created from `Box::into_raw` in `create_plugin_context`.
//   2. Only dereferenced through the `extern "C"` callback functions.
//   3. The underlying `HostData` synchronizes mutable access to the state
//      store via `Arc<RwLock<HashMap>>`, which is itself `Send + Sync`.
//   4. The pointer is only freed in `drop_host_data` after the plugin has
//      been shut down, so no concurrent access occurs during deallocation.
//
// The function pointer fields are all `extern "C" fn(...)` which are `Copy`,
// `Send`, and `Sync` by nature (they are plain function pointers).
//
// Therefore it is safe to send a `PluginContext` across threads and share
// references to it, provided the `host_data` lifetime contract is upheld.
unsafe impl Send for PluginContext {}
unsafe impl Sync for PluginContext {}

// ---------------------------------------------------------------------------
// extern "C" callback implementations
// ---------------------------------------------------------------------------

/// Reconstruct a `&HostData` reference from the opaque host pointer.
///
/// # Safety
///
/// The caller must guarantee that `host` was originally created from
/// `Box::into_raw(Box::new(HostData { .. }))` and has not been freed.
/// The returned reference borrows the pointee and must not outlive
/// the plugin session.
unsafe fn host_data_ref<'a>(host: *mut std::ffi::c_void) -> Option<&'a HostData> {
    if host.is_null() {
        return None;
    }
    // SAFETY: `host` is a pointer produced by `Box::into_raw` in
    // `create_plugin_context`. It is valid for the lifetime of the plugin
    // and is only freed in `LoadedPlugin::unload` after the plugin has
    // been shut down and can no longer invoke callbacks.
    Some(&*(host as *const HostData))
}

/// Safely convert a `(ptr, len)` pair into a `&[u8]`.
///
/// # Safety
///
/// The caller must ensure that `ptr` is valid for `len` bytes and that the
/// memory will not be mutated for the duration of the returned borrow.
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
    // SAFETY: `host` was created from `Box::into_raw(Box::new(HostData))` in
    // `create_plugin_context` and remains valid for the plugin's lifetime.
    // `key_ptr`/`key_len` must be provided by the plugin as a valid slice.
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
                // SAFETY: `val_buf` is provided by the plugin and must be
                // valid for `val_buf_len` bytes. We copy at most `copy_len`
                // bytes which is <= `val_buf_len`.
                unsafe {
                    std::ptr::copy_nonoverlapping(value.as_ptr(), val_buf, copy_len);
                }
            }
            copy_len as i32
        }
        None => -2, // key not found
    }
}

extern "C" fn cb_set_state(
    host: *mut std::ffi::c_void,
    key_ptr: *const u8,
    key_len: usize,
    val_ptr: *const u8,
    val_len: usize,
) -> i32 {
    // SAFETY: same invariants as `cb_get_state` — `host`, `key_ptr`, and
    // `val_ptr` come from the plugin and must be valid for their stated
    // lengths.
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
    // SAFETY: `host` and `key_ptr` follow the same contract as above.
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
    // SAFETY: `host` and `msg_ptr` follow the same contract as above.
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
    _topic_ptr: *const u8,
    _topic_len: usize,
    _payload_ptr: *const u8,
    _payload_len: usize,
) -> i32 {
    // Stub: MQTT publish will be wired in a future phase.
    -1
}

extern "C" fn cb_subscribe(
    _host: *mut std::ffi::c_void,
    _topic_ptr: *const u8,
    _topic_len: usize,
) -> i32 {
    // Stub: MQTT subscribe will be wired in a future phase.
    -1
}

extern "C" fn cb_report_task_progress(
    _host: *mut std::ffi::c_void,
    _task_id_ptr: *const u8,
    _task_id_len: usize,
    _progress: u8,
) -> i32 {
    // Stub: task progress reporting will be wired in a future phase.
    -1
}

extern "C" fn cb_report_task_complete(
    _host: *mut std::ffi::c_void,
    _task_id_ptr: *const u8,
    _task_id_len: usize,
    _result_ptr: *const u8,
    _result_len: usize,
) -> i32 {
    // Stub: task completion reporting will be wired in a future phase.
    -1
}

extern "C" fn cb_report_task_failed(
    _host: *mut std::ffi::c_void,
    _task_id_ptr: *const u8,
    _task_id_len: usize,
    _error_ptr: *const u8,
    _error_len: usize,
) -> i32 {
    // Stub: task failure reporting will be wired in a future phase.
    -1
}

extern "C" fn cb_platform_invoke(
    _host: *mut std::ffi::c_void,
    _capability_ptr: *const u8,
    _capability_len: usize,
    _method_ptr: *const u8,
    _method_len: usize,
    _args_ptr: *const u8,
    _args_len: usize,
    _result_buf: *mut u8,
    _result_buf_len: usize,
) -> i32 {
    // Stub: platform invoke (Android API bridge) is not implemented yet.
    -1
}

// ---------------------------------------------------------------------------
// Context factory
// ---------------------------------------------------------------------------

/// Create a `PluginContext` for the given plugin.
///
/// The returned context owns a heap-allocated `HostData` (stored behind the
/// opaque `host_data` pointer). The caller is responsible for eventually
/// freeing it by calling [`drop_host_data`].
///
/// The `state_store` is shared with the registry so that state set by the
/// plugin is visible to the host and vice versa.
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

/// Free the `HostData` behind a `PluginContext`'s `host_data` pointer.
///
/// # Safety
///
/// Must only be called once per context, and only after the plugin has been
/// shut down and will never invoke callbacks again.
pub unsafe fn drop_host_data(ctx: &mut PluginContext) {
    if !ctx.host_data.is_null() {
        // SAFETY: `host_data` was created by `Box::into_raw` in
        // `create_plugin_context`. We are the sole owner and the plugin
        // has been shut down, so no concurrent access is possible.
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

    #[test]
    fn test_create_plugin_context_returns_non_null_host_data() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);
        assert!(!ctx.host_data.is_null());

        // Clean up
        let mut ctx = ctx;
        // SAFETY: we just created this context and nothing else holds a reference.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_function_pointers_are_populated() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        // Verify all function pointers are the expected callbacks.
        // We can't easily compare fn pointers directly, but we can verify
        // they are non-null (they are statically defined, so always valid).
        assert!(ctx.get_state as usize != 0);
        assert!(ctx.set_state as usize != 0);
        assert!(ctx.delete_state as usize != 0);
        assert!(ctx.log as usize != 0);
        assert!(ctx.publish as usize != 0);
        assert!(ctx.subscribe as usize != 0);
        assert!(ctx.report_task_progress as usize != 0);
        assert!(ctx.report_task_complete as usize != 0);
        assert!(ctx.report_task_failed as usize != 0);
        assert!(ctx.platform_invoke as usize != 0);

        let mut ctx = ctx;
        // SAFETY: we just created this context and nothing else holds a reference.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_set_and_get_state_via_callbacks() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store.clone());

        let key = b"my_key";
        let value = b"hello world";

        // Set state
        let result = (ctx.set_state)(
            ctx.host_data,
            key.as_ptr(),
            key.len(),
            value.as_ptr(),
            value.len(),
        );
        assert_eq!(result, 0);

        // Get state
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

        // Verify the store was actually updated
        let s = store.read().unwrap();
        assert_eq!(s.get("my_key").unwrap(), value);
        drop(s);

        let mut ctx = ctx;
        // SAFETY: we just created this context and no plugin is running.
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
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_delete_state_via_callback() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store.clone());

        let key = b"to_delete";
        let value = b"data";

        // Set, then delete
        (ctx.set_state)(
            ctx.host_data,
            key.as_ptr(),
            key.len(),
            value.as_ptr(),
            value.len(),
        );
        let result = (ctx.delete_state)(ctx.host_data, key.as_ptr(), key.len());
        assert_eq!(result, 0);

        // Verify deleted
        let s = store.read().unwrap();
        assert!(s.get("to_delete").is_none());
        drop(s);

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_delete_state_nonexistent_key() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let key = b"does_not_exist";
        let result = (ctx.delete_state)(ctx.host_data, key.as_ptr(), key.len());
        // Should return 0 even if key was not present
        assert_eq!(result, 0);

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_log_callback_returns_success() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let msg = b"test log message";
        // Test all log levels
        for level in 1..=5 {
            let result = (ctx.log)(ctx.host_data, level, msg.as_ptr(), msg.len());
            assert_eq!(result, 0);
        }

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_publish_stub_returns_negative() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let topic = b"test/topic";
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
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_subscribe_stub_returns_negative() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let topic = b"test/topic";
        let result = (ctx.subscribe)(ctx.host_data, topic.as_ptr(), topic.len());
        assert_eq!(result, -1);

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_task_progress_stub_returns_negative() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let task_id = b"task-123";
        let result =
            (ctx.report_task_progress)(ctx.host_data, task_id.as_ptr(), task_id.len(), 50);
        assert_eq!(result, -1);

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_task_complete_stub_returns_negative() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let task_id = b"task-123";
        let result_data = b"done";
        let result = (ctx.report_task_complete)(
            ctx.host_data,
            task_id.as_ptr(),
            task_id.len(),
            result_data.as_ptr(),
            result_data.len(),
        );
        assert_eq!(result, -1);

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_task_failed_stub_returns_negative() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let task_id = b"task-123";
        let error = b"failed";
        let result = (ctx.report_task_failed)(
            ctx.host_data,
            task_id.as_ptr(),
            task_id.len(),
            error.as_ptr(),
            error.len(),
        );
        assert_eq!(result, -1);

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_platform_invoke_stub_returns_negative() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let cap = b"sms";
        let method = b"send";
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
        // SAFETY: cleanup.
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
    fn test_set_state_with_null_host_returns_error() {
        let key = b"key";
        let val = b"val";
        let result = cb_set_state(
            std::ptr::null_mut(),
            key.as_ptr(),
            key.len(),
            val.as_ptr(),
            val.len(),
        );
        assert_eq!(result, -1);
    }

    #[test]
    fn test_delete_state_with_null_host_returns_error() {
        let key = b"key";
        let result = cb_delete_state(std::ptr::null_mut(), key.as_ptr(), key.len());
        assert_eq!(result, -1);
    }

    #[test]
    fn test_log_with_null_host_returns_error() {
        let msg = b"hello";
        let result = cb_log(std::ptr::null_mut(), 3, msg.as_ptr(), msg.len());
        assert_eq!(result, -1);
    }

    #[test]
    fn test_drop_host_data_clears_pointer() {
        let store = make_state_store();
        let mut ctx = create_plugin_context("test-plugin", store);
        assert!(!ctx.host_data.is_null());

        // SAFETY: we just created this context.
        unsafe { drop_host_data(&mut ctx) };
        assert!(ctx.host_data.is_null());
    }

    #[test]
    fn test_drop_host_data_null_is_safe() {
        let store = make_state_store();
        let mut ctx = create_plugin_context("test-plugin", store);

        // SAFETY: first drop.
        unsafe { drop_host_data(&mut ctx) };
        // SAFETY: second drop of already-null pointer should be a no-op.
        unsafe { drop_host_data(&mut ctx) };
        assert!(ctx.host_data.is_null());
    }

    #[test]
    fn test_state_store_shared_between_context_and_caller() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store.clone());

        // Write via callback
        let key = b"shared_key";
        let value = b"shared_value";
        (ctx.set_state)(
            ctx.host_data,
            key.as_ptr(),
            key.len(),
            value.as_ptr(),
            value.len(),
        );

        // Read via direct store access
        let s = store.read().unwrap();
        assert_eq!(s.get("shared_key").unwrap().as_slice(), b"shared_value");
        drop(s);

        // Write via direct store access
        store
            .write()
            .unwrap()
            .insert("external_key".to_string(), b"external_value".to_vec());

        // Read via callback
        let key2 = b"external_key";
        let mut buf = [0u8; 64];
        let n = (ctx.get_state)(
            ctx.host_data,
            key2.as_ptr(),
            key2.len(),
            buf.as_mut_ptr(),
            buf.len(),
        );
        assert_eq!(n, 14); // "external_value".len()
        assert_eq!(&buf[..14], b"external_value");

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }

    #[test]
    fn test_get_state_truncates_to_buffer_size() {
        let store = make_state_store();
        let ctx = create_plugin_context("test-plugin", store);

        let key = b"big";
        let value = b"this is a long value that exceeds the tiny buffer";
        (ctx.set_state)(
            ctx.host_data,
            key.as_ptr(),
            key.len(),
            value.as_ptr(),
            value.len(),
        );

        // Read into a tiny buffer
        let mut buf = [0u8; 4];
        let n = (ctx.get_state)(
            ctx.host_data,
            key.as_ptr(),
            key.len(),
            buf.as_mut_ptr(),
            buf.len(),
        );
        assert_eq!(n, 4);
        assert_eq!(&buf, b"this");

        let mut ctx = ctx;
        // SAFETY: cleanup.
        unsafe { drop_host_data(&mut ctx) };
    }
}
