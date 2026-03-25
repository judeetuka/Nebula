use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use sea_orm_migration::MigratorTrait;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info};

use nebula_server::api;
use nebula_server::cluster::ClusterRegistry;
use nebula_server::config::NebulaServerConfig;
use nebula_server::database::{migrations::Migrator, pool};
use nebula_server::server::run_server;

/// NEBULA multi-tenant reverse proxy server.
#[derive(Parser, Debug)]
#[clap(name = "nebula-server", version, about)]
struct Args {
    /// Path to the TOML configuration file.
    #[clap(short, long, default_value = "nebula-server.toml")]
    config: PathBuf,

    /// Database connection URL.
    ///
    /// Supports `sqlite://path` and `postgres://user:pass@host/db`.
    /// Defaults to `sqlite://nebula_server.db?mode=rwc`.
    #[clap(long, env = "NEBULA_DATABASE_URL", default_value = pool::DEFAULT_DATABASE_URL)]
    database_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    info!(
        "nebula-server v{} starting",
        env!("CARGO_PKG_VERSION")
    );

    // Raise `nofile` limit on linux and mac
    fdlimit::raise_fd_limit();

    // ── Database ────────────────────────────────────────────────────────
    let db = pool::connect(&args.database_url).await?;

    info!("Running database migrations");
    Migrator::up(&db, None)
        .await
        .with_context(|| "Failed to run database migrations")?;
    info!("Database migrations complete");

    // Load configuration
    let config = NebulaServerConfig::from_file(&args.config)
        .await
        .with_context(|| format!("Failed to load config from {:?}", args.config))?;

    info!("Loaded configuration from {:?}", args.config);

    // Initialize cluster registry (shared between tunnel server and API)
    let cluster_registry = Arc::new(RwLock::new(ClusterRegistry::new()));

    // Set up shutdown signaling
    let (shutdown_tx, _) = broadcast::channel::<bool>(1);

    // Service update channel (for hot reload)
    let (_update_tx, update_rx) = mpsc::channel(1024);

    // Spawn the tunnel server
    let server_config = config.server.clone();
    let registry_clone = cluster_registry.clone();
    let shutdown_rx = shutdown_tx.subscribe();
    let tunnel_handle = tokio::spawn(async move {
        if let Err(e) = run_server(server_config, shutdown_rx, update_rx, registry_clone).await {
            error!("Tunnel server error: {:#}", e);
        }
    });

    // Spawn the REST API if configured
    let api_handle = if let Some(api_config) = config.api {
        let bind_addr = api_config.bind_addr.clone();
        let jwt_config = nebula_server::api::auth::JwtConfig::default();
        let state = api::AppState::with_db(cluster_registry.clone(), db, jwt_config);
        let router = api::build_router(state);

        info!("Starting REST API on {}", bind_addr);

        let shutdown_rx = shutdown_tx.subscribe();
        Some(tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!("Failed to bind REST API to {}: {:#}", bind_addr, e);
                    return;
                }
            };

            let mut shutdown_rx = shutdown_rx;
            let server = axum::serve(listener, router);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        error!("REST API error: {:#}", e);
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("REST API shutting down");
                }
            }
        }))
    } else {
        None
    };

    // Wait for Ctrl+C
    tokio::signal::ctrl_c()
        .await
        .with_context(|| "Failed to listen for ctrl+c")?;

    info!("Received shutdown signal");
    let _ = shutdown_tx.send(true);

    // Wait for tasks to finish
    let _ = tunnel_handle.await;
    if let Some(handle) = api_handle {
        let _ = handle.await;
    }

    info!("nebula-server stopped");
    Ok(())
}
