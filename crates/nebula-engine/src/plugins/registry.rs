use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{bail, Result};

use super::loader::LoadedPlugin;
use super::types::{PluginManifest, PluginState};

/// Central registry that tracks all installed plugins, their lifecycle
/// states, and per-plugin key-value stores.
///
/// The registry owns the `LoadedPlugin` instances and the shared state
/// stores that are passed to each plugin's `PluginContext`.
pub struct PluginRegistry {
    /// Map from plugin ID to the loaded (or stub) plugin instance.
    plugins: HashMap<String, LoadedPlugin>,
    /// Per-plugin key-value state. Each plugin's `PluginContext` holds an
    /// `Arc` clone of its entry so that state set via callbacks is visible
    /// to the registry and vice versa.
    plugin_state: HashMap<String, Arc<RwLock<HashMap<String, Vec<u8>>>>>,
    /// Directory where `.so` files are stored on disk.
    plugin_dir: String,
}

impl PluginRegistry {
    /// Create a new empty registry rooted at the given directory.
    pub fn new(plugin_dir: &str) -> Self {
        Self {
            plugins: HashMap::new(),
            plugin_state: HashMap::new(),
            plugin_dir: plugin_dir.to_string(),
        }
    }

    /// Install a plugin: load the `.so`, initialize it, and register it.
    ///
    /// A per-plugin state store is created (or reused if one already exists
    /// from a prior install of the same plugin ID). The plugin is moved to
    /// `Active` state on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin ID is already registered and active,
    /// or if loading/initialization fails.
    pub fn install_plugin(&mut self, manifest: PluginManifest, so_path: &str) -> Result<()> {
        let id = manifest.id.clone();

        if let Some(existing) = self.plugins.get(&id) {
            if existing.state == PluginState::Active {
                bail!("Plugin {} is already installed and active", id);
            }
        }

        // Create or reuse the shared state store
        let state_store = self
            .plugin_state
            .entry(id.clone())
            .or_insert_with(|| Arc::new(RwLock::new(HashMap::new())))
            .clone();

        let mut plugin = LoadedPlugin::load(so_path, manifest)
            .map_err(|e| anyhow::anyhow!("Failed to load plugin {}: {}", id, e))?;

        plugin
            .initialize(state_store)
            .map_err(|e| anyhow::anyhow!("Failed to initialize plugin {}: {}", id, e))?;

        self.plugins.insert(id, plugin);
        Ok(())
    }

    /// Install a stub plugin (no real `.so`). Useful for testing.
    ///
    /// The plugin is placed in `Downloaded` state. It cannot be executed
    /// but can be tracked, have state set on it, and be listed.
    pub fn install_stub(&mut self, manifest: PluginManifest, so_path: &str) {
        let id = manifest.id.clone();

        let _ = self
            .plugin_state
            .entry(id.clone())
            .or_insert_with(|| Arc::new(RwLock::new(HashMap::new())));

        let plugin = LoadedPlugin::new_stub(manifest, so_path);
        self.plugins.insert(id, plugin);
    }

    /// Uninstall a plugin: shut it down, unload the library, and remove
    /// it from the registry.
    ///
    /// The plugin's state store is **not** removed so that a future
    /// reinstall can pick up where it left off.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin ID is not registered.
    pub fn uninstall_plugin(&mut self, plugin_id: &str) -> Result<()> {
        let mut plugin = self
            .plugins
            .remove(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", plugin_id))?;

        if plugin.is_loaded() {
            let _ = plugin.shutdown();
            plugin.unload()?;
        }

        Ok(())
    }

    /// Hot-reload a plugin with a new `.so` binary.
    ///
    /// The plugin's key-value state is preserved across the reload because
    /// the same `Arc<RwLock<HashMap>>` is passed to the new `PluginContext`.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin ID is not registered or if loading
    /// the new binary fails.
    pub fn hot_reload_plugin(&mut self, plugin_id: &str, new_so_path: &str) -> Result<()> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", plugin_id))?;

        let state_store = self
            .plugin_state
            .get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("State store not found for plugin: {}", plugin_id))?
            .clone();

