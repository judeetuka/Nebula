//! JNI bridge to the Kotlin `NebulaPlatformBridge` on Android.
//!
//! This module is only compiled when `target_os = "android"`. It uses the
//! `jni` crate to call `@JvmStatic` methods on the singleton
//! `com.nebula.nebula_node.platform.NebulaPlatformBridge` object.
//!
//! # Architecture
//!
//! The Rust engine runs on native threads (tokio, plugin threads, etc.).
//! Each call attaches the current thread to the JVM, finds the bridge class,
//! and invokes the appropriate static method. The JVM reference is obtained
//! via `JavaVM::get_created_jvms()` -- there is exactly one JVM per Android
//! process.
//!
//! # JNI Signatures
//!
//! Each Kotlin method maps to a JNI type signature. The helper functions
//! (`call_string_method`, `call_bool_method`, etc.) handle result extraction
//! and JNI exception clearing.

use jni::objects::{JClass, JObject, JString, JValue, JValueGen};
use jni::sys::{jboolean, jint, jlong};
use jni::JNIEnv;
use jni::JavaVM;
use std::sync::OnceLock;

/// Global JVM reference, set during JNI_OnLoad or from Flutter's JNI_OnLoad.
static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

/// Store the JVM pointer. Called from `JNI_OnLoad` (via flutter_rust_bridge).
pub fn set_jvm(vm: JavaVM) {
    if JAVA_VM.set(vm).is_err() {
        tracing::warn!("JVM already set, ignoring duplicate set_jvm call");
    }
}

/// The fully-qualified JNI class path for the Kotlin bridge.
const BRIDGE_CLASS: &str = "com/nebula/nebula_node/platform/NebulaPlatformBridge";

/// Call a `@JvmStatic` method on `NebulaPlatformBridge` via JNI.
///
/// The `service` and `method` parameters come from the `InvokeRouter` after
/// parsing a capability string like `"android:telephony:sendSms"` into
/// `service = "telephony"` and `method = "sendSms"`.
///
/// # Arguments
///
/// * `service` - The service group (e.g. "telephony", "device", "files").
/// * `method` - The Kotlin method name (e.g. "sendSms", "getDeviceInfo").
/// * `args_json` - JSON-encoded arguments for the method.
///
/// # Errors
///
/// Returns `Err` if the JVM cannot be found, the thread cannot attach, the
/// bridge class is missing, or the method call itself fails.
pub fn call_platform_bridge(
    service: &str,
    method: &str,
    args_json: &str,
) -> Result<String, String> {
    let jvm = get_jvm()?;

    // Attach the current native thread to the JVM. This is safe to call
    // repeatedly -- if the thread is already attached, it returns the
    // existing JNIEnv.
    let mut env = jvm
        .attach_current_thread()
        .map_err(|e| format!("Failed to attach thread to JVM: {e}"))?;

    let class = env
        .find_class(BRIDGE_CLASS)
        .map_err(|e| format!("Failed to find {BRIDGE_CLASS}: {e}"))?;

    route_call(&mut env, &class, service, method, args_json)
}

/// Obtain the stored JavaVM reference.
fn get_jvm() -> Result<&'static JavaVM, String> {
    JAVA_VM
        .get()
        .ok_or_else(|| "JVM not initialized -- set_jvm() was not called".to_string())
}

