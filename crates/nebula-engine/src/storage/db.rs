use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::Connection;
use tracing::info;

use super::encryption::StorageEncryption;
use super::migrations;
use super::models::*;

/// On-device storage manager backed by rusqlite with SQLCipher encryption.
///
/// Provides typed CRUD operations for all NEBULA domain models and an
/// encrypted key-value store powered by `SecurityEnvelope`.
pub struct StorageManager {
    conn: Mutex<Connection>,
    encryption: StorageEncryption,
}

impl StorageManager {
    /// Open (or create) a persistent database at `storage_path`.
    ///
    /// The database file is opened, never recreated. SQLCipher encrypts the
    /// file at rest using a key derived from `encryption_secret`. WAL mode
    /// and a busy timeout are configured for robustness.
    pub fn new(storage_path: &str, encryption_secret: &[u8]) -> Result<Self> {
        let encryption = StorageEncryption::new(encryption_secret)?;
        let db_path = Path::new(storage_path).join("nebula_storage.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("failed to create storage directory: {}", e))?;
        }

        let conn = Connection::open(&db_path)
            .with_context(|| format!("open database at {}", db_path.display()))?;

        // SQLCipher encryption key -- derived from the stable node identity
        let key_hex = hex::encode(&encryption_secret[..encryption_secret.len().min(32)]);
        conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";\n", key_hex))
            .context("set SQLCipher key")?;

        Self::apply_pragmas(&conn, false)?;
        migrations::apply_migrations(&conn)?;
        info!(path = %db_path.display(), "storage database opened");

        Ok(Self {
            conn: Mutex::new(conn),
            encryption,
        })
    }

    /// Create an in-memory database (useful for tests).
    pub fn new_in_memory(encryption_secret: &[u8]) -> Result<Self> {
        let encryption = StorageEncryption::new(encryption_secret)?;
        let conn = Connection::open_in_memory().context("open in-memory database")?;
        Self::apply_pragmas(&conn, true)?;
        migrations::apply_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            encryption,
        })
    }

    /// Apply standard PRAGMAs for safety and performance.
    fn apply_pragmas(conn: &Connection, in_memory: bool) -> Result<()> {
        if !in_memory {
            conn.execute_batch("PRAGMA journal_mode = WAL;")
                .context("set journal_mode")?;
        }
        conn.execute_batch(
            "PRAGMA synchronous = NORMAL;\n\
             PRAGMA foreign_keys = ON;\n\
             PRAGMA busy_timeout = 5000;",
        )
        .context("set pragmas")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // PluginStateRecord CRUD
    // -----------------------------------------------------------------------

    pub fn insert_plugin_state(&self, state: PluginStateRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO plugin_states (plugin_id, version, enabled, installed_at, last_executed_at, config_json, checksum)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                state.plugin_id,
                state.version,
                state.enabled as i32,
                state.installed_at,
                state.last_executed_at,
                state.config_json,
                state.checksum
            ],
        )
        .context("insert plugin state")?;
        Ok(())
    }

    pub fn get_plugin_state(&self, plugin_id: &str) -> Result<Option<PluginStateRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT plugin_id, version, enabled, installed_at, last_executed_at, config_json, checksum FROM plugin_states WHERE plugin_id = ?1")
            .context("prepare get_plugin_state")?;
        let mut rows = stmt
            .query_map(rusqlite::params![plugin_id], row_to_plugin_state)
            .context("query get_plugin_state")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read plugin_state row")?)),
            None => Ok(None),
        }
    }

    pub fn list_plugin_states(&self) -> Result<Vec<PluginStateRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT plugin_id, version, enabled, installed_at, last_executed_at, config_json, checksum FROM plugin_states")
            .context("prepare list_plugin_states")?;
        let rows = stmt
            .query_map([], row_to_plugin_state)
            .context("query list_plugin_states")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("collect plugin states")
    }

    pub fn update_plugin_state(
        &self,
        _old: PluginStateRecord,
        new: PluginStateRecord,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "UPDATE plugin_states SET version = ?1, enabled = ?2, installed_at = ?3, last_executed_at = ?4, config_json = ?5, checksum = ?6 WHERE plugin_id = ?7",
            rusqlite::params![new.version, new.enabled as i32, new.installed_at, new.last_executed_at, new.config_json, new.checksum, new.plugin_id],
        )
        .context("update plugin state")?;
        Ok(())
    }

    pub fn remove_plugin_state(&self, state: PluginStateRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "DELETE FROM plugin_states WHERE plugin_id = ?1",
            rusqlite::params![state.plugin_id],
        )
        .context("remove plugin state")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // TaskQueueItem CRUD
    // -----------------------------------------------------------------------

    pub fn enqueue_task(&self, task: TaskQueueItem) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO task_queue (task_id, status, priority, task_type, payload_json, submitted_at, started_at, completed_at, assigned_node, timeout_secs, retry_count, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                task.task_id, task.status, task.priority as i32, task.task_type,
                task.payload_json, task.submitted_at, task.started_at, task.completed_at,
                task.assigned_node, task.timeout_secs, task.retry_count as i32, task.error_message
            ],
        )
        .context("enqueue task")?;
        Ok(())
    }

    pub fn dequeue_next_task(&self) -> Result<Option<TaskQueueItem>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let tx = conn.unchecked_transaction().context("begin dequeue tx")?;
        let result = {
            let mut stmt = tx.prepare(
                "SELECT task_id, status, priority, task_type, payload_json, submitted_at, started_at, completed_at, assigned_node, timeout_secs, retry_count, error_message
                 FROM task_queue WHERE status = 'pending' ORDER BY priority DESC, submitted_at ASC LIMIT 1",
            ).context("prepare dequeue")?;
            let mut rows = stmt.query_map([], row_to_task).context("query dequeue")?;
            match rows.next() {
                Some(row) => Some(row.context("read task row")?),
                None => None,
            }
        };
        if let Some(ref task) = result {
            tx.execute(
                "DELETE FROM task_queue WHERE task_id = ?1",
                rusqlite::params![task.task_id],
            )
            .context("delete dequeued task")?;
        }
        tx.commit().context("commit dequeue")?;
        Ok(result)
    }

    pub fn get_tasks_by_status(&self, status: &str) -> Result<Vec<TaskQueueItem>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT task_id, status, priority, task_type, payload_json, submitted_at, started_at, completed_at, assigned_node, timeout_secs, retry_count, error_message
             FROM task_queue WHERE status = ?1",
        ).context("prepare get_tasks_by_status")?;
        let rows = stmt
            .query_map(rusqlite::params![status], row_to_task)
            .context("query get_tasks_by_status")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("collect tasks")
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<TaskQueueItem>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT task_id, status, priority, task_type, payload_json, submitted_at, started_at, completed_at, assigned_node, timeout_secs, retry_count, error_message
             FROM task_queue WHERE task_id = ?1",
        ).context("prepare get_task")?;
        let mut rows = stmt
            .query_map(rusqlite::params![task_id], row_to_task)
            .context("query get_task")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read task row")?)),
            None => Ok(None),
        }
    }

    pub fn update_task(&self, _old: TaskQueueItem, new: TaskQueueItem) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "UPDATE task_queue SET status=?1, priority=?2, task_type=?3, payload_json=?4, submitted_at=?5, started_at=?6, completed_at=?7, assigned_node=?8, timeout_secs=?9, retry_count=?10, error_message=?11 WHERE task_id=?12",
            rusqlite::params![new.status, new.priority as i32, new.task_type, new.payload_json, new.submitted_at, new.started_at, new.completed_at, new.assigned_node, new.timeout_secs, new.retry_count as i32, new.error_message, new.task_id],
        ).context("update task")?;
        Ok(())
    }

    pub fn remove_task(&self, task: TaskQueueItem) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "DELETE FROM task_queue WHERE task_id = ?1",
            rusqlite::params![task.task_id],
        )
        .context("remove task")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // ClusterMemberRecord CRUD
    // -----------------------------------------------------------------------

    pub fn upsert_cluster_member(&self, m: ClusterMemberRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO cluster_members (node_id, role, join_time, last_heartbeat, battery_level, cpu_load, memory_available_mb, active_tasks, network_type, peer_address, is_stale)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(node_id) DO UPDATE SET role=excluded.role, join_time=excluded.join_time, last_heartbeat=excluded.last_heartbeat, battery_level=excluded.battery_level, cpu_load=excluded.cpu_load, memory_available_mb=excluded.memory_available_mb, active_tasks=excluded.active_tasks, network_type=excluded.network_type, peer_address=excluded.peer_address, is_stale=excluded.is_stale",
            rusqlite::params![m.node_id, m.role, m.join_time, m.last_heartbeat, m.battery_level as i32, m.cpu_load as f64, m.memory_available_mb, m.active_tasks as i32, m.network_type, m.peer_address, m.is_stale as i32],
        ).context("upsert cluster member")?;
        Ok(())
    }

    pub fn get_cluster_member(&self, node_id: &str) -> Result<Option<ClusterMemberRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT node_id, role, join_time, last_heartbeat, battery_level, cpu_load, memory_available_mb, active_tasks, network_type, peer_address, is_stale FROM cluster_members WHERE node_id = ?1").context("prepare")?;
        let mut rows = stmt
            .query_map(rusqlite::params![node_id], row_to_cluster_member)
            .context("query")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read row")?)),
            None => Ok(None),
        }
    }

    pub fn get_cluster_members(&self) -> Result<Vec<ClusterMemberRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT node_id, role, join_time, last_heartbeat, battery_level, cpu_load, memory_available_mb, active_tasks, network_type, peer_address, is_stale FROM cluster_members").context("prepare")?;
        let rows = stmt.query_map([], row_to_cluster_member).context("query")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("collect")
    }

    pub fn get_stale_members(&self) -> Result<Vec<ClusterMemberRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT node_id, role, join_time, last_heartbeat, battery_level, cpu_load, memory_available_mb, active_tasks, network_type, peer_address, is_stale FROM cluster_members WHERE is_stale = 1").context("prepare")?;
        let rows = stmt.query_map([], row_to_cluster_member).context("query")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("collect")
    }

    pub fn remove_cluster_member(&self, member: ClusterMemberRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "DELETE FROM cluster_members WHERE node_id = ?1",
            rusqlite::params![member.node_id],
        )
        .context("remove")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // HeartbeatRecord CRUD
    // -----------------------------------------------------------------------

    pub fn append_heartbeat(&self, r: HeartbeatRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO heartbeats (id, node_id, timestamp, battery_level, cpu_load, memory_available_mb, active_tasks, network_type) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![r.id, r.node_id, r.timestamp, r.battery_level as i32, r.cpu_load as f64, r.memory_available_mb, r.active_tasks as i32, r.network_type],
        ).context("append heartbeat")?;
        Ok(())
    }

    pub fn get_heartbeats_for_node(&self, node_id: &str) -> Result<Vec<HeartbeatRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT id, node_id, timestamp, battery_level, cpu_load, memory_available_mb, active_tasks, network_type FROM heartbeats WHERE node_id = ?1").context("prepare")?;
        let rows = stmt
            .query_map(rusqlite::params![node_id], row_to_heartbeat)
            .context("query")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("collect")
    }

    pub fn get_heartbeat(&self, id: &str) -> Result<Option<HeartbeatRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT id, node_id, timestamp, battery_level, cpu_load, memory_available_mb, active_tasks, network_type FROM heartbeats WHERE id = ?1").context("prepare")?;
        let mut rows = stmt
            .query_map(rusqlite::params![id], row_to_heartbeat)
            .context("query")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read row")?)),
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // MqttOfflineMessage CRUD
    // -----------------------------------------------------------------------

    pub fn enqueue_offline_message(&self, msg: MqttOfflineMessage) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO mqtt_offline_queue (id, topic, payload, qos, queued_at, retry_count) VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![msg.id, msg.topic, msg.payload_bytes, msg.qos as i32, msg.queued_at, msg.retry_count as i32],
        ).context("enqueue offline message")?;
        Ok(())
    }

    pub fn drain_offline_messages(&self) -> Result<Vec<MqttOfflineMessage>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let tx = conn.unchecked_transaction().context("begin drain tx")?;
        let results = {
            let mut stmt = tx.prepare("SELECT id, topic, payload, qos, queued_at, retry_count FROM mqtt_offline_queue ORDER BY queued_at ASC").context("prepare")?;
            let rows = stmt.query_map([], row_to_mqtt_offline).context("query")?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("collect")?
        };
        tx.execute_batch("DELETE FROM mqtt_offline_queue;")
            .context("delete drained")?;
        tx.commit().context("commit drain")?;
        Ok(results)
    }

    pub fn get_offline_message(&self, id: &str) -> Result<Option<MqttOfflineMessage>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT id, topic, payload, qos, queued_at, retry_count FROM mqtt_offline_queue WHERE id = ?1").context("prepare")?;
        let mut rows = stmt
            .query_map(rusqlite::params![id], row_to_mqtt_offline)
            .context("query")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read row")?)),
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // SuccessionRecord CRUD
    // -----------------------------------------------------------------------

    pub fn set_succession_line(&self, r: SuccessionRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO succession (cluster_id, succession_json, computed_at, computed_by) VALUES (?1,?2,?3,?4)
             ON CONFLICT(cluster_id) DO UPDATE SET succession_json=excluded.succession_json, computed_at=excluded.computed_at, computed_by=excluded.computed_by",
            rusqlite::params![r.cluster_id, r.succession_json, r.computed_at, r.computed_by],
        ).context("set succession line")?;
        Ok(())
    }

    pub fn get_succession_line(&self, cluster_id: &str) -> Result<Option<SuccessionRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT cluster_id, succession_json, computed_at, computed_by FROM succession WHERE cluster_id = ?1").context("prepare")?;
        let mut rows = stmt
            .query_map(rusqlite::params![cluster_id], row_to_succession)
            .context("query")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read row")?)),
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // PeerNodeRecord CRUD
    // -----------------------------------------------------------------------

    pub fn upsert_peer_node(&self, p: PeerNodeRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO peer_nodes (node_id, role, address, port, roles_json, capabilities_json, last_seen, connection_state)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(node_id) DO UPDATE SET role=excluded.role, address=excluded.address, port=excluded.port, roles_json=excluded.roles_json, capabilities_json=excluded.capabilities_json, last_seen=excluded.last_seen, connection_state=excluded.connection_state",
            rusqlite::params![p.node_id, p.role, p.address, p.port as i32, p.roles_json, p.capabilities_json, p.last_seen, p.connection_state],
        ).context("upsert peer node")?;
        Ok(())
    }

    pub fn get_peer_node(&self, node_id: &str) -> Result<Option<PeerNodeRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT node_id, role, address, port, roles_json, capabilities_json, last_seen, connection_state FROM peer_nodes WHERE node_id = ?1").context("prepare")?;
        let mut rows = stmt
            .query_map(rusqlite::params![node_id], row_to_peer_node)
            .context("query")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read row")?)),
            None => Ok(None),
        }
    }

    pub fn get_peer_nodes(&self) -> Result<Vec<PeerNodeRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn.prepare("SELECT node_id, role, address, port, roles_json, capabilities_json, last_seen, connection_state FROM peer_nodes").context("prepare")?;
        let rows = stmt.query_map([], row_to_peer_node).context("query")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("collect")
    }

    pub fn remove_peer_node(&self, peer: PeerNodeRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "DELETE FROM peer_nodes WHERE node_id = ?1",
            rusqlite::params![peer.node_id],
        )
        .context("remove")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // NodeConfigRecord (key-value config)
    // -----------------------------------------------------------------------

    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO node_config (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, value, now],
        ).context("set config")?;
        Ok(())
    }

    pub fn get_config(&self, key: &str) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT value FROM node_config WHERE key = ?1")
            .context("prepare")?;
        let mut rows = stmt
            .query_map(rusqlite::params![key], |row| row.get::<_, String>(0))
            .context("query")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read value")?)),
            None => Ok(None),
        }
    }

    pub fn remove_config(&self, key: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "DELETE FROM node_config WHERE key = ?1",
            rusqlite::params![key],
        )
        .context("remove")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // EncryptedBlobRecord (encrypted key-value store)
    // -----------------------------------------------------------------------

    pub fn store_encrypted(&self, key: &str, category: &str, data: &[u8]) -> Result<()> {
        let encrypted_data = self.encryption.encrypt(data)?;
        let now = chrono::Utc::now().timestamp();
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "INSERT INTO encrypted_blobs (key, category, encrypted_data, created_at, updated_at) VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(key) DO UPDATE SET category=excluded.category, encrypted_data=excluded.encrypted_data, updated_at=excluded.updated_at",
            rusqlite::params![key, category, encrypted_data, now, now],
        ).context("store encrypted")?;
        Ok(())
    }

    pub fn retrieve_encrypted(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT encrypted_data FROM encrypted_blobs WHERE key = ?1")
            .context("prepare")?;
        let mut rows = stmt
            .query_map(rusqlite::params![key], |row| row.get::<_, Vec<u8>>(0))
            .context("query")?;
        match rows.next() {
            Some(row) => {
                let encrypted = row.context("read encrypted_data")?;
                let plaintext = self.encryption.decrypt(&encrypted)?;
                Ok(Some(plaintext))
            }
            None => Ok(None),
        }
    }

    pub fn remove_encrypted(&self, key: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        conn.execute(
            "DELETE FROM encrypted_blobs WHERE key = ?1",
            rusqlite::params![key],
        )
        .context("remove")?;
        Ok(())
    }

    /// Returns a reference to the encryption context.
    pub fn encryption(&self) -> &StorageEncryption {
        &self.encryption
    }

    /// Test helper: read raw encrypted_data column for a given key.
    #[cfg(test)]
    fn raw_encrypted_blob(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT encrypted_data FROM encrypted_blobs WHERE key = ?1")
            .context("prepare")?;
        let mut rows = stmt
            .query_map(rusqlite::params![key], |row| row.get::<_, Vec<u8>>(0))
            .context("query")?;
        match rows.next() {
            Some(row) => Ok(Some(row.context("read raw")?)),
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Row-to-struct helpers
// ---------------------------------------------------------------------------

fn row_to_plugin_state(row: &rusqlite::Row) -> rusqlite::Result<PluginStateRecord> {
    Ok(PluginStateRecord {
        plugin_id: row.get(0)?,
        version: row.get(1)?,
        enabled: row.get::<_, i32>(2)? != 0,
        installed_at: row.get(3)?,
        last_executed_at: row.get(4)?,
        config_json: row.get(5)?,
        checksum: row.get(6)?,
    })
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<TaskQueueItem> {
    Ok(TaskQueueItem {
        task_id: row.get(0)?,
        status: row.get(1)?,
        priority: row.get::<_, i32>(2)? as u8,
        task_type: row.get(3)?,
        payload_json: row.get(4)?,
        submitted_at: row.get(5)?,
        started_at: row.get(6)?,
        completed_at: row.get(7)?,
        assigned_node: row.get(8)?,
        timeout_secs: row.get::<_, i64>(9)? as u32,
        retry_count: row.get::<_, i32>(10)? as u8,
        error_message: row.get(11)?,
    })
}

fn row_to_cluster_member(row: &rusqlite::Row) -> rusqlite::Result<ClusterMemberRecord> {
    Ok(ClusterMemberRecord {
        node_id: row.get(0)?,
        role: row.get(1)?,
        join_time: row.get(2)?,
        last_heartbeat: row.get(3)?,
        battery_level: row.get::<_, i32>(4)? as u8,
        cpu_load: row.get::<_, f64>(5)? as f32,
        memory_available_mb: row.get::<_, i64>(6)? as u32,
        active_tasks: row.get::<_, i32>(7)? as u16,
        network_type: row.get(8)?,
        peer_address: row.get(9)?,
        is_stale: row.get::<_, i32>(10)? != 0,
    })
}

fn row_to_heartbeat(row: &rusqlite::Row) -> rusqlite::Result<HeartbeatRecord> {
    Ok(HeartbeatRecord {
        id: row.get(0)?,
        node_id: row.get(1)?,
        timestamp: row.get(2)?,
        battery_level: row.get::<_, i32>(3)? as u8,
        cpu_load: row.get::<_, f64>(4)? as f32,
        memory_available_mb: row.get::<_, i64>(5)? as u32,
        active_tasks: row.get::<_, i32>(6)? as u16,
        network_type: row.get(7)?,
    })
}

fn row_to_mqtt_offline(row: &rusqlite::Row) -> rusqlite::Result<MqttOfflineMessage> {
    Ok(MqttOfflineMessage {
        id: row.get(0)?,
        topic: row.get(1)?,
        payload_bytes: row.get(2)?,
        qos: row.get::<_, i32>(3)? as u8,
        queued_at: row.get(4)?,
        retry_count: row.get::<_, i32>(5)? as u8,
    })
}

fn row_to_succession(row: &rusqlite::Row) -> rusqlite::Result<SuccessionRecord> {
    Ok(SuccessionRecord {
        cluster_id: row.get(0)?,
        succession_json: row.get(1)?,
        computed_at: row.get(2)?,
        computed_by: row.get(3)?,
    })
}

fn row_to_peer_node(row: &rusqlite::Row) -> rusqlite::Result<PeerNodeRecord> {
    Ok(PeerNodeRecord {
        node_id: row.get(0)?,
        role: row.get(1)?,
        address: row.get(2)?,
        port: row.get::<_, i32>(3)? as u16,
        roles_json: row.get(4)?,
        capabilities_json: row.get(5)?,
        last_seen: row.get(6)?,
        connection_state: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager() -> StorageManager {
        StorageManager::new_in_memory(b"test-encryption-key").unwrap()
    }

    #[test]
    fn test_insert_and_get_plugin_state() {
        let mgr = test_manager();
        let state = PluginStateRecord {
            plugin_id: "com.example.plugin".to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            installed_at: 1700000000,
            last_executed_at: None,
            config_json: None,
            checksum: Some("abc".to_string()),
        };
        mgr.insert_plugin_state(state.clone()).unwrap();
        assert_eq!(
            mgr.get_plugin_state("com.example.plugin").unwrap(),
            Some(state)
        );
    }

    #[test]
    fn test_get_nonexistent_plugin_state() {
        assert_eq!(
            test_manager().get_plugin_state("nonexistent").unwrap(),
            None
        );
    }

    #[test]
    fn test_list_plugin_states() {
        let mgr = test_manager();
        for i in 0..3 {
            mgr.insert_plugin_state(PluginStateRecord {
                plugin_id: format!("plugin-{}", i),
                version: "1.0".to_string(),
                enabled: true,
                installed_at: 1700000000 + i,
                last_executed_at: None,
                config_json: None,
                checksum: None,
            })
            .unwrap();
        }
        assert_eq!(mgr.list_plugin_states().unwrap().len(), 3);
    }

    #[test]
    fn test_update_plugin_state() {
        let mgr = test_manager();
        let old = PluginStateRecord {
            plugin_id: "p1".to_string(),
            version: "1.0".to_string(),
            enabled: true,
            installed_at: 100,
            last_executed_at: None,
            config_json: None,
            checksum: None,
        };
        mgr.insert_plugin_state(old.clone()).unwrap();
        let new = PluginStateRecord {
            plugin_id: "p1".to_string(),
            version: "2.0".to_string(),
            enabled: false,
            installed_at: 100,
            last_executed_at: Some(200),
            config_json: Some("{}".to_string()),
            checksum: None,
        };
        mgr.update_plugin_state(old, new).unwrap();
        let r = mgr.get_plugin_state("p1").unwrap().unwrap();
        assert_eq!(r.version, "2.0");
        assert!(!r.enabled);
    }

    #[test]
    fn test_remove_plugin_state() {
        let mgr = test_manager();
        let state = PluginStateRecord {
            plugin_id: "p1".to_string(),
            version: "1.0".to_string(),
            enabled: true,
            installed_at: 100,
            last_executed_at: None,
            config_json: None,
            checksum: None,
        };
        mgr.insert_plugin_state(state.clone()).unwrap();
        mgr.remove_plugin_state(state).unwrap();
        assert_eq!(mgr.get_plugin_state("p1").unwrap(), None);
    }

    #[test]
    fn test_enqueue_and_dequeue_task() {
        let mgr = test_manager();
        let mk = |id: &str, pri: u8, at: i64| TaskQueueItem {
            task_id: id.to_string(),
            status: "pending".to_string(),
            priority: pri,
            task_type: "compute".to_string(),
            payload_json: "{}".to_string(),
            submitted_at: at,
            started_at: None,
            completed_at: None,
            assigned_node: None,
            timeout_secs: 60,
            retry_count: 0,
            error_message: None,
        };
        mgr.enqueue_task(mk("t1", 5, 100)).unwrap();
        mgr.enqueue_task(mk("t2", 10, 101)).unwrap();
        assert_eq!(mgr.dequeue_next_task().unwrap().unwrap().task_id, "t2");
        assert_eq!(mgr.dequeue_next_task().unwrap().unwrap().task_id, "t1");
        assert_eq!(mgr.dequeue_next_task().unwrap(), None);
    }

    #[test]
    fn test_dequeue_skips_non_pending() {
        let mgr = test_manager();
        mgr.enqueue_task(TaskQueueItem {
            task_id: "t1".to_string(),
            status: "running".to_string(),
            priority: 10,
            task_type: "compute".to_string(),
            payload_json: "{}".to_string(),
            submitted_at: 100,
            started_at: None,
            completed_at: None,
            assigned_node: None,
            timeout_secs: 60,
            retry_count: 0,
            error_message: None,
        })
        .unwrap();
        assert_eq!(mgr.dequeue_next_task().unwrap(), None);
    }

    #[test]
    fn test_get_tasks_by_status() {
        let mgr = test_manager();
        for (id, s) in [("t1", "pending"), ("t2", "running"), ("t3", "pending")] {
            mgr.enqueue_task(TaskQueueItem {
                task_id: id.to_string(),
                status: s.to_string(),
                priority: 5,
                task_type: "compute".to_string(),
                payload_json: "{}".to_string(),
                submitted_at: 100,
                started_at: None,
                completed_at: None,
                assigned_node: None,
                timeout_secs: 60,
                retry_count: 0,
                error_message: None,
            })
            .unwrap();
        }
        assert_eq!(mgr.get_tasks_by_status("pending").unwrap().len(), 2);
        assert_eq!(mgr.get_tasks_by_status("running").unwrap().len(), 1);
    }

    #[test]
    fn test_upsert_and_get_cluster_member() {
        let mgr = test_manager();
        let m = ClusterMemberRecord {
            node_id: "node-1".to_string(),
            role: "worker".to_string(),
            join_time: 100,
            last_heartbeat: 200,
            battery_level: 80,
            cpu_load: 0.5,
            memory_available_mb: 2048,
            active_tasks: 2,
            network_type: "wifi".to_string(),
            peer_address: None,
            is_stale: false,
        };
        mgr.upsert_cluster_member(m.clone()).unwrap();
        assert_eq!(mgr.get_cluster_member("node-1").unwrap(), Some(m));
    }

    #[test]
    fn test_upsert_updates_existing_member() {
        let mgr = test_manager();
        let mk = |role: &str, hb: i64| ClusterMemberRecord {
            node_id: "node-1".to_string(),
            role: role.to_string(),
            join_time: 100,
            last_heartbeat: hb,
            battery_level: 80,
            cpu_load: 0.5,
            memory_available_mb: 2048,
            active_tasks: 2,
            network_type: "wifi".to_string(),
            peer_address: None,
            is_stale: false,
        };
        mgr.upsert_cluster_member(mk("worker", 200)).unwrap();
        mgr.upsert_cluster_member(mk("master", 300)).unwrap();
        let r = mgr.get_cluster_member("node-1").unwrap().unwrap();
        assert_eq!(r.role, "master");
        assert_eq!(r.last_heartbeat, 300);
        assert_eq!(mgr.get_cluster_members().unwrap().len(), 1);
    }

    #[test]
    fn test_get_stale_members() {
        let mgr = test_manager();
        for (id, stale) in [("n1", false), ("n2", true), ("n3", true)] {
            mgr.upsert_cluster_member(ClusterMemberRecord {
                node_id: id.to_string(),
                role: "worker".to_string(),
                join_time: 100,
                last_heartbeat: 200,
                battery_level: 80,
                cpu_load: 0.5,
                memory_available_mb: 2048,
                active_tasks: 0,
                network_type: "wifi".to_string(),
                peer_address: None,
                is_stale: stale,
            })
            .unwrap();
        }
        assert_eq!(mgr.get_stale_members().unwrap().len(), 2);
    }

    #[test]
    fn test_append_and_get_heartbeats() {
        let mgr = test_manager();
        for i in 0..3 {
            mgr.append_heartbeat(HeartbeatRecord {
                id: format!("hb-{}", i),
                node_id: "node-1".to_string(),
                timestamp: 100 + i,
                battery_level: 90,
                cpu_load: 0.1,
                memory_available_mb: 4096,
                active_tasks: 0,
                network_type: "wifi".to_string(),
            })
            .unwrap();
        }
        assert_eq!(mgr.get_heartbeats_for_node("node-1").unwrap().len(), 3);
    }

    #[test]
    fn test_enqueue_and_drain_offline_messages() {
        let mgr = test_manager();
        for i in 0..3 {
            mgr.enqueue_offline_message(MqttOfflineMessage {
                id: format!("msg-{}", i),
                topic: "test/topic".to_string(),
                payload_bytes: vec![i as u8],
                qos: 1,
                queued_at: 100 + i as i64,
                retry_count: 0,
            })
            .unwrap();
        }
        assert_eq!(mgr.drain_offline_messages().unwrap().len(), 3);
        assert_eq!(mgr.drain_offline_messages().unwrap().len(), 0);
    }

    #[test]
    fn test_set_and_get_succession_line() {
        let mgr = test_manager();
        let r = SuccessionRecord {
            cluster_id: "c1".to_string(),
            succession_json: r#"["n1","n2"]"#.to_string(),
            computed_at: 100,
            computed_by: "n1".to_string(),
        };
        mgr.set_succession_line(r.clone()).unwrap();
        assert_eq!(mgr.get_succession_line("c1").unwrap(), Some(r));
    }

    #[test]
    fn test_succession_upsert_overwrites() {
        let mgr = test_manager();
        mgr.set_succession_line(SuccessionRecord {
            cluster_id: "c1".to_string(),
            succession_json: "[]".to_string(),
            computed_at: 100,
            computed_by: "a".to_string(),
        })
        .unwrap();
        mgr.set_succession_line(SuccessionRecord {
            cluster_id: "c1".to_string(),
            succession_json: r#"["b"]"#.to_string(),
            computed_at: 200,
            computed_by: "b".to_string(),
        })
        .unwrap();
        let r = mgr.get_succession_line("c1").unwrap().unwrap();
        assert_eq!(r.computed_at, 200);
    }

    #[test]
    fn test_upsert_and_get_peer_nodes() {
        let mgr = test_manager();
        let p = PeerNodeRecord {
            node_id: "peer-1".to_string(),
            role: Some("worker".to_string()),
            address: "10.0.0.1".to_string(),
            port: 9090,
            roles_json: r#"["worker"]"#.to_string(),
            capabilities_json: "{}".to_string(),
            last_seen: 100,
            connection_state: "connected".to_string(),
        };
        mgr.upsert_peer_node(p.clone()).unwrap();
        assert_eq!(mgr.get_peer_node("peer-1").unwrap(), Some(p));
    }

    #[test]
    fn test_remove_peer_node() {
        let mgr = test_manager();
        let p = PeerNodeRecord {
            node_id: "peer-1".to_string(),
            role: None,
            address: "10.0.0.1".to_string(),
            port: 8080,
            roles_json: "[]".to_string(),
            capabilities_json: "{}".to_string(),
            last_seen: 100,
            connection_state: "disconnected".to_string(),
        };
        mgr.upsert_peer_node(p.clone()).unwrap();
        mgr.remove_peer_node(p).unwrap();
        assert_eq!(mgr.get_peer_node("peer-1").unwrap(), None);
    }

    #[test]
    fn test_set_and_get_config() {
        let mgr = test_manager();
        mgr.set_config("max_tasks", "10").unwrap();
        assert_eq!(mgr.get_config("max_tasks").unwrap(), Some("10".to_string()));
    }

    #[test]
    fn test_set_config_overwrites() {
        let mgr = test_manager();
        mgr.set_config("key", "old").unwrap();
        mgr.set_config("key", "new").unwrap();
        assert_eq!(mgr.get_config("key").unwrap(), Some("new".to_string()));
    }

    #[test]
    fn test_remove_config() {
        let mgr = test_manager();
        mgr.set_config("key", "value").unwrap();
        mgr.remove_config("key").unwrap();
        assert_eq!(mgr.get_config("key").unwrap(), None);
    }

    #[test]
    fn test_store_and_retrieve_encrypted() {
        let mgr = test_manager();
        mgr.store_encrypted("api-key", "credentials", b"sensitive")
            .unwrap();
        assert_eq!(
            mgr.retrieve_encrypted("api-key").unwrap().unwrap(),
            b"sensitive"
        );
    }

    #[test]
    fn test_retrieve_nonexistent_encrypted() {
        assert_eq!(test_manager().retrieve_encrypted("nope").unwrap(), None);
    }

    #[test]
    fn test_store_encrypted_overwrites() {
        let mgr = test_manager();
        mgr.store_encrypted("key", "cat", b"old").unwrap();
        mgr.store_encrypted("key", "cat", b"new").unwrap();
        assert_eq!(mgr.retrieve_encrypted("key").unwrap().unwrap(), b"new");
    }

    #[test]
    fn test_remove_encrypted() {
        let mgr = test_manager();
        mgr.store_encrypted("key", "cat", b"data").unwrap();
        mgr.remove_encrypted("key").unwrap();
        assert_eq!(mgr.retrieve_encrypted("key").unwrap(), None);
    }

    #[test]
    fn test_encrypted_data_is_not_plaintext() {
        let mgr = test_manager();
        mgr.store_encrypted("key", "cat", b"this should be encrypted")
            .unwrap();
        let raw = mgr.raw_encrypted_blob("key").unwrap().unwrap();
        assert_ne!(raw, b"this should be encrypted");
        assert!(raw.len() > b"this should be encrypted".len());
    }

    #[test]
    fn test_in_memory_mode() {
        let mgr = StorageManager::new_in_memory(b"secret").unwrap();
        mgr.set_config("hello", "world").unwrap();
        assert_eq!(mgr.get_config("hello").unwrap(), Some("world".to_string()));
    }

    #[test]
    fn test_persistent_mode() {
        let dir = std::env::temp_dir()
            .join("nebula_storage_tests")
            .join(uuid::Uuid::new_v4().to_string());
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mgr = StorageManager::new(dir.to_str().unwrap(), b"secret").unwrap();
            mgr.set_config("persist-key", "persist-value").unwrap();
        }
        {
            let mgr = StorageManager::new(dir.to_str().unwrap(), b"secret").unwrap();
            assert_eq!(
                mgr.get_config("persist-key").unwrap(),
                Some("persist-value".to_string())
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_migration_idempotency() {
        let dir = std::env::temp_dir()
            .join("nebula_storage_tests")
            .join(uuid::Uuid::new_v4().to_string());
        let _ = std::fs::remove_dir_all(&dir);
        {
            let _mgr = StorageManager::new(dir.to_str().unwrap(), b"secret").unwrap();
        }
        {
            let mgr = StorageManager::new(dir.to_str().unwrap(), b"secret").unwrap();
            mgr.set_config("after-reopen", "works").unwrap();
            assert_eq!(
                mgr.get_config("after-reopen").unwrap(),
                Some("works".to_string())
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
