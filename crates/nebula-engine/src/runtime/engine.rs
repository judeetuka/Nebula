use std::sync::{Arc, RwLock as StdRwLock};

use anyhow::{bail, Context, Result};
use nebula_core::identity::node_id::NodeId;
use nebula_core::identity::roles::NodeRole;
use tokio::sync::{broadcast, Mutex as TokioMutex, RwLock};
use tracing::info;

use crate::cluster::membership::ClusterMembership;
use crate::identity::local_identity::LocalIdentity;
use crate::mqtt::broker::EmbeddedBroker;
use crate::mqtt::client::MqttClient;
use crate::runtime::state::NodeState;

/// The NEBULA node engine.
///
/// Manages the node lifecycle: identity persistence, cluster configuration,
/// state transitions, shutdown signaling, MQTT infrastructure, and cluster
/// membership tracking.
pub struct NebulaEngine {
    state: Arc<RwLock<NodeState>>,
    identity: LocalIdentity,
    shutdown_tx: Option<broadcast::Sender<bool>>,
    mqtt_broker: Arc<TokioMutex<Option<EmbeddedBroker>>>,
    mqtt_client: Arc<TokioMutex<Option<MqttClient>>>,
    membership: Arc<StdRwLock<ClusterMembership>>,
}

impl NebulaEngine {
    /// Create a new engine, loading or generating the node identity from disk.
    ///
    /// The engine starts in the `Uninitialized` state. Call `configure()` to
    /// provide cluster information before starting.
    pub fn new(storage_path: &str) -> Result<Self> {
        let identity = LocalIdentity::load_or_create(storage_path)
            .with_context(|| "Failed to load or create node identity")?;

        let (shutdown_tx, _) = broadcast::channel(1);

        let initial_state = if identity.is_configured() {
            NodeState::Configured {
                cluster_id: identity.cluster_id().unwrap_or_default().to_string(),
            }
        } else {
            NodeState::Uninitialized
        };

        let node_id = identity.node_id();
        let membership = ClusterMembership::new(node_id, NodeRole::Worker);

        Ok(Self {
            state: Arc::new(RwLock::new(initial_state)),
            identity,
            shutdown_tx: Some(shutdown_tx),
            mqtt_broker: Arc::new(TokioMutex::new(None)),
            mqtt_client: Arc::new(TokioMutex::new(None)),
            membership: Arc::new(StdRwLock::new(membership)),
        })
    }

    /// Configure the engine with cluster connection details.
    ///
    /// Persists the configuration to disk and transitions the state to
    /// `Configured`. Only valid from `Uninitialized` state.
    pub fn configure(
        &mut self,
        cluster_id: &str,
        server_url: &str,
        auth_token: &str,
    ) -> Result<()> {
        let target = NodeState::Configured {
            cluster_id: cluster_id.to_string(),
        };

        self.transition_state(target)?;

        self.identity
            .configure_cluster(cluster_id, server_url, auth_token)
            .with_context(|| "Failed to persist cluster configuration")?;

        Ok(())
    }

    /// Transition the engine to `Connecting` state.
    ///
    /// MQTT broker and client initialization will occur when the node
    /// reaches the `Active` state. For now this validates the state
    /// transition and signals readiness to connect.
    pub fn start(&self) -> Result<()> {
        info!("Engine starting — MQTT will be initialized when Active");
        self.transition_state(NodeState::Connecting)
    }

    /// Initiate a graceful shutdown.
    ///
    /// Transitions to `ShuttingDown` and broadcasts a shutdown signal to all
    /// listeners.
    pub fn shutdown(&self) -> Result<()> {
        self.transition_state(NodeState::ShuttingDown)?;

        if let Some(tx) = &self.shutdown_tx {
            // Ignore errors -- receivers may have been dropped already.
            let _ = tx.send(true);
        }

        Ok(())
    }

    /// Returns a clone of the current engine state.
    ///
    /// Uses `blocking_read()` which is safe to call from non-async contexts
    /// (e.g. Flutter FFI sync functions).
    pub fn state(&self) -> NodeState {
        self.state.blocking_read().clone()
    }

    /// Returns the node's persistent identity.
    pub fn node_id(&self) -> NodeId {
        self.identity.node_id()
    }

    /// Returns the configured cluster ID, if any.
    pub fn cluster_id(&self) -> Option<String> {
        self.identity.cluster_id().map(String::from)
    }

    /// Returns `true` if the cluster has been configured.
    pub fn is_configured(&self) -> bool {
        self.identity.is_configured()
    }

