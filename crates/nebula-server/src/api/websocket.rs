use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::handlers::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ServerEvent {
    NodeStatusChanged { cluster_id: String, node_id: String, is_online: bool },
    ClusterMembershipChanged { cluster_id: String, member_count: u32 },
    TaskStatusChanged { cluster_id: String, task_id: String, status: String },
    MasterRotated { cluster_id: String, old_master: String, new_master: String },
    PluginChanged { node_id: String, plugin_id: String, action: String },
    MetricsUpdate { node_id: String, battery: u8, cpu: f32, memory_mb: u32 },
    Heartbeat { timestamp: i64 },
}

pub struct EventBroadcaster {
    sender: broadcast::Sender<ServerEvent>,
}

impl EventBroadcaster {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    pub fn publish(&self, event: ServerEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.sender.subscribe()
    }
}

/// GET /api/ws/events
pub async fn ws_events(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    let mut rx = state.event_broadcaster.subscribe();
    let welcome = ServerEvent::Heartbeat { timestamp: chrono::Utc::now().timestamp() };
    let _ = socket.send(Message::Text(serde_json::to_string(&welcome).unwrap())).await;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(ev) => {
                        let json = serde_json::to_string(&ev).unwrap();
                        if socket.send(Message::Text(json)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "WebSocket client lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => { let _ = socket.send(Message::Pong(data)).await; }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_event_serialization_roundtrip() {
        let events = vec![
            ServerEvent::NodeStatusChanged { cluster_id: "c".into(), node_id: "n".into(), is_online: true },
            ServerEvent::ClusterMembershipChanged { cluster_id: "c".into(), member_count: 5 },
            ServerEvent::TaskStatusChanged { cluster_id: "c".into(), task_id: "t".into(), status: "running".into() },
            ServerEvent::MasterRotated { cluster_id: "c".into(), old_master: "a".into(), new_master: "b".into() },
            ServerEvent::PluginChanged { node_id: "n".into(), plugin_id: "p".into(), action: "installed".into() },
            ServerEvent::MetricsUpdate { node_id: "n".into(), battery: 85, cpu: 42.5, memory_mb: 1024 },
            ServerEvent::Heartbeat { timestamp: 1700000000 },
        ];
        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let back: ServerEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(*event, back);
        }
    }

    #[tokio::test]
    async fn test_event_broadcaster_publish_subscribe() {
        let b = EventBroadcaster::new();
        let mut rx = b.subscribe();
        let ev = ServerEvent::Heartbeat { timestamp: 1 };
        assert_eq!(b.publish(ev.clone()), 1);
        assert_eq!(rx.recv().await.unwrap(), ev);
    }

    #[test]
    fn test_event_broadcaster_no_subscribers_ok() {
        assert_eq!(EventBroadcaster::new().publish(ServerEvent::Heartbeat { timestamp: 0 }), 0);
    }
}
