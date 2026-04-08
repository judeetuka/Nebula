use anyhow::{Context, Result};
use rusqlite::Connection;
use tracing::info;

/// A single forward-only migration.
struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

/// All migrations in order. NEVER remove or modify existing migrations.
/// Only append new ones.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: r#"
-- Plugin state
CREATE TABLE plugin_states (
    plugin_id TEXT PRIMARY KEY NOT NULL,
    version TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    installed_at INTEGER NOT NULL,
    last_executed_at INTEGER,
    config_json TEXT,
    checksum TEXT
);

-- Task queue
CREATE TABLE task_queue (
    task_id TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    priority INTEGER NOT NULL DEFAULT 0,
    task_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    submitted_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    assigned_node TEXT,
    timeout_secs INTEGER NOT NULL DEFAULT 300,
    retry_count INTEGER NOT NULL DEFAULT 0,
    error_message TEXT
);
CREATE INDEX idx_task_queue_status ON task_queue(status);
CREATE INDEX idx_task_queue_priority ON task_queue(priority DESC);

-- Cluster members
CREATE TABLE cluster_members (
    node_id TEXT PRIMARY KEY NOT NULL,
    role TEXT NOT NULL DEFAULT 'worker',
    join_time INTEGER NOT NULL,
    last_heartbeat INTEGER NOT NULL,
    battery_level INTEGER NOT NULL DEFAULT 0,
    cpu_load REAL NOT NULL DEFAULT 0.0,
    memory_available_mb INTEGER NOT NULL DEFAULT 0,
    active_tasks INTEGER NOT NULL DEFAULT 0,
    network_type TEXT NOT NULL DEFAULT 'unknown',
    peer_address TEXT,
    is_stale INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_cluster_members_role ON cluster_members(role);

-- Heartbeat history
CREATE TABLE heartbeats (
    id TEXT PRIMARY KEY NOT NULL,
    node_id TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    battery_level INTEGER NOT NULL,
    cpu_load REAL NOT NULL,
    memory_available_mb INTEGER NOT NULL,
    active_tasks INTEGER NOT NULL,
    network_type TEXT NOT NULL
);
CREATE INDEX idx_heartbeats_node_id ON heartbeats(node_id);
CREATE INDEX idx_heartbeats_timestamp ON heartbeats(timestamp);

-- MQTT offline queue
CREATE TABLE mqtt_offline_queue (
    id TEXT PRIMARY KEY NOT NULL,
    topic TEXT NOT NULL,
    payload BLOB NOT NULL,
    qos INTEGER NOT NULL DEFAULT 1,
    queued_at INTEGER NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_mqtt_offline_topic ON mqtt_offline_queue(topic);

-- Succession line
CREATE TABLE succession (
    cluster_id TEXT PRIMARY KEY NOT NULL,
    succession_json TEXT NOT NULL,
    computed_at INTEGER NOT NULL,
    computed_by TEXT NOT NULL
);

-- Peer nodes
CREATE TABLE peer_nodes (
    node_id TEXT PRIMARY KEY NOT NULL,
    role TEXT,
    address TEXT NOT NULL,
    port INTEGER NOT NULL,
    roles_json TEXT NOT NULL DEFAULT '[]',
    capabilities_json TEXT NOT NULL DEFAULT '{}',
    last_seen INTEGER NOT NULL,
    connection_state TEXT NOT NULL DEFAULT 'disconnected'
);
CREATE INDEX idx_peer_nodes_role ON peer_nodes(role);

-- Node config (key-value)
CREATE TABLE node_config (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Encrypted blobs
CREATE TABLE encrypted_blobs (
    key TEXT PRIMARY KEY NOT NULL,
    category TEXT NOT NULL,
    encrypted_data BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_encrypted_blobs_category ON encrypted_blobs(category);
"#,
    },
    Migration {
        version: 2,
        name: "performance_indexes_and_limits",
        sql: r#"
-- Composite index for efficient task dequeue (PF-3)
CREATE INDEX IF NOT EXISTS idx_task_queue_dequeue ON task_queue(status, priority DESC, submitted_at ASC);

-- Index for heartbeat pruning by timestamp
CREATE INDEX IF NOT EXISTS idx_heartbeats_prune ON heartbeats(timestamp);
"#,
    },
];

/// Apply all pending migrations to the database.
///
/// 1. Creates the `_migrations` tracking table if it does not exist.
/// 2. Queries the highest applied version.
/// 3. For each migration with version > max, executes the SQL inside a
///    transaction and records the version in `_migrations`.
pub fn apply_migrations(conn: &Connection) -> Result<()> {
    // The _migrations table is the only CREATE IF NOT EXISTS -- it bootstraps
    // the migration tracking itself.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .context("create _migrations table")?;

    let max_version: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )
        .context("query max migration version")?;

    for migration in MIGRATIONS {
        if migration.version <= max_version {
            continue;
        }
        let tx = conn.unchecked_transaction().context("begin migration tx")?;
        tx.execute_batch(migration.sql)
            .with_context(|| format!("migration V{} ({})", migration.version, migration.name))?;
        tx.execute(
            "INSERT INTO _migrations (version, name) VALUES (?1, ?2)",
            rusqlite::params![migration.version, migration.name],
        )
        .with_context(|| {
            format!(
                "record migration V{} ({})",
                migration.version, migration.name
            )
        })?;
        tx.commit()
            .with_context(|| format!("commit migration V{}", migration.version))?;
        info!(
            version = migration.version,
            name = migration.name,
            "applied migration"
        );
    }

    Ok(())
}