    /// Subscribe to shutdown signals.
    ///
    /// Returns a receiver that will receive `true` when `shutdown()` is called.
    pub fn subscribe_shutdown(&self) -> Option<broadcast::Receiver<bool>> {
        self.shutdown_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// Returns a reference to the cluster membership tracker.
    pub fn membership(&self) -> &Arc<StdRwLock<ClusterMembership>> {
        &self.membership
    }

    /// Returns a reference to the MQTT broker handle.
    pub fn mqtt_broker(&self) -> &Arc<TokioMutex<Option<EmbeddedBroker>>> {
        &self.mqtt_broker
    }

    /// Returns a reference to the MQTT client handle.
    pub fn mqtt_client(&self) -> &Arc<TokioMutex<Option<MqttClient>>> {
        &self.mqtt_client
    }

    /// Validate and apply a state transition.
    ///
    /// Returns an error if the transition is not valid according to the state
    /// machine rules defined in `NodeState::can_transition_to`.
    fn transition_state(&self, target: NodeState) -> Result<()> {
        let mut state = self.state.blocking_write();
        if !state.can_transition_to(&target) {
            bail!(
                "Invalid state transition: {} -> {}",
                state.display_name(),
                target.display_name()
            );
        }
        *state = target;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Create a unique temporary directory for test isolation.
    fn temp_test_dir(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("nebula_engine_tests")
            .join("engine")
            .join(test_name)
            .join(uuid::Uuid::new_v4().to_string());
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_new_engine_starts_uninitialized() {
        let dir = temp_test_dir("new_uninit");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        assert_eq!(engine.state(), NodeState::Uninitialized);
        assert!(!engine.node_id().to_string().is_empty());

        cleanup(&dir);
    }

    #[test]
    fn test_configure_transitions_to_configured() {
        let dir = temp_test_dir("configure");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        engine
            .configure("cluster-1", "wss://proxy.test", "token-abc")
            .unwrap();

        assert_eq!(
            engine.state(),
            NodeState::Configured {
                cluster_id: "cluster-1".to_string()
            }
        );
        assert!(engine.is_configured());
        assert_eq!(engine.cluster_id(), Some("cluster-1".to_string()));

        cleanup(&dir);
    }

    #[test]
    fn test_start_transitions_to_connecting() {
        let dir = temp_test_dir("start");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        engine
            .configure("cluster-1", "wss://proxy.test", "token")
            .unwrap();
        engine.start().unwrap();

        assert_eq!(engine.state(), NodeState::Connecting);

        cleanup(&dir);
    }

    #[test]
    fn test_shutdown_transitions_to_shutting_down() {
        let dir = temp_test_dir("shutdown");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        engine
            .configure("cluster-1", "wss://proxy.test", "token")
            .unwrap();
        engine.start().unwrap();
        engine.shutdown().unwrap();

        assert_eq!(engine.state(), NodeState::ShuttingDown);

        cleanup(&dir);
    }

    #[test]
    fn test_start_without_configure_fails() {
        let dir = temp_test_dir("start_no_config");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let result = engine.start();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid state transition")
        );

        cleanup(&dir);
    }

    #[test]
    fn test_double_configure_fails() {
        let dir = temp_test_dir("double_configure");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        engine
            .configure("cluster-1", "wss://proxy.test", "token")
            .unwrap();

        // Configured -> Configured is not a valid transition
        let result = engine.configure("cluster-2", "wss://other.test", "token2");
        assert!(result.is_err());

        cleanup(&dir);
    }

    #[test]
    fn test_full_lifecycle() {
        let dir = temp_test_dir("full_lifecycle");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        // Uninitialized -> Configured
        engine
            .configure("cluster-1", "wss://proxy.test", "token")
            .unwrap();
        assert_eq!(
            engine.state(),
            NodeState::Configured {
                cluster_id: "cluster-1".to_string()
            }
        );

        // Configured -> Connecting
        engine.start().unwrap();
        assert_eq!(engine.state(), NodeState::Connecting);

        // Connecting -> ShuttingDown (always allowed)
        engine.shutdown().unwrap();
        assert_eq!(engine.state(), NodeState::ShuttingDown);

        cleanup(&dir);
    }

    #[test]
    fn test_engine_preserves_identity_across_instances() {
        let dir = temp_test_dir("preserve_identity");

        let mut engine1 = NebulaEngine::new(dir.to_str().unwrap()).unwrap();
        let node_id_1 = engine1.node_id();
        engine1
            .configure("cluster-1", "wss://proxy.test", "token")
            .unwrap();
        drop(engine1);

        // Second instance should reload the same identity and start as Configured
        let engine2 = NebulaEngine::new(dir.to_str().unwrap()).unwrap();
        assert_eq!(engine2.node_id(), node_id_1);
        assert_eq!(
            engine2.state(),
            NodeState::Configured {
                cluster_id: "cluster-1".to_string()
            }
        );

        cleanup(&dir);
    }

    #[test]
    fn test_shutdown_signal_is_broadcast() {
        let dir = temp_test_dir("shutdown_signal");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let mut rx = engine.subscribe_shutdown().unwrap();

        engine
            .configure("cluster-1", "wss://proxy.test", "token")
            .unwrap();
        engine.start().unwrap();
        engine.shutdown().unwrap();

        // The receiver should have received the shutdown signal
        let signal = rx.try_recv().unwrap();
        assert!(signal);

        cleanup(&dir);
    }

    #[test]
    fn test_shutdown_from_any_state() {
        // Test that shutdown works from Uninitialized (catch-all rule)
        let dir = temp_test_dir("shutdown_any");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        engine.shutdown().unwrap();
        assert_eq!(engine.state(), NodeState::ShuttingDown);

        cleanup(&dir);
    }

    #[test]
    fn test_engine_has_membership() {
        let dir = temp_test_dir("has_membership");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let membership = engine.membership().read().unwrap();
        assert_eq!(membership.member_count(), 0);
        assert_eq!(membership.local_node_id(), engine.node_id());

        cleanup(&dir);
    }

    #[test]
    fn test_engine_mqtt_broker_initially_none() {
        let dir = temp_test_dir("mqtt_broker_none");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let broker = engine.mqtt_broker().blocking_lock();
        assert!(broker.is_none());

        cleanup(&dir);
    }

    #[test]
    fn test_engine_mqtt_client_initially_none() {
        let dir = temp_test_dir("mqtt_client_none");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let client = engine.mqtt_client().blocking_lock();
        assert!(client.is_none());

        cleanup(&dir);
    }
}
