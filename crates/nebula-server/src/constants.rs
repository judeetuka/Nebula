use backoff::ExponentialBackoff;
use std::time::Duration;

// ── Rathole constants ───────────────────────────────────────────────────────

/// UDP MTU. Currently far larger than necessary.
pub const UDP_BUFFER_SIZE: usize = 2048;
pub const UDP_SENDQ_SIZE: usize = 1024;
pub const UDP_TIMEOUT: u64 = 60;

pub fn listen_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        max_elapsed_time: None,
        max_interval: Duration::from_secs(1),
        ..Default::default()
    }
}

pub fn run_control_chan_backoff(interval: u64) -> ExponentialBackoff {
    ExponentialBackoff {
        randomization_factor: 0.2,
        max_elapsed_time: None,
        multiplier: 3.0,
        max_interval: Duration::from_secs(interval),
        ..Default::default()
    }
}

// ── NEBULA-specific constants ───────────────────────────────────────────────

/// Default REST API bind address.
pub const DEFAULT_API_BIND_ADDR: &str = "0.0.0.0:8080";

/// Maximum number of nodes allowed in a single cluster.
pub const MAX_NODES_PER_CLUSTER: usize = 1024;

/// Timeout for node registration handshake (seconds).
pub const NODE_REGISTRATION_TIMEOUT: u64 = 10;

/// Interval between node heartbeat checks (seconds).
pub const NODE_HEARTBEAT_CHECK_INTERVAL: u64 = 60;

/// If a node misses heartbeat for this duration, mark it stale (seconds).
pub const NODE_STALE_THRESHOLD: u64 = 120;
