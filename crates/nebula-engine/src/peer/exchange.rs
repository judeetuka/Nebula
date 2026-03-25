//! Data exchange protocol — request/response pattern over the peer mesh.
//!
//! Provides [`RequestTracker`] which pairs outbound `DataRequest` messages with
//! their `DataResponse` answers using one-shot channels keyed by `request_id`.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use tokio::sync::{oneshot, RwLock};
use tokio::time::{timeout, Duration};
use uuid::Uuid;

use super::manager::PeerManager;
use super::protocol::PeerMessage;

/// The resolved response payload from a peer.
#[derive(Debug)]
pub struct DataExchangeResponse {
    pub success: bool,
    pub payload: Vec<u8>,
    pub error: Option<String>,
}

/// Trait for handling incoming data requests.
pub trait DataHandler: Send + Sync {
    fn handle(&self, action: &str, payload: &[u8]) -> Result<Vec<u8>>;
}

/// Tracks in-flight requests so that an incoming `DataResponse` can be matched
/// back to the caller that sent the original `DataRequest`.
pub struct RequestTracker {
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<DataExchangeResponse>>>>,
    default_timeout: Duration,
}

impl RequestTracker {
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            default_timeout,
        }
    }

    /// Register a pending request and return the `(request_id, receiver)`.
    pub async fn register_pending(&self) -> (String, oneshot::Receiver<DataExchangeResponse>) {
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.write().await.insert(request_id.clone(), tx);
        (request_id, rx)
    }

    /// Send a `DataRequest` to `target_node` via the `PeerManager` and wait
    /// for the matching `DataResponse`.
    pub async fn request(
        &self,
        peer_manager: &PeerManager,
        target_node: &str,
        action: &str,
        payload: Vec<u8>,
    ) -> Result<DataExchangeResponse> {
        let (request_id, rx) = self.register_pending().await;

        let msg = PeerMessage::DataRequest {
            request_id: request_id.clone(),
            action: action.to_string(),
            payload,
        };

        if let Err(e) = peer_manager.send_to(target_node, msg).await {
            self.pending.write().await.remove(&request_id);
            return Err(e);
        }

        match timeout(self.default_timeout, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => bail!("Response channel closed for request {}", request_id),
            Err(_) => {
                self.pending.write().await.remove(&request_id);
                bail!("Request {} timed out after {:?}", request_id, self.default_timeout)
            }
        }
    }

    /// Resolve a pending request when a `DataResponse` arrives.
    pub async fn handle_response(&self, request_id: &str, response: DataExchangeResponse) {
        if let Some(tx) = self.pending.write().await.remove(request_id) {
            let _ = tx.send(response);
        }
    }

    /// Process an incoming `DataRequest` by dispatching to `handler`, then
    /// return the `PeerMessage::DataResponse` that should be sent back.
    pub async fn handle_request(
        &self,
        _from_node: &str,
        request_id: &str,
        action: &str,
        payload: &[u8],
        handler: &dyn DataHandler,
    ) -> PeerMessage {
        match handler.handle(action, payload) {
            Ok(response_payload) => PeerMessage::DataResponse {
                request_id: request_id.to_string(),
                success: true,
                payload: response_payload,
                error: None,
            },
            Err(e) => PeerMessage::DataResponse {
                request_id: request_id.to_string(),
                success: false,
                payload: Vec::new(),
                error: Some(e.to_string()),
            },
        }
    }

    /// Returns the number of currently pending (in-flight) requests.
    pub async fn pending_count(&self) -> usize {
        self.pending.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoHandler;
    impl DataHandler for EchoHandler {
        fn handle(&self, _action: &str, payload: &[u8]) -> Result<Vec<u8>> {
            Ok(payload.to_vec())
        }
    }

    struct FailHandler;
    impl DataHandler for FailHandler {
        fn handle(&self, _action: &str, _payload: &[u8]) -> Result<Vec<u8>> {
            bail!("handler error")
        }
    }

    struct ActionRouter;
    impl DataHandler for ActionRouter {
        fn handle(&self, action: &str, payload: &[u8]) -> Result<Vec<u8>> {
            match action {
                "echo" => Ok(payload.to_vec()),
                "upper" => Ok(String::from_utf8_lossy(payload).to_uppercase().into_bytes()),
                _ => bail!("unknown action: {}", action),
            }
        }
    }

    #[tokio::test]
    async fn test_register_and_resolve() {
        let tracker = RequestTracker::new(Duration::from_secs(5));
        let (req_id, rx) = tracker.register_pending().await;
        assert_eq!(tracker.pending_count().await, 1);

        tracker
            .handle_response(
                &req_id,
                DataExchangeResponse { success: true, payload: b"ok".to_vec(), error: None },
            )
            .await;

        let resp = rx.await.unwrap();
        assert!(resp.success);
        assert_eq!(resp.payload, b"ok");
        assert_eq!(tracker.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_timeout_on_no_response() {
        let tracker = RequestTracker::new(Duration::from_millis(50));
        let (req_id, rx) = tracker.register_pending().await;
        assert_eq!(tracker.pending_count().await, 1);

        let result = timeout(Duration::from_millis(100), rx).await;
        assert!(result.is_err());

        tracker.pending.write().await.remove(&req_id);
        assert_eq!(tracker.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_handle_response_unknown_id_is_noop() {
        let tracker = RequestTracker::new(Duration::from_secs(5));
        tracker
            .handle_response(
                "nonexistent",
                DataExchangeResponse { success: false, payload: vec![], error: Some("orphan".into()) },
            )
            .await;
        assert_eq!(tracker.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_handle_request_echo() {
        let tracker = RequestTracker::new(Duration::from_secs(5));
        let handler = EchoHandler;
        let resp = tracker.handle_request("peer-1", "req-1", "echo", b"hello", &handler).await;
        match resp {
            PeerMessage::DataResponse { request_id, success, payload, error } => {
                assert_eq!(request_id, "req-1");
                assert!(success);
                assert_eq!(payload, b"hello");
                assert!(error.is_none());
            }
            other => panic!("Expected DataResponse, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_handle_request_failure() {
        let tracker = RequestTracker::new(Duration::from_secs(5));
        let handler = FailHandler;
        let resp = tracker.handle_request("peer-1", "req-2", "anything", b"data", &handler).await;
        match resp {
            PeerMessage::DataResponse { request_id, success, payload, error } => {
                assert_eq!(request_id, "req-2");
                assert!(!success);
                assert!(payload.is_empty());
                assert!(error.unwrap().contains("handler error"));
            }
            other => panic!("Expected DataResponse, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_handle_request_action_routing() {
        let tracker = RequestTracker::new(Duration::from_secs(5));
        let handler = ActionRouter;

        let resp = tracker.handle_request("peer-1", "req-a", "echo", b"abc", &handler).await;
        match resp {
            PeerMessage::DataResponse { success, payload, .. } => {
                assert!(success);
                assert_eq!(payload, b"abc");
            }
            other => panic!("Expected DataResponse, got {:?}", other),
        }

        let resp = tracker.handle_request("peer-1", "req-b", "upper", b"hello", &handler).await;
        match resp {
            PeerMessage::DataResponse { success, payload, .. } => {
                assert!(success);
                assert_eq!(payload, b"HELLO");
            }
            other => panic!("Expected DataResponse, got {:?}", other),
        }

        let resp = tracker.handle_request("peer-1", "req-c", "delete", b"x", &handler).await;
        match resp {
            PeerMessage::DataResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.unwrap().contains("unknown action"));
            }
            other => panic!("Expected DataResponse, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_multiple_pending_requests() {
        let tracker = RequestTracker::new(Duration::from_secs(5));

        let (id1, rx1) = tracker.register_pending().await;
        let (id2, rx2) = tracker.register_pending().await;
        let (id3, rx3) = tracker.register_pending().await;
        assert_eq!(tracker.pending_count().await, 3);

        tracker.handle_response(&id3, DataExchangeResponse { success: true, payload: b"three".to_vec(), error: None }).await;
        tracker.handle_response(&id1, DataExchangeResponse { success: true, payload: b"one".to_vec(), error: None }).await;
        tracker.handle_response(&id2, DataExchangeResponse { success: true, payload: b"two".to_vec(), error: None }).await;

        assert_eq!(rx1.await.unwrap().payload, b"one");
        assert_eq!(rx2.await.unwrap().payload, b"two");
        assert_eq!(rx3.await.unwrap().payload, b"three");
        assert_eq!(tracker.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_data_exchange_response_debug() {
        let resp = DataExchangeResponse { success: true, payload: vec![1, 2, 3], error: None };
        let debug = format!("{:?}", resp);
        assert!(debug.contains("success: true"));
    }
}
