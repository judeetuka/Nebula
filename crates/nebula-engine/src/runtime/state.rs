use nebula_core::identity::roles::NodeRole;
use serde::{Deserialize, Serialize};

/// Represents the lifecycle state of a NEBULA node.
///
/// The state machine enforces valid transitions between states,
/// preventing the engine from entering inconsistent configurations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeState {
    /// Engine created but not configured with cluster info.
    Uninitialized,
    /// Cluster config received (cluster_id, server_url, auth_token).
    Configured { cluster_id: String },
    /// Attempting to connect to proxy server.
    Connecting,
    /// Connected, registering with cluster.
    Registering,
    /// Fully active with assigned role.
    Active { role: NodeRole },
    /// Lost connection, attempting to reconnect.
    Reconnecting { attempts: u32 },
    /// Graceful shutdown in progress.
    ShuttingDown,
    /// Unrecoverable error.
    Error { message: String },
}

impl NodeState {
    /// Check if a transition to the target state is valid.
    ///
    /// The state machine defines these valid transitions:
    /// - `Uninitialized -> Configured`
    /// - `Configured -> Connecting`
    /// - `Connecting -> Registering | Reconnecting | Error`
    /// - `Registering -> Active | Error`
    /// - `Active -> Reconnecting | ShuttingDown`
    /// - `Reconnecting -> Connecting | Error | ShuttingDown`
    /// - Any state -> ShuttingDown (graceful shutdown always allowed)
    /// - Any state -> Error (errors can happen anywhere)
    pub fn can_transition_to(&self, target: &NodeState) -> bool {
        matches!(
            (self, target),
            (NodeState::Uninitialized, NodeState::Configured { .. })
                | (NodeState::Configured { .. }, NodeState::Connecting)
                | (NodeState::Connecting, NodeState::Registering)
                | (NodeState::Connecting, NodeState::Reconnecting { .. })
                | (NodeState::Connecting, NodeState::Error { .. })
                | (NodeState::Registering, NodeState::Active { .. })
                | (NodeState::Registering, NodeState::Error { .. })
                | (NodeState::Active { .. }, NodeState::Reconnecting { .. })
                | (NodeState::Active { .. }, NodeState::ShuttingDown)
                | (NodeState::Reconnecting { .. }, NodeState::Connecting)
                | (NodeState::Reconnecting { .. }, NodeState::Error { .. })
                | (NodeState::Reconnecting { .. }, NodeState::ShuttingDown)
                | (_, NodeState::ShuttingDown) // Can always shut down
                | (_, NodeState::Error { .. }) // Can always error
        )
    }

    /// Returns `true` if the node is in the `Active` state.
    pub fn is_active(&self) -> bool {
        matches!(self, NodeState::Active { .. })
    }

