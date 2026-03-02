/// Android platform bridge via JNI.
///
/// On Android, this module calls into the Kotlin `NebulaPlatformBridge` class
/// using JNI. On all other platforms, calls return an error indicating that
/// the Android platform is unavailable.
#[cfg(target_os = "android")]
pub mod android;

/// All 82 `@JvmStatic` methods in `NebulaPlatformBridge`, grouped by service.
///
/// This constant is used by tests to verify that the routing table covers
/// every method without gaps.
pub const ALL_ANDROID_ROUTES: &[(&str, &str)] = &[
    // Telephony
    ("telephony", "sendSms"),
    ("telephony", "readSmsInbox"),
    ("telephony", "executeUssd"),
    ("telephony", "getPhoneState"),
    // USSD
    ("ussd", "startUssdSession"),
    ("ussd", "sendUssdReply"),
    ("ussd", "cancelUssdSession"),
    // Calls
    ("calls", "triggerCall"),
    ("calls", "endCall"),
    ("calls", "readCallLog"),
    // Contacts
    ("contacts", "readContacts"),
    ("contacts", "addContact"),
    ("contacts", "deleteContact"),
    // Files
    ("files", "readFile"),
    ("files", "writeFile"),
    ("files", "listDirectory"),
    ("files", "deleteFile"),
    ("files", "fileExists"),
    ("files", "getStorageInfo"),
    // UI
    ("ui", "showToast"),
    ("ui", "showNotification"),
    // Accessibility
    ("accessibility", "isAccessibilityEnabled"),
    ("accessibility", "getScreenContent"),
    ("accessibility", "performClick"),
    ("accessibility", "performText"),
    // Notification monitoring
    ("notification", "getActiveNotifications"),
    ("notification", "dismissNotification"),
    ("notification", "isNotificationListenerEnabled"),
    // Email
    ("email", "sendEmail"),
    ("email", "readEmails"),
    // Device info
    ("device", "getDeviceInfo"),
    ("device", "getBatteryInfo"),
    ("device", "getNetworkInfo"),
    ("device", "getDeviceSignature"),
    // Location
    ("location", "getLastKnownLocation"),
    ("location", "isLocationEnabled"),
    // Camera
    ("camera", "capturePhoto"),
    ("camera", "listCameras"),
    // Audio
    ("audio", "startAudioRecording"),
    ("audio", "stopAudioRecording"),
    ("audio", "getVolume"),
    ("audio", "setVolume"),
    // WiFi
    ("wifi", "getWifiInfo"),
    ("wifi", "scanWifiNetworks"),
    ("wifi", "isWifiEnabled"),
    // Bluetooth
    ("bluetooth", "isBluetoothEnabled"),
    ("bluetooth", "getBluetoothDevices"),
    ("bluetooth", "scanBluetoothDevices"),
    // Clipboard
    ("clipboard", "getClipboard"),
    ("clipboard", "setClipboard"),
    // Apps
    ("apps", "listInstalledApps"),
    ("apps", "launchApp"),
    ("apps", "isAppInstalled"),
    // System / hardware
    ("system", "getCpuInfo"),
    ("system", "getCpuTemperature"),
    ("system", "getRamInfo"),
    ("system", "getSensorList"),
    // SIM
    ("sim", "getSimCards"),
    ("sim", "getSignalStrength"),
    ("sim", "getDefaultSimSlot"),
    ("sim", "setDefaultSimSlot"),
    ("sim", "getLastUsedSimSlot"),
    // Screen
    ("screen", "getScreenInfo"),
    ("screen", "setBrightness"),
    // Power
    ("power", "acquireWakeLock"),
    ("power", "releaseWakeLock"),
    ("power", "isBatteryOptimizationDisabled"),
    // WebView
    ("webview", "loadUrl"),
    ("webview", "executeJavascript"),
    // SMS receive queue
    ("sms", "getReceivedSms"),
    ("sms", "clearReceivedSms"),
    // Content observers
    ("observer", "startContentObserving"),
    ("observer", "stopContentObserving"),
    ("observer", "getContentChanges"),
    // Screen capture
    ("capture", "startScreenCapture"),
    ("capture", "stopScreenCapture"),
    ("capture", "isScreenCaptureActive"),
    ("capture", "getScreenFrame"),
    ("capture", "getScreenCaptureConfig"),
    // Foreground service
    ("service", "startForegroundService"),
    ("service", "stopForegroundService"),
    ("service", "isForegroundServiceRunning"),
];

/// Call an Android platform method via JNI.
///
/// On non-Android platforms, returns an error indicating the platform is
/// unavailable. On Android, delegates to `android::call_platform_bridge`.
///
/// # Arguments
///
/// * `service` - The service group (e.g. "telephony", "device", "files").
/// * `method` - The method name within that service (e.g. "sendSms", "getDeviceInfo").
/// * `args_json` - JSON-encoded arguments for the method.
///
/// # Returns
///
/// On success, a JSON string (or base64-encoded data for byte-returning methods).
/// On failure, a human-readable error message wrapped in `Err`.
pub fn invoke_android(service: &str, method: &str, args_json: &str) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        android::call_platform_bridge(service, method, args_json)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (service, method, args_json);
        Err("Android platform not available on this OS".to_string())
    }
}

