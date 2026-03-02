//! Plugin capability declarations.
//!
//! A capability represents a platform feature or resource that a plugin may
//! request access to. The host engine checks these against the device's
//! security policy at install time.

use serde::{Deserialize, Serialize};

/// A platform capability that a plugin may request access to.
///
/// The host decides at install time whether to grant each capability based
/// on the device's security policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PluginCapability {
    // Communication
    Sms,
    Ussd,
    Telephony,
    Email,
    // Monitoring
    Notification,
    Accessibility,
    // Storage & Files
    FileAccess,
    Storage,
    // Network & Connectivity
    Network,
    Wifi,
    Bluetooth,
    // Hardware & Sensors
    Camera,
    Audio,
    Location,
    Sensors,
    // System
    Clipboard,
    AppManagement,
    ScreenControl,
    PowerManagement,
    DeviceAdmin,
    WebView,
    // Contacts & Calendar
    Contacts,
    Calendar,
    // UI
    Overlay,
    // Extensible
    Custom(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_capability_variants_serialize() {
        let capabilities = vec![
            PluginCapability::Sms,
            PluginCapability::Ussd,
            PluginCapability::Telephony,
            PluginCapability::Email,
            PluginCapability::Notification,
            PluginCapability::Accessibility,
            PluginCapability::FileAccess,
            PluginCapability::Storage,
            PluginCapability::Network,
            PluginCapability::Wifi,
            PluginCapability::Bluetooth,
            PluginCapability::Camera,
            PluginCapability::Audio,
            PluginCapability::Location,
            PluginCapability::Sensors,
            PluginCapability::Clipboard,
            PluginCapability::AppManagement,
            PluginCapability::ScreenControl,
            PluginCapability::PowerManagement,
            PluginCapability::DeviceAdmin,
            PluginCapability::WebView,
            PluginCapability::Contacts,
            PluginCapability::Calendar,
            PluginCapability::Overlay,
            PluginCapability::Custom("my_cap".to_string()),
        ];

        for cap in &capabilities {
            let json = serde_json::to_string(cap).unwrap();
            let deserialized: PluginCapability = serde_json::from_str(&json).unwrap();
            assert_eq!(*cap, deserialized);
        }
    }

    #[test]
    fn test_capability_equality() {
        assert_eq!(PluginCapability::Sms, PluginCapability::Sms);
        assert_ne!(PluginCapability::Sms, PluginCapability::Ussd);
        assert_ne!(
            PluginCapability::Custom("a".to_string()),
            PluginCapability::Custom("b".to_string())
        );
        assert_eq!(
            PluginCapability::Custom("same".to_string()),
            PluginCapability::Custom("same".to_string())
        );
    }

    #[test]
    fn test_capability_hash_for_set_usage() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(PluginCapability::Sms);
        set.insert(PluginCapability::Sms); // duplicate
        set.insert(PluginCapability::Network);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_capability_clone() {
        let cap = PluginCapability::Custom("test".to_string());
        let cloned = cap.clone();
        assert_eq!(cap, cloned);
    }

    #[test]
    fn test_capability_debug() {
        let cap = PluginCapability::Sms;
        let debug = format!("{:?}", cap);
        assert_eq!(debug, "Sms");
    }
}
