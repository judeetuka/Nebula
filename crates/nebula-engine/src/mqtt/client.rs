use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use tokio::sync::mpsc;
use tracing::info;

/// Callback type for MQTT message handlers.
///
/// Receives the topic string and the raw payload bytes. Handlers are
/// invoked synchronously on the event-loop task so they should be fast
/// and non-blocking. Heavy work should be spawned onto a separate task.
pub type MessageHandler = Arc<dyn Fn(String, Vec<u8>) + Send + Sync>;

/// Check whether an MQTT topic matches a subscription filter.
///
/// Implements the standard MQTT wildcard rules:
/// - `+` matches exactly one topic level.
/// - `#` matches zero or more remaining levels and must be the last
///   segment of the filter.
/// - All other segments require an exact string match.
pub fn topic_matches(filter: &str, topic: &str) -> bool {
    let filter_parts: Vec<&str> = filter.split('/').collect();
    let topic_parts: Vec<&str> = topic.split('/').collect();

    let mut fi = 0;
    let mut ti = 0;

    while fi < filter_parts.len() {
        let fp = filter_parts[fi];

        if fp == "#" {
            // '#' is only valid as the very last segment.
            return fi == filter_parts.len() - 1;
        }

        // If the topic has fewer levels than the filter, no match.
        if ti >= topic_parts.len() {
            return false;
        }

        if fp != "+" && fp != topic_parts[ti] {
            return false;
        }

        fi += 1;
        ti += 1;
    }

    // Both iterators must be exhausted for an exact match.
    ti == topic_parts.len()
}

/// MQTT client for node-to-broker communication.
///
/// Wraps `rumqttc::AsyncClient` to provide publish/subscribe operations.
/// The event loop is driven by a spawned background task after `connect()`.
///
/// Incoming PUBLISH messages are dispatched to registered [`MessageHandler`]
/// callbacks and forwarded through an `mpsc` channel that external code
/// can consume via [`message_receiver`](MqttClient::message_receiver).
pub struct MqttClient {
    client: AsyncClient,
    event_loop: Option<EventLoop>,
    client_id: String,
    handlers: Arc<RwLock<HashMap<String, MessageHandler>>>,
    message_tx: mpsc::Sender<(String, Vec<u8>)>,
    message_rx: Option<mpsc::Receiver<(String, Vec<u8>)>>,
}

/// Channel buffer size for the broadcast message channel.
const MESSAGE_CHANNEL_BUFFER: usize = 256;

impl MqttClient {
    /// Create a new MQTT client.
    ///
    /// The client is not connected until `connect()` is called.
    pub fn new(
        client_id: &str,
        host: &str,
        port: u16,
        tls_config: Option<std::sync::Arc<rumqttc::tokio_rustls::rustls::ClientConfig>>,
    ) -> Result<Self> {
        let mut options = MqttOptions::new(client_id, host, port);
        options.set_keep_alive(std::time::Duration::from_secs(30));
        options.set_clean_session(true);
        if let Some(tls) = tls_config {
            let transport = Transport::tls_with_config(TlsConfiguration::Rustls(tls));
            options.set_transport(transport);
        }

        let (client, event_loop) = AsyncClient::new(options, 100);
        let (message_tx, message_rx) = mpsc::channel(MESSAGE_CHANNEL_BUFFER);

        Ok(Self {
            client,
            event_loop: Some(event_loop),
            client_id: client_id.to_string(),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            message_tx,
            message_rx: Some(message_rx),
        })
    }

    /// Returns the client ID.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Register a message handler for the given topic filter.
    ///
    /// The handler is invoked for every incoming PUBLISH whose topic
    /// matches the filter according to MQTT wildcard rules.
    ///
    /// **Note:** this only registers the dispatch callback. You must also
    /// call [`subscribe`](MqttClient::subscribe) so that the broker
    /// actually delivers messages for this filter.
    pub fn on_message(&self, topic_filter: &str, handler: MessageHandler) {
        let mut handlers = self.handlers.write().expect("handlers lock poisoned");
        handlers.insert(topic_filter.to_string(), handler);
    }

    /// Take the message receiver channel.
    ///
    /// Returns `Some(Receiver)` on the first call and `None` thereafter.
    /// The receiver yields `(topic, payload)` pairs for every incoming
    /// PUBLISH message, regardless of registered handlers.
    pub fn message_receiver(&mut self) -> Option<mpsc::Receiver<(String, Vec<u8>)>> {
        self.message_rx.take()
    }

    /// Connect to the broker by spawning a background event loop poller.
    ///
    /// The event loop must be polled continuously for the client to function.
    /// This method spawns a tokio task that polls the loop until it errors
    /// or the client is disconnected.
    pub async fn connect(&mut self) -> Result<()> {
        let event_loop = self
            .event_loop
            .take()
            .context("Event loop already consumed (already connected)")?;

        let handlers = Arc::clone(&self.handlers);
        let message_tx = self.message_tx.clone();

        info!(client_id = %self.client_id, "Spawning MQTT event loop");

        tokio::spawn(async move {
            drive_event_loop(event_loop, handlers, message_tx).await;
        });

        Ok(())
    }

