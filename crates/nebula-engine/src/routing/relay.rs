/// Manages the tunnel relay fallback route through the proxy server.
///
/// This route always works (assuming the proxy server is reachable) but
/// has the highest latency since all traffic is relayed through a central
/// server. It serves as the last-resort route when LAN discovery and
/// NAT hole-punching both fail.
pub struct TunnelRelay {
    server_url: String,
    is_connected: bool,
}

impl TunnelRelay {
    /// Create a new tunnel relay pointed at the given proxy server URL.
    pub fn new(server_url: &str) -> Self {
        Self {
            server_url: server_url.to_string(),
            is_connected: false,
        }
    }

    /// Update the tunnel connection status.
    ///
    /// Set to `true` when the WebSocket tunnel to the proxy server is
    /// established, `false` when it disconnects.
    pub fn set_connected(&mut self, connected: bool) {
        self.is_connected = connected;
    }

    /// Returns `true` if the tunnel relay is currently available.
    ///
    /// The relay is available when the node has an active connection
    /// to the proxy server.
    pub fn is_available(&self) -> bool {
        self.is_connected
    }

    /// Returns the proxy server URL.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_relay_is_disconnected() {
        let relay = TunnelRelay::new("wss://proxy.example.com");

        assert!(!relay.is_available());
        assert_eq!(relay.server_url(), "wss://proxy.example.com");
    }

    // -----------------------------------------------------------------------
    // set_connected / is_available
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_connected_true() {
        let mut relay = TunnelRelay::new("wss://proxy.test");

        relay.set_connected(true);
        assert!(relay.is_available());
    }

    #[test]
    fn test_set_connected_false() {
        let mut relay = TunnelRelay::new("wss://proxy.test");

        relay.set_connected(true);
        relay.set_connected(false);
        assert!(!relay.is_available());
    }

    // -----------------------------------------------------------------------
    // server_url
    // -----------------------------------------------------------------------

    #[test]
    fn test_server_url_stored_correctly() {
        let relay = TunnelRelay::new("wss://nebula-proxy.cloud:443/tunnel");
        assert_eq!(relay.server_url(), "wss://nebula-proxy.cloud:443/tunnel");
    }

    #[test]
    fn test_server_url_empty_string() {
        let relay = TunnelRelay::new("");
        assert_eq!(relay.server_url(), "");
    }
}
