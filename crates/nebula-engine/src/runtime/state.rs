use nebula_core::identity::roles::NodeRole;
use serde::{Deserialize, Serialize};

/// Represents the lifecycle state of a NEBULA node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeState {
    Uninitialized,
    Configured {
        cluster_id: String,
    },
    Connecting,
    Registering,
    Active {
        role: NodeRole,
    },
    /// Worker is promoting to master (starting MQTT broker, notifying cluster).
    Promoting {
        from_role: NodeRole,
    },
    /// Master is demoting to worker (voluntary rotation handoff).
    Demoting {
        to_role: NodeRole,
    },
    Reconnecting {
        attempts: u32,
    },
    ShuttingDown,
    Error {
        message: String,
    },
}

impl NodeState {
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
                | (NodeState::Active { .. }, NodeState::Promoting { .. })
                | (NodeState::Promoting { .. }, NodeState::Active { .. })
                | (NodeState::Promoting { .. }, NodeState::Error { .. })
                | (NodeState::Promoting { .. }, NodeState::ShuttingDown)
                | (NodeState::Active { .. }, NodeState::Demoting { .. })
                | (NodeState::Demoting { .. }, NodeState::Active { .. })
                | (NodeState::Demoting { .. }, NodeState::Error { .. })
                | (NodeState::Demoting { .. }, NodeState::ShuttingDown)
                | (NodeState::Reconnecting { .. }, NodeState::Connecting)
                | (NodeState::Reconnecting { .. }, NodeState::Error { .. })
                | (NodeState::Reconnecting { .. }, NodeState::ShuttingDown)
                | (_, NodeState::ShuttingDown)
                | (_, NodeState::Error { .. })
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(self, NodeState::Active { .. })
    }
    pub fn is_promoting(&self) -> bool {
        matches!(self, NodeState::Promoting { .. })
    }
    pub fn is_demoting(&self) -> bool {
        matches!(self, NodeState::Demoting { .. })
    }

    pub fn display_name(&self) -> &str {
        match self {
            NodeState::Uninitialized => "Uninitialized",
            NodeState::Configured { .. } => "Configured",
            NodeState::Connecting => "Connecting",
            NodeState::Registering => "Registering",
            NodeState::Active { .. } => "Active",
            NodeState::Promoting { .. } => "Promoting",
            NodeState::Demoting { .. } => "Demoting",
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

    #[test]
    fn test_uninitialized_to_configured() {
        assert!(
            NodeState::Uninitialized.can_transition_to(&NodeState::Configured {
                cluster_id: "c".into()
            })
        );
    }
    #[test]
    fn test_configured_to_connecting() {
        assert!(NodeState::Configured {
            cluster_id: "c".into()
        }
        .can_transition_to(&NodeState::Connecting));
    }
    #[test]
    fn test_connecting_to_registering() {
        assert!(NodeState::Connecting.can_transition_to(&NodeState::Registering));
    }
    #[test]
    fn test_registering_to_active() {
        assert!(
            NodeState::Registering.can_transition_to(&NodeState::Active {
                role: NodeRole::Worker
            })
        );
    }
    #[test]
    fn test_active_to_reconnecting() {
        assert!(NodeState::Active {
            role: NodeRole::Master
        }
        .can_transition_to(&NodeState::Reconnecting { attempts: 1 }));
    }
    #[test]
    fn test_active_to_shutting_down() {
        assert!(NodeState::Active {
            role: NodeRole::Worker
        }
        .can_transition_to(&NodeState::ShuttingDown));
    }
    #[test]
    fn test_reconnecting_to_connecting() {
        assert!(NodeState::Reconnecting { attempts: 3 }.can_transition_to(&NodeState::Connecting));
    }

    // Promoting transitions
    #[test]
    fn test_active_worker_to_promoting() {
        assert!(NodeState::Active {
            role: NodeRole::Worker
        }
        .can_transition_to(&NodeState::Promoting {
            from_role: NodeRole::Worker
        }));
    }
    #[test]
    fn test_promoting_to_active_master() {
        assert!(NodeState::Promoting {
            from_role: NodeRole::Worker
        }
        .can_transition_to(&NodeState::Active {
            role: NodeRole::Master
        }));
    }
    #[test]
    fn test_promoting_to_error() {
        assert!(NodeState::Promoting {
            from_role: NodeRole::Worker
        }
        .can_transition_to(&NodeState::Error {
            message: "fail".into()
        }));
    }
    #[test]
    fn test_promoting_to_shutting_down() {
        assert!(NodeState::Promoting {
            from_role: NodeRole::Worker
        }
        .can_transition_to(&NodeState::ShuttingDown));
    }

    // Demoting transitions
    #[test]
    fn test_active_master_to_demoting() {
        assert!(NodeState::Active {
            role: NodeRole::Master
        }
        .can_transition_to(&NodeState::Demoting {
            to_role: NodeRole::Worker
        }));
    }
    #[test]
    fn test_demoting_to_active_worker() {
        assert!(NodeState::Demoting {
            to_role: NodeRole::Worker
        }
        .can_transition_to(&NodeState::Active {
            role: NodeRole::Worker
        }));
    }
    #[test]
    fn test_demoting_to_error() {
        assert!(NodeState::Demoting {
            to_role: NodeRole::Worker
        }
        .can_transition_to(&NodeState::Error {
            message: "fail".into()
        }));
    }

    // Invalid transitions
    #[test]
    fn test_uninitialized_to_promoting_invalid() {
        assert!(
            !NodeState::Uninitialized.can_transition_to(&NodeState::Promoting {
                from_role: NodeRole::Worker
            })
        );
    }
    #[test]
    fn test_connecting_to_promoting_invalid() {
        assert!(
            !NodeState::Connecting.can_transition_to(&NodeState::Promoting {
                from_role: NodeRole::Worker
            })
        );
    }
    #[test]
    fn test_promoting_to_connecting_invalid() {
        assert!(!NodeState::Promoting {
            from_role: NodeRole::Worker
        }
        .can_transition_to(&NodeState::Connecting));
    }
    #[test]
    fn test_uninitialized_to_connecting_invalid() {
        assert!(!NodeState::Uninitialized.can_transition_to(&NodeState::Connecting));
    }
    #[test]
    fn test_uninitialized_to_active_invalid() {
        assert!(
            !NodeState::Uninitialized.can_transition_to(&NodeState::Active {
                role: NodeRole::Worker
            })
        );
    }
    #[test]
    fn test_shutting_down_to_active_invalid() {
        assert!(
            !NodeState::ShuttingDown.can_transition_to(&NodeState::Active {
                role: NodeRole::Worker
            })
        );
    }
    #[test]
    fn test_error_to_active_invalid() {
        assert!(!NodeState::Error {
            message: "fatal".into()
        }
        .can_transition_to(&NodeState::Active {
            role: NodeRole::Worker
        }));
    }

    // Helpers
    #[test]
    fn test_is_active_true() {
        assert!(NodeState::Active {
            role: NodeRole::Worker
        }
        .is_active());
    }
    #[test]
    fn test_is_promoting_true() {
        assert!(NodeState::Promoting {
            from_role: NodeRole::Worker
        }
        .is_promoting());
    }
    #[test]
    fn test_is_demoting_true() {
        assert!(NodeState::Demoting {
            to_role: NodeRole::Worker
        }
        .is_demoting());
    }
    #[test]
    fn test_is_promoting_false() {
        assert!(!NodeState::Active {
            role: NodeRole::Worker
        }
        .is_promoting());
    }
    #[test]
    fn test_is_demoting_false() {
        assert!(!NodeState::Active {
            role: NodeRole::Master
        }
        .is_demoting());
    }

    #[test]
    fn test_any_state_can_shut_down() {
        let states = vec![
            NodeState::Uninitialized,
            NodeState::Configured {
                cluster_id: "c".into(),
            },
            NodeState::Connecting,
            NodeState::Registering,
            NodeState::Active {
                role: NodeRole::Worker,
            },
            NodeState::Promoting {
                from_role: NodeRole::Worker,
            },
            NodeState::Demoting {
                to_role: NodeRole::Worker,
            },
            NodeState::Reconnecting { attempts: 1 },
            NodeState::Error {
                message: "err".into(),
            },
        ];
        for state in states {
            assert!(
                state.can_transition_to(&NodeState::ShuttingDown),
                "{:?}",
                state
            );
        }
    }

    #[test]
    fn test_any_state_can_error() {
        let error = NodeState::Error {
            message: "fatal".into(),
        };
        let states = vec![
            NodeState::Uninitialized,
            NodeState::Configured {
                cluster_id: "c".into(),
            },
            NodeState::Connecting,
            NodeState::Registering,
            NodeState::Active {
                role: NodeRole::Worker,
            },
            NodeState::Promoting {
                from_role: NodeRole::Worker,
            },
            NodeState::Demoting {
                to_role: NodeRole::Worker,
            },
            NodeState::Reconnecting { attempts: 1 },
            NodeState::ShuttingDown,
        ];
        for state in states {
            assert!(state.can_transition_to(&error), "{:?}", state);
        }
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let states = vec![
            NodeState::Uninitialized,
            NodeState::Active {
                role: NodeRole::Master,
            },
            NodeState::Promoting {
                from_role: NodeRole::Worker,
            },
            NodeState::Demoting {
                to_role: NodeRole::Master,
            },
            NodeState::Reconnecting { attempts: 5 },
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let back: NodeState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, back);
        }
    }

    #[test]
    fn test_display_names() {
        assert_eq!(
            NodeState::Promoting {
                from_role: NodeRole::Worker
            }
            .display_name(),
            "Promoting"
        );
        assert_eq!(
            NodeState::Demoting {
                to_role: NodeRole::Worker
            }
            .display_name(),
            "Demoting"
        );
    }
}
