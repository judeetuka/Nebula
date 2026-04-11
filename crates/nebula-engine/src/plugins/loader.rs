use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, RwLock};

use anyhow::{bail, Context, Result};
use libloading::{Library, Symbol};
use sha2::{Digest, Sha256};

use super::sdk::{create_plugin_context, drop_host_data, PluginContext};
use super::types::{PluginManifest, PluginState};

// Use ABI type definitions from the SDK.
use nebula_plugin_sdk::abi::{PluginExecuteFn, PluginInitFn, PluginShutdownFn};

/// Not used yet -- kept as part of the ABI specification for future use.
#[allow(dead_code)]
type PluginVersionFn = nebula_plugin_sdk::abi::PluginVersionFn;

/// A plugin that has been (or can be) loaded from a shared object file.
///
/// Manages the full lifecycle: load -> initialize -> execute -> shutdown -> unload.
/// The `library` field is `None` when the plugin is not loaded (either before
/// first load or after explicit unload).
pub struct LoadedPlugin {
    /// Metadata describing the plugin.
    pub manifest: PluginManifest,
    /// Current lifecycle state.
    pub state: PluginState,
    /// The loaded dynamic library handle. `None` when not loaded.
    library: Option<Library>,
    /// The `PluginContext` handed to the plugin at init time.
    /// Owns the `HostData` allocation behind `host_data`.
    context: Option<Box<PluginContext>>,
    /// Path to the `.so` file on disk.
    so_path: String,
}

/// Manual `Debug` implementation because `Library` and `PluginContext`
/// do not derive `Debug`.
impl fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("manifest", &self.manifest)
            .field("state", &self.state)
            .field("library_loaded", &self.library.is_some())
            .field("context_present", &self.context.is_some())
            .field("so_path", &self.so_path)
            .finish()
    }
}

/// Verify an Ed25519 signature over the SHA-256 digest of a plugin binary.
pub fn verify_plugin_signature(so_path: &str, sig_path: &str, pub_key: &[u8; 32]) -> Result<()> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let so_bytes = std::fs::read(so_path).with_context(|| format!("read plugin: {so_path}"))?;
    let sig_bytes = std::fs::read(sig_path).with_context(|| format!("read sig: {sig_path}"))?;
    let digest = Sha256::digest(&so_bytes);
    let signature =
        Signature::from_slice(&sig_bytes).map_err(|e| anyhow::anyhow!("bad sig: {e}"))?;
    let vk = VerifyingKey::from_bytes(pub_key).map_err(|e| anyhow::anyhow!("bad key: {e}"))?;
    vk.verify(&digest, &signature)
        .map_err(|e| anyhow::anyhow!("Signature FAILED: {e}"))
}