        plugin.hot_reload(new_so_path, state_store)
    }

    /// Get a reference to a loaded plugin by ID.
    pub fn get_plugin(&self, plugin_id: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(plugin_id)
    }

    /// List manifests of all registered plugins.
    pub fn list_plugins(&self) -> Vec<&PluginManifest> {
        self.plugins.values().map(|p| &p.manifest).collect()
    }

    /// Execute a plugin's main function.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found or not loaded.
    pub fn execute_plugin(
        &self,
        plugin_id: &str,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<i32> {
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", plugin_id))?;

        plugin.execute(input, output)
    }

    /// Read a value from a plugin's key-value state store.
    pub fn get_plugin_state(&self, plugin_id: &str, key: &str) -> Option<Vec<u8>> {
        let store = self.plugin_state.get(plugin_id)?;
        let s = store.read().ok()?;
        s.get(key).cloned()
    }

    /// Write a value into a plugin's key-value state store.
    ///
    /// Creates the store if the plugin ID is not yet known.
    pub fn set_plugin_state(&mut self, plugin_id: &str, key: &str, value: Vec<u8>) {
        let store = self
            .plugin_state
            .entry(plugin_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(HashMap::new())));

        if let Ok(mut s) = store.write() {
            s.insert(key.to_string(), value);
        }
    }

    /// Delete a key from a plugin's state store.
    ///
    /// Returns `true` if the key existed and was removed.
    pub fn delete_plugin_state(&mut self, plugin_id: &str, key: &str) -> bool {
        let store = match self.plugin_state.get(plugin_id) {
            Some(s) => s,
            None => return false,
        };

        match store.write() {
            Ok(mut s) => s.remove(key).is_some(),
            Err(_) => false,
        }
    }

    /// Returns the total number of registered plugins (any state).
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Returns the IDs of plugins that are currently in `Active` state.
    pub fn active_plugins(&self) -> Vec<&str> {
        self.plugins
            .iter()
            .filter(|(_, p)| p.state == PluginState::Active)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Returns the plugin directory path.
    pub fn plugin_dir(&self) -> &str {
        &self.plugin_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::types::PluginCapability;

    fn sample_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: format!("Plugin {}", id),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test".to_string(),
            abi: "x86_64".to_string(),
            entry_symbol: "nebula_plugin_init".to_string(),
            capabilities: vec![PluginCapability::Network],
        }
    }

    // -------------------------------------------------------------------
    // Construction
    // -------------------------------------------------------------------

    #[test]
    fn test_new_registry_is_empty() {
        let registry = PluginRegistry::new("/tmp/plugins");
        assert_eq!(registry.plugin_count(), 0);
        assert!(registry.list_plugins().is_empty());
        assert!(registry.active_plugins().is_empty());
    }

    #[test]
    fn test_registry_plugin_dir() {
        let registry = PluginRegistry::new("/data/nebula/plugins");
        assert_eq!(registry.plugin_dir(), "/data/nebula/plugins");
    }

    // -------------------------------------------------------------------
    // Stub install / uninstall
    // -------------------------------------------------------------------

    #[test]
    fn test_install_stub_increases_count() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn test_install_multiple_stubs() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        registry.install_stub(sample_manifest("plugin-b"), "/tmp/b.so");
        registry.install_stub(sample_manifest("plugin-c"), "/tmp/c.so");
        assert_eq!(registry.plugin_count(), 3);
    }

    #[test]
    fn test_install_stub_is_not_active() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        assert!(registry.active_plugins().is_empty());
    }

    #[test]
    fn test_get_plugin_after_stub_install() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        let manifest = sample_manifest("plugin-a");
        registry.install_stub(manifest.clone(), "/tmp/a.so");

        let plugin = registry.get_plugin("plugin-a");
        assert!(plugin.is_some());
        assert_eq!(plugin.unwrap().manifest, manifest);
    }

    #[test]
    fn test_get_plugin_not_found() {
        let registry = PluginRegistry::new("/tmp/plugins");
        assert!(registry.get_plugin("nonexistent").is_none());
    }

    #[test]
    fn test_list_plugins_returns_all_manifests() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("a"), "/tmp/a.so");
        registry.install_stub(sample_manifest("b"), "/tmp/b.so");

        let manifests = registry.list_plugins();
        assert_eq!(manifests.len(), 2);

        let ids: Vec<&str> = manifests.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
    }

    #[test]
    fn test_uninstall_plugin_removes_it() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        assert_eq!(registry.plugin_count(), 1);

        let result = registry.uninstall_plugin("plugin-a");
        assert!(result.is_ok());
        assert_eq!(registry.plugin_count(), 0);
        assert!(registry.get_plugin("plugin-a").is_none());
    }

    #[test]
    fn test_uninstall_nonexistent_plugin_fails() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        let result = registry.uninstall_plugin("ghost");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Plugin not found"));
    }

    #[test]
    fn test_install_stub_replaces_existing() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a_v1.so");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a_v2.so");
        assert_eq!(registry.plugin_count(), 1);
        assert_eq!(
            registry.get_plugin("plugin-a").unwrap().so_path(),
            "/tmp/a_v2.so"
        );
    }

    // -------------------------------------------------------------------
    // State management
    // -------------------------------------------------------------------

    #[test]
    fn test_set_and_get_plugin_state() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");

        registry.set_plugin_state("plugin-a", "counter", vec![42]);
        let val = registry.get_plugin_state("plugin-a", "counter");
        assert_eq!(val, Some(vec![42]));
    }

    #[test]
    fn test_get_state_nonexistent_plugin() {
        let registry = PluginRegistry::new("/tmp/plugins");
        assert!(registry.get_plugin_state("ghost", "key").is_none());
    }

    #[test]
    fn test_get_state_nonexistent_key() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        assert!(registry.get_plugin_state("plugin-a", "missing").is_none());
    }

    #[test]
    fn test_delete_plugin_state() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        registry.set_plugin_state("plugin-a", "key", vec![1, 2, 3]);

        let removed = registry.delete_plugin_state("plugin-a", "key");
        assert!(removed);
        assert!(registry.get_plugin_state("plugin-a", "key").is_none());
    }

    #[test]
    fn test_delete_nonexistent_key_returns_false() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        assert!(!registry.delete_plugin_state("plugin-a", "nonexistent"));
    }

    #[test]
    fn test_delete_state_nonexistent_plugin_returns_false() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        assert!(!registry.delete_plugin_state("ghost", "key"));
    }

    #[test]
    fn test_state_survives_uninstall() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        registry.set_plugin_state("plugin-a", "persistent", vec![99]);

        registry.uninstall_plugin("plugin-a").unwrap();

        let val = registry.get_plugin_state("plugin-a", "persistent");
        assert_eq!(val, Some(vec![99]));
    }

    #[test]
    fn test_set_state_for_unregistered_plugin_creates_store() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.set_plugin_state("future-plugin", "preconfig", vec![7]);
        let val = registry.get_plugin_state("future-plugin", "preconfig");
        assert_eq!(val, Some(vec![7]));
    }

    #[test]
    fn test_multiple_state_keys() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");

        registry.set_plugin_state("plugin-a", "key1", vec![1]);
        registry.set_plugin_state("plugin-a", "key2", vec![2]);
        registry.set_plugin_state("plugin-a", "key3", vec![3]);

        assert_eq!(registry.get_plugin_state("plugin-a", "key1"), Some(vec![1]));
        assert_eq!(registry.get_plugin_state("plugin-a", "key2"), Some(vec![2]));
        assert_eq!(registry.get_plugin_state("plugin-a", "key3"), Some(vec![3]));
    }

    #[test]
    fn test_overwrite_state() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");

        registry.set_plugin_state("plugin-a", "key", vec![1]);
        registry.set_plugin_state("plugin-a", "key", vec![2]);

        assert_eq!(registry.get_plugin_state("plugin-a", "key"), Some(vec![2]));
    }

    // -------------------------------------------------------------------
    // Execute stub (should fail since no .so is loaded)
    // -------------------------------------------------------------------

    #[test]
    fn test_execute_stub_plugin_fails() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");

        let input = b"test";
        let mut output = [0u8; 64];
        let result = registry.execute_plugin("plugin-a", input, &mut output);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_nonexistent_plugin_fails() {
        let registry = PluginRegistry::new("/tmp/plugins");
        let input = b"test";
        let mut output = [0u8; 64];
        let result = registry.execute_plugin("ghost", input, &mut output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Plugin not found"));
    }

    // -------------------------------------------------------------------
    // Active plugins filtering
    // -------------------------------------------------------------------

    #[test]
    fn test_active_plugins_empty_when_all_stubs() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("a"), "/tmp/a.so");
        registry.install_stub(sample_manifest("b"), "/tmp/b.so");
        assert!(registry.active_plugins().is_empty());
    }

    // -------------------------------------------------------------------
    // Real plugin install (ignored -- needs .so binary)
    // -------------------------------------------------------------------

    #[test]
    #[ignore = "Requires a real compiled .so plugin binary"]
    fn test_install_real_plugin() {
        let path = std::env::var("TEST_PLUGIN_PATH").unwrap();
        let mut registry = PluginRegistry::new("/tmp/plugins");
        let result = registry.install_plugin(sample_manifest("real"), &path);
        assert!(result.is_ok());
        assert_eq!(registry.active_plugins().len(), 1);
    }

    #[test]
    fn test_install_plugin_with_bad_path_fails() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        let result = registry.install_plugin(
            sample_manifest("bad"),
            "/tmp/nonexistent_99999.so",
        );
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // Hot reload (without real .so -- test error paths)
    // -------------------------------------------------------------------

    #[test]
    fn test_hot_reload_nonexistent_plugin_fails() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        let result = registry.hot_reload_plugin("ghost", "/tmp/new.so");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Plugin not found"));
    }

    #[test]
    fn test_hot_reload_stub_with_bad_path_fails() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");

        let result = registry.hot_reload_plugin("plugin-a", "/tmp/nonexistent_99999.so");
        assert!(result.is_err());
    }

    #[test]
    fn test_hot_reload_preserves_state() {
        let mut registry = PluginRegistry::new("/tmp/plugins");
        registry.install_stub(sample_manifest("plugin-a"), "/tmp/a.so");
        registry.set_plugin_state("plugin-a", "counter", vec![42]);

        // Attempt hot reload -- it will fail (no real .so) but the state
        // store should still contain the old value.
        let _ = registry.hot_reload_plugin("plugin-a", "/tmp/nonexistent.so");

        let val = registry.get_plugin_state("plugin-a", "counter");
        assert_eq!(val, Some(vec![42]));
    }
}
