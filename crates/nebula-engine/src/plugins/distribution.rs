use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Client for the plugin store — handles ABI-aware downloads and verification.
///
/// In the current phase this is mostly a stub: actual HTTP downloads and
/// checksum verification will be implemented once the server-side plugin
/// store is built. The ABI-matching logic is fully functional.
pub struct PluginStoreClient {
    /// Base URL of the plugin store API.
    store_url: String,
    /// ABI of the local device (e.g. "aarch64", "armv7", "x86_64").
    local_abi: String,
    /// Local directory where downloaded `.so` files are saved.
    plugin_dir: String,
}

/// A request to install a specific plugin version.
///
/// Contains download URLs keyed by target ABI so that the client can
/// pick the correct binary for the current device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginInstallRequest {
    /// Unique identifier of the plugin.
    pub plugin_id: String,
    /// Version to install.
    pub version: String,
    /// Authentication key for the store.
    pub auth_key: String,
    /// Map from ABI string to download URL.
    pub download_urls: HashMap<String, String>,
}

impl PluginStoreClient {
    /// Create a new store client.
    pub fn new(store_url: &str, local_abi: &str, plugin_dir: &str) -> Self {
        Self {
            store_url: store_url.to_string(),
            local_abi: local_abi.to_string(),
            plugin_dir: plugin_dir.to_string(),
        }
    }

    /// Pick the download URL that matches the local ABI.
    ///
    /// Returns `None` if the request does not contain a URL for the
    /// current device architecture.
    pub fn get_download_url<'a>(&self, request: &'a PluginInstallRequest) -> Option<&'a str> {
        request.download_urls.get(&self.local_abi).map(|s| s.as_str())
    }

    /// Download a plugin binary from `url` and save it to `dest_path`.
    ///
    /// **Stub**: always returns `Ok(())`. Actual HTTP download will be
    /// implemented in a future phase.
    pub fn download_plugin(&self, _url: &str, _dest_path: &str) -> Result<()> {
        // Stub: real HTTP download will be implemented when the plugin
        // store server is ready.
        Ok(())
    }

    /// Verify that the file at `path` matches the expected SHA-256 checksum.
    ///
    /// **Stub**: always returns `Ok(true)`. Real verification will use
    /// `sha2` to hash the file contents.
    pub fn verify_checksum(&self, _path: &str, _expected: &str) -> Result<bool> {
        // Stub: real checksum verification will be implemented alongside
        // the download logic.
        Ok(true)
    }

    /// Returns the local device's ABI string.
    pub fn local_abi(&self) -> &str {
        &self.local_abi
    }

    /// Returns the store URL.
    pub fn store_url(&self) -> &str {
        &self.store_url
    }

    /// Returns the local plugin directory path.
    pub fn plugin_dir(&self) -> &str {
        &self.plugin_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> PluginInstallRequest {
        let mut urls = HashMap::new();
        urls.insert(
            "aarch64".to_string(),
            "https://store.nebula.io/plugins/sms/1.0.0/aarch64.so".to_string(),
        );
        urls.insert(
            "armv7".to_string(),
            "https://store.nebula.io/plugins/sms/1.0.0/armv7.so".to_string(),
        );
        urls.insert(
            "x86_64".to_string(),
            "https://store.nebula.io/plugins/sms/1.0.0/x86_64.so".to_string(),
        );

        PluginInstallRequest {
            plugin_id: "com.nebula.sms".to_string(),
            version: "1.0.0".to_string(),
            auth_key: "secret-key".to_string(),
            download_urls: urls,
        }
    }

    // -------------------------------------------------------------------
    // Construction
    // -------------------------------------------------------------------

    #[test]
    fn test_new_client() {
        let client = PluginStoreClient::new(
            "https://store.nebula.io",
            "aarch64",
            "/data/plugins",
        );
        assert_eq!(client.store_url(), "https://store.nebula.io");
        assert_eq!(client.local_abi(), "aarch64");
        assert_eq!(client.plugin_dir(), "/data/plugins");
    }

    // -------------------------------------------------------------------
    // ABI matching
    // -------------------------------------------------------------------

    #[test]
    fn test_get_download_url_matching_abi() {
        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", "/data/plugins");
        let request = sample_request();

        let url = client.get_download_url(&request);
        assert_eq!(
            url,
            Some("https://store.nebula.io/plugins/sms/1.0.0/aarch64.so")
        );
    }

    #[test]
    fn test_get_download_url_armv7() {
        let client = PluginStoreClient::new("https://store.nebula.io", "armv7", "/data/plugins");
        let request = sample_request();

        let url = client.get_download_url(&request);
        assert_eq!(
            url,
            Some("https://store.nebula.io/plugins/sms/1.0.0/armv7.so")
        );
    }

    #[test]
    fn test_get_download_url_x86_64() {
        let client = PluginStoreClient::new("https://store.nebula.io", "x86_64", "/data/plugins");
        let request = sample_request();

        let url = client.get_download_url(&request);
        assert_eq!(
            url,
            Some("https://store.nebula.io/plugins/sms/1.0.0/x86_64.so")
        );
    }

    #[test]
    fn test_get_download_url_no_matching_abi() {
        let client = PluginStoreClient::new("https://store.nebula.io", "riscv64", "/data/plugins");
        let request = sample_request();

        let url = client.get_download_url(&request);
        assert!(url.is_none());
    }

    #[test]
    fn test_get_download_url_empty_urls() {
        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", "/data/plugins");
        let request = PluginInstallRequest {
            plugin_id: "empty".to_string(),
            version: "0.0.1".to_string(),
            auth_key: "key".to_string(),
            download_urls: HashMap::new(),
        };

        assert!(client.get_download_url(&request).is_none());
    }

    // -------------------------------------------------------------------
    // Stubs
    // -------------------------------------------------------------------

    #[test]
    fn test_download_plugin_stub_succeeds() {
        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", "/data/plugins");
        let result = client.download_plugin("https://example.com/plugin.so", "/tmp/plugin.so");
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_checksum_stub_returns_true() {
        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", "/data/plugins");
        let result = client.verify_checksum("/tmp/plugin.so", "abc123");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    // -------------------------------------------------------------------
    // PluginInstallRequest serialization
    // -------------------------------------------------------------------

    #[test]
    fn test_install_request_serialization_roundtrip() {
        let request = sample_request();
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: PluginInstallRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_install_request_json_fields() {
        let request = sample_request();
        let value: serde_json::Value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["plugin_id"], "com.nebula.sms");
        assert_eq!(value["version"], "1.0.0");
        assert_eq!(value["auth_key"], "secret-key");
        assert!(value["download_urls"]["aarch64"].is_string());
    }
}
