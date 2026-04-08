use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use nebula_core::identity::node_id::{ClusterId, NodeId};
use nebula_core::identity::roles::NodeRole;
use nebula_core::protocol::messages::{NetworkType, NodeHeartBeatPayload};
use tokio::sync::{broadcast, Mutex as TokioMutex, RwLock};
use tracing::{error, info, warn};

use crate::cluster::membership::ClusterMembership;
use crate::identity::local_identity::LocalIdentity;
use crate::mqtt::broker::EmbeddedBroker;
use crate::mqtt::client::MqttClient;
use crate::net::tunnel_client::TunnelClient;
use crate::plugins::registry::PluginRegistry;
use crate::routing::smart_router::SmartRouter;
use crate::runtime::state::NodeState;
use crate::tasks::dispatcher::TaskDispatcher;
use crate::storage::db::StorageManager;
use crate::tasks::executor::TaskExecutor;

/// Default maximum pending tasks in the dispatcher queue.
const DEFAULT_MAX_PENDING: usize = 1000;

/// Default maximum concurrent tasks on a worker executor.
const DEFAULT_MAX_CONCURRENT: usize = 5;

/// Default plugin directory name (relative to the storage path).
const DEFAULT_PLUGIN_DIR: &str = "plugins";

/// Derive the encryption secret for on-device storage.
///
/// On Android, attempts to retrieve a hardware-backed key from the Android
/// Keystore via the Kotlin platform bridge. If that fails (e.g., non-Android
/// platform or Keystore not available), falls back to HKDF derivation from
/// the node identity.
fn get_encryption_secret(node_id: &NodeId) -> Vec<u8> {
    #[cfg(target_os = "android")]
    {
        // Try Android Keystore first
        match crate::platform::invoke_android("security", "getOrCreateStorageKey", "nebula_storage") {
            Ok(key_hex) => {
                if let Ok(key_bytes) = hex::decode(key_hex.trim()) {
                    if key_bytes.len() >= 32 {
                        tracing::info!("Using Android Keystore for storage encryption");
                        return key_bytes;
                    }
                }
                tracing::warn!("Android Keystore key invalid, falling back to node_id derivation");
            }
            Err(e) => {
                tracing::warn!("Android Keystore unavailable ({e}), falling back to node_id derivation");
            }
        }
    }

    // Fallback: derive from node identity via HKDF
    let node_bytes = node_id.0.as_bytes();
    let mut secret = vec![0u8; 32];
    // Simple HKDF-like derivation: SHA-256(salt || node_id)
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(b"nebula-storage-key-v1");
    hasher.update(node_bytes);
    let result = hasher.finalize();
    secret.copy_from_slice(&result[..32]);
    secret
}

