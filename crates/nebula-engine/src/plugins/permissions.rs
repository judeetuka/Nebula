use std::collections::{HashMap, HashSet};

use nebula_plugin_sdk::capabilities::PluginCapability;
use nebula_plugin_sdk::manifest::PluginManifest;
use serde::{Deserialize, Serialize};

/// Manages capability grants per plugin, controlled by the admin dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPolicy {
    /// Map of plugin_id -> set of granted capabilities.
    grants: HashMap<String, HashSet<PluginCapability>>,
    /// Default policy for new plugins.
    default_policy: DefaultPolicy,
}

/// Default policy for newly installed plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DefaultPolicy {
    /// Deny all capabilities until explicitly granted by admin.
    DenyAll,
    /// Auto-grant all capabilities declared in the manifest.
    GrantDeclared,
}

impl PermissionPolicy {
    /// Create a new permission policy with the given default.
    pub fn new(default_policy: DefaultPolicy) -> Self {
        Self {
            grants: HashMap::new(),
            default_policy,
        }
    }

    /// Grant a capability to a plugin.
    pub fn grant(&mut self, plugin_id: &str, capability: PluginCapability) {
        self.grants
            .entry(plugin_id.to_string())
            .or_default()
            .insert(capability);
    }

    /// Revoke a capability from a plugin.
    pub fn revoke(&mut self, plugin_id: &str, capability: &PluginCapability) {
        if let Some(caps) = self.grants.get_mut(plugin_id) {
            caps.remove(capability);
        }
    }

    /// Check if a plugin has a specific capability granted.
    pub fn is_granted(&self, plugin_id: &str, capability: &PluginCapability) -> bool {
        self.grants
            .get(plugin_id)
            .map_or(false, |caps| caps.contains(capability))
    }

    /// Get all granted capabilities for a plugin.
    pub fn get_grants(&self, plugin_id: &str) -> Option<&HashSet<PluginCapability>> {
        self.grants.get(plugin_id)
    }

    /// Apply default policy for a newly installed plugin.
    pub fn apply_default(&mut self, manifest: &PluginManifest) {
        match self.default_policy {
            DefaultPolicy::DenyAll => {
                // Ensure the plugin has an entry but with no grants
                self.grants
                    .entry(manifest.id.clone())
                    .or_default();
            }
            DefaultPolicy::GrantDeclared => {
                let caps = self
                    .grants
                    .entry(manifest.id.clone())
                    .or_default();
                for cap in &manifest.capabilities {
                    caps.insert(cap.clone());
                }
            }
        }
    }

