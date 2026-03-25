use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use sea_orm::DatabaseConnection;
use tokio::sync::RwLock;

use crate::cluster::registry::RotationState;
use crate::cluster::ClusterRegistry;
use nebula_core::identity::node_id::{ClusterId, NodeId};

use super::auth::{JwtConfig, LoginRequest};
use super::websocket::EventBroadcaster;

/// Shared application state injected into all handlers.
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<RwLock<ClusterRegistry>>,
    pub db: Option<DatabaseConnection>,
    pub jwt_config: JwtConfig,
    pub event_broadcaster: Arc<EventBroadcaster>,
}

impl AppState {
    /// Create state without a database (used in tests).
    pub fn new(registry: Arc<RwLock<ClusterRegistry>>) -> Self {
        Self {
            registry,
            db: None,
            jwt_config: JwtConfig::default(),
            event_broadcaster: Arc::new(EventBroadcaster::new()),
        }
    }

    /// Create state with a database connection.
    pub fn with_db(
        registry: Arc<RwLock<ClusterRegistry>>,
        db: DatabaseConnection,
        jwt_config: JwtConfig,
    ) -> Self {
        Self {
            registry,
            db: Some(db),
            jwt_config,
            event_broadcaster: Arc::new(EventBroadcaster::new()),
        }
    }
}

/// Request body for triggering a rotation.
#[derive(serde::Deserialize)]
pub struct TriggerRotationRequest {
    pub new_master: String,
}

/// GET /api/health
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "nebula-server",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// GET /api/clusters
pub async fn list_clusters(State(state): State<AppState>) -> impl IntoResponse {
    let registry = state.registry.read().await;
    let clusters = registry.list_clusters();
    Json(serde_json::json!({
        "clusters": clusters,
    }))
}

/// GET /api/clusters/:id/nodes
pub async fn list_cluster_nodes(
    State(state): State<AppState>,
    Path(cluster_id): Path<String>,
) -> impl IntoResponse {
    let registry = state.registry.read().await;
    let cluster_id = ClusterId(cluster_id);

    match registry.list_nodes(&cluster_id) {
        Some(nodes) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "cluster_id": cluster_id,
                "nodes": nodes,
            })),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Cluster not found",
                "cluster_id": cluster_id,
            })),
        ),
    }
}

/// POST /api/clusters/:id/rotate — Trigger manual rotation
pub async fn trigger_rotation(
    State(state): State<AppState>,
    Path(cluster_id): Path<String>,
    Json(body): Json<TriggerRotationRequest>,
) -> impl IntoResponse {
    let cluster_id = ClusterId(cluster_id);

    let new_master = match NodeId::from_str(&body.new_master) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Invalid node ID format",
                    "new_master": body.new_master,
                })),
            );
        }
    };

    let mut registry = state.registry.write().await;
    match registry.begin_rotation(&cluster_id, &new_master) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "rotation_started",
                "cluster_id": cluster_id,
                "new_master": new_master,
            })),
        ),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": e.to_string(),
                "cluster_id": cluster_id,
            })),
        ),
    }
}

/// GET /api/clusters/:id/rotation — Get rotation status
pub async fn get_rotation_status(
    State(state): State<AppState>,
    Path(cluster_id): Path<String>,
) -> impl IntoResponse {
    let registry = state.registry.read().await;
    let cluster_id = ClusterId(cluster_id);

    match registry.rotation_status(&cluster_id) {
        Some(state) => {
            let (status, details) = match state {
                RotationState::None => ("none", serde_json::json!(null)),
                RotationState::InProgress {
                    old_master,
                    new_master,
                    started_at,
                } => (
                    "in_progress",
                    serde_json::json!({
                        "old_master": old_master.to_string(),
                        "new_master": new_master.to_string(),
                        "started_at": started_at.to_rfc3339(),
                    }),
                ),
            };
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "cluster_id": cluster_id,
                    "rotation_status": status,
                    "details": details,
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Cluster not found",
                "cluster_id": cluster_id,
            })),
        ),
    }
}

/// POST /api/auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let db = match &state.db {
        Some(db) => db,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({ "error": "Database not available" }))),
    };

    use crate::database::entities::users;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let user = match users::Entity::find().filter(users::Column::Email.eq(&req.email)).one(db).await {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Invalid credentials" }))),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Database error" }))),
    };

    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    let parsed_hash = match PasswordHash::new(&user.password_hash) {
        Ok(h) => h,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Invalid stored hash" }))),
    };
    if Argon2::default().verify_password(req.password.as_bytes(), &parsed_hash).is_err() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Invalid credentials" })));
    }

    match super::auth::generate_token(&user.id, &user.email, &user.role, &state.jwt_config) {
        Ok(token) => (StatusCode::OK, Json(serde_json::json!({ "token": token, "expires_in": state.jwt_config.expiry_hours * 3600, "role": user.role }))),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Token generation failed" }))),
    }
}

