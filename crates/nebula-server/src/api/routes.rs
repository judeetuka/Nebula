use axum::routing::{get, post};
use axum::Router;

use super::handlers::{self, AppState};

/// Build the REST API router with shared cluster registry state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(handlers::health))
        .route("/api/clusters", get(handlers::list_clusters))
        .route("/api/clusters/:id/nodes", get(handlers::list_cluster_nodes))
        .route(
            "/api/clusters/:id/rotate",
            post(handlers::trigger_rotation),
        )
        .route(
            "/api/clusters/:id/rotation",
            get(handlers::get_rotation_status),
        )
        .with_state(state)
}
