use flutter_rust_bridge::frb;
use std::sync::{OnceLock, RwLock};

use crate::cluster::hierarchy::HierarchyManager;
use crate::routing::table::RouteMethod;
use crate::runtime::engine::NebulaEngine;

/// Global engine singleton.
///
/// Initialized once by `init_engine` and accessed by all other FFI functions.
/// Uses `OnceLock` for one-time initialization and `std::sync::RwLock` for
/// interior mutability so that both sync and async FFI functions can access it.
static ENGINE: OnceLock<RwLock<NebulaEngine>> = OnceLock::new();

/// Get a read reference to the global engine, returning an FFI-friendly error
/// if it has not been initialized yet.
fn with_engine<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&NebulaEngine) -> Result<T, String>,
{
    let lock = ENGINE
        .get()
        .ok_or_else(|| "Engine not initialized. Call init_engine first.".to_string())?;
    let engine = lock
        .read()
        .map_err(|e| format!("Engine lock poisoned: {}", e))?;
    f(&engine)
}

/// Get a write reference to the global engine, returning an FFI-friendly error
/// if it has not been initialized yet.
fn with_engine_mut<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut NebulaEngine) -> Result<T, String>,
{
    let lock = ENGINE
        .get()
        .ok_or_else(|| "Engine not initialized. Call init_engine first.".to_string())?;
    let mut engine = lock
        .write()
        .map_err(|e| format!("Engine lock poisoned: {}", e))?;
    f(&mut engine)
}

/// Initialize the node engine with a persistent storage path.
///
/// Creates or loads the node identity from `{storage_path}/node_identity.json`.
/// Returns the node ID as a string. This must be the first FFI function called.
///
/// Calling this function more than once is a no-op -- the engine is initialized
/// only on the first call.
#[frb(sync)]
pub fn init_engine(storage_path: String) -> Result<String, String> {
    // If engine is already initialized, return its node_id
    if let Some(lock) = ENGINE.get() {
        let engine = lock
            .read()
            .map_err(|e| format!("Engine lock poisoned: {}", e))?;
        return Ok(engine.node_id().to_string());
    }

    let engine =
        NebulaEngine::new(&storage_path).map_err(|e| format!("Failed to init engine: {}", e))?;
    let node_id = engine.node_id().to_string();

    // set() returns Err if another thread raced us. That is fine -- use
    // whichever instance won.
    let _ = ENGINE.set(RwLock::new(engine));

    Ok(node_id)
}

/// Get current node status as a JSON string.
///
/// Returns a JSON object with the following fields:
/// - `state`: current state display name
/// - `node_id`: the node's unique identifier
/// - `cluster_id`: the configured cluster ID (null if not configured)
/// - `is_configured`: whether cluster configuration is present
/// - `is_active`: whether the node is in the Active state
#[frb(sync)]
pub fn get_node_status() -> Result<String, String> {
    with_engine(|engine| {
        let state = engine.state();
        let status = serde_json::json!({
            "state": state.display_name(),
            "node_id": engine.node_id().to_string(),
            "cluster_id": engine.cluster_id(),
            "is_configured": engine.is_configured(),
            "is_active": state.is_active(),
        });
        serde_json::to_string(&status).map_err(|e| format!("Failed to serialize status: {}", e))
    })
}

/// Start the engine and initiate connection to the proxy server.
///
/// Transitions to Connecting state, then spawns an async task that:
/// 1. Connects to the proxy server via TCP
/// 2. Registers the node (handshake)
/// 3. Transitions through Registering -> Active
/// 4. Starts MQTT broker if assigned Master role
/// 5. Spawns a background heartbeat loop
///
/// Returns the assigned role as a string ("Master", "Worker", etc.) on
/// success. On connection failure the engine transitions to Reconnecting.
pub fn start_engine() -> Result<String, String> {
    with_engine(|engine| {
        // Synchronous state transition: Configured -> Connecting
        engine.start().map_err(|e| format!("{}", e))?;

        // Create a new tokio runtime for the async connection work.
        // flutter_rust_bridge runs non-sync functions on a thread pool, so
        // we need our own runtime to drive async I/O.
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;

        let role = rt.block_on(engine.connect_to_server()).map_err(|e| {
            format!("Connection failed: {}", e)
        })?;

        Ok(format!("{}", role))
    })
}

/// Shut down the engine gracefully.
///
/// Sends a shutdown signal to all internal listeners and transitions the
/// engine to the ShuttingDown state.
pub fn shutdown_engine() -> Result<(), String> {
    with_engine(|engine| engine.shutdown().map_err(|e| format!("{}", e)))
}

