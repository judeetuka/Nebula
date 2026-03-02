use serde::{Deserialize, Serialize};

use crate::identity::node_id::{ClusterId, NodeId};
use crate::identity::roles::NodeRole;
use crate::protocol::version::ProtocolVersion;
use crate::types::Digest;

/// Initial handshake message — extends rathole's Hello enum.
#[derive(Deserialize, Serialize, Debug)]
pub enum Hello {
    /// Rathole-compatible: establish a control channel for a service.
    ControlChannelHello(ProtocolVersion, Digest),
    /// Rathole-compatible: establish a data channel with a session nonce.
    DataChannelHello(ProtocolVersion, Digest),
    /// NEBULA: node registration with cluster membership.
    NodeRegistrationHello(ProtocolVersion, NodeId, ClusterId),
}

/// Authentication payload.
#[derive(Deserialize, Serialize, Debug)]
pub struct Auth(pub Digest);

/// Acknowledgement response.
#[derive(Deserialize, Serialize, Debug)]
pub enum Ack {
    Ok,
    ServiceNotExist,
    AuthFailed,
    // NEBULA extensions
    ClusterNotFound,
    NodeAlreadyRegistered,
    RegistrationAccepted { assigned_role: NodeRole },
    /// Sent in response to rotation commands to confirm the server is processing.
    RotationInProgress,
}

/// Commands sent over control channels.
#[derive(Deserialize, Serialize, Debug)]
pub enum ControlChannelCmd {
    /// Rathole-compatible: request the client to create a new data channel.
    CreateDataChannel,
    /// Rathole-compatible: keepalive heartbeat.
    HeartBeat,
    /// NEBULA: node heartbeat with device metrics.
    NodeHeartBeat(NodeHeartBeatPayload),
    /// NEBULA: request cluster status from master.
    ClusterStatusRequest,
    /// NEBULA: cluster status response.
    ClusterStatusResponse(ClusterStatus),
    /// NEBULA: master initiates rotation — tells server to prepare for a new master.
    RotationPrepare { new_master: NodeId },
    /// NEBULA: new master signals it is ready to accept traffic.
    RotationReady { new_master: NodeId },
    /// NEBULA: old master confirms handoff is complete.
    RotationComplete {
        old_master: NodeId,
        new_master: NodeId,
    },
}

/// Commands sent over data channels.
#[derive(Deserialize, Serialize, Debug)]
pub enum DataChannelCmd {
    StartForwardTcp,
    StartForwardUdp,
}

/// Heartbeat payload carrying device health metrics.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct NodeHeartBeatPayload {
    pub node_id: NodeId,
    pub battery_level: u8,
    pub cpu_load: f32,
    pub memory_available_mb: u32,
    pub uptime_secs: u64,
    pub active_tasks: u16,
    pub network_type: NetworkType,
    pub timestamp: i64,
}

/// Network connectivity type.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub enum NetworkType {
    Wifi,
    Cellular,
    Ethernet,
    Unknown,
}

/// Summary of a cluster's state.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ClusterStatus {
    pub cluster_id: ClusterId,
    pub node_count: u32,
    pub master_id: Option<NodeId>,
}
