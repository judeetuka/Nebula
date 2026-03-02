use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use tokio::sync::RwLock;

use crate::cluster::registry::RotationState;
use crate::cluster::ClusterRegistry;
use nebula_core::identity::node_id::{ClusterId, NodeId};

/// Shared application state injected into all handlers.
pub type AppState = Arc<RwLock<ClusterRegistry>>;

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
pub async fn list_clusters(State(registry): State<AppState>) -> impl IntoResponse {
    let registry = registry.read().await;
    let clusters = registry.list_clusters();
    Json(serde_json::json!({
        "clusters": clusters,
    }))
}

/// GET /api/clusters/:id/nodes
pub async fn list_cluster_nodes(
    State(registry): State<AppState>,
    Path(cluster_id): Path<String>,
) -> impl IntoResponse {
    let registry = registry.read().await;
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
    State(registry): State<AppState>,
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

    let mut registry = registry.write().await;
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
    State(registry): State<AppState>,
    Path(cluster_id): Path<String>,
) -> impl IntoResponse {
    let registry = registry.read().await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use crate::api::routes::build_router;

    fn test_state() -> AppState {
        Arc::new(RwLock::new(ClusterRegistry::new()))
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
            let mut registry = state.write().await;
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
            let mut registry = state.write().await;
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