impl LoadedPlugin {
    /// Load a shared object from `path` and verify that the required ABI
    /// symbols are present.
    ///
    /// The plugin is **not** initialized yet -- call [`initialize`] next.
    ///
    /// # Errors
    ///
    /// Returns an error if `dlopen` fails or if the library does not export
    /// the symbols named by `manifest.entry_symbol`, `nebula_plugin_execute`,
    /// and `nebula_plugin_shutdown`.
    pub fn load(path: &str, manifest: PluginManifest) -> Result<Self> {
        let mut plugin = Self {
            manifest,
            state: PluginState::Loading,
            library: None,
            context: None,
            so_path: path.to_string(),
        };

        // S-3: Check Ed25519 signature if a verification key is configured.
        let sig_path = format!("{path}.sig");
        if let Ok(hex_key) = std::env::var("NEBULA_PLUGIN_VERIFY_KEY") {
            let key_bytes = hex::decode(&hex_key).context("NEBULA_PLUGIN_VERIFY_KEY bad hex")?;
            if key_bytes.len() != 32 {
                bail!("NEBULA_PLUGIN_VERIFY_KEY must be 32 bytes (64 hex chars)");
            }
            let pub_key: [u8; 32] = key_bytes.try_into().unwrap();
            if Path::new(&sig_path).exists() {
                verify_plugin_signature(path, &sig_path, &pub_key)
                    .with_context(|| format!("Signature verification failed for {path}"))?;
                tracing::info!(plugin = %path, "Plugin signature verified");
            } else {
                bail!("Plugin signature not found at {sig_path} — refusing unsigned plugin");
            }
        } else if Path::new(&sig_path).exists() {
            tracing::warn!(plugin = %path, "Plugin has .sig but no NEBULA_PLUGIN_VERIFY_KEY — skipping verification");
        }

        // SAFETY: Loading a shared library is inherently unsafe. We mitigate by:
        //   1. Validating plugin manifests.
        //   2. Verifying Ed25519 signature when key is configured.
        //   3. Verifying required symbols exist before calling.
        let lib = unsafe {
            Library::new(path).with_context(|| format!("Failed to dlopen plugin at {}", path))?
        };

        // Verify required symbols exist (but do not call them yet).
        let entry = &plugin.manifest.entry_symbol;
        {
            // SAFETY: We are only resolving the symbol address, not calling it.
            // The symbol name comes from the manifest, which is user-provided but
            // the worst that can happen is a lookup failure (Err), not UB.
            let _init: Symbol<PluginInitFn> = unsafe {
                lib.get(entry.as_bytes())
                    .with_context(|| format!("Missing init symbol: {}", entry))?
            };
            let _exec: Symbol<PluginExecuteFn> = unsafe {
                lib.get(b"nebula_plugin_execute")
                    .with_context(|| "Missing symbol: nebula_plugin_execute")?
            };
            let _shutdown: Symbol<PluginShutdownFn> = unsafe {
                lib.get(b"nebula_plugin_shutdown")
                    .with_context(|| "Missing symbol: nebula_plugin_shutdown")?
            };
        }

        plugin.library = Some(lib);
        plugin.state = PluginState::Downloaded; // Loaded but not yet initialized
        Ok(plugin)
    }

    /// Create a `LoadedPlugin` in the `Downloaded` state without actually
    /// loading a `.so` file.
    ///
    /// This is useful for testing registry operations where real shared
    /// objects are not available.
    pub fn new_stub(manifest: PluginManifest, so_path: &str) -> Self {
        Self {
            manifest,
            state: PluginState::Downloaded,
            library: None,
            context: None,
            so_path: so_path.to_string(),
        }
    }

    /// Initialize the plugin by calling its init symbol with the provided
    /// `PluginContext`.
    ///
    /// The context is stored internally so that its `HostData` (and by
    /// extension the shared state store) lives for the plugin's lifetime.
    ///
    /// # Errors
    ///
    /// Returns an error if the library is not loaded or the init function
    /// returns a non-zero code.
    pub fn initialize(&mut self, state_store: Arc<RwLock<HashMap<String, Vec<u8>>>>) -> Result<()> {
        let lib = self
            .library
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Library not loaded"))?;

        let ctx = create_plugin_context(&self.manifest.id, state_store);
        let ctx = Box::new(ctx);

        let entry = &self.manifest.entry_symbol;
        // SAFETY: We verified this symbol exists in `load()`. The
        // `PluginContext` pointer is valid for the duration of this call
        // and the plugin is contractually obligated to only store it (not
        // free it).
        let init_fn: Symbol<PluginInitFn> = unsafe {
            lib.get(entry.as_bytes())
                .with_context(|| format!("Failed to resolve init symbol: {}", entry))?
        };

        // SAFETY: Calling foreign code through the ABI. The init function
        // receives a valid `PluginContext` pointer. If it returns non-zero
        // we treat it as a failure.
        let result = unsafe { init_fn(&*ctx as *const PluginContext) };

        if result != 0 {
            self.state = PluginState::Error {
                message: format!("Plugin init returned error code: {}", result),
            };
            bail!("Plugin init returned error code: {}", result);
        }

        self.context = Some(ctx);
        self.state = PluginState::Active;
        Ok(())
    }