/// GET /api/auth/me
pub async fn get_current_user(req: axum::http::Request<axum::body::Body>) -> impl IntoResponse {
    match req.extensions().get::<super::auth::Claims>() {
        Some(claims) => (StatusCode::OK, Json(serde_json::json!({ "user_id": claims.sub, "email": claims.email, "role": claims.role }))),
        None => (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Not authenticated" }))),
    }
}

/// DELETE /api/clusters/:id
pub async fn delete_cluster(
    State(state): State<AppState>,
    Path(cluster_id): Path<String>,
) -> impl IntoResponse {
    let mut registry = state.registry.write().await;
    let cluster_id = ClusterId(cluster_id.clone());
    if registry.remove_cluster(&cluster_id) {
        (StatusCode::OK, Json(serde_json::json!({ "deleted": true, "cluster_id": cluster_id })))
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Cluster not found" })))
    }
}

/// GET /api/plugins
pub async fn list_plugins(State(state): State<AppState>) -> impl IntoResponse {
    let db = match &state.db {
        Some(db) => db,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({ "error": "Database not available" }))),
    };
    use crate::database::entities::plugins;
    use sea_orm::EntityTrait;
    match plugins::Entity::find().all(db).await {
        Ok(plugins) => (StatusCode::OK, Json(serde_json::json!({ "plugins": plugins }))),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Database error" }))),
    }
}

/// POST /api/clusters/:id/tasks
#[derive(serde::Deserialize)]
pub struct SubmitTaskRequest {
    pub task_type: String,
    pub payload: serde_json::Value,
}

pub async fn submit_task(
    State(_state): State<AppState>,
    Path(cluster_id): Path<String>,
    Json(req): Json<SubmitTaskRequest>,
) -> impl IntoResponse {
    // Task submission will relay to the cluster master via tunnel.
    // For now, return accepted with a generated task_id.
    let task_id = uuid::Uuid::new_v4().to_string();
    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "task_id": task_id,
        "cluster_id": cluster_id,
        "task_type": req.task_type,
        "status": "queued",
    })))
}

/// GET /api/nodes/:id/metrics
pub async fn get_node_metrics(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    let db = match &state.db {
        Some(db) => db,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({ "error": "Database not available" }))),
    };
    use crate::database::entities::nodes;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    match nodes::Entity::find().filter(nodes::Column::Id.eq(&node_id)).one(db).await {
        Ok(Some(node)) => (StatusCode::OK, Json(serde_json::json!({
            "node_id": node.id, "battery_level": node.battery_level,
            "cpu_load": node.cpu_load, "memory_available_mb": node.memory_available_mb,
            "is_online": node.is_online,
        }))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Node not found" }))),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Database error" }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use crate::api::routes::build_router;

    fn test_state() -> AppState {
        AppState::new(Arc::new(RwLock::new(ClusterRegistry::new())))
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_clusters_empty() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/clusters")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cluster_nodes_not_found() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/clusters/nonexistent/nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_rotation_status_cluster_not_found() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/clusters/nonexistent/rotation")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_rotation_status_none() {
        let state = test_state();

        // Register a node to create the cluster
        {
            let mut registry = state.registry.write().await;
            let node = NodeId::generate();
            registry
                .register_node(&ClusterId("my-cluster".into()), node)
                .unwrap();
        }

        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/clusters/my-cluster/rotation")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["rotation_status"], "none");
    }

    #[tokio::test]
    async fn test_trigger_rotation_success() {
        let state = test_state();
        let master = NodeId::generate();
        let worker = NodeId::generate();

        {
            let mut registry = state.registry.write().await;
            let cluster_id = ClusterId("my-cluster".into());
            registry.register_node(&cluster_id, master).unwrap();
            registry.register_node(&cluster_id, worker).unwrap();
        }

        let app = build_router(state);

        let body = serde_json::json!({ "new_master": worker.to_string() });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/clusters/my-cluster/rotate")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "rotation_started");
    }

    #[tokio::test]
    async fn test_trigger_rotation_invalid_node_id() {
        let state = test_state();
        let app = build_router(state);

        let body = serde_json::json!({ "new_master": "not-a-uuid" });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/clusters/my-cluster/rotate")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_trigger_rotation_cluster_not_found() {
        let state = test_state();
        let node = NodeId::generate();
        let app = build_router(state);

        let body = serde_json::json!({ "new_master": node.to_string() });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/clusters/nonexistent/rotate")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
}
