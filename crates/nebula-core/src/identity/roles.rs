use serde::{Deserialize, Serialize};
use std::fmt;

/// The role a node plays in the cluster hierarchy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeRole {
    /// Manages tunnel connection, routes requests, runs embedded MQTT broker.
    Master,
    /// Executes tasks, runs plugins, heartbeats to master.
    Worker,
    /// Manages a sub-cluster of workers (for 100+ node deployments).
    RegionalMaster,
    /// Single-node mode: performs all roles.
    Standalone,
}

impl fmt::Display for NodeRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeRole::Master => write!(f, "Master"),
            NodeRole::Worker => write!(f, "Worker"),
            NodeRole::RegionalMaster => write!(f, "RegionalMaster"),
            NodeRole::Standalone => write!(f, "Standalone"),
        }
    }
}
