//! Plugin manifest and lifecycle state definitions.
//!
//! A manifest describes a plugin binary: its identity, ABI target, entry
//! symbol, capabilities, and dependencies on other plugins.

use serde::{Deserialize, Serialize};

use crate::capabilities::PluginCapability;

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
    /// Plugin dependencies -- other plugins this one requires.
    #[serde(default)]
    pub depends_on: Vec<PluginDependency>,
}

/// A dependency on another plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginDependency {
    /// The plugin ID this depends on (e.g., "com.nebula.email").
    pub id: String,
    /// Minimum required version (semver string).
    pub min_version: String,
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
            depends_on: vec![],
        }
    }

    // -------------------------------------------------------------------
    // PluginManifest
    // -------------------------------------------------------------------

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
            depends_on: vec![],
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
            depends_on: vec![],
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
        // depends_on defaults to empty
        assert!(manifest.depends_on.is_empty());
    }

    #[test]
    fn test_manifest_with_dependencies() {
        let manifest = PluginManifest {
            id: "com.nebula.forwarder".to_string(),
            name: "SMS Forwarder".to_string(),
            version: "2.0.0".to_string(),
            description: "Forwards SMS via email".to_string(),
            author: "Nebula".to_string(),
            abi: "aarch64".to_string(),
            entry_symbol: "nebula_plugin_init".to_string(),
            capabilities: vec![PluginCapability::Sms, PluginCapability::Email],
            depends_on: vec![
                PluginDependency {
                    id: "com.nebula.email".to_string(),
                    min_version: "1.0.0".to_string(),
                },
                PluginDependency {
                    id: "com.nebula.contacts".to_string(),
                    min_version: "0.5.0".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, deserialized);
        assert_eq!(deserialized.depends_on.len(), 2);
        assert_eq!(deserialized.depends_on[0].id, "com.nebula.email");
        assert_eq!(deserialized.depends_on[0].min_version, "1.0.0");
    }

    // -------------------------------------------------------------------
    // PluginDependency
    // -------------------------------------------------------------------

    #[test]
    fn test_dependency_serialization_roundtrip() {
        let dep = PluginDependency {
            id: "com.nebula.email".to_string(),
            min_version: "1.2.3".to_string(),
        };

        let json = serde_json::to_string(&dep).unwrap();
        let deserialized: PluginDependency = serde_json::from_str(&json).unwrap();
        assert_eq!(dep, deserialized);
    }

    #[test]
    fn test_dependency_equality() {
        let dep_a = PluginDependency {
            id: "com.nebula.email".to_string(),
            min_version: "1.0.0".to_string(),
        };
        let dep_b = PluginDependency {
            id: "com.nebula.email".to_string(),
            min_version: "1.0.0".to_string(),
        };
        let dep_c = PluginDependency {
            id: "com.nebula.email".to_string(),
            min_version: "2.0.0".to_string(),
        };

        assert_eq!(dep_a, dep_b);
        assert_ne!(dep_a, dep_c);
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