/// Heartbeat interval for sending node health metrics to the proxy server.
const HEARTBEAT_INTERVAL_SECS: u64 = 10;

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
    tunnel_client: Arc<TokioMutex<Option<TunnelClient>>>,
    storage: Arc<StorageManager>,
    /// Monotonic instant recorded when the engine is created, used to compute
    /// uptime in heartbeat payloads.
    started_at: Instant,
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

        // Derive encryption secret for storage.
        // On Android, prefer the hardware-backed Keystore via the Kotlin bridge.
        // Fallback to HKDF derivation from node_id for non-Android platforms.
        let encryption_secret = get_encryption_secret(&node_id);
        let storage = StorageManager::new(storage_path, &encryption_secret)
            .with_context(|| "Failed to initialize storage manager")?;

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
            storage: Arc::new(storage),
            tunnel_client: Arc::new(TokioMutex::new(None)),
            started_at: Instant::now(),
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
    /// This is the synchronous entry point called from FFI. It validates the
    /// state transition but does not perform any network I/O. Call
    /// `connect_to_server()` afterwards to perform the actual registration.
    pub fn start(&self) -> Result<()> {
        info!("Engine starting -- transitioning to Connecting");
        self.transition_state(NodeState::Connecting)
    }

    /// Connect to the proxy server, register, and enter the active state.
    ///
    /// This is the async counterpart to `start()`. It performs:
    /// 1. TCP connection to the proxy server
    /// 2. Node registration handshake
    /// 3. State transition through Registering to Active
    /// 4. MQTT broker start (if assigned Master role)
    /// 5. Heartbeat loop spawn
    ///
    /// On failure, transitions to `Reconnecting` state.
    pub async fn connect_to_server(&self) -> Result<NodeRole> {
        let server_url = self
            .identity
            .server_url()
            .context("Server URL not configured")?
            .to_string();
        let cluster_id_str = self
            .identity
            .cluster_id()
            .context("Cluster ID not configured")?
            .to_string();
        let node_id = self.identity.node_id();

        // Create tunnel client and attempt registration
        let mut tunnel = TunnelClient::new(&server_url);
        let role = match tunnel
            .connect_and_register(node_id, ClusterId(cluster_id_str))
            .await
        {
            Ok(role) => role,
            Err(e) => {
                warn!(error = %e, "Failed to connect to server, transitioning to Reconnecting");
                self.transition_state(NodeState::Reconnecting { attempts: 1 })?;
                return Err(e);
            }
        };

        // Transition: Connecting -> Registering -> Active
        self.transition_state(NodeState::Registering)?;
        self.transition_state(NodeState::Active { role })?;

        // If Master, start MQTT broker
        if role == NodeRole::Master {
            let mut broker_lock = self.mqtt_broker.lock().await;
            let mut broker = EmbeddedBroker::new(1883, None);
            broker.start()?;
            *broker_lock = Some(broker);
            info!("MQTT broker started (master role)");
        }

        // Store the tunnel client
        {
            let mut tc = self.tunnel_client.lock().await;
            *tc = Some(tunnel);
        }

        // Spawn heartbeat loop with real device metrics collection
        let tunnel_client = Arc::clone(&self.tunnel_client);
        let task_executor = Arc::clone(&self.task_executor);
        let shutdown_rx = self.subscribe_shutdown();
        let hb_node_id = node_id;
        let started_at = self.started_at;

        tokio::spawn(async move {
            run_heartbeat_loop(
                tunnel_client,
                hb_node_id,
                shutdown_rx,
                task_executor,
                started_at,
            )
            .await;
        });

        Ok(role)
    }

    /// Combined sync start + async connect.
    ///
    /// Transitions to Connecting, then spawns the async connection work.
    /// Returns a `JoinHandle` for the async connection task. The caller
    /// can await the handle to get the assigned role or an error.
    pub fn start_and_connect(
        &self,
    ) -> Result<tokio::task::JoinHandle<Result<NodeRole>>> {
        self.start()?;

        let state = Arc::clone(&self.state);
        let tunnel_client = Arc::clone(&self.tunnel_client);
        let mqtt_broker = Arc::clone(&self.mqtt_broker);
        let task_executor = Arc::clone(&self.task_executor);
        let shutdown_tx = self.shutdown_tx.as_ref().map(|tx| tx.subscribe());
        let started_at = self.started_at;

        let server_url = self
            .identity
            .server_url()
            .context("Server URL not configured")?
            .to_string();
        let cluster_id_str = self
            .identity
            .cluster_id()
            .context("Cluster ID not configured")?
            .to_string();
        let node_id = self.identity.node_id();

        let handle = tokio::spawn(async move {
            // Create tunnel client and attempt registration
            let mut tunnel = TunnelClient::new(&server_url);
            let role = match tunnel
                .connect_and_register(node_id, ClusterId(cluster_id_str))
                .await
            {
                Ok(role) => role,
                Err(e) => {
                    warn!(error = %e, "Failed to connect to server");
                    let mut s = state.write().await;
                    *s = NodeState::Reconnecting { attempts: 1 };
                    return Err(e);
                }
            };

            // Transition: Connecting -> Registering -> Active
            {
                let mut s = state.write().await;
                *s = NodeState::Registering;
            }
            {
                let mut s = state.write().await;
                *s = NodeState::Active { role };
            }

            // If Master, start MQTT broker
            if role == NodeRole::Master {
                let mut broker_lock = mqtt_broker.lock().await;
                let mut broker = EmbeddedBroker::new(1883, None);
                broker.start()?;
                *broker_lock = Some(broker);
                info!("MQTT broker started (master role)");
            }

            // Store the tunnel client
            {
                let mut tc = tunnel_client.lock().await;
                *tc = Some(tunnel);
            }

            // Spawn heartbeat loop with real device metrics
            let hb_tunnel = Arc::clone(&tunnel_client);
            let hb_executor = Arc::clone(&task_executor);
            tokio::spawn(async move {
                run_heartbeat_loop(hb_tunnel, node_id, shutdown_tx, hb_executor, started_at).await;
            });

            Ok(role)
        });

        Ok(handle)
    }

    /// Initiate a graceful shutdown.
    ///
    /// Transitions to `ShuttingDown` and broadcasts a shutdown signal to all
    /// listeners. Also disconnects the tunnel client if connected.
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
    /// Safe to call from both sync (FFI) and async contexts.
    /// Uses `try_read()` first (non-blocking), falls back to `blocking_read()`
    /// when not inside a tokio runtime.
    pub fn state(&self) -> NodeState {
        match self.state.try_read() {
            Ok(guard) => guard.clone(),
            Err(_) => self.state.blocking_read().clone(),
        }
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

    /// Returns a reference to the storage manager.
    pub fn storage(&self) -> &Arc<StorageManager> {
        &self.storage
    }

    /// Returns a reference to the tunnel client handle.
    pub fn tunnel_client(&self) -> &Arc<TokioMutex<Option<TunnelClient>>> {
        &self.tunnel_client
    }

    /// Returns a reference to the global event bus.
    pub fn event_bus(&self) -> &'static crate::runtime::events::EventBus {
        crate::runtime::events::global_event_bus()
    }

    /// Register the global EngineHandle so that plugin C-ABI callbacks can
    /// route through the engine's MQTT client, plugin registry, and engine
    /// command layer. Must be called exactly once.
    pub fn register_plugin_callbacks(&self) {
        use crate::plugins::sdk;

        let mqtt_client = Arc::clone(&self.mqtt_client);
        let plugin_registry = Arc::clone(&self.plugin_registry);

        sdk::register_engine_handle(sdk::EngineHandle {
            // Uses mpsc channels + background drainer tasks to avoid deadlock.
            // The rumqttc::AsyncClient is Clone+Send+Sync so it can be moved
            // into spawned tasks safely.
            mqtt_publish: {
                let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, Vec<u8>)>(256);
                let client_arc = Arc::clone(&mqtt_client);
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        // Extract the AsyncClient once (it's Clone+Send+Sync)
                        let async_client = {
                            let lock = client_arc.lock().await;
                            lock.as_ref().map(|c| c.async_client())
                        };
                        if let Some(ac) = async_client {
                            while let Some((topic, payload)) = rx.recv().await {
                                if let Err(e) = ac.publish(&topic, rumqttc::QoS::AtLeastOnce, false, payload).await {
                                    tracing::warn!(error = %e, topic = %topic, "Plugin MQTT publish failed");
                                }
                            }
                        }
                    });
                }
                Arc::new(move |topic: String, payload: Vec<u8>| {
                    match tx.try_send((topic, payload)) {
                        Ok(_) => 0,
                        Err(_) => -1,
                    }
                })
            },
            mqtt_subscribe: {
                let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);
                let client_arc = Arc::clone(&mqtt_client);
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let async_client = {
                            let lock = client_arc.lock().await;
                            lock.as_ref().map(|c| c.async_client())
                        };
                        if let Some(ac) = async_client {
                            while let Some(topic) = rx.recv().await {
                                if let Err(e) = ac.subscribe(&topic, rumqttc::QoS::AtLeastOnce).await {
                                    tracing::warn!(error = %e, topic = %topic, "Plugin MQTT subscribe failed");
                                }
                            }
                        }
                    });
                }
                Arc::new(move |topic: String| {
                    match tx.try_send(topic) {
                        Ok(_) => 0,
                        Err(_) => -1,
                    }
                })
            },
            plugin_invoke: Arc::new(move |plugin_id, action, payload| {
                let registry = plugin_registry
                    .read()
                    .map_err(|e| format!("Registry lock poisoned: {e}"))?;
                registry.invoke_plugin(&plugin_id, &action, &payload)
            }),
            engine_invoke: {
                let state = Arc::clone(&self.state);
                let identity_node_id = self.identity.node_id().to_string();
                let identity_cluster = self.identity.cluster_id().unwrap_or_default().to_string();
                let membership = Arc::clone(&self.membership);
                let started_at = self.started_at;
                let storage = Arc::clone(&self.storage);
                Arc::new(move |command: String, _payload: Vec<u8>| {
                    match command.as_str() {
                        "status" => {
                            let st = state.try_read().map_err(|e| format!("lock: {e}"))?;
                            Ok(serde_json::json!({
                                "node_id": identity_node_id,
                                "cluster_id": identity_cluster,
                                "state": st.display_name(),
                                "uptime_secs": started_at.elapsed().as_secs(),
                            }).to_string().into_bytes())
                        }
                        "metrics" => {
                            let m = membership.read().map_err(|e| format!("lock: {e}"))?;
                            let members = m.get_members();
                            let count = members.len();
                            Ok(serde_json::json!({
                                "member_count": count,
                                "uptime_secs": started_at.elapsed().as_secs(),
                            }).to_string().into_bytes())
                        }
                        "succession" => {
                            match storage.get_succession_line(&identity_cluster) {
                                Ok(Some(rec)) => Ok(rec.succession_json.into_bytes()),
                                Ok(None) => Ok(b"[]".to_vec()),
                                Err(e) => Err(format!("storage error: {e}")),
                            }
                        }
                        "config" => {
                            Ok(serde_json::json!({
                                "node_id": identity_node_id,
                                "cluster_id": identity_cluster,
                            }).to_string().into_bytes())
                        }
                        other => Err(format!("Unknown engine command: {other}")),
                    }
                })
            },
            task_progress: {
                let storage = Arc::clone(&self.storage);
                let event_bus = self.event_bus();
                Arc::new(move |task_id: String, progress: u8| {
                    // Update task in storage and emit event
                    if let Ok(Some(task)) = storage.get_task(&task_id) {
                        let mut updated = task.clone();
                        // Store progress in payload_json as a simple update
                        updated.payload_json = format!(r#"{{"progress":{}}}"#, progress);
                        let _ = storage.update_task(task, updated);
                    }
                    event_bus.publish(crate::runtime::events::EngineEvent::TaskUpdate {
                        task_id, status: format!("progress:{}", progress),
                    });
                    0
                })
            },
            task_complete: {
                let storage = Arc::clone(&self.storage);
                let event_bus = self.event_bus();
                Arc::new(move |task_id: String, _result_data: Vec<u8>| {
                    if let Ok(Some(task)) = storage.get_task(&task_id) {
                        let mut updated = task.clone();
                        updated.status = "completed".to_string();
                        updated.completed_at = Some(chrono::Utc::now().timestamp());
                        let _ = storage.update_task(task, updated);
                    }
                    event_bus.publish(crate::runtime::events::EngineEvent::TaskUpdate {
                        task_id, status: "completed".to_string(),
                    });
                    0
                })
            },
            task_failed: {
                let storage = Arc::clone(&self.storage);
                let event_bus = self.event_bus();
                Arc::new(move |task_id: String, error_msg: String| {
                    if let Ok(Some(task)) = storage.get_task(&task_id) {
                        let mut updated = task.clone();
                        updated.status = "failed".to_string();
                        updated.error_message = Some(error_msg.clone());
                        updated.completed_at = Some(chrono::Utc::now().timestamp());
                        let _ = storage.update_task(task, updated);
                    }
                    event_bus.publish(crate::runtime::events::EngineEvent::TaskUpdate {
                        task_id, status: format!("failed:{}", error_msg),
                    });
                    0
                })
            },
        });
    }

    /// Validate and apply a state transition.
    ///
    /// Safe to call from both sync and async contexts. Uses `try_write()` which
    /// is non-blocking. The state lock is never contended in normal operation
    /// since all state transitions are sequential.
    fn transition_state(&self, target: NodeState) -> Result<()> {
        let mut state = self
            .state
            .try_write()
            .map_err(|_| anyhow::anyhow!("State lock contended during transition"))?;
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

/// Collect real device metrics from Android platform channels.
///
/// On Android, this calls into the Kotlin `NebulaPlatformBridge` via JNI
/// to read battery level, CPU load, RAM, and network type. On non-Android
/// platforms (desktop, tests), the platform calls return errors and the
/// values gracefully fall back to zero/defaults.
fn collect_device_metrics(
    node_id: NodeId,
    task_executor: &Arc<StdRwLock<TaskExecutor>>,
    started_at: Instant,
) -> NodeHeartBeatPayload {
    // Battery level from Android device.getBatteryInfo
    let battery_level = crate::platform::invoke_android("device", "getBatteryInfo", "{}")
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|v| v["level"].as_u64())
        .unwrap_or(0) as u8;

    // CPU load from Android system.getCpuInfo
    let cpu_load = crate::platform::invoke_android("system", "getCpuInfo", "{}")
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|v| v["loadPercent"].as_f64())
        .unwrap_or(0.0) as f32;

    // Available RAM from Android system.getRamInfo
    let memory_available_mb = crate::platform::invoke_android("system", "getRamInfo", "{}")
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|v| v["availableMb"].as_u64())
        .unwrap_or(0) as u32;

    // Network type from Android device.getNetworkInfo
    let network_type = crate::platform::invoke_android("device", "getNetworkInfo", "{}")
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|v| v["type"].as_str().map(String::from))
        .map(|t| match t.to_lowercase().as_str() {
            "wifi" => NetworkType::Wifi,
            "cellular" | "mobile" => NetworkType::Cellular,
            "ethernet" => NetworkType::Ethernet,
            _ => NetworkType::Unknown,
        })
        .unwrap_or(NetworkType::Unknown);

    // Active task count from the executor
    let active_tasks = task_executor
        .read()
        .map(|exec| exec.active_count() as u16)
        .unwrap_or(0);

    // Uptime in seconds since the engine was created
    let uptime_secs = started_at.elapsed().as_secs();

    NodeHeartBeatPayload {
        node_id,
        battery_level,
        cpu_load,
        memory_available_mb,
        uptime_secs,
        active_tasks,
        network_type,
        timestamp: chrono::Utc::now().timestamp(),
    }
}