/// Route a `(service, method)` pair to the corresponding JNI call.
///
/// This is a large match table covering all 82 `@JvmStatic` methods in
/// `NebulaPlatformBridge`. Methods are grouped by service for readability.
fn route_call(
    env: &mut JNIEnv,
    class: &JClass,
    service: &str,
    method: &str,
    args_json: &str,
) -> Result<String, String> {
    // Parse args once; many methods need individual fields.
    let args: serde_json::Value =
        serde_json::from_str(args_json).map_err(|e| format!("Invalid args JSON: {e}"))?;

    match (service, method) {
        // =================================================================
        // TELEPHONY
        // =================================================================
        ("telephony", "sendSms") => {
            let phone = jstring(env, args["phone"].as_str().unwrap_or(""))?;
            let message = jstring(env, args["message"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "sendSms",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
                &[JValue::Object(&phone), JValue::Object(&message)],
            )
        }
        ("telephony", "readSmsInbox") => {
            let limit = args["limit"].as_i64().unwrap_or(50) as jint;
            call_string_method(
                env,
                class,
                "readSmsInbox",
                "(I)Ljava/lang/String;",
                &[JValue::Int(limit)],
            )
        }
        ("telephony", "executeUssd") => {
            let code = jstring(env, args["code"].as_str().unwrap_or(""))?;
            call_string_method(
                env,
                class,
                "executeUssd",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&code)],
            )
        }
        ("telephony", "getPhoneState") => {
            call_string_method(env, class, "getPhoneState", "()Ljava/lang/String;", &[])
        }

        // =================================================================
        // USSD MULTI-STEP SESSION
        // =================================================================
        ("ussd", "startUssdSession") => {
            let code = jstring(env, args["code"].as_str().unwrap_or(""))?;
            let sim_slot = args["simSlot"].as_i64().unwrap_or(0) as jint;
            call_string_method(
                env,
                class,
                "startUssdSession",
                "(Ljava/lang/String;I)Ljava/lang/String;",
                &[JValue::Object(&code), JValue::Int(sim_slot)],
            )
        }
        ("ussd", "sendUssdReply") => {
            let text = jstring(env, args["text"].as_str().unwrap_or(""))?;
            call_string_method(
                env,
                class,
                "sendUssdReply",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&text)],
            )
        }
        ("ussd", "cancelUssdSession") => {
            call_bool_method(env, class, "cancelUssdSession", "()Z", &[])
        }

        // =================================================================
        // CALL MANAGEMENT
        // =================================================================
        ("calls", "triggerCall") => {
            let number = jstring(env, args["number"].as_str().unwrap_or(""))?;
            let auto_hangup_ms = args["autoHangupMs"].as_i64().unwrap_or(0) as jlong;
            let sim_slot = args["simSlot"].as_i64().unwrap_or(0) as jint;
            call_bool_method(
                env,
                class,
                "triggerCall",
                "(Ljava/lang/String;JI)Z",
                &[
                    JValue::Object(&number),
                    JValue::Long(auto_hangup_ms),
                    JValue::Int(sim_slot),
                ],
            )
        }
        ("calls", "endCall") => call_bool_method(env, class, "endCall", "()Z", &[]),
        ("calls", "readCallLog") => {
            let log_type = jstring(env, args["type"].as_str().unwrap_or("all"))?;
            let limit = args["limit"].as_i64().unwrap_or(50) as jint;
            call_string_method(
                env,
                class,
                "readCallLog",
                "(Ljava/lang/String;I)Ljava/lang/String;",
                &[JValue::Object(&log_type), JValue::Int(limit)],
            )
        }

        // =================================================================
        // CONTACTS
        // =================================================================
        ("contacts", "readContacts") => {
            let limit = args["limit"].as_i64().unwrap_or(50) as jint;
            call_string_method(
                env,
                class,
                "readContacts",
                "(I)Ljava/lang/String;",
                &[JValue::Int(limit)],
            )
        }
        ("contacts", "addContact") => {
            let name = jstring(env, args["name"].as_str().unwrap_or(""))?;
            let phone = jstring(env, args["phone"].as_str().unwrap_or(""))?;
            let email = jstring(env, args["email"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "addContact",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
                &[
                    JValue::Object(&name),
                    JValue::Object(&phone),
                    JValue::Object(&email),
                ],
            )
        }
        ("contacts", "deleteContact") => {
            let contact_id = jstring(env, args["contactId"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "deleteContact",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&contact_id)],
            )
        }

        // =================================================================
        // FILE ACCESS
        // =================================================================
        ("files", "readFile") => {
            let path = jstring(env, args["path"].as_str().unwrap_or(""))?;
            call_bytearray_method(
                env,
                class,
                "readFile",
                "(Ljava/lang/String;)[B",
                &[JValue::Object(&path)],
            )
        }
        ("files", "writeFile") => {
            let path = jstring(env, args["path"].as_str().unwrap_or(""))?;
            let data_b64 = args["data"].as_str().unwrap_or("");
            let data_bytes = base64_decode(data_b64)?;
            let byte_array = env
                .byte_array_from_slice(&data_bytes)
                .map_err(|e| format!("Failed to create byte array: {e}"))?;
            call_bool_method(
                env,
                class,
                "writeFile",
                "(Ljava/lang/String;[B)Z",
                &[
                    JValue::Object(&path),
                    JValue::Object(&JObject::from(byte_array)),
                ],
            )
        }
        ("files", "listDirectory") => {
            let path = jstring(env, args["path"].as_str().unwrap_or(""))?;
            call_string_method(
                env,
                class,
                "listDirectory",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&path)],
            )
        }
        ("files", "deleteFile") => {
            let path = jstring(env, args["path"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "deleteFile",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&path)],
            )
        }
        ("files", "fileExists") => {
            let path = jstring(env, args["path"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "fileExists",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&path)],
            )
        }
        ("files", "getStorageInfo") => {
            call_string_method(env, class, "getStorageInfo", "()Ljava/lang/String;", &[])
        }

        // =================================================================
        // UI (Toast + Notification)
        // =================================================================
        ("ui", "showToast") => {
            let message = jstring(env, args["message"].as_str().unwrap_or(""))?;
            let long = args["long"].as_bool().unwrap_or(false);
            call_bool_method(
                env,
                class,
                "showToast",
                "(Ljava/lang/String;Z)Z",
                &[JValue::Object(&message), JValue::Bool(jboolean::from(long))],
            )
        }
        ("ui", "showNotification") => {
            let title = jstring(env, args["title"].as_str().unwrap_or(""))?;
            let body = jstring(env, args["body"].as_str().unwrap_or(""))?;
            let channel_id = jstring(env, args["channelId"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "showNotification",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
                &[
                    JValue::Object(&title),
                    JValue::Object(&body),
                    JValue::Object(&channel_id),
                ],
            )
        }

        // =================================================================
        // ACCESSIBILITY
        // =================================================================
        ("accessibility", "isAccessibilityEnabled") => {
            call_bool_method(env, class, "isAccessibilityEnabled", "()Z", &[])
        }
        ("accessibility", "getScreenContent") => {
            call_string_method(env, class, "getScreenContent", "()Ljava/lang/String;", &[])
        }
        ("accessibility", "performClick") => {
            let node_id = jstring(env, args["nodeId"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "performClick",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&node_id)],
            )
        }
        ("accessibility", "performText") => {
            let node_id = jstring(env, args["nodeId"].as_str().unwrap_or(""))?;
            let text = jstring(env, args["text"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "performText",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
                &[JValue::Object(&node_id), JValue::Object(&text)],
            )
        }

        // =================================================================
        // NOTIFICATION MONITORING
        // =================================================================
        ("notification", "getActiveNotifications") => call_string_method(
            env,
            class,
            "getActiveNotifications",
            "()Ljava/lang/String;",
            &[],
        ),
        ("notification", "dismissNotification") => {
            let key = jstring(env, args["key"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "dismissNotification",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&key)],
            )
        }
        ("notification", "isNotificationListenerEnabled") => {
            call_bool_method(env, class, "isNotificationListenerEnabled", "()Z", &[])
        }

        // =================================================================
        // EMAIL
        // =================================================================
        ("email", "sendEmail") => {
            let smtp_host = jstring(env, args["smtpHost"].as_str().unwrap_or(""))?;
            let port = args["port"].as_i64().unwrap_or(587) as jint;
            let username = jstring(env, args["username"].as_str().unwrap_or(""))?;
            let password = jstring(env, args["password"].as_str().unwrap_or(""))?;
            let to = jstring(env, args["to"].as_str().unwrap_or(""))?;
            let subject = jstring(env, args["subject"].as_str().unwrap_or(""))?;
            let body = jstring(env, args["body"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "sendEmail",
                "(Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
                &[
                    JValue::Object(&smtp_host),
                    JValue::Int(port),
                    JValue::Object(&username),
                    JValue::Object(&password),
                    JValue::Object(&to),
                    JValue::Object(&subject),
                    JValue::Object(&body),
                ],
            )
        }
        ("email", "readEmails") => {
            let imap_host = jstring(env, args["imapHost"].as_str().unwrap_or(""))?;
            let port = args["port"].as_i64().unwrap_or(993) as jint;
            let username = jstring(env, args["username"].as_str().unwrap_or(""))?;
            let password = jstring(env, args["password"].as_str().unwrap_or(""))?;
            let folder = jstring(env, args["folder"].as_str().unwrap_or("INBOX"))?;
            let limit = args["limit"].as_i64().unwrap_or(10) as jint;
            call_string_method(
                env,
                class,
                "readEmails",
                "(Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;Ljava/lang/String;I)Ljava/lang/String;",
                &[
                    JValue::Object(&imap_host),
                    JValue::Int(port),
                    JValue::Object(&username),
                    JValue::Object(&password),
                    JValue::Object(&folder),
                    JValue::Int(limit),
                ],
            )
        }

        // =================================================================
        // DEVICE INFO
        // =================================================================
        ("device", "getDeviceInfo") => {
            call_string_method(env, class, "getDeviceInfo", "()Ljava/lang/String;", &[])
        }
        ("device", "getBatteryInfo") => {
            call_string_method(env, class, "getBatteryInfo", "()Ljava/lang/String;", &[])
        }
        ("device", "getNetworkInfo") => {
            call_string_method(env, class, "getNetworkInfo", "()Ljava/lang/String;", &[])
        }
        ("device", "getDeviceSignature") => call_string_method(
            env,
            class,
            "getDeviceSignature",
            "()Ljava/lang/String;",
            &[],
        ),

        // =================================================================
        // LOCATION
        // =================================================================
        ("location", "getLastKnownLocation") => call_string_method(
            env,
            class,
            "getLastKnownLocation",
            "()Ljava/lang/String;",
            &[],
        ),
        ("location", "isLocationEnabled") => {
            call_bool_method(env, class, "isLocationEnabled", "()Z", &[])
        }

        // =================================================================
        // CAMERA
        // =================================================================
        ("camera", "capturePhoto") => {
            let output_path = jstring(env, args["outputPath"].as_str().unwrap_or(""))?;
            let camera_id = jstring(env, args["cameraId"].as_str().unwrap_or("back"))?;
            call_bool_method(
                env,
                class,
                "capturePhoto",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
                &[JValue::Object(&output_path), JValue::Object(&camera_id)],
            )
        }
        ("camera", "listCameras") => {
            call_string_method(env, class, "listCameras", "()Ljava/lang/String;", &[])
        }

        // =================================================================
        // AUDIO
        // =================================================================
        ("audio", "startAudioRecording") => {
            let output_path = jstring(env, args["outputPath"].as_str().unwrap_or(""))?;
            let max_duration_ms = args["maxDurationMs"].as_i64().unwrap_or(0) as jlong;
            call_bool_method(
                env,
                class,
                "startAudioRecording",
                "(Ljava/lang/String;J)Z",
                &[JValue::Object(&output_path), JValue::Long(max_duration_ms)],
            )
        }
        ("audio", "stopAudioRecording") => {
            call_bool_method(env, class, "stopAudioRecording", "()Z", &[])
        }
        ("audio", "getVolume") => {
            call_string_method(env, class, "getVolume", "()Ljava/lang/String;", &[])
        }
        ("audio", "setVolume") => {
            let stream = jstring(env, args["stream"].as_str().unwrap_or(""))?;
            let level = args["level"].as_i64().unwrap_or(0) as jint;
            call_bool_method(
                env,
                class,
                "setVolume",
                "(Ljava/lang/String;I)Z",
                &[JValue::Object(&stream), JValue::Int(level)],
            )
        }

        // =================================================================
        // WIFI
        // =================================================================
        ("wifi", "getWifiInfo") => {
            call_string_method(env, class, "getWifiInfo", "()Ljava/lang/String;", &[])
        }
        ("wifi", "scanWifiNetworks") => {
            call_string_method(env, class, "scanWifiNetworks", "()Ljava/lang/String;", &[])
        }
        ("wifi", "isWifiEnabled") => call_bool_method(env, class, "isWifiEnabled", "()Z", &[]),

        // =================================================================
        // BLUETOOTH
        // =================================================================
        ("bluetooth", "isBluetoothEnabled") => {
            call_bool_method(env, class, "isBluetoothEnabled", "()Z", &[])
        }
        ("bluetooth", "getBluetoothDevices") => call_string_method(
            env,
            class,
            "getBluetoothDevices",
            "()Ljava/lang/String;",
            &[],
        ),
        ("bluetooth", "scanBluetoothDevices") => call_string_method(
            env,
            class,
            "scanBluetoothDevices",
            "()Ljava/lang/String;",
            &[],
        ),

        // =================================================================
        // CLIPBOARD
        // =================================================================
        ("clipboard", "getClipboard") => {
            call_string_method(env, class, "getClipboard", "()Ljava/lang/String;", &[])
        }
        ("clipboard", "setClipboard") => {
            let text = jstring(env, args["text"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "setClipboard",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&text)],
            )
        }

        // =================================================================
        // APP MANAGEMENT
        // =================================================================
        ("apps", "listInstalledApps") => {
            call_string_method(env, class, "listInstalledApps", "()Ljava/lang/String;", &[])
        }
        ("apps", "launchApp") => {
            let package_name = jstring(env, args["packageName"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "launchApp",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&package_name)],
            )
        }
        ("apps", "isAppInstalled") => {
            let package_name = jstring(env, args["packageName"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "isAppInstalled",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&package_name)],
            )
        }

        // =================================================================
        // SYSTEM / HARDWARE INFO
        // =================================================================
        ("system", "getCpuInfo") => {
            call_string_method(env, class, "getCpuInfo", "()Ljava/lang/String;", &[])
        }
        ("system", "getCpuTemperature") => {
            call_string_method(env, class, "getCpuTemperature", "()Ljava/lang/String;", &[])
        }
        ("system", "getRamInfo") => {
            call_string_method(env, class, "getRamInfo", "()Ljava/lang/String;", &[])
        }
        ("system", "getSensorList") => {
            call_string_method(env, class, "getSensorList", "()Ljava/lang/String;", &[])
        }

        // =================================================================
        // SIM INFO
        // =================================================================
        ("sim", "getSimCards") => {
            call_string_method(env, class, "getSimCards", "()Ljava/lang/String;", &[])
        }
        ("sim", "getSignalStrength") => {
            call_int_method(env, class, "getSignalStrength", "()I", &[])
        }
        ("sim", "getDefaultSimSlot") => {
            call_int_method(env, class, "getDefaultSimSlot", "()I", &[])
        }
        ("sim", "setDefaultSimSlot") => {
            let slot = args["slot"].as_i64().unwrap_or(0) as jint;
            call_bool_method(
                env,
                class,
                "setDefaultSimSlot",
                "(I)Z",
                &[JValue::Int(slot)],
            )
        }
        ("sim", "getLastUsedSimSlot") => {
            call_int_method(env, class, "getLastUsedSimSlot", "()I", &[])
        }

        // =================================================================
        // SCREEN
        // =================================================================
        ("screen", "getScreenInfo") => {
            call_string_method(env, class, "getScreenInfo", "()Ljava/lang/String;", &[])
        }
        ("screen", "setBrightness") => {
            let level = args["level"].as_i64().unwrap_or(128) as jint;
            call_bool_method(env, class, "setBrightness", "(I)Z", &[JValue::Int(level)])
        }

        // =================================================================
        // POWER MANAGEMENT
        // =================================================================
        ("power", "acquireWakeLock") => {
            let tag = jstring(env, args["tag"].as_str().unwrap_or(""))?;
            let timeout_ms = args["timeoutMs"].as_i64().unwrap_or(0) as jlong;
            call_bool_method(
                env,
                class,
                "acquireWakeLock",
                "(Ljava/lang/String;J)Z",
                &[JValue::Object(&tag), JValue::Long(timeout_ms)],
            )
        }
        ("power", "releaseWakeLock") => {
            let tag = jstring(env, args["tag"].as_str().unwrap_or(""))?;
            call_bool_method(
                env,
                class,
                "releaseWakeLock",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&tag)],
            )
        }
        ("power", "isBatteryOptimizationDisabled") => {
            call_bool_method(env, class, "isBatteryOptimizationDisabled", "()Z", &[])
        }

        // =================================================================
        // WEBVIEW (Headless)
        // =================================================================
        ("webview", "loadUrl") => {
            let url = jstring(env, args["url"].as_str().unwrap_or(""))?;
            let timeout_ms = args["timeoutMs"].as_i64().unwrap_or(30000) as jlong;
            call_string_method(
                env,
                class,
                "loadUrl",
                "(Ljava/lang/String;J)Ljava/lang/String;",
                &[JValue::Object(&url), JValue::Long(timeout_ms)],
            )
        }
        ("webview", "executeJavascript") => {
            let script = jstring(env, args["script"].as_str().unwrap_or(""))?;
            call_string_method(
                env,
                class,
                "executeJavascript",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&script)],
            )
        }

        // =================================================================
        // SMS RECEIVE QUEUE
        // =================================================================
        ("sms", "getReceivedSms") => {
            call_string_method(env, class, "getReceivedSms", "()Ljava/lang/String;", &[])
        }
        ("sms", "clearReceivedSms") => call_bool_method(env, class, "clearReceivedSms", "()Z", &[]),

        // =================================================================
        // CONTENT OBSERVERS
        // =================================================================
        ("observer", "startContentObserving") => {
            call_bool_method(env, class, "startContentObserving", "()Z", &[])
        }
        ("observer", "stopContentObserving") => {
            call_bool_method(env, class, "stopContentObserving", "()Z", &[])
        }
        ("observer", "getContentChanges") => {
            call_string_method(env, class, "getContentChanges", "()Ljava/lang/String;", &[])
        }

        // =================================================================
        // SCREEN CAPTURE
        // =================================================================
        ("capture", "startScreenCapture") => {
            let width = args["width"].as_i64().unwrap_or(720) as jint;
            let height = args["height"].as_i64().unwrap_or(1280) as jint;
            let fps = args["fps"].as_i64().unwrap_or(15) as jint;
            let bitrate = args["bitrate"].as_i64().unwrap_or(1_000_000) as jint;
            call_bool_method(
                env,
                class,
                "startScreenCapture",
                "(IIII)Z",
                &[
                    JValue::Int(width),
                    JValue::Int(height),
                    JValue::Int(fps),
                    JValue::Int(bitrate),
                ],
            )
        }
        ("capture", "stopScreenCapture") => {
            call_bool_method(env, class, "stopScreenCapture", "()Z", &[])
        }
        ("capture", "isScreenCaptureActive") => {
            call_bool_method(env, class, "isScreenCaptureActive", "()Z", &[])
        }
        ("capture", "getScreenFrame") => {
            call_bytearray_method(env, class, "getScreenFrame", "()[B", &[])
        }
        ("capture", "getScreenCaptureConfig") => call_string_method(
            env,
            class,
            "getScreenCaptureConfig",
            "()Ljava/lang/String;",
            &[],
        ),

        // =================================================================
        // FOREGROUND SERVICE
        // =================================================================
        ("service", "startForegroundService") => {
            call_bool_method(env, class, "startForegroundService", "()Z", &[])
        }
        ("service", "stopForegroundService") => {
            call_bool_method(env, class, "stopForegroundService", "()Z", &[])
        }
        ("service", "isForegroundServiceRunning") => {
            call_bool_method(env, class, "isForegroundServiceRunning", "()Z", &[])
        }

        // =================================================================
        // CATCH-ALL
        // =================================================================
        _ => Err(format!("Unknown android method: {service}:{method}")),
    }
}

