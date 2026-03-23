//! Database connection pool manager.
//!
//! Supports both `sqlite://` and `postgres://` URLs, automatically detected
//! from the scheme.  Call [`connect`] at server startup and pass the returned
//! [`DatabaseConnection`] into the shared application state.

use anyhow::{Context, Result};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tracing::info;

/// Default database URL used when no `--database-url` is supplied.
pub const DEFAULT_DATABASE_URL: &str = "sqlite://nebula_server.db?mode=rwc";

/// Open (and optionally create) a database connection pool.
///
/// The URL scheme determines the backend:
/// - `sqlite://…`   → SQLite via sqlx
/// - `postgres://…`  → PostgreSQL via sqlx
pub async fn connect(database_url: &str) -> Result<DatabaseConnection> {
    info!(url = %database_url, "Connecting to database");

    let mut opts = ConnectOptions::new(database_url.to_owned());
    opts.max_connections(100)
        .min_connections(5)
        .sqlx_logging(false);

    let db = Database::connect(opts)
        .await
        .with_context(|| format!("Failed to connect to database at {}", database_url))?;

    info!("Database connection established");
    Ok(db)
}