/// Background heartbeat loop that sends periodic health metrics to the proxy
/// server over the tunnel client's control channel.
///
/// The loop runs until a shutdown signal is received or the tunnel client
/// is disconnected. Collects real device metrics via Android platform channels
/// on each interval.
async fn run_heartbeat_loop(
    tunnel_client: Arc<TokioMutex<Option<TunnelClient>>>,
    node_id: NodeId,
    mut shutdown_rx: Option<broadcast::Receiver<bool>>,
    task_executor: Arc<StdRwLock<TaskExecutor>>,
    started_at: Instant,
) {
    let interval = Duration::from_secs(HEARTBEAT_INTERVAL_SECS);

    loop {
        // Check for shutdown signal
        if let Some(ref mut rx) = shutdown_rx {
            match rx.try_recv() {
                Ok(true) => {
                    info!("Heartbeat loop: shutdown signal received");
                    break;
                }
                Err(broadcast::error::TryRecvError::Closed) => {
                    info!("Heartbeat loop: shutdown channel closed");
                    break;
                }
                _ => {}
            }
        }

        tokio::time::sleep(interval).await;

        let payload = collect_device_metrics(node_id, &task_executor, started_at);

        let mut tc = tunnel_client.lock().await;
        if let Some(ref mut client) = *tc {
            if let Err(e) = client.send_heartbeat(payload).await {
                error!(error = %e, "Failed to send heartbeat");
                break;
            }
        } else {
            warn!("Heartbeat loop: tunnel client not available");
            break;
        }
    }

    info!("Heartbeat loop exited");
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
        assert!(router.table().target_count() == 0); // freshly created router

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

    #[test]
    fn test_engine_tunnel_client_initially_none() {
        let dir = temp_test_dir("tunnel_none");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let tc = engine.tunnel_client().blocking_lock();
        assert!(tc.is_none());

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_connect_to_server_without_config_fails() {
        let dir = temp_test_dir("connect_no_config");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let result = engine.connect_to_server().await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not configured")
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_connect_to_server_with_mock_server() {
        use tokio::net::TcpListener;

        let dir = temp_test_dir("connect_mock");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        engine
            .configure("test-cluster", &addr.to_string(), "token")
            .unwrap();
        engine.start().unwrap();

        // Mock server
        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            use crate::net::codec;
            let _hello = codec::read_hello(&mut stream).await.unwrap();
            let ack = nebula_core::protocol::messages::Ack::RegistrationAccepted {
                assigned_role: NodeRole::Worker,
            };
            codec::write_msg(&mut stream, &ack).await.unwrap();

            // Hold the connection open briefly so heartbeat loop doesn't fail
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        let role = engine.connect_to_server().await.unwrap();
        assert_eq!(role, NodeRole::Worker);
        assert_eq!(
            engine.state(),
            NodeState::Active {
                role: NodeRole::Worker
            }
        );

        // Tunnel client should be stored
        let tc = engine.tunnel_client().lock().await;
        assert!(tc.is_some());
        drop(tc);

        // Shutdown to stop heartbeat loop
        engine.shutdown().unwrap();

        server_handle.await.unwrap();
        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_connect_to_server_failure_transitions_to_reconnecting() {
        let dir = temp_test_dir("connect_fail_reconnect");
        let mut engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        // Use a port that nothing is listening on
        engine
            .configure("test-cluster", "127.0.0.1:1", "token")
            .unwrap();
        engine.start().unwrap();

        let result = engine.connect_to_server().await;
        assert!(result.is_err());

        assert_eq!(
            engine.state(),
            NodeState::Reconnecting { attempts: 1 }
        );

        cleanup(&dir);
    }

    // -----------------------------------------------------------------------
    // collect_device_metrics tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_collect_device_metrics_returns_defaults_on_non_android() {
        let dir = temp_test_dir("metrics_defaults");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        let payload = collect_device_metrics(
            engine.node_id(),
            engine.task_executor(),
            engine.started_at,
        );

        // On non-Android, platform calls fail => all values default to 0
        assert_eq!(payload.node_id, engine.node_id());
        assert_eq!(payload.battery_level, 0);
        assert_eq!(payload.cpu_load, 0.0);
        assert_eq!(payload.memory_available_mb, 0);
        assert_eq!(payload.active_tasks, 0);
        assert_eq!(payload.network_type, NetworkType::Unknown);
        assert!(payload.uptime_secs < 5); // Just created, uptime should be near zero
        assert!(payload.timestamp > 0);

        cleanup(&dir);
    }

    #[test]
    fn test_collect_device_metrics_reads_active_tasks() {
        use crate::tasks::types::{TaskId, TaskPayload, TaskPriority, TaskType};

        let dir = temp_test_dir("metrics_tasks");
        let engine = NebulaEngine::new(dir.to_str().unwrap()).unwrap();

        // Add some tasks to the executor
        {
            let mut executor = engine.task_executor().write().unwrap();
            executor
                .accept_task(TaskPayload {
                    task_id: TaskId("t1".to_string()),
                    task_type: TaskType::Ping,
                    data: serde_json::json!({}),
                    timeout_secs: 90,
                    submitted_at: chrono::Utc::now().timestamp_millis(),
                    priority: TaskPriority::Normal,
                })
                .unwrap();
            executor
                .accept_task(TaskPayload {
                    task_id: TaskId("t2".to_string()),
                    task_type: TaskType::Ping,
                    data: serde_json::json!({}),
                    timeout_secs: 90,
                    submitted_at: chrono::Utc::now().timestamp_millis(),
                    priority: TaskPriority::Normal,
                })
                .unwrap();
        }

        let payload = collect_device_metrics(
            engine.node_id(),
            engine.task_executor(),
            engine.started_at,
        );

        assert_eq!(payload.active_tasks, 2);

        cleanup(&dir);
    }
}