// =============================================================================
// JNI helper functions
// =============================================================================

/// Create a JNI string from a Rust `&str`.
fn jstring<'a>(env: &mut JNIEnv<'a>, s: &str) -> Result<JObject<'a>, String> {
    env.new_string(s)
        .map(JObject::from)
        .map_err(|e| format!("Failed to create JNI string: {e}"))
}

/// Call a static method that returns `String` (or `null`).
///
/// Returns the Java String converted to a Rust `String`. If the Java method
/// returns `null`, returns an empty string.
fn call_string_method(
    env: &mut JNIEnv,
    class: &JClass,
    method_name: &str,
    sig: &str,
    args: &[JValue],
) -> Result<String, String> {
    let result = env
        .call_static_method(class, method_name, sig, args)
        .map_err(|e| {
            // Clear any pending JNI exception before returning the error.
            let _ = env.exception_clear();
            format!("JNI call {method_name} failed: {e}")
        })?;

    check_and_clear_exception(env)?;

    match result {
        JValueGen::Object(obj) => {
            if obj.is_null() {
                return Ok(String::new());
            }
            let jstr = JString::from(obj);
            let rust_str = env
                .get_string(&jstr)
                .map_err(|e| format!("Failed to convert JNI string from {method_name}: {e}"))?;
            Ok(rust_str.into())
        }
        other => Err(format!(
            "{method_name} returned unexpected JNI type: {other:?}"
        )),
    }
}

