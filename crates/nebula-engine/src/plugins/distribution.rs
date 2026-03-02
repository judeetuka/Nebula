use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Client for the plugin store -- handles ABI-aware downloads and verification.
///
/// Downloads plugin `.so` binaries from the plugin store API and verifies
/// their SHA-256 checksums before loading.
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
    /// Uses `reqwest` to perform an HTTP GET request, streaming the response
    /// body to a file. Creates parent directories if they do not exist.
    pub async fn download_plugin(&self, url: &str, dest_path: &str) -> Result<()> {
        let dest = Path::new(dest_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let response = reqwest::get(url)
            .await
            .with_context(|| format!("HTTP request failed for {url}"))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Download failed with status {status} for {url}");
        }

        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("Failed to read response body from {url}"))?;

        let mut file = std::fs::File::create(dest)
            .with_context(|| format!("Failed to create file: {dest_path}"))?;

        file.write_all(&bytes)
            .with_context(|| format!("Failed to write to file: {dest_path}"))?;

        Ok(())
    }

    /// Verify that the file at `path` matches the expected SHA-256 checksum.
    ///
    /// Reads the file contents, computes the SHA-256 hash, and compares the
    /// hex-encoded digest against the expected value (case-insensitive).
    pub fn verify_checksum(&self, path: &str, expected: &str) -> Result<bool> {
        let data = std::fs::read(path)
            .with_context(|| format!("Failed to read file for checksum: {path}"))?;

        let mut hasher = Sha256::new();
        hasher.update(&data);
        let digest = hasher.finalize();
        let hex_digest = hex::encode(digest);

        Ok(hex_digest.eq_ignore_ascii_case(expected))
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
    // SHA-256 checksum verification
    // -------------------------------------------------------------------

    #[test]
    fn test_verify_checksum_correct_hash() {
        let dir = std::env::temp_dir()
            .join("nebula_plugin_tests")
            .join("checksum_correct")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_plugin.so");

        let content = b"hello world plugin binary";
        std::fs::write(&path, content).unwrap();

        // Compute expected SHA-256 of the content
        let mut hasher = Sha256::new();
        hasher.update(content);
        let expected = hex::encode(hasher.finalize());

        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", dir.to_str().unwrap());
        let result = client.verify_checksum(path.to_str().unwrap(), &expected);
        assert!(result.is_ok());
        assert!(result.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_verify_checksum_wrong_hash() {
        let dir = std::env::temp_dir()
            .join("nebula_plugin_tests")
            .join("checksum_wrong")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_plugin.so");

        std::fs::write(&path, b"some binary content").unwrap();

        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", dir.to_str().unwrap());
        let result = client.verify_checksum(path.to_str().unwrap(), "0000000000000000000000000000000000000000000000000000000000000000");
        assert!(result.is_ok());
        assert!(!result.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_verify_checksum_case_insensitive() {
        let dir = std::env::temp_dir()
            .join("nebula_plugin_tests")
            .join("checksum_case")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_plugin.so");

        let content = b"case test data";
        std::fs::write(&path, content).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(content);
        let expected_upper = hex::encode(hasher.finalize()).to_uppercase();

        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", dir.to_str().unwrap());
        let result = client.verify_checksum(path.to_str().unwrap(), &expected_upper);
        assert!(result.is_ok());
        assert!(result.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_verify_checksum_missing_file() {
        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", "/tmp");
        let result = client.verify_checksum("/nonexistent/path/to/file.so", "abc");
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // Download (async) -- validates error on unreachable URL
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_download_plugin_unreachable_url() {
        let dir = std::env::temp_dir()
            .join("nebula_plugin_tests")
            .join("download_fail")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("plugin.so");

        let client = PluginStoreClient::new("https://store.nebula.io", "aarch64", dir.to_str().unwrap());
        let result = client
            .download_plugin("http://127.0.0.1:1/nonexistent.so", dest.to_str().unwrap())
            .await;
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
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