    /// Returns a human-readable name for the current state.
    pub fn display_name(&self) -> &str {
        match self {
            NodeState::Uninitialized => "Uninitialized",
            NodeState::Configured { .. } => "Configured",
            NodeState::Connecting => "Connecting",
            NodeState::Registering => "Registering",
            NodeState::Active { .. } => "Active",
            NodeState::Reconnecting { .. } => "Reconnecting",
            NodeState::ShuttingDown => "Shutting Down",
            NodeState::Error { .. } => "Error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::identity::roles::NodeRole;

    // -----------------------------------------------------------------------
    // Valid transitions
    // -----------------------------------------------------------------------

    #[test]
    fn test_uninitialized_to_configured() {
        let from = NodeState::Uninitialized;
        let to = NodeState::Configured {
            cluster_id: "cluster-1".to_string(),
        };
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_configured_to_connecting() {
        let from = NodeState::Configured {
            cluster_id: "cluster-1".to_string(),
        };
        let to = NodeState::Connecting;
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_connecting_to_registering() {
        let from = NodeState::Connecting;
        let to = NodeState::Registering;
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_connecting_to_reconnecting() {
        let from = NodeState::Connecting;
        let to = NodeState::Reconnecting { attempts: 1 };
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_connecting_to_error() {
        let from = NodeState::Connecting;
        let to = NodeState::Error {
            message: "connection refused".to_string(),
        };
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_registering_to_active() {
        let from = NodeState::Registering;
        let to = NodeState::Active {
            role: NodeRole::Worker,
        };
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_registering_to_error() {
        let from = NodeState::Registering;
        let to = NodeState::Error {
            message: "auth failed".to_string(),
        };
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_active_to_reconnecting() {
        let from = NodeState::Active {
            role: NodeRole::Master,
        };
        let to = NodeState::Reconnecting { attempts: 1 };
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_active_to_shutting_down() {
        let from = NodeState::Active {
            role: NodeRole::Worker,
        };
        let to = NodeState::ShuttingDown;
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_reconnecting_to_connecting() {
        let from = NodeState::Reconnecting { attempts: 3 };
        let to = NodeState::Connecting;
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_reconnecting_to_error() {
        let from = NodeState::Reconnecting { attempts: 10 };
        let to = NodeState::Error {
            message: "max retries exceeded".to_string(),
        };
        assert!(from.can_transition_to(&to));
    }

    #[test]
    fn test_reconnecting_to_shutting_down() {
        let from = NodeState::Reconnecting { attempts: 2 };
        let to = NodeState::ShuttingDown;
        assert!(from.can_transition_to(&to));
    }

    // -----------------------------------------------------------------------
    // Catch-all: any state -> ShuttingDown and any state -> Error
    // -----------------------------------------------------------------------

    #[test]
    fn test_any_state_can_shut_down() {
        let states = vec![
            NodeState::Uninitialized,
            NodeState::Configured {
                cluster_id: "c".to_string(),
            },
            NodeState::Connecting,
            NodeState::Registering,
            NodeState::Active {
                role: NodeRole::Worker,
            },
            NodeState::Reconnecting { attempts: 1 },
            NodeState::Error {
                message: "err".to_string(),
            },
        ];

        for state in states {
            assert!(
                state.can_transition_to(&NodeState::ShuttingDown),
                "{:?} should be able to transition to ShuttingDown",
                state
            );
        }
    }

    #[test]
    fn test_any_state_can_error() {
        let error = NodeState::Error {
            message: "fatal".to_string(),
        };
        let states = vec![
            NodeState::Uninitialized,
            NodeState::Configured {
                cluster_id: "c".to_string(),
            },
            NodeState::Connecting,
            NodeState::Registering,
            NodeState::Active {
                role: NodeRole::Worker,
            },
            NodeState::Reconnecting { attempts: 1 },
            NodeState::ShuttingDown,
        ];

        for state in states {
            assert!(
                state.can_transition_to(&error),
                "{:?} should be able to transition to Error",
                state
            );
        }
    }

    // -----------------------------------------------------------------------
    // Invalid transitions
    // -----------------------------------------------------------------------

    #[test]
    fn test_uninitialized_to_connecting_invalid() {
        let from = NodeState::Uninitialized;
        let to = NodeState::Connecting;
        assert!(!from.can_transition_to(&to));
    }

    #[test]
    fn test_uninitialized_to_active_invalid() {
        let from = NodeState::Uninitialized;
        let to = NodeState::Active {
            role: NodeRole::Worker,
        };
        assert!(!from.can_transition_to(&to));
    }

    #[test]
    fn test_configured_to_active_invalid() {
        let from = NodeState::Configured {
            cluster_id: "c".to_string(),
        };
        let to = NodeState::Active {
            role: NodeRole::Worker,
        };
        assert!(!from.can_transition_to(&to));
    }

    #[test]
    fn test_configured_to_registering_invalid() {
        let from = NodeState::Configured {
            cluster_id: "c".to_string(),
        };
        let to = NodeState::Registering;
        assert!(!from.can_transition_to(&to));
    }

    #[test]
    fn test_active_to_configured_invalid() {
        let from = NodeState::Active {
            role: NodeRole::Master,
        };
        let to = NodeState::Configured {
            cluster_id: "c".to_string(),
        };
        assert!(!from.can_transition_to(&to));
    }

    #[test]
    fn test_active_to_connecting_invalid() {
        let from = NodeState::Active {
            role: NodeRole::Worker,
        };
        let to = NodeState::Connecting;
        assert!(!from.can_transition_to(&to));
    }

    #[test]
    fn test_shutting_down_to_active_invalid() {
        let from = NodeState::ShuttingDown;
        let to = NodeState::Active {
            role: NodeRole::Worker,
        };
        assert!(!from.can_transition_to(&to));
    }

    #[test]
    fn test_shutting_down_to_connecting_invalid() {
        let from = NodeState::ShuttingDown;
        let to = NodeState::Connecting;
        assert!(!from.can_transition_to(&to));
    }

    #[test]
    fn test_error_to_active_invalid() {
        let from = NodeState::Error {
            message: "fatal".to_string(),
        };
        let to = NodeState::Active {
            role: NodeRole::Worker,
        };
        assert!(!from.can_transition_to(&to));
    }

    // -----------------------------------------------------------------------
    // Helper methods
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_active_true() {
        let state = NodeState::Active {
            role: NodeRole::Worker,
        };
        assert!(state.is_active());
    }

    #[test]
    fn test_is_active_false_for_other_states() {
        let non_active = vec![
            NodeState::Uninitialized,
            NodeState::Configured {
                cluster_id: "c".to_string(),
            },
            NodeState::Connecting,
            NodeState::Registering,
            NodeState::Reconnecting { attempts: 1 },
            NodeState::ShuttingDown,
            NodeState::Error {
                message: "err".to_string(),
            },
        ];

        for state in non_active {
            assert!(
                !state.is_active(),
                "{:?} should not be active",
                state
            );
        }
    }

    #[test]
    fn test_display_names() {
        assert_eq!(NodeState::Uninitialized.display_name(), "Uninitialized");
        assert_eq!(
            NodeState::Configured {
                cluster_id: "c".to_string()
            }
            .display_name(),
            "Configured"
        );
        assert_eq!(NodeState::Connecting.display_name(), "Connecting");
        assert_eq!(NodeState::Registering.display_name(), "Registering");
        assert_eq!(
            NodeState::Active {
                role: NodeRole::Worker
            }
            .display_name(),
            "Active"
        );
        assert_eq!(
            NodeState::Reconnecting { attempts: 1 }.display_name(),
            "Reconnecting"
        );
        assert_eq!(NodeState::ShuttingDown.display_name(), "Shutting Down");
        assert_eq!(
            NodeState::Error {
                message: "err".to_string()
            }
            .display_name(),
            "Error"
        );
    }

    // -----------------------------------------------------------------------
    // Serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let states = vec![
            NodeState::Uninitialized,
            NodeState::Configured {
                cluster_id: "cluster-abc".to_string(),
            },
            NodeState::Active {
                role: NodeRole::Master,
            },
            NodeState::Reconnecting { attempts: 5 },
            NodeState::Error {
                message: "something broke".to_string(),
            },
        ];

        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let deserialized: NodeState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, deserialized);
        }
    }
}
