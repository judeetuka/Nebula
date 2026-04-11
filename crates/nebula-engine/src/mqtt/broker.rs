use std::collections::HashMap;

use anyhow::Result;
use rumqttd::{Broker, Config, ConnectionSettings, ServerSettings};
use tracing::info;

/// Embedded MQTT broker for master nodes.
///
/// Wraps `rumqttd::Broker` to provide a simple start/stop interface.
/// The broker listens on the specified port and accepts connections
/// from worker nodes in the same cluster.
pub struct EmbeddedBroker {
    config: Config,
    broker: Option<Broker>,
    listen_port: u16,
}

impl EmbeddedBroker {
    /// Create a new embedded broker configured to listen on the given port.
    pub fn new(listen_port: u16, tls: Option<rumqttd::TlsConfig>) -> Self {
        let config = build_broker_config(listen_port, tls);
        Self {
            config,
            broker: None,
            listen_port,
        }
    }

    /// Returns the port the broker is configured to listen on.
    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    /// Start the embedded MQTT broker.
    ///
    /// Creates the broker instance. The broker runs until `stop()` is called
    /// or the `EmbeddedBroker` is dropped.
    pub fn start(&mut self) -> Result<()> {
        if self.broker.is_some() {
            anyhow::bail!("Broker is already running");
        }

        let broker = Broker::new(self.config.clone());
        info!(port = self.listen_port, "Starting embedded MQTT broker");

        self.broker = Some(broker);
        Ok(())
    }

    /// Stop the embedded MQTT broker.
    pub fn stop(&mut self) -> Result<()> {
        if self.broker.is_none() {
            anyhow::bail!("Broker is not running");
        }

        info!(port = self.listen_port, "Stopping embedded MQTT broker");
        self.broker = None;
        Ok(())
    }

    /// Returns `true` if the broker is currently running.
    pub fn is_running(&self) -> bool {
        self.broker.is_some()
    }
}

/// Build a minimal rumqttd Config for an embedded broker.
fn build_broker_config(listen_port: u16, tls: Option<rumqttd::TlsConfig>) -> Config {
    let mut servers = HashMap::new();
    servers.insert(
        "nebula-v4".to_string(),
        ServerSettings {
            name: "nebula-v4".to_string(),
            listen: format!("0.0.0.0:{listen_port}")
                .parse()
                .expect("valid socket addr"),
            tls,
            next_connection_delay_ms: 100,
            connections: ConnectionSettings {
                connection_timeout_ms: 5000,
                max_payload_size: 65_536, // 64 KB — sufficient for control messages
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
    fn test_new_broker() {
        let broker = EmbeddedBroker::new(1883, None);
        assert_eq!(broker.listen_port(), 1883);
        assert!(!broker.is_running());
    }

    #[test]
    fn test_start_sets_running() {
        let mut broker = EmbeddedBroker::new(11883, None);
        broker.start().unwrap();
        assert!(broker.is_running());
    }

    #[test]
    fn test_stop_clears_running() {
        let mut broker = EmbeddedBroker::new(11884, None);
        broker.start().unwrap();
        broker.stop().unwrap();
        assert!(!broker.is_running());
    }

    #[test]
    fn test_double_start_fails() {
        let mut broker = EmbeddedBroker::new(11885, None);
        broker.start().unwrap();

        let result = broker.start();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already running"));
    }

    #[test]
    fn test_stop_without_start_fails() {
        let mut broker = EmbeddedBroker::new(11886, None);

        let result = broker.stop();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not running"));
    }
}
