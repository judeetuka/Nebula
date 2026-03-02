use serde::{Deserialize, Serialize};

/// Describes a plugin binary: its identity, ABI target, entry symbol, and
/// the set of platform capabilities it requires.
///
/// A manifest is shipped alongside (or embedded in) the `.so` file and is
/// persisted by the registry so that plugin metadata survives engine restarts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginManifest {
    /// Unique identifier for this plugin (e.g. "com.nebula.sms-forwarder").
    pub id: String,
    /// Human-readable name shown in the admin dashboard.
    pub name: String,
    /// Semantic version string (e.g. "1.2.0").
    pub version: String,
    /// Short description of what the plugin does.
    pub description: String,
    /// Author or organization name.
    pub author: String,
    /// Target ABI the `.so` was compiled for (e.g. "aarch64", "armv7", "x86_64").
    pub abi: String,
    /// Name of the exported `extern "C"` init symbol. Default: `"nebula_plugin_init"`.
    pub entry_symbol: String,
    /// Platform capabilities this plugin declares it needs.
    pub capabilities: Vec<PluginCapability>,
}

/// A platform capability that a plugin may request access to.
///
/// The host decides at install time whether to grant each capability based
/// on the device's security policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PluginCapability {
    // Communication
    Sms,
    Ussd,
    Telephony,
    Email,
    // Monitoring
    Notification,
    Accessibility,
    // Storage & Files
    FileAccess,
    Storage,
    // Network & Connectivity
    Network,
    Wifi,
    Bluetooth,
    // Hardware & Sensors
    Camera,
    Audio,
    Location,
    Sensors,
    // System
    Clipboard,
    AppManagement,
    ScreenControl,
    PowerManagement,
    DeviceAdmin,
    WebView,
    // Contacts & Calendar
    Contacts,
    Calendar,
    // UI
    Overlay,
    // Extensible
    Custom(String),
}

