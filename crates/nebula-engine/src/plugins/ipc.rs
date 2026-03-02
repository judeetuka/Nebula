/// Routes `platform_invoke` calls based on capability prefix.
///
/// Routing rules:
/// - `"android:*"` -> JNI bridge to Kotlin (stub for now, wired in JNI phase)
/// - `"plugin:*"` -> Inter-plugin call via the registry
/// - `"engine:*"` -> Engine system command
pub struct InvokeRouter;

/// The resolved target of a `platform_invoke` call.
#[derive(Debug, PartialEq)]
pub enum InvokeTarget {
    /// Route to Android platform via JNI.
    Android { service: String, method: String },
    /// Route to another plugin's execute function.
    Plugin { plugin_id: String, action: String },
    /// Route to an engine system command.
    Engine { command: String },
    /// The capability string did not match any known routing prefix.
    Unknown,
}

impl InvokeRouter {
    /// Parse a capability string into a routing target.
    ///
    /// The capability string uses a colon-delimited prefix scheme:
    /// - `"android:telephony:sendSms"` -> `InvokeTarget::Android { service: "telephony", method: "sendSms" }`
    /// - `"plugin:classifier:classify"` -> `InvokeTarget::Plugin { plugin_id: "classifier", action: "classify" }`
    /// - `"engine:device_info"` -> `InvokeTarget::Engine { command: "device_info" }`
    /// - Anything else -> `InvokeTarget::Unknown`
    pub fn parse_target(capability: &str) -> InvokeTarget {
        if let Some(rest) = capability.strip_prefix("android:") {
            let parts: Vec<&str> = rest.splitn(2, ':').collect();
            if parts.len() == 2 {
                return InvokeTarget::Android {
                    service: parts[0].to_string(),
                    method: parts[1].to_string(),
                };
            }
        }
        if let Some(rest) = capability.strip_prefix("plugin:") {
            let parts: Vec<&str> = rest.splitn(2, ':').collect();
            if parts.len() == 2 {
                return InvokeTarget::Plugin {
                    plugin_id: parts[0].to_string(),
                    action: parts[1].to_string(),
                };
            }
        }
        if let Some(rest) = capability.strip_prefix("engine:") {
            return InvokeTarget::Engine {
                command: rest.to_string(),
            };
        }
        InvokeTarget::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // Android routing
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_android_target() {
        let target = InvokeRouter::parse_target("android:telephony:sendSms");
        assert_eq!(
            target,
            InvokeTarget::Android {
                service: "telephony".to_string(),
                method: "sendSms".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_android_with_complex_method() {
        let target = InvokeRouter::parse_target("android:camera:takePicture:front");
        assert_eq!(
            target,
            InvokeTarget::Android {
                service: "camera".to_string(),
                method: "takePicture:front".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_android_no_method_is_unknown() {
        // "android:telephony" has no second colon, so it can't split into service:method
        let target = InvokeRouter::parse_target("android:telephony");
        assert_eq!(target, InvokeTarget::Unknown);
    }

    // -------------------------------------------------------------------
    // Plugin routing
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_plugin_target() {
        let target = InvokeRouter::parse_target("plugin:classifier:classify");
        assert_eq!(
            target,
            InvokeTarget::Plugin {
                plugin_id: "classifier".to_string(),
                action: "classify".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_plugin_with_complex_action() {
        let target = InvokeRouter::parse_target("plugin:email:send:attachment");
        assert_eq!(
            target,
            InvokeTarget::Plugin {
                plugin_id: "email".to_string(),
                action: "send:attachment".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_plugin_no_action_is_unknown() {
        let target = InvokeRouter::parse_target("plugin:classifier");
        assert_eq!(target, InvokeTarget::Unknown);
    }

    // -------------------------------------------------------------------
    // Engine routing
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_engine_target() {
        let target = InvokeRouter::parse_target("engine:device_info");
        assert_eq!(
            target,
            InvokeTarget::Engine {
                command: "device_info".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_engine_with_subcommand() {
        let target = InvokeRouter::parse_target("engine:status:detailed");
        assert_eq!(
            target,
            InvokeTarget::Engine {
                command: "status:detailed".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_engine_empty_command() {
        let target = InvokeRouter::parse_target("engine:");
        assert_eq!(
            target,
            InvokeTarget::Engine {
                command: "".to_string(),
            }
        );
    }

    // -------------------------------------------------------------------
    // Unknown routing
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_unknown_prefix() {
        let target = InvokeRouter::parse_target("custom:something");
        assert_eq!(target, InvokeTarget::Unknown);
    }

    #[test]
    fn test_parse_empty_string() {
        let target = InvokeRouter::parse_target("");
        assert_eq!(target, InvokeTarget::Unknown);
    }

    #[test]
    fn test_parse_no_prefix() {
        let target = InvokeRouter::parse_target("just_a_string");
        assert_eq!(target, InvokeTarget::Unknown);
    }

    #[test]
    fn test_parse_partial_prefix() {
        let target = InvokeRouter::parse_target("android");
        assert_eq!(target, InvokeTarget::Unknown);
    }

    #[test]
    fn test_parse_case_sensitive() {
        // "Android:" (capital A) should not match
        let target = InvokeRouter::parse_target("Android:telephony:sendSms");
        assert_eq!(target, InvokeTarget::Unknown);
    }
}
