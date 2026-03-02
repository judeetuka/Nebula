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
use crate::plugins::registry::PluginRegistry;
use crate::routing::smart_router::SmartRouter;
use crate::runtime::state::NodeState;
use crate::tasks::dispatcher::TaskDispatcher;
use crate::tasks::executor::TaskExecutor;

/// Default maximum pending tasks in the dispatcher queue.
const DEFAULT_MAX_PENDING: usize = 1000;

/// Default maximum concurrent tasks on a worker executor.
const DEFAULT_MAX_CONCURRENT: usize = 5;

/// Default plugin directory name (relative to the storage path).
const DEFAULT_PLUGIN_DIR: &str = "plugins";

/// The NEBULA node engine.
///
/// Manages the node lifecycle: identity persistence, cluster configuration,
/// state transitions, shutdown signaling, MQTT infrastructure, cluster
/// membership tracking, smart routing, task dispatch/execution, and the
/// plugin registry.
pub struct NebulaEngine {
    state: Arc<RwLock<NodeState>>,
    identity: LocalIdentity,
    shutdown_tx: Option<broadcast::Sender<bool>>,
    mqtt_broker: Arc<TokioMutex<Option<EmbeddedBroker>>>,
    mqtt_client: Arc<TokioMutex<Option<MqttClient>>>,
    membership: Arc<StdRwLock<ClusterMembership>>,
    smart_router: Arc<StdRwLock<Option<SmartRouter>>>,
    task_dispatcher: Arc<StdRwLock<TaskDispatcher>>,
    task_executor: Arc<StdRwLock<TaskExecutor>>,
    plugin_registry: Arc<StdRwLock<PluginRegistry>>,
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

        // Initialize smart router if cluster is already configured
        let smart_router = if identity.is_configured() {
            let cluster_id = identity.cluster_id().unwrap_or_default();
            let server_url = identity.server_url().unwrap_or_default();
            Some(SmartRouter::new(cluster_id, node_id, 0, server_url))
        } else {
            None
        };

        let task_dispatcher = TaskDispatcher::new(DEFAULT_MAX_PENDING);
        let task_executor = TaskExecutor::new(node_id, DEFAULT_MAX_CONCURRENT);

        let plugin_dir = format!("{}/{}", storage_path, DEFAULT_PLUGIN_DIR);
        let plugin_registry = PluginRegistry::new(&plugin_dir);

        Ok(Self {
            state: Arc::new(RwLock::new(initial_state)),
            identity,
            shutdown_tx: Some(shutdown_tx),
            mqtt_broker: Arc::new(TokioMutex::new(None)),
            mqtt_client: Arc::new(TokioMutex::new(None)),
            membership: Arc::new(StdRwLock::new(membership)),
            smart_router: Arc::new(StdRwLock::new(smart_router)),
            task_dispatcher: Arc::new(StdRwLock::new(task_dispatcher)),
            task_executor: Arc::new(StdRwLock::new(task_executor)),
            plugin_registry: Arc::new(StdRwLock::new(plugin_registry)),
        })
    }

    /// Configure the engine with cluster connection details.
    ///
    /// Persists the configuration to disk and transitions the state to
    /// `Configured`. Only valid from `Uninitialized` state. Also initializes
    /// the smart router for the configured cluster.
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

        // Initialize the smart router now that we have cluster info
        let node_id = self.identity.node_id();
        let router = SmartRouter::new(cluster_id, node_id, 0, server_url);
        *self.smart_router.write().unwrap() = Some(router);

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

    /// Returns a reference to the smart router handle.
    ///
    /// The smart router is `None` until the engine is configured with
    /// cluster details.
    pub fn smart_router(&self) -> &Arc<StdRwLock<Option<SmartRouter>>> {
        &self.smart_router
    }

    /// Returns a reference to the task dispatcher (master-side).
    pub fn task_dispatcher(&self) -> &Arc<StdRwLock<TaskDispatcher>> {
        &self.task_dispatcher
    }

    /// Returns a reference to the task executor (worker-side).
    pub fn task_executor(&self) -> &Arc<StdRwLock<TaskExecutor>> {
        &self.task_executor
    }

    /// Returns a reference to the plugin registry.
    pub fn plugin_registry(&self) -> &Arc<StdRwLock<PluginRegistry>> {
        &self.plugin_registry
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

    #[test]
    fn test_engine_smart_router_none_before_configure() {
        let dir = temp_test_dir("router_none");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let router = engine.smart_router().read().unwrap();
        assert!(router.is_none());

        cleanup(&dir);
    }

    #[test]
    fn test_engine_smart_router_initialized_after_configure() {
        let dir = temp_test_dir("router_after_config");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        engine
            .configure("cluster-1", "wss://proxy.test", "token")
            .unwrap();

        let router = engine.smart_router().read().unwrap();
        assert!(router.is_some());

        let router = router.as_ref().unwrap();
        assert_eq!(router.lan_discovery().cluster_id(), "cluster-1");
        assert_eq!(router.tunnel_relay().server_url(), "wss://proxy.test");

        cleanup(&dir);
    }

    #[test]
    fn test_engine_smart_router_restored_on_reload() {
        let dir = temp_test_dir("router_reload");

        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();
        engine
            .configure("cluster-99", "wss://server.test", "tok")
            .unwrap();
        drop(engine);

        // Reload: since identity is configured, router should be initialized
        let engine2 = NebulaEngine::new(dir.to_str().unwrap()).unwrap();
        let router = engine2.smart_router().read().unwrap();
        assert!(router.is_some());

        let router = router.as_ref().unwrap();
        assert_eq!(router.lan_discovery().cluster_id(), "cluster-99");

        cleanup(&dir);
    }

    #[test]
    fn test_engine_has_task_dispatcher() {
        let dir = temp_test_dir("has_dispatcher");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let dispatcher = engine.task_dispatcher().read().unwrap();
        assert_eq!(dispatcher.pending_count(), 0);
        assert_eq!(dispatcher.dispatched_count(), 0);
        assert_eq!(dispatcher.completed_count(), 0);

        cleanup(&dir);
    }

    #[test]
    fn test_engine_has_task_executor() {
        let dir = temp_test_dir("has_executor");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let executor = engine.task_executor().read().unwrap();
        assert_eq!(executor.active_count(), 0);
        assert!(executor.can_accept_task());

        cleanup(&dir);
    }

    #[test]
    fn test_engine_has_plugin_registry() {
        let dir = temp_test_dir("has_plugins");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let registry = engine.plugin_registry().read().unwrap();
        assert_eq!(registry.plugin_count(), 0);
        assert!(registry.list_plugins().is_empty());

        let expected_dir = format!("{}/{}", dir.to_str().unwrap(), DEFAULT_PLUGIN_DIR);
        assert_eq!(registry.plugin_dir(), expected_dir);

        cleanup(&dir);
    }
}
