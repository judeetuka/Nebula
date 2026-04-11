use anyhow::{Context, Result};
use nebula_core::identity::node_id::NodeId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const IDENTITY_FILENAME: &str = "node_identity.json";

/// On-disk representation of the node's persistent identity.
#[derive(Debug, Serialize, Deserialize)]
struct PersistedIdentity {
    node_id: String,
    cluster_id: Option<String>,
    server_url: Option<String>,
    auth_token: Option<String>,
}

/// Manages the node's persistent identity (NodeId + cluster membership).
///
/// On first creation a new `NodeId` is generated and saved to
/// `{storage_path}/node_identity.json`. On subsequent loads the existing
/// identity is read from the same file.
pub struct LocalIdentity {
    node_id: NodeId,
    storage_path: PathBuf,
    cluster_id: Option<String>,
    server_url: Option<String>,
    auth_token: Option<String>,
}

impl LocalIdentity {
    /// Load an existing identity from disk or create a new one.
    ///
    /// The identity file is stored at `{storage_path}/node_identity.json`.
    /// If the file does not exist, a new `NodeId` is generated and persisted.
    pub fn load_or_create(storage_path: &str) -> Result<Self> {
        let path = PathBuf::from(storage_path);
        let identity_file = path.join(IDENTITY_FILENAME);

        if identity_file.exists() {
            let data = std::fs::read_to_string(&identity_file)
                .with_context(|| format!("Failed to read identity file: {:?}", identity_file))?;
            let persisted: PersistedIdentity =
                serde_json::from_str(&data).with_context(|| "Failed to parse identity file")?;
            let node_id = NodeId::from_str(&persisted.node_id).with_context(|| {
                format!("Invalid NodeId in identity file: {}", persisted.node_id)
            })?;

            Ok(Self {
                node_id,
                storage_path: path,
                cluster_id: persisted.cluster_id,
                server_url: persisted.server_url,
                auth_token: persisted.auth_token,
            })
        } else {
            let node_id = NodeId::generate();
            let identity = Self {
                node_id,
                storage_path: path,
                cluster_id: None,
                server_url: None,
                auth_token: None,
            };
            identity
                .save()
                .with_context(|| "Failed to save new identity")?;
            Ok(identity)
        }
    }

    /// Persist the current identity state to disk.
    fn save(&self) -> Result<()> {
        std::fs::create_dir_all(&self.storage_path).with_context(|| {
            format!(
                "Failed to create storage directory: {:?}",
                self.storage_path
            )
        })?;

        let persisted = PersistedIdentity {
            node_id: self.node_id.to_string(),
            cluster_id: self.cluster_id.clone(),
            server_url: self.server_url.clone(),
            // S-4: Auth token is NOT persisted in plaintext JSON.
            // It is stored in the engine's encrypted blob store instead.
            // The in-memory field is populated from encrypted storage at startup.
            auth_token: None,
        };

        let json = serde_json::to_string_pretty(&persisted)
            .with_context(|| "Failed to serialize identity")?;

        let identity_file = self.storage_path.join(IDENTITY_FILENAME);
        std::fs::write(&identity_file, json)
            .with_context(|| format!("Failed to write identity file: {:?}", identity_file))?;

        Ok(())
    }

    /// Store cluster configuration and persist it to disk.
    pub fn configure_cluster(
        &mut self,
        cluster_id: &str,
        server_url: &str,
        auth_token: &str,
    ) -> Result<()> {
        self.cluster_id = Some(cluster_id.to_string());
        self.server_url = Some(server_url.to_string());
        self.auth_token = Some(auth_token.to_string());
        self.save()
            .with_context(|| "Failed to persist cluster configuration")
    }

    /// Returns the node's unique identifier.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the configured cluster ID, if any.
    pub fn cluster_id(&self) -> Option<&str> {
        self.cluster_id.as_deref()
    }

    /// Returns the configured server URL, if any.
    pub fn server_url(&self) -> Option<&str> {
        self.server_url.as_deref()
    }

