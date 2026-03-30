use super::traits::StanzaHandler;
use crate::client::Client;
use async_trait::async_trait;
use std::sync::Arc;
use wacore_binary::node::Node;

/// Handler for `<call>` stanzas.
///
/// Processes voice/video call signaling:
/// - `<offer>` — incoming call
/// - `<terminate>` — call ended
/// - `<relaylatency>` — relay ping measurements (logged, not dispatched)
#[derive(Default)]
pub struct CallHandler;

#[async_trait]
impl StanzaHandler for CallHandler {
    fn tag(&self) -> &'static str {
        "call"
    }

    async fn handle(&self, client: Arc<Client>, node: Arc<Node>, _cancelled: &mut bool) -> bool {
        client.handle_call(&node).await;
        true
    }
}
