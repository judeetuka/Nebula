use flutter_rust_bridge::frb;

use crate::api::node_api::{with_engine_read, with_engine_write};

/// Configure the engine with cluster connection details.
///
/// This must be called after `init_engine` and before `start_engine`.
/// Persists the configuration to disk so that subsequent engine starts
/// will automatically restore the configured state.
///
/// # Arguments
/// * `cluster_id` - Unique identifier for the cluster to join
/// * `server_url` - WebSocket URL of the proxy server
/// * `auth_token` - Authentication token for the cluster
#[frb(sync)]
pub fn configure_cluster(
    cluster_id: String,
    server_url: String,
    auth_token: String,
) -> Result<(), String> {
    with_engine_write(|engine| {
        engine
            .configure(&cluster_id, &server_url, &auth_token)
            .map_err(|e| format!("{}", e))
    })
}

/// Get the current cluster configuration as a JSON string.
///
/// Returns a JSON object with `cluster_id`, `server_url`, and
/// `is_configured` fields. The `auth_token` is deliberately excluded
/// from the response for security.
#[frb(sync)]
pub fn get_cluster_config() -> Result<String, String> {
    with_engine_read(|engine| {
        let config = serde_json::json!({
            "cluster_id": engine.cluster_id(),
            "is_configured": engine.is_configured(),
        });
        serde_json::to_string(&config).map_err(|e| format!("Failed to serialize config: {}", e))
    })
}

/// Check if the engine has been configured with cluster details.
///
/// Returns `true` if `configure_cluster` has been called (or the
/// configuration was restored from a previous session).
#[frb(sync)]
pub fn is_configured() -> bool {
    with_engine_read(|engine| Ok(engine.is_configured())).unwrap_or(false)
}
