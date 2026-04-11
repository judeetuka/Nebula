use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;

use super::auth;
use super::handlers::{self, AppState};
use super::middleware as app_middleware;
use super::websocket;

/// Build the REST API router with shared cluster registry state.
///
/// Routes are split into public (no auth) and protected (JWT required).
/// The auth middleware inserts `Claims` into request extensions for all
/// protected routes.
pub fn build_router(state: AppState) -> Router {
    // Public routes — no authentication required
    let public = Router::new()
        .route("/api/health", get(handlers::health))
        .route("/api/auth/login", post(handlers::login))
        .route("/api/ws/events", get(websocket::ws_events));

    // Protected routes — require valid JWT in Authorization header
    let protected = Router::new()
        .route("/api/auth/me", get(handlers::get_current_user))
        .route("/api/clusters", get(handlers::list_clusters))
        .route("/api/clusters/:id/nodes", get(handlers::list_cluster_nodes))
        .route("/api/clusters/:id/rotate", post(handlers::trigger_rotation))
        .route(
            "/api/clusters/:id/rotation",
            get(handlers::get_rotation_status),
        )
        .route("/api/clusters/:id", delete(handlers::delete_cluster))
        .route(
            "/api/clusters/:id/failover",
            post(handlers::report_master_timeout),
        )
        .route("/api/clusters/:id/tasks", post(handlers::submit_task))
        .route("/api/plugins", get(handlers::list_plugins))
        .route("/api/nodes/:id/metrics", get(handlers::get_node_metrics))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    public
        .merge(protected)
        .layer(app_middleware::cors_layer())
        .with_state(state)
}
