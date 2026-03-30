//! Plugin context definition.
//!
//! The `PluginContext` is the primary interface between a plugin and the host
//! engine. It is passed to the plugin at initialization time and contains
//! function pointers for calling back into the host.

/// The context passed to plugins at initialization.
/// Plugins call back into the host engine via these function pointers.
///
/// # Routing Convention
///
/// The `platform_invoke` function uses a capability string prefix to route calls:
/// - `"android:telephony:sendSms"` -> JNI -> Kotlin NebulaPlatformBridge
/// - `"plugin:email:readInbox"` -> Engine -> email plugin's execute()
/// - `"engine:status"` -> Engine -> system command
///
/// # Safety
///
/// All function pointers are valid for the lifetime of the plugin (from init to shutdown).
/// The `host_data` pointer must only be passed back to the callback functions -- never
/// dereferenced by the plugin.
#[repr(C)]
pub struct PluginContext {
    /// Opaque pointer to host-managed data. Pass to all callbacks.
    pub host_data: *mut std::ffi::c_void,

    // -- State Management ---------------------------------------------------
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
    pub delete_state:
        extern "C" fn(host: *mut std::ffi::c_void, key_ptr: *const u8, key_len: usize) -> i32,

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
    /// Returns 0 on success, -1 on error.
    pub publish: extern "C" fn(
        host: *mut std::ffi::c_void,
        topic_ptr: *const u8,
        topic_len: usize,
        payload_ptr: *const u8,
        payload_len: usize,
    ) -> i32,

    /// Subscribe to an MQTT topic.
    /// Returns 0 on success, -1 on error.
    pub subscribe:
        extern "C" fn(host: *mut std::ffi::c_void, topic_ptr: *const u8, topic_len: usize) -> i32,

    // -- Task management ----------------------------------------------------
    /// Report incremental progress on a task (0-100).
    /// Returns 0 on success, -1 on error.
    pub report_task_progress: extern "C" fn(
        host: *mut std::ffi::c_void,
        task_id_ptr: *const u8,
        task_id_len: usize,
        progress: u8,
    ) -> i32,

    /// Report that a task completed successfully.
    /// Returns 0 on success, -1 on error.
    pub report_task_complete: extern "C" fn(
        host: *mut std::ffi::c_void,
        task_id_ptr: *const u8,
        task_id_len: usize,
        result_ptr: *const u8,
        result_len: usize,
    ) -> i32,

    /// Report that a task failed.
    /// Returns 0 on success, -1 on error.
    pub report_task_failed: extern "C" fn(
        host: *mut std::ffi::c_void,
        task_id_ptr: *const u8,
        task_id_len: usize,
        error_ptr: *const u8,
        error_len: usize,
    ) -> i32,

    // -- Platform / Plugin / Engine Invocation ------------------------------
    /// Universal routing function.
    ///
    /// `capability_ptr` is a UTF-8 string like:
    /// - `"android:telephony:sendSms"` -> routes to Kotlin via JNI
    /// - `"plugin:classifier:classify"` -> routes to another plugin
    /// - `"engine:device_info"` -> routes to engine system command
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