/// Lifecycle state of a plugin within the registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PluginState {
    /// Binary is on disk but has not been loaded.
    Downloaded,
    /// The library is being dlopen'd and symbols resolved.
    Loading,
    /// Plugin is initialized and ready to execute tasks.
    Active,
    /// An error occurred during loading, initialization, or execution.
    Error { message: String },
    /// The plugin is being shut down and unloaded.
    Unloading,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // PluginManifest
    // -------------------------------------------------------------------

    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            id: "com.nebula.sms-forwarder".to_string(),
            name: "SMS Forwarder".to_string(),
            version: "1.0.0".to_string(),
            description: "Forwards incoming SMS to a remote endpoint".to_string(),
            author: "Nebula Team".to_string(),
            abi: "aarch64".to_string(),
            entry_symbol: "nebula_plugin_init".to_string(),
            capabilities: vec![PluginCapability::Sms, PluginCapability::Network],
        }
    }

    #[test]
    fn test_manifest_serialization_roundtrip() {
        let manifest = sample_manifest();
        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, deserialized);
    }

    #[test]
    fn test_manifest_json_fields() {
        let manifest = sample_manifest();
        let value: serde_json::Value = serde_json::to_value(&manifest).unwrap();

        assert_eq!(value["id"], "com.nebula.sms-forwarder");
        assert_eq!(value["name"], "SMS Forwarder");
        assert_eq!(value["version"], "1.0.0");
        assert_eq!(value["abi"], "aarch64");
        assert_eq!(value["entry_symbol"], "nebula_plugin_init");
    }

    #[test]
    fn test_manifest_with_no_capabilities() {
        let manifest = PluginManifest {
            id: "com.nebula.noop".to_string(),
            name: "No-Op".to_string(),
            version: "0.1.0".to_string(),
            description: "Does nothing".to_string(),
            author: "Test".to_string(),
            abi: "x86_64".to_string(),
            entry_symbol: "nebula_plugin_init".to_string(),
            capabilities: vec![],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, deserialized);
        assert!(deserialized.capabilities.is_empty());
    }

    #[test]
    fn test_manifest_with_custom_capability() {
        let manifest = PluginManifest {
            id: "com.nebula.custom".to_string(),
            name: "Custom Plugin".to_string(),
            version: "2.0.0".to_string(),
            description: "Uses a custom capability".to_string(),
            author: "Test".to_string(),
            abi: "armv7".to_string(),
            entry_symbol: "my_custom_init".to_string(),
            capabilities: vec![PluginCapability::Custom("camera".to_string())],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, deserialized);
    }

    #[test]
    fn test_manifest_deserialization_from_raw_json() {
        let raw = r#"{
            "id": "com.nebula.test",
            "name": "Test Plugin",
            "version": "0.0.1",
            "description": "A test plugin",
            "author": "Author",
            "abi": "x86_64",
            "entry_symbol": "nebula_plugin_init",
            "capabilities": ["Sms", "Network"]
        }"#;

        let manifest: PluginManifest = serde_json::from_str(raw).unwrap();
        assert_eq!(manifest.id, "com.nebula.test");
        assert_eq!(manifest.capabilities.len(), 2);
        assert_eq!(manifest.capabilities[0], PluginCapability::Sms);
        assert_eq!(manifest.capabilities[1], PluginCapability::Network);
    }

    // -------------------------------------------------------------------
    // PluginCapability
    // -------------------------------------------------------------------

    #[test]
    fn test_all_capability_variants_serialize() {
        let capabilities = vec![
            PluginCapability::Sms,
            PluginCapability::Ussd,
            PluginCapability::Telephony,
            PluginCapability::Email,
            PluginCapability::Notification,
            PluginCapability::Accessibility,
            PluginCapability::FileAccess,
            PluginCapability::Storage,
            PluginCapability::Network,
            PluginCapability::Wifi,
            PluginCapability::Bluetooth,
            PluginCapability::Camera,
            PluginCapability::Audio,
            PluginCapability::Location,
            PluginCapability::Sensors,
            PluginCapability::Clipboard,
            PluginCapability::AppManagement,
            PluginCapability::ScreenControl,
            PluginCapability::PowerManagement,
            PluginCapability::DeviceAdmin,
            PluginCapability::WebView,
            PluginCapability::Contacts,
            PluginCapability::Calendar,
            PluginCapability::Overlay,
            PluginCapability::Custom("my_cap".to_string()),
        ];

        for cap in &capabilities {
            let json = serde_json::to_string(cap).unwrap();
            let deserialized: PluginCapability = serde_json::from_str(&json).unwrap();
            assert_eq!(*cap, deserialized);
        }
    }

    #[test]
    fn test_capability_equality() {
        assert_eq!(PluginCapability::Sms, PluginCapability::Sms);
        assert_ne!(PluginCapability::Sms, PluginCapability::Ussd);
        assert_ne!(
            PluginCapability::Custom("a".to_string()),
            PluginCapability::Custom("b".to_string())
        );
        assert_eq!(
            PluginCapability::Custom("same".to_string()),
            PluginCapability::Custom("same".to_string())
        );
    }

    // -------------------------------------------------------------------
    // PluginState
    // -------------------------------------------------------------------

    #[test]
    fn test_plugin_state_serialization_roundtrip() {
        let states = vec![
            PluginState::Downloaded,
            PluginState::Loading,
            PluginState::Active,
            PluginState::Error {
                message: "symbol not found".to_string(),
            },
            PluginState::Unloading,
        ];

        for state in &states {
            let json = serde_json::to_string(state).unwrap();
            let deserialized: PluginState = serde_json::from_str(&json).unwrap();
            assert_eq!(*state, deserialized);
        }
    }

    #[test]
    fn test_plugin_state_equality() {
        assert_eq!(PluginState::Active, PluginState::Active);
        assert_ne!(PluginState::Active, PluginState::Loading);
        assert_eq!(
            PluginState::Error {
                message: "x".to_string()
            },
            PluginState::Error {
                message: "x".to_string()
            }
        );
        assert_ne!(
            PluginState::Error {
                message: "x".to_string()
            },
            PluginState::Error {
                message: "y".to_string()
            }
        );
    }

    #[test]
    fn test_plugin_state_clone() {
        let state = PluginState::Error {
            message: "test".to_string(),
        };
        let cloned = state.clone();
        assert_eq!(state, cloned);
    }

    #[test]
    fn test_plugin_state_debug() {
        let state = PluginState::Active;
        let debug = format!("{:?}", state);
        assert_eq!(debug, "Active");
    }
}
