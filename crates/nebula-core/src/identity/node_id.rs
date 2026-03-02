use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a node in the cluster.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeId(pub uuid::Uuid);

impl NodeId {
    /// Generate a new random node ID.
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Create from an existing UUID string.
    pub fn from_str(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a cluster (tenant namespace).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ClusterId(pub String);

impl fmt::Display for ClusterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