/// Get current cluster members as a JSON string.
///
/// Returns a JSON array of objects, each containing:
/// - `node_id`: the member's unique identifier
/// - `role`: the member's current role (Master, Worker, etc.)
/// - `battery_level`: last reported battery percentage
/// - `cpu_load`: last reported CPU load
/// - `memory_available_mb`: last reported available memory
/// - `active_tasks`: last reported task count
/// - `uptime_secs`: last reported uptime
#[frb(sync)]
pub fn get_cluster_members() -> Result<String, String> {
    with_engine(|engine| {
        let membership = engine
            .membership()
            .read()
            .map_err(|e| format!("Membership lock poisoned: {}", e))?;

        let members: Vec<serde_json::Value> = membership
            .get_members()
            .values()
            .map(|info| {
                serde_json::json!({
                    "node_id": info.node_id.to_string(),
                    "role": format!("{}", info.role),
                    "battery_level": info.metrics.battery_level,
                    "cpu_load": info.metrics.cpu_load,
                    "memory_available_mb": info.metrics.memory_available_mb,
                    "active_tasks": info.metrics.active_tasks,
                    "uptime_secs": info.metrics.uptime_secs,
                })
            })
            .collect();

        serde_json::to_string(&members)
            .map_err(|e| format!("Failed to serialize members: {}", e))
    })
}

/// Get current cluster topology as a JSON string.
///
/// Returns a JSON object describing the topology type and structure:
/// - `type`: "Standalone", "Flat", or "Hierarchical"
/// - For Flat: includes `master` and `workers` fields
/// - For Hierarchical: includes `super_master` and `regions` fields
#[frb(sync)]
pub fn get_cluster_topology() -> Result<String, String> {
    with_engine(|engine| {
        let membership = engine
            .membership()
            .read()
            .map_err(|e| format!("Membership lock poisoned: {}", e))?;

        let members_map = membership.get_members();
        let member_refs: Vec<_> = members_map.values().collect();

        let hierarchy = HierarchyManager::new(membership.max_workers_per_master());
        let topology = hierarchy.determine_topology(&member_refs);

        let json = match topology {
            crate::cluster::hierarchy::ClusterTopology::Standalone => {
                serde_json::json!({ "type": "Standalone" })
            }
            crate::cluster::hierarchy::ClusterTopology::Flat { master, workers } => {
                serde_json::json!({
                    "type": "Flat",
                    "master": master.to_string(),
                    "workers": workers.iter().map(|w| w.to_string()).collect::<Vec<_>>(),
                })
            }
            crate::cluster::hierarchy::ClusterTopology::Hierarchical {
                super_master,
                regions,
            } => {
                let regions_json: Vec<serde_json::Value> = regions
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "regional_master": r.regional_master.to_string(),
                            "workers": r.workers.iter().map(|w| w.to_string()).collect::<Vec<_>>(),
                        })
                    })
                    .collect();

                serde_json::json!({
                    "type": "Hierarchical",
                    "super_master": super_master.to_string(),
                    "regions": regions_json,
                })
            }
        };

        serde_json::to_string(&json)
            .map_err(|e| format!("Failed to serialize topology: {}", e))
    })
}

/// Get the current routing table as a JSON string.
///
/// Returns a JSON object mapping node IDs to their best route:
/// - `routes`: array of objects, each containing:
///   - `target`: the target node ID
///   - `method`: "LanDirect", "HolePunch", or "TunnelRelay"
///   - `addr`: the address (for LanDirect and HolePunch; null for TunnelRelay)
///
/// Returns an empty routes array if the router is not initialized (engine
/// not configured).
#[frb(sync)]
pub fn get_routing_table() -> Result<String, String> {
    with_engine(|engine| {
        let router_lock = engine
            .smart_router()
            .read()
            .map_err(|e| format!("Router lock poisoned: {}", e))?;

        let routes: Vec<serde_json::Value> = match router_lock.as_ref() {
            Some(router) => router
                .routing_summary()
                .iter()
                .map(|(target, method)| {
                    let (method_name, addr) = match method {
                        RouteMethod::LanDirect { addr } => {
                            ("LanDirect", Some(addr.to_string()))
                        }
                        RouteMethod::HolePunch { addr } => {
                            ("HolePunch", Some(addr.to_string()))
                        }
                        RouteMethod::TunnelRelay => ("TunnelRelay", None),
                    };

                    serde_json::json!({
                        "target": target.to_string(),
                        "method": method_name,
                        "addr": addr,
                    })
                })
                .collect(),
            None => Vec::new(),
        };

        let json = serde_json::json!({ "routes": routes });
        serde_json::to_string(&json)
            .map_err(|e| format!("Failed to serialize routing table: {}", e))
    })
}

/// Provide access to the global engine for other API modules.
pub(crate) fn with_engine_read<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&NebulaEngine) -> Result<T, String>,
{
    with_engine(f)
}

/// Provide mutable access to the global engine for other API modules.
pub(crate) fn with_engine_write<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut NebulaEngine) -> Result<T, String>,
{
    with_engine_mut(f)
}