/// Call a static method that returns `boolean`.
///
/// Returns a JSON string `"true"` or `"false"`.
fn call_bool_method(
    env: &mut JNIEnv,
    class: &JClass,
    method_name: &str,
    sig: &str,
    args: &[JValue],
) -> Result<String, String> {
    let result = env
        .call_static_method(class, method_name, sig, args)
        .map_err(|e| {
            let _ = env.exception_clear();
            format!("JNI call {method_name} failed: {e}")
        })?;

    check_and_clear_exception(env)?;

    match result {
        JValueGen::Bool(b) => Ok(if u8::from(b) != 0 { "true" } else { "false" }.to_string()),
        other => Err(format!(
            "{method_name} returned unexpected JNI type: {other:?}"
        )),
    }
}

/// Call a static method that returns `int`.
///
/// Returns the integer value as a JSON number string.
fn call_int_method(
    env: &mut JNIEnv,
    class: &JClass,
    method_name: &str,
    sig: &str,
    args: &[JValue],
) -> Result<String, String> {
    let result = env
        .call_static_method(class, method_name, sig, args)
        .map_err(|e| {
            let _ = env.exception_clear();
            format!("JNI call {method_name} failed: {e}")
        })?;

    check_and_clear_exception(env)?;

    match result {
        JValueGen::Int(i) => Ok(i.to_string()),
        other => Err(format!(
            "{method_name} returned unexpected JNI type: {other:?}"
        )),
    }
}

