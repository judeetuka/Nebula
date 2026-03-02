use anyhow::{Context, Result};
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use tracing::info;

/// MQTT client for node-to-broker communication.
///
/// Wraps `rumqttc::AsyncClient` to provide publish/subscribe operations.
/// The event loop is driven by a spawned background task after `connect()`.
pub struct MqttClient {
    client: AsyncClient,
    event_loop: Option<EventLoop>,
    client_id: String,
}

impl MqttClient {
    /// Create a new MQTT client.
    ///
    /// The client is not connected until `connect()` is called.
    pub fn new(client_id: &str, host: &str, port: u16) -> Result<Self> {
        let mut options = MqttOptions::new(client_id, host, port);
        options.set_keep_alive(std::time::Duration::from_secs(30));
        options.set_clean_session(true);

        let (client, event_loop) = AsyncClient::new(options, 100);

        Ok(Self {
            client,
            event_loop: Some(event_loop),
            client_id: client_id.to_string(),
        })
    }

    /// Returns the client ID.
    pub fn client_id(&self) -> &str {
        &self.client_id
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

        info!(client_id = %self.client_id, "Spawning MQTT event loop");

        tokio::spawn(async move {
            drive_event_loop(event_loop).await;
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
}

/// Drive the rumqttc event loop until it terminates.
async fn drive_event_loop(mut event_loop: EventLoop) {
    loop {
        match event_loop.poll().await {
            Ok(_event) => {
                // Events can be processed here in the future
                // (e.g., incoming messages dispatched to handlers)
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

    #[test]
    fn test_new_client() {
        let client = MqttClient::new("test-node-1", "localhost", 1883).unwrap();
        assert_eq!(client.client_id(), "test-node-1");
        assert!(!client.is_connected());
    }

    #[test]
    fn test_new_client_different_params() {
        let client = MqttClient::new("worker-xyz", "192.168.1.100", 8883).unwrap();
        assert_eq!(client.client_id(), "worker-xyz");
        assert!(!client.is_connected());
    }
}