/// Check whether a `(service, method)` pair is a known Android route.
///
/// This is useful for validating capability strings before attempting a JNI
/// call. It does not require an Android runtime -- it is a pure lookup against
/// the static routing table.
pub fn is_known_android_route(service: &str, method: &str) -> bool {
    ALL_ANDROID_ROUTES
        .iter()
        .any(|(s, m)| *s == service && *m == method)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::ipc::{InvokeRouter, InvokeTarget};

    // -----------------------------------------------------------------
    // Cross-platform fallback
    // -----------------------------------------------------------------

    #[test]
    fn invoke_android_returns_error_on_non_android() {
        let result = invoke_android("device", "getDeviceInfo", "{}");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("not available"),
            "Expected platform-unavailable error, got: {err}"
        );
    }

    #[test]
    fn invoke_android_returns_error_for_every_service_on_non_android() {
        for &(service, method) in ALL_ANDROID_ROUTES {
            let result = invoke_android(service, method, "{}");
            assert!(
                result.is_err(),
                "Expected error for {service}:{method} on non-Android"
            );
        }
    }

    // -----------------------------------------------------------------
    // Route table completeness
    // -----------------------------------------------------------------

    #[test]
    fn all_android_routes_has_82_entries() {
        assert_eq!(ALL_ANDROID_ROUTES.len(), 82);
    }

    #[test]
    fn all_routes_have_non_empty_service_and_method() {
        for &(service, method) in ALL_ANDROID_ROUTES {
            assert!(!service.is_empty(), "Empty service in route table");
            assert!(!method.is_empty(), "Empty method in route table");
        }
    }

    #[test]
    fn no_duplicate_routes() {
        let mut seen = std::collections::HashSet::new();
        for &(service, method) in ALL_ANDROID_ROUTES {
            let key = format!("{service}:{method}");
            assert!(seen.insert(key.clone()), "Duplicate route: {key}");
        }
    }

    #[test]
    fn is_known_android_route_returns_true_for_valid() {
        assert!(is_known_android_route("telephony", "sendSms"));
        assert!(is_known_android_route("device", "getDeviceInfo"));
        assert!(is_known_android_route("service", "isForegroundServiceRunning"));
    }

    #[test]
    fn is_known_android_route_returns_false_for_invalid() {
        assert!(!is_known_android_route("telephony", "nonexistent"));
        assert!(!is_known_android_route("bogus", "getDeviceInfo"));
        assert!(!is_known_android_route("", ""));
    }

    // -----------------------------------------------------------------
    // InvokeRouter integration -- every route parses correctly
    // -----------------------------------------------------------------

    #[test]
    fn every_route_produces_valid_android_invoke_target() {
        for &(service, method) in ALL_ANDROID_ROUTES {
            let capability = format!("android:{service}:{method}");
            let target = InvokeRouter::parse_target(&capability);
            match target {
                InvokeTarget::Android {
                    service: parsed_svc,
                    method: parsed_method,
                } => {
                    assert_eq!(parsed_svc, service, "Service mismatch for {capability}");
                    assert_eq!(parsed_method, method, "Method mismatch for {capability}");
                }
                other => panic!(
                    "Expected InvokeTarget::Android for {capability}, got {other:?}"
                ),
            }
        }
    }

    // -----------------------------------------------------------------
    // Service grouping coverage
    // -----------------------------------------------------------------

    #[test]
    fn all_expected_services_are_present() {
        let expected_services = [
            "telephony",
            "ussd",
            "calls",
            "contacts",
            "files",
            "ui",
            "accessibility",
            "notification",
            "email",
            "device",
            "location",
            "camera",
            "audio",
            "wifi",
            "bluetooth",
            "clipboard",
            "apps",
            "system",
            "sim",
            "screen",
            "power",
            "webview",
            "sms",
            "observer",
            "capture",
            "service",
        ];

        let actual_services: std::collections::HashSet<&str> =
            ALL_ANDROID_ROUTES.iter().map(|(s, _)| *s).collect();

        for expected in &expected_services {
            assert!(
                actual_services.contains(expected),
                "Missing service: {expected}"
            );
        }

        // Verify no unexpected services crept in
        for actual in &actual_services {
            assert!(
                expected_services.contains(actual),
                "Unexpected service: {actual}"
            );
        }
    }

    // -----------------------------------------------------------------
    // Method count per service
    // -----------------------------------------------------------------

    #[test]
    fn telephony_has_4_methods() {
        let count = ALL_ANDROID_ROUTES
            .iter()
            .filter(|(s, _)| *s == "telephony")
            .count();
        assert_eq!(count, 4);
    }

    #[test]
    fn files_has_6_methods() {
        let count = ALL_ANDROID_ROUTES
            .iter()
            .filter(|(s, _)| *s == "files")
            .count();
        assert_eq!(count, 6);
    }

    #[test]
    fn device_has_4_methods() {
        let count = ALL_ANDROID_ROUTES
            .iter()
            .filter(|(s, _)| *s == "device")
            .count();
        assert_eq!(count, 4);
    }

    #[test]
    fn sim_has_5_methods() {
        let count = ALL_ANDROID_ROUTES
            .iter()
            .filter(|(s, _)| *s == "sim")
            .count();
        assert_eq!(count, 5);
    }

    #[test]
    fn capture_has_5_methods() {
        let count = ALL_ANDROID_ROUTES
            .iter()
            .filter(|(s, _)| *s == "capture")
            .count();
        assert_eq!(count, 5);
    }

    #[test]
    fn service_has_3_methods() {
        let count = ALL_ANDROID_ROUTES
            .iter()
            .filter(|(s, _)| *s == "service")
            .count();
        assert_eq!(count, 3);
    }
}
