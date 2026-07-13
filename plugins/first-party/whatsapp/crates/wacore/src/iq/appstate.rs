use crate::iq::spec::IqSpec;
use crate::request::InfoQuery;
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, SERVER_JID};
use wacore_binary::node::{Node, NodeContent};

/// IQ namespace for app state synchronization.
pub const APP_STATE_NAMESPACE: &str = "w:sync:app:state";

/// Response from sending an app state patch.
#[derive(Debug, Clone)]
pub enum SendPatchResponse {
    /// Patch was accepted by the server.
    Ok,
    /// 409 Conflict — the server returned updated patches that must be applied
    /// before retrying. The contained `Node` is the response collection node
    /// which can be parsed into a `PatchList` for conflict resolution.
    Conflict(Node),
}

/// IQ specification for sending an encoded app state patch to the server.
///
/// Corresponds to whatsmeow's `sendAppState` IQ:
/// ```xml
/// <iq xmlns="w:sync:app:state" type="set" to="s.whatsapp.net">
///   <sync>
///     <collection name="regular_high" version="N" return_snapshot="false">
///       <patch>ENCODED_PATCH_BYTES</patch>
///     </collection>
///   </sync>
/// </iq>
/// ```
pub struct SendAppStatePatchSpec {
    pub collection_name: String,
    pub version: u64,
    pub patch_bytes: Vec<u8>,
}

impl SendAppStatePatchSpec {
    pub fn new(collection_name: &str, version: u64, patch_bytes: Vec<u8>) -> Self {
        Self {
            collection_name: collection_name.to_string(),
            version,
            patch_bytes,
        }
    }
}

impl IqSpec for SendAppStatePatchSpec {
    type Response = SendPatchResponse;

    fn build_iq(&self) -> InfoQuery<'static> {
        let patch_node = NodeBuilder::new("patch")
            .bytes(self.patch_bytes.clone())
            .build();

        let collection_node = NodeBuilder::new("collection")
            .attr("name", self.collection_name.clone())
            .attr("version", self.version.to_string())
            .attr("return_snapshot", "false")
            .children([patch_node])
            .build();

        let sync_node = NodeBuilder::new("sync").children([collection_node]).build();

        InfoQuery::set(
            APP_STATE_NAMESPACE,
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![sync_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response, anyhow::Error> {
        // Check if the response contains an error collection (409 conflict)
        if let Some(collection) = response.get_optional_child_by_tag(&["sync", "collection"]) {
            let mut ag = collection.attrs();
            if ag.optional_string("type") == Some("error") {
                // Check for 409 conflict specifically
                if let Some(error_node) = collection.get_optional_child("error") {
                    let code = error_node
                        .attrs
                        .get("code")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<u16>().ok());
                    if code == Some(409) {
                        return Ok(SendPatchResponse::Conflict(response.clone()));
                    }
                }
                // Non-409 error — report as an error
                return Err(anyhow::anyhow!(
                    "app state update error for collection '{}'",
                    self.collection_name,
                ));
            }
        }
        Ok(SendPatchResponse::Ok)
    }
}

/// IQ specification for fetching app state patches from the server.
///
/// ```xml
/// <iq xmlns="w:sync:app:state" type="set" to="s.whatsapp.net">
///   <sync>
///     <collection name="regular_high" version="N" return_snapshot="false"/>
///   </sync>
/// </iq>
/// ```
pub struct FetchAppStatePatchesSpec {
    pub collection_name: String,
    pub from_version: Option<u64>,
    pub return_snapshot: bool,
}

impl FetchAppStatePatchesSpec {
    pub fn new(collection_name: &str, from_version: u64, return_snapshot: bool) -> Self {
        Self {
            collection_name: collection_name.to_string(),
            from_version: if return_snapshot {
                None
            } else {
                Some(from_version)
            },
            return_snapshot,
        }
    }
}

impl IqSpec for FetchAppStatePatchesSpec {
    type Response = Node;

    fn build_iq(&self) -> InfoQuery<'static> {
        let mut builder = NodeBuilder::new("collection")
            .attr("name", self.collection_name.clone())
            .attr("return_snapshot", self.return_snapshot.to_string());

        if let Some(version) = self.from_version {
            builder = builder.attr("version", version.to_string());
        }

        let sync_node = NodeBuilder::new("sync").children([builder.build()]).build();

        InfoQuery::set(
            APP_STATE_NAMESPACE,
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![sync_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response, anyhow::Error> {
        Ok(response.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_patch_spec_build_iq() {
        let spec = SendAppStatePatchSpec::new("regular_high", 5, vec![1, 2, 3]);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, APP_STATE_NAMESPACE);
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Set);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].tag, "sync");

            let children = nodes[0].children().expect("sync should have children");
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].tag, "collection");
            assert_eq!(
                children[0].attrs.get("name").and_then(|v| v.as_str()),
                Some("regular_high")
            );
            assert_eq!(
                children[0].attrs.get("version").and_then(|v| v.as_str()),
                Some("5")
            );
            assert_eq!(
                children[0]
                    .attrs
                    .get("return_snapshot")
                    .and_then(|v| v.as_str()),
                Some("false")
            );

            let patch_children = children[0]
                .children()
                .expect("collection should have children");
            assert_eq!(patch_children.len(), 1);
            assert_eq!(patch_children[0].tag, "patch");
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_send_patch_spec_parse_ok_response() {
        let spec = SendAppStatePatchSpec::new("regular", 1, vec![]);
        let response = NodeBuilder::new("iq").attr("type", "result").build();

        let result = spec.parse_response(&response);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), SendPatchResponse::Ok));
    }

    #[test]
    fn test_fetch_patches_spec_with_version() {
        let spec = FetchAppStatePatchesSpec::new("regular_low", 3, false);
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let children = nodes[0].children().expect("sync should have children");
            assert_eq!(
                children[0].attrs.get("version").and_then(|v| v.as_str()),
                Some("3")
            );
            assert_eq!(
                children[0]
                    .attrs
                    .get("return_snapshot")
                    .and_then(|v| v.as_str()),
                Some("false")
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_fetch_patches_spec_snapshot() {
        let spec = FetchAppStatePatchesSpec::new("regular", 0, true);
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let children = nodes[0].children().expect("sync should have children");
            // Version should not be set when requesting snapshot
            assert!(children[0].attrs.get("version").is_none());
            assert_eq!(
                children[0]
                    .attrs
                    .get("return_snapshot")
                    .and_then(|v| v.as_str()),
                Some("true")
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }
}
