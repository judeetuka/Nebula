use flutter_rust_bridge::frb;

use crate::api::node_api::with_engine_read;
use crate::plugins::types::PluginManifest;

/// List all installed plugins as a JSON array of manifests.
///
/// Each entry includes the plugin's id, name, version, description,
/// author, ABI, entry symbol, capabilities, and current state.
#[frb(sync)]
pub fn list_plugins() -> Result<String, String> {
    with_engine_read(|engine| {
        let registry = engine
            .plugin_registry()
            .read()
            .map_err(|e| format!("Plugin registry lock poisoned: {}", e))?;

        let manifests: Vec<serde_json::Value> = registry
            .list_plugins()
            .iter()
            .map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "version": m.version,
                    "description": m.description,
                    "author": m.author,
                    "abi": m.abi,
                    "entry_symbol": m.entry_symbol,
                    "capabilities": m.capabilities,
                })
            })
            .collect();

        serde_json::to_string(&manifests)
            .map_err(|e| format!("Failed to serialize plugin list: {}", e))
    })
}

/// Get detailed information about a specific plugin.
///
/// Returns a JSON object with the manifest fields plus the current
/// plugin state. Returns an error if the plugin is not found.
#[frb(sync)]
pub fn get_plugin_info(plugin_id: String) -> Result<String, String> {
    with_engine_read(|engine| {
        let registry = engine
            .plugin_registry()
            .read()
            .map_err(|e| format!("Plugin registry lock poisoned: {}", e))?;

        let plugin = registry
            .get_plugin(&plugin_id)
            .ok_or_else(|| format!("Plugin not found: {}", plugin_id))?;

        let info = serde_json::json!({
            "id": plugin.manifest.id,
            "name": plugin.manifest.name,
            "version": plugin.manifest.version,
            "description": plugin.manifest.description,
            "author": plugin.manifest.author,
            "abi": plugin.manifest.abi,
            "entry_symbol": plugin.manifest.entry_symbol,
            "capabilities": plugin.manifest.capabilities,
            "state": plugin.state,
        });

        serde_json::to_string(&info).map_err(|e| format!("Failed to serialize plugin info: {}", e))
    })
}

/// Install a plugin from a manifest and `.so` file path.
///
/// The manifest is provided as a JSON string. The `so_path` points to the
/// shared object binary on disk. The plugin is loaded and initialized
/// immediately.
#[frb(sync)]
pub fn install_plugin(manifest_json: String, so_path: String) -> Result<(), String> {
    with_engine_read(|engine| {
        let manifest: PluginManifest = serde_json::from_str(&manifest_json)
            .map_err(|e| format!("Invalid manifest JSON: {}", e))?;

        let mut registry = engine
            .plugin_registry()
            .write()
            .map_err(|e| format!("Plugin registry lock poisoned: {}", e))?;

        registry
            .install_plugin(manifest, &so_path)
            .map_err(|e| format!("Failed to install plugin: {}", e))
    })
}

/// Uninstall a plugin by ID.
///
/// Shuts down the plugin, unloads the library, and removes it from the
/// registry. The plugin's state store is preserved for potential reinstall.
#[frb(sync)]
pub fn uninstall_plugin(plugin_id: String) -> Result<(), String> {
    with_engine_read(|engine| {
        let mut registry = engine
            .plugin_registry()
            .write()
            .map_err(|e| format!("Plugin registry lock poisoned: {}", e))?;

        registry
            .uninstall_plugin(&plugin_id)
            .map_err(|e| format!("Failed to uninstall plugin: {}", e))
    })
}

/// Execute a plugin's main function.
///
/// Accepts a JSON input string and returns the plugin's output as a JSON
/// string. The input is UTF-8 encoded and passed to the plugin as raw
/// bytes; the output is read from the plugin's output buffer and returned
/// as a string.
#[frb(sync)]
pub fn execute_plugin(plugin_id: String, input_json: String) -> Result<String, String> {
    with_engine_read(|engine| {
        let registry = engine
            .plugin_registry()
            .read()
            .map_err(|e| format!("Plugin registry lock poisoned: {}", e))?;

        let input = input_json.as_bytes();
        let mut output = vec![0u8; 4096];

        let bytes_written = registry
            .execute_plugin(&plugin_id, input, &mut output)
            .map_err(|e| format!("Failed to execute plugin: {}", e))?;

        if bytes_written < 0 {
            return Err(format!(
                "Plugin execution returned error code: {}",
                bytes_written
            ));
        }

        let result = String::from_utf8(output[..bytes_written as usize].to_vec())
            .map_err(|e| format!("Plugin output is not valid UTF-8: {}", e))?;

        Ok(result)
    })
}