    /// Returns the configured auth token, if any.
    pub fn auth_token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }

    /// Returns `true` if the cluster has been configured.
    /// Note: auth_token is stored separately in encrypted storage (S-4),
    /// so we only check cluster_id and server_url here.
    pub fn is_configured(&self) -> bool {
        self.cluster_id.is_some() && self.server_url.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a unique temporary directory for test isolation.
    fn temp_test_dir(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("nebula_engine_tests")
            .join(test_name)
            .join(uuid::Uuid::new_v4().to_string());
        // Ensure clean state
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Clean up a test directory.
    fn cleanup(dir: &PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_create_new_identity() {
        let dir = temp_test_dir("create_new");

        let identity = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();

        // A new NodeId should have been generated
        assert!(!identity.node_id().to_string().is_empty());

        // Identity file should exist on disk
        let identity_file = dir.join(IDENTITY_FILENAME);
        assert!(identity_file.exists());

        // Should not be configured yet
        assert!(!identity.is_configured());
        assert!(identity.cluster_id().is_none());
        assert!(identity.server_url().is_none());
        assert!(identity.auth_token().is_none());

        cleanup(&dir);
    }

    #[test]
    fn test_reload_preserves_node_id() {
        let dir = temp_test_dir("reload_preserves");

        let identity1 = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();
        let node_id_1 = identity1.node_id();

        // Drop and reload
        drop(identity1);
        let identity2 = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();
        let node_id_2 = identity2.node_id();

        assert_eq!(node_id_1, node_id_2);

        cleanup(&dir);
    }

    #[test]
    fn test_configure_cluster() {
        let dir = temp_test_dir("configure_cluster");

        let mut identity = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();

        identity
            .configure_cluster("cluster-42", "wss://proxy.example.com", "secret-token")
            .unwrap();

        assert!(identity.is_configured());
        assert_eq!(identity.cluster_id(), Some("cluster-42"));
        assert_eq!(identity.server_url(), Some("wss://proxy.example.com"));
        assert_eq!(identity.auth_token(), Some("secret-token"));

        cleanup(&dir);
    }

    #[test]
    fn test_configure_persists_across_reloads() {
        let dir = temp_test_dir("configure_persists");

        let mut identity = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();
        let original_id = identity.node_id();

        identity
            .configure_cluster("cluster-99", "wss://server.test", "tok-abc")
            .unwrap();
        drop(identity);

        // Reload from disk
        let reloaded = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();

        assert_eq!(reloaded.node_id(), original_id);
        assert_eq!(reloaded.cluster_id(), Some("cluster-99"));
        assert_eq!(reloaded.server_url(), Some("wss://server.test"));
        // S-4: auth_token is no longer persisted in plaintext JSON.
        // It's stored in the encrypted blob store by the engine.
        assert_eq!(reloaded.auth_token(), None);
        // is_configured requires auth_token, so use cluster_id check instead
        assert!(reloaded.cluster_id().is_some());

        cleanup(&dir);
    }

    #[test]
    fn test_identity_file_is_valid_json() {
        let dir = temp_test_dir("valid_json");

        let identity = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();
        let file_content = std::fs::read_to_string(dir.join(IDENTITY_FILENAME)).unwrap();

        // Should parse as valid JSON
        let value: serde_json::Value = serde_json::from_str(&file_content).unwrap();
        assert!(value.get("node_id").is_some());
        assert_eq!(
            value["node_id"].as_str().unwrap(),
            identity.node_id().to_string()
        );

        cleanup(&dir);
    }

    #[test]
    fn test_reconfigure_overwrites_previous() {
        let dir = temp_test_dir("reconfigure");

        let mut identity = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();

        identity
            .configure_cluster("old-cluster", "wss://old.com", "old-token")
            .unwrap();
        identity
            .configure_cluster("new-cluster", "wss://new.com", "new-token")
            .unwrap();

        assert_eq!(identity.cluster_id(), Some("new-cluster"));
        assert_eq!(identity.server_url(), Some("wss://new.com"));
        assert_eq!(identity.auth_token(), Some("new-token"));

        // Verify persistence
        drop(identity);
        let reloaded = LocalIdentity::load_or_create(dir.to_str().unwrap()).unwrap();
        assert_eq!(reloaded.cluster_id(), Some("new-cluster"));

        cleanup(&dir);
    }
}