/// Call a static method that returns `byte[]` (or `null`).
///
/// Returns the bytes as a base64-encoded string. If the Java method returns
/// `null`, returns an empty string.
fn call_bytearray_method(
    env: &mut JNIEnv,
    class: &JClass,
    method_name: &str,
    sig: &str,
    args: &[JValue],
) -> Result<String, String> {
    let result = env
        .call_static_method(class, method_name, sig, args)
        .map_err(|e| {
            let _ = env.exception_clear();
            format!("JNI call {method_name} failed: {e}")
        })?;

    check_and_clear_exception(env)?;

    match result {
        JValueGen::Object(obj) => {
            if obj.is_null() {
                return Ok(String::new());
            }
            let byte_array = jni::objects::JByteArray::from(obj);
            let bytes = env
                .convert_byte_array(byte_array)
                .map_err(|e| format!("Failed to convert byte array from {method_name}: {e}"))?;
            Ok(base64_encode(&bytes))
        }
        other => Err(format!(
            "{method_name} returned unexpected JNI type: {other:?}"
        )),
    }
}

/// Check whether a JNI exception is pending and clear it.
///
/// If an exception occurred on the Java side, this clears it and returns
/// an `Err` with the exception's string representation.
fn check_and_clear_exception(env: &mut JNIEnv) -> Result<(), String> {
    if env.exception_check().unwrap_or(false) {
        // Describe prints to stderr (logcat on Android) for debugging.
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err("JNI exception occurred (see logcat for details)".to_string());
    }
    Ok(())
}

