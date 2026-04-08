use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

const EVENT_BUS_CAPACITY: usize = 256;

/// Events streamed from the Rust engine to the Flutter UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum EngineEvent {
    StateChanged { new_state: String, role: Option<String> },
    MembershipChanged { member_count: u32, master_id: Option<String> },
    HeartbeatReceived { node_id: String, battery: u8, cpu: f32 },
    TaskUpdate { task_id: String, status: String },
    PluginResult { plugin_id: String, action: String, success: bool },
    MqttStatus { connected: bool },
    PeerMeshStatus { connected_peers: u32 },
    SuccessionUpdated { line: Vec<String> },
    Error { message: String, source: String },
    Log { level: String, message: String },
}

/// Broadcast-based event bus for pushing engine events to subscribers.
pub struct EventBus {
    sender: broadcast::Sender<EngineEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUS_CAPACITY);
        Self { sender }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: EngineEvent) -> usize {
        match self.sender.send(event) {
            Ok(n) => n,
            Err(_) => 0,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.sender.subscribe()
    }

    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

static EVENT_BUS: OnceLock<EventBus> = OnceLock::new();

/// Returns a reference to the global `EventBus` singleton.
pub fn global_event_bus() -> &'static EventBus {
    EVENT_BUS.get_or_init(EventBus::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_event_state_changed_roundtrip() {
        let event = EngineEvent::StateChanged { new_state: "Active".into(), role: Some("Master".into()) };
        let json = serde_json::to_string(&event).unwrap();
        let back: EngineEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn test_engine_event_all_variants_serialize() {
        let events = vec![
            EngineEvent::StateChanged { new_state: "Connecting".into(), role: None },
            EngineEvent::MembershipChanged { member_count: 0, master_id: None },
            EngineEvent::HeartbeatReceived { node_id: "n".into(), battery: 0, cpu: 0.0 },
            EngineEvent::TaskUpdate { task_id: "t".into(), status: "Pending".into() },
            EngineEvent::PluginResult { plugin_id: "p".into(), action: "a".into(), success: false },
            EngineEvent::MqttStatus { connected: false },
            EngineEvent::PeerMeshStatus { connected_peers: 0 },
            EngineEvent::SuccessionUpdated { line: vec![] },
            EngineEvent::Error { message: "e".into(), source: "s".into() },
            EngineEvent::Log { level: "l".into(), message: "m".into() },
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let back: EngineEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, back);
        }
    }

    #[test]
    fn test_engine_event_json_contains_type_tag() {
        let event = EngineEvent::StateChanged { new_state: "Active".into(), role: None };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"StateChanged"#));
    }

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let event = EngineEvent::MqttStatus { connected: true };
        bus.publish(event.clone());
        let received = rx.recv().await.unwrap();
        assert_eq!(received, event);
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        let event = EngineEvent::MqttStatus { connected: true };
        bus.publish(event.clone());
        assert_eq!(rx1.recv().await.unwrap(), event);
        assert_eq!(rx2.recv().await.unwrap(), event);
    }

    #[test]
    fn test_event_bus_publish_without_subscribers() {
        let bus = EventBus::new();
        let count = bus.publish(EngineEvent::Log { level: "debug".into(), message: "nobody".into() });
        assert_eq!(count, 0);
    }

    #[test]
    fn test_event_bus_subscriber_count() {
        let bus = EventBus::new();
        assert_eq!(bus.subscriber_count(), 0);
        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);
        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);
        drop(_rx1);
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[test]
    fn test_global_event_bus_returns_same_instance() {
        let bus1 = global_event_bus() as *const EventBus;
        let bus2 = global_event_bus() as *const EventBus;
        assert_eq!(bus1, bus2);
    }
}