    /// Serialize to JSON (for storage/transmission).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Failed to deserialize PermissionPolicy: {}", e))
    }

    /// Returns the default policy.
    pub fn default_policy(&self) -> &DefaultPolicy {
        &self.default_policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(id: &str, capabilities: Vec<PluginCapability>) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: format!("Plugin {}", id),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            author: "Test".to_string(),
            abi: "x86_64".to_string(),
            entry_symbol: "nebula_plugin_init".to_string(),
            capabilities,
            depends_on: vec![],
        }
    }

    // -------------------------------------------------------------------
    // Construction
    // -------------------------------------------------------------------

    #[test]
    fn test_new_deny_all_policy() {
        let policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        assert_eq!(*policy.default_policy(), DefaultPolicy::DenyAll);
    }

    #[test]
    fn test_new_grant_declared_policy() {
        let policy = PermissionPolicy::new(DefaultPolicy::GrantDeclared);
        assert_eq!(*policy.default_policy(), DefaultPolicy::GrantDeclared);
    }

    // -------------------------------------------------------------------
    // Grant / Revoke / Is_granted
    // -------------------------------------------------------------------

    #[test]
    fn test_grant_capability() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        policy.grant("plugin-a", PluginCapability::Sms);
        assert!(policy.is_granted("plugin-a", &PluginCapability::Sms));
    }

    #[test]
    fn test_not_granted_by_default() {
        let policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        assert!(!policy.is_granted("plugin-a", &PluginCapability::Sms));
    }

    #[test]
    fn test_revoke_capability() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        policy.grant("plugin-a", PluginCapability::Sms);
        assert!(policy.is_granted("plugin-a", &PluginCapability::Sms));

        policy.revoke("plugin-a", &PluginCapability::Sms);
        assert!(!policy.is_granted("plugin-a", &PluginCapability::Sms));
    }

    #[test]
    fn test_revoke_nonexistent_plugin_is_noop() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        policy.revoke("ghost", &PluginCapability::Sms); // Should not panic
    }

    #[test]
    fn test_revoke_nonexistent_capability_is_noop() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        policy.grant("plugin-a", PluginCapability::Sms);
        policy.revoke("plugin-a", &PluginCapability::Network); // Should not panic
        assert!(policy.is_granted("plugin-a", &PluginCapability::Sms));
    }

    #[test]
    fn test_multiple_grants_same_plugin() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        policy.grant("plugin-a", PluginCapability::Sms);
        policy.grant("plugin-a", PluginCapability::Network);
        policy.grant("plugin-a", PluginCapability::Location);

        assert!(policy.is_granted("plugin-a", &PluginCapability::Sms));
        assert!(policy.is_granted("plugin-a", &PluginCapability::Network));
        assert!(policy.is_granted("plugin-a", &PluginCapability::Location));
        assert!(!policy.is_granted("plugin-a", &PluginCapability::Camera));
    }

    #[test]
    fn test_grants_isolated_between_plugins() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        policy.grant("plugin-a", PluginCapability::Sms);
        policy.grant("plugin-b", PluginCapability::Network);

        assert!(policy.is_granted("plugin-a", &PluginCapability::Sms));
        assert!(!policy.is_granted("plugin-a", &PluginCapability::Network));
        assert!(!policy.is_granted("plugin-b", &PluginCapability::Sms));
        assert!(policy.is_granted("plugin-b", &PluginCapability::Network));
    }

    // -------------------------------------------------------------------
    // get_grants
    // -------------------------------------------------------------------

    #[test]
    fn test_get_grants_returns_set() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        policy.grant("plugin-a", PluginCapability::Sms);
        policy.grant("plugin-a", PluginCapability::Network);

        let grants = policy.get_grants("plugin-a").unwrap();
        assert_eq!(grants.len(), 2);
        assert!(grants.contains(&PluginCapability::Sms));
        assert!(grants.contains(&PluginCapability::Network));
    }

    #[test]
    fn test_get_grants_unknown_plugin() {
        let policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        assert!(policy.get_grants("ghost").is_none());
    }

    // -------------------------------------------------------------------
    // apply_default
    // -------------------------------------------------------------------

    #[test]
    fn test_apply_default_deny_all() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        let manifest = make_manifest(
            "plugin-a",
            vec![PluginCapability::Sms, PluginCapability::Network],
        );

        policy.apply_default(&manifest);

        // Plugin should have an entry but no grants
        assert!(!policy.is_granted("plugin-a", &PluginCapability::Sms));
        assert!(!policy.is_granted("plugin-a", &PluginCapability::Network));
        // But the entry exists
        assert!(policy.get_grants("plugin-a").is_some());
        assert!(policy.get_grants("plugin-a").unwrap().is_empty());
    }

    #[test]
    fn test_apply_default_grant_declared() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::GrantDeclared);
        let manifest = make_manifest(
            "plugin-a",
            vec![PluginCapability::Sms, PluginCapability::Network],
        );

        policy.apply_default(&manifest);

        assert!(policy.is_granted("plugin-a", &PluginCapability::Sms));
        assert!(policy.is_granted("plugin-a", &PluginCapability::Network));
        // Undeclared capability should not be granted
        assert!(!policy.is_granted("plugin-a", &PluginCapability::Camera));
    }

    #[test]
    fn test_apply_default_no_capabilities() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::GrantDeclared);
        let manifest = make_manifest("plugin-a", vec![]);

        policy.apply_default(&manifest);
        assert!(policy.get_grants("plugin-a").unwrap().is_empty());
    }

    // -------------------------------------------------------------------
    // JSON serialization
    // -------------------------------------------------------------------

    #[test]
    fn test_to_json_and_from_json_roundtrip() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::GrantDeclared);
        policy.grant("plugin-a", PluginCapability::Sms);
        policy.grant("plugin-a", PluginCapability::Network);
        policy.grant("plugin-b", PluginCapability::Camera);

        let json = policy.to_json();
        let restored = PermissionPolicy::from_json(&json).unwrap();

        assert!(restored.is_granted("plugin-a", &PluginCapability::Sms));
        assert!(restored.is_granted("plugin-a", &PluginCapability::Network));
        assert!(restored.is_granted("plugin-b", &PluginCapability::Camera));
        assert!(!restored.is_granted("plugin-b", &PluginCapability::Sms));
        assert_eq!(*restored.default_policy(), DefaultPolicy::GrantDeclared);
    }

    #[test]
    fn test_from_json_invalid() {
        let result = PermissionPolicy::from_json("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_to_json_empty_policy() {
        let policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        let json = policy.to_json();
        let restored = PermissionPolicy::from_json(&json).unwrap();
        assert_eq!(*restored.default_policy(), DefaultPolicy::DenyAll);
    }

    #[test]
    fn test_roundtrip_with_custom_capability() {
        let mut policy = PermissionPolicy::new(DefaultPolicy::DenyAll);
        policy.grant("plugin-a", PluginCapability::Custom("my_custom".to_string()));

        let json = policy.to_json();
        let restored = PermissionPolicy::from_json(&json).unwrap();

        assert!(restored.is_granted(
            "plugin-a",
            &PluginCapability::Custom("my_custom".to_string())
        ));
    }
}