// =============================================================================
// Base64 helpers (no extra dependency -- simple RFC 4648 encode/decode)
// =============================================================================

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as base64 (standard alphabet with padding).
fn base64_encode(data: &[u8]) -> String {
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(BASE64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(BASE64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(BASE64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(BASE64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Decode a base64 string to bytes (standard alphabet with padding).
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim_end_matches('=');
    let mut result = Vec::with_capacity(input.len() * 3 / 4);

    let decode_char = |c: u8| -> Result<u32, String> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(format!("Invalid base64 character: {}", c as char)),
        }
    };

    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let a = decode_char(bytes[i])?;
        let b = if i + 1 < bytes.len() {
            decode_char(bytes[i + 1])?
        } else {
            0
        };
        let c = if i + 2 < bytes.len() {
            decode_char(bytes[i + 2])?
        } else {
            0
        };
        let d = if i + 3 < bytes.len() {
            decode_char(bytes[i + 3])?
        } else {
            0
        };

        let triple = (a << 18) | (b << 12) | (c << 6) | d;

        result.push(((triple >> 16) & 0xFF) as u8);
        if i + 2 < bytes.len() {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if i + 3 < bytes.len() {
            result.push((triple & 0xFF) as u8);
        }

        i += 4;
    }

    Ok(result)
}
