use std::collections::HashMap;

use anyhow::Result;
use rumqttd::{Broker, Config, ConnectionSettings, ServerSettings};
use tracing::info;

/// Default MQTT port for the server relay broker.
/// Uses 1884 to avoid conflict with cluster master brokers on 1883.
pub const DEFAULT_RELAY_PORT: u16 = 1884;

/// Embedded MQTT broker running on the NEBULA server.
///
/// Acts as a cloud relay / rendezvous point for external nodes that
/// cannot directly reach their cluster master's local broker.
pub struct ServerBroker {
    config: Config,
    broker: Option<Broker>,
    mqtt_port: u16,
}

impl ServerBroker {
    /// Create a new server relay broker on the given port.
    pub fn new(mqtt_port: u16) -> Result<Self> {
        let config = build_relay_config(mqtt_port);
        Ok(Self {
            config,
            broker: None,
            mqtt_port,
        })
    }

    /// Start the embedded MQTT relay broker.
    pub async fn start(&mut self) -> Result<()> {
        if self.broker.is_some() {
            anyhow::bail!("Server relay broker is already running");
        }

        let broker = Broker::new(self.config.clone());
        info!(port = self.mqtt_port, "Starting MQTT relay broker");

        self.broker = Some(broker);
        Ok(())
    }

    /// Stop the relay broker.
    pub fn stop(&mut self) -> Result<()> {
        if self.broker.is_none() {
            anyhow::bail!("Server relay broker is not running");
        }
        info!(port = self.mqtt_port, "Stopping MQTT relay broker");
        self.broker = None;
        Ok(())
    }

    /// Returns the MQTT port this broker listens on.
    pub fn mqtt_port(&self) -> u16 {
        self.mqtt_port
    }

    /// Returns true if the broker is currently running.
    pub fn is_running(&self) -> bool {
        self.broker.is_some()
    }
}

/// Build a rumqttd Config for the server relay broker.
fn build_relay_config(listen_port: u16) -> Config {
    let mut servers = HashMap::new();
    servers.insert(
        "nebula-relay-v4".to_string(),
        ServerSettings {
            name: "nebula-relay-v4".to_string(),
            listen: format!("0.0.0.0:{listen_port}")
                .parse()
                .expect("valid socket addr"),
            tls: None,
            next_connection_delay_ms: 100,
            connections: ConnectionSettings {
                connection_timeout_ms: 5000,
                max_payload_size: 65_536,
                max_inflight_count: 20,
                auth: None,
                external_auth: None,
                dynamic_filters: false,
            },
        },
    );

    Config {
        id: 0,
        router: Default::default(),
        v4: Some(servers),
        v5: None,
        ws: None,
        cluster: None,
        console: None,
        prometheus: None,
        bridge: None,
        metrics: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_server_broker() {
        let broker = ServerBroker::new(1884).unwrap();
        assert_eq!(broker.mqtt_port(), 1884);
        assert!(!broker.is_running());
    }

    #[test]
    fn test_default_relay_port() {
        assert_eq!(DEFAULT_RELAY_PORT, 1884);
    }

    #[tokio::test]
    async fn test_start_sets_running() {
        let mut broker = ServerBroker::new(21884).unwrap();
        broker.start().await.unwrap();
        assert!(broker.is_running());
    }

    #[tokio::test]
    async fn test_stop_clears_running() {
        let mut broker = ServerBroker::new(21885).unwrap();
        broker.start().await.unwrap();
        broker.stop().unwrap();
        assert!(!broker.is_running());
    }

    #[tokio::test]
    async fn test_double_start_fails() {
        let mut broker = ServerBroker::new(21886).unwrap();
        broker.start().await.unwrap();
        let result = broker.start().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already running"));
    }

    #[test]
    fn test_stop_without_start_fails() {
        let mut broker = ServerBroker::new(21887).unwrap();
        let result = broker.stop();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not running"));
    }
}