    /// Execute the plugin's main function with the given input/output buffers.
    ///
    /// Returns the number of bytes the plugin wrote to `output`, or a
    /// negative error code.
    ///
    /// # Errors
    ///
    /// Returns an error if the library is not loaded.
    pub fn execute(&self, input: &[u8], output: &mut [u8]) -> Result<i32> {
        let lib = self
            .library
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Library not loaded"))?;

        // SAFETY: Symbol was verified in `load()`. Input and output slices
        // are valid Rust slices so their pointers and lengths are correct.
        let exec_fn: Symbol<PluginExecuteFn> = unsafe {
            lib.get(b"nebula_plugin_execute")
                .with_context(|| "Failed to resolve nebula_plugin_execute")?
        };

        // SAFETY: Calling foreign code. The input pointer is valid for
        // `input.len()` bytes, and the output pointer is valid for
        // `output.len()` bytes. The plugin must not write beyond
        // `output.len()`.
        let result = unsafe {
            exec_fn(
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                output.len(),
            )
        };

        Ok(result)
    }

    /// Call the plugin's shutdown hook.
    ///
    /// This notifies the plugin to release its resources. The library
    /// remains loaded until [`unload`] is called.
    ///
    /// # Errors
    ///
    /// Returns an error if the library is not loaded or shutdown returns
    /// a non-zero code.
    pub fn shutdown(&mut self) -> Result<()> {
        let lib = self
            .library
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Library not loaded"))?;

        self.state = PluginState::Unloading;

        // SAFETY: Symbol was verified in `load()`.
        let shutdown_fn: Symbol<PluginShutdownFn> = unsafe {
            lib.get(b"nebula_plugin_shutdown")
                .with_context(|| "Failed to resolve nebula_plugin_shutdown")?
        };

        // SAFETY: Calling foreign code's shutdown routine.
        let result = unsafe { shutdown_fn() };

        if result != 0 {
            self.state = PluginState::Error {
                message: format!("Plugin shutdown returned error code: {}", result),
            };
            bail!("Plugin shutdown returned error code: {}", result);
        }

        Ok(())
    }

    /// Unload the dynamic library (triggers `dlclose`).
    ///
    /// Also frees the `HostData` allocation behind the `PluginContext`.
    /// After this call, the plugin cannot be executed. Call [`load`] to
    /// reload.
    pub fn unload(&mut self) -> Result<()> {
        // Free HostData first, while the library is still loaded
        // (in case the plugin stored references to it).
        if let Some(ref mut ctx) = self.context {
            // SAFETY: The plugin has been shut down and will not invoke
            // any more callbacks, so it is safe to free the HostData.
            unsafe { drop_host_data(ctx) };
        }
        self.context = None;

        // Dropping the Library triggers dlclose.
        self.library = None;
        self.state = PluginState::Downloaded;
        Ok(())
    }

    /// Perform a hot-reload: shutdown -> unload -> load new binary ->
    /// initialize with the same state store.
    ///
    /// **WARNING (M-3):** On Android/Bionic, `dlclose` does not always unmap
    /// shared library memory. Each hot-reload may leak the old `.so`'s pages.
    /// Limit hot-reloads to a few per session; restart the app for a clean slate.
    ///
    /// The plugin's key-value state is preserved because the `state_store`
    /// `Arc` is cloned into the new `PluginContext`.
    ///
    /// # Errors
    ///
    /// Returns an error if any step in the sequence fails.
    pub fn hot_reload(
        &mut self,
        new_path: &str,
        state_store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    ) -> Result<()> {
        // Shutdown the current instance if it is loaded
        if self.library.is_some() {
            // Best-effort shutdown -- if the library has no shutdown symbol
            // (shouldn't happen after a successful load) we still proceed.
            let _ = self.shutdown();
            self.unload()?;
        }

        // Load the new binary
        let manifest = self.manifest.clone();
        // SAFETY: `Library::new` is the only unsafe operation and is
        // contained within `load()` with the same justification.
        let lib = unsafe {
            Library::new(new_path)
                .with_context(|| format!("Failed to dlopen new plugin at {}", new_path))?
        };

        // Verify symbols
        {
            let entry = &manifest.entry_symbol;
            let _init: Symbol<PluginInitFn> = unsafe {
                lib.get(entry.as_bytes())
                    .with_context(|| format!("Missing init symbol: {}", entry))?
            };
            let _exec: Symbol<PluginExecuteFn> = unsafe {
                lib.get(b"nebula_plugin_execute")
                    .with_context(|| "Missing symbol: nebula_plugin_execute")?
            };
            let _shutdown: Symbol<PluginShutdownFn> = unsafe {
                lib.get(b"nebula_plugin_shutdown")
                    .with_context(|| "Missing symbol: nebula_plugin_shutdown")?
            };
        }

        self.library = Some(lib);
        self.so_path = new_path.to_string();

        // Initialize with the (potentially pre-populated) state store
        self.initialize(state_store)?;

        Ok(())
    }