    /// Publish a message to the given topic with QoS 1 (at least once).
    pub async fn publish(&self, topic: &str, payload: &[u8]) -> Result<()> {
        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload)
            .await
            .with_context(|| format!("Failed to publish to topic: {topic}"))
    }

    /// Subscribe to the given topic with QoS 1 (at least once).
    pub async fn subscribe(&self, topic: &str) -> Result<()> {
        self.client
            .subscribe(topic, QoS::AtLeastOnce)
            .await
            .with_context(|| format!("Failed to subscribe to topic: {topic}"))
    }

    /// Request a graceful disconnect from the broker.
    pub async fn disconnect(&self) -> Result<()> {
        self.client
            .disconnect()
            .await
            .context("Failed to disconnect MQTT client")
    }

    /// Returns `true` if the event loop has been consumed (client was connected).
    pub fn is_connected(&self) -> bool {
        self.event_loop.is_none()
    }

    /// Returns a reference to the handler registry (primarily for testing).
    pub fn handlers(&self) -> &Arc<RwLock<HashMap<String, MessageHandler>>> {
        &self.handlers
    }
}

/// Drive the rumqttc event loop until it terminates.
///
/// Incoming PUBLISH packets are dispatched to all registered handlers
/// whose topic filter matches the message topic, and forwarded through
/// the `mpsc` channel for external consumers.
async fn drive_event_loop(
    mut event_loop: EventLoop,
    handlers: Arc<RwLock<HashMap<String, MessageHandler>>>,
    message_tx: mpsc::Sender<(String, Vec<u8>)>,
) {
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                let topic = publish.topic.clone();
                let payload = publish.payload.to_vec();

                // Dispatch to registered handlers.
                if let Ok(handlers) = handlers.read() {
                    for (filter, handler) in handlers.iter() {
                        if topic_matches(filter, &topic) {
                            handler(topic.clone(), payload.clone());
                        }
                    }
                }

                // Forward through the channel for external consumers.
                if let Err(e) = message_tx.try_send((topic, payload)) {
                    tracing::warn!(error = %e, "MQTT message channel full or closed, dropping message");
                }
            }
            Ok(_) => {
                // Other events (ConnAck, SubAck, PingResp, etc.)
            }
            Err(e) => {
                tracing::warn!(error = %e, "MQTT event loop error, stopping");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // Original tests (unchanged)
    // -------------------------------------------------------------------

    #[test]
    fn test_new_client() {
        let client = MqttClient::new("test-node-1", "localhost", 1883, None).unwrap();
        assert_eq!(client.client_id(), "test-node-1");
        assert!(!client.is_connected());
    }

    #[test]
    fn test_new_client_different_params() {
        let client = MqttClient::new("worker-xyz", "192.168.1.100", 8883, None).unwrap();
        assert_eq!(client.client_id(), "worker-xyz");
        assert!(!client.is_connected());
    }

    // -------------------------------------------------------------------
    // topic_matches tests
    // -------------------------------------------------------------------

    #[test]
    fn test_topic_matches_exact() {
        assert!(topic_matches("a/b/c", "a/b/c"));
    }

    #[test]
    fn test_topic_matches_single_level_wildcard() {
        assert!(topic_matches("a/+/c", "a/b/c"));
        assert!(topic_matches("a/+/c", "a/x/c"));
    }

    #[test]
    fn test_topic_matches_single_level_wildcard_no_match() {
        assert!(!topic_matches("a/+/c", "a/b/d/c"));
    }

    #[test]
    fn test_topic_matches_multi_level_wildcard() {
        assert!(topic_matches("a/#", "a/b/c"));
        assert!(topic_matches("a/#", "a/b"));
        assert!(topic_matches("a/#", "a/b/c/d/e"));
    }

    #[test]
    fn test_topic_matches_root_multi_level() {
        assert!(topic_matches("#", "a/b/c"));
        assert!(topic_matches("#", "anything"));
        assert!(topic_matches("#", ""));
    }

    #[test]
    fn test_topic_matches_no_match() {
        assert!(!topic_matches("a/b", "a/c"));
    }

    #[test]
    fn test_topic_matches_empty() {
        assert!(topic_matches("", ""));
    }

    #[test]
    fn test_topic_matches_plus_at_start() {
        assert!(topic_matches("+/b/c", "x/b/c"));
        assert!(topic_matches("+/b/c", "anything/b/c"));
    }

    #[test]
    fn test_topic_matches_hash_must_be_last() {
        assert!(!topic_matches("a/#/b", "a/x/b"));
    }

    #[test]
    fn test_topic_matches_filter_longer_than_topic() {
        assert!(!topic_matches("a/b/c/d", "a/b"));
    }

    // -------------------------------------------------------------------
    // MqttClient handler / channel tests
    // -------------------------------------------------------------------

    #[test]
    fn test_new_client_has_empty_handlers() {
        let client = MqttClient::new("test", "localhost", 1883, None).unwrap();
        let handlers = client.handlers().read().unwrap();
        assert!(handlers.is_empty());
    }

    #[test]
    fn test_on_message_registers_handler() {
        let client = MqttClient::new("test", "localhost", 1883, None).unwrap();

        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = Arc::clone(&called);
        let handler: MessageHandler = Arc::new(move |_topic, _payload| {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        client.on_message("test/topic", handler);

        let handlers = client.handlers().read().unwrap();
        assert_eq!(handlers.len(), 1);
        assert!(handlers.contains_key("test/topic"));
    }

    #[test]
    fn test_message_receiver_returns_some_once() {
        let mut client = MqttClient::new("test", "localhost", 1883, None).unwrap();
        let rx = client.message_receiver();
        assert!(rx.is_some());
        let rx2 = client.message_receiver();
        assert!(rx2.is_none());
    }
}