    /// Returns `true` if the dynamic library is currently loaded.
    pub fn is_loaded(&self) -> bool {
        self.library.is_some()
    }

    /// Returns the path to the `.so` file on disk.
    pub fn so_path(&self) -> &str {
        &self.so_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::types::{PluginCapability, PluginManifest, PluginState};

    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            id: "com.nebula.test".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            author: "Test".to_string(),
            abi: "x86_64".to_string(),
            entry_symbol: "nebula_plugin_init".to_string(),
            capabilities: vec![PluginCapability::Network],
            depends_on: vec![],
        }
    }

    #[test]
    fn test_new_stub_creates_downloaded_state() {
        let plugin = LoadedPlugin::new_stub(sample_manifest(), "/tmp/test.so");
        assert_eq!(plugin.state, PluginState::Downloaded);
        assert!(!plugin.is_loaded());
        assert_eq!(plugin.so_path(), "/tmp/test.so");
    }

    #[test]
    fn test_new_stub_preserves_manifest() {
        let manifest = sample_manifest();
        let plugin = LoadedPlugin::new_stub(manifest.clone(), "/tmp/test.so");
        assert_eq!(plugin.manifest, manifest);
    }

    #[test]
    fn test_stub_has_no_library() {
        let plugin = LoadedPlugin::new_stub(sample_manifest(), "/tmp/test.so");
        assert!(!plugin.is_loaded());
    }

    #[test]
    fn test_stub_unload_is_idempotent() {
        let mut plugin = LoadedPlugin::new_stub(sample_manifest(), "/tmp/test.so");
        let result = plugin.unload();
        assert!(result.is_ok());
        assert!(!plugin.is_loaded());
    }

    #[test]
    fn test_execute_without_library_fails() {
        let plugin = LoadedPlugin::new_stub(sample_manifest(), "/tmp/test.so");
        let input = b"hello";
        let mut output = [0u8; 64];
        let result = plugin.execute(input, &mut output);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Library not loaded"));
    }

    #[test]
    fn test_shutdown_without_library_fails() {
        let mut plugin = LoadedPlugin::new_stub(sample_manifest(), "/tmp/test.so");
        let result = plugin.shutdown();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Library not loaded"));
    }

    #[test]
    fn test_initialize_without_library_fails() {
        let mut plugin = LoadedPlugin::new_stub(sample_manifest(), "/tmp/test.so");
        let store = Arc::new(RwLock::new(HashMap::new()));
        let result = plugin.initialize(store);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Library not loaded"));
    }

    #[test]
    #[ignore = "Requires a real compiled .so plugin binary -- see plugins/test_plugin/ for how to build one"]
    fn test_load_real_so() {
        let path = std::env::var("TEST_PLUGIN_PATH").unwrap();
        let result = LoadedPlugin::load(&path, sample_manifest());
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_nonexistent_file_fails() {
        let result = LoadedPlugin::load("/tmp/nonexistent_plugin_98765.so", sample_manifest());
        assert!(result.is_err());
    }

    #[test]
    fn test_state_after_failed_load() {
        let result = LoadedPlugin::load("/tmp/nonexistent_plugin_98765.so", sample_manifest());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to dlopen"));
    }

    #[test]
    fn test_hot_reload_without_library_tries_fresh_load() {
        let mut plugin = LoadedPlugin::new_stub(sample_manifest(), "/tmp/old.so");
        let store = Arc::new(RwLock::new(HashMap::new()));
        let result = plugin.hot_reload("/tmp/nonexistent_new_plugin.so", store);
        assert!(result.is_err());
    }

    #[test]
    fn test_debug_impl() {
        let plugin = LoadedPlugin::new_stub(sample_manifest(), "/tmp/test.so");
        let debug = format!("{:?}", plugin);
        assert!(debug.contains("LoadedPlugin"));
        assert!(debug.contains("com.nebula.test"));
        assert!(debug.contains("library_loaded: false"));
    }
}
