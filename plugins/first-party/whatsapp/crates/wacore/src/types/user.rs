use chrono::{DateTime, Utc};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wacore_binary::jid::Jid;
use wacore_binary::node::{Node, NodeContent};
use waproto::whatsapp as wa;

#[derive(Debug, Clone)]
pub struct VerifiedName {
    pub certificate: Box<wa::VerifiedNameCertificate>,
    pub details: Box<wa::verified_name_certificate::Details>,
}

/// Parse a verified business name from a `<business>` node.
///
/// The `<business>` node is expected to contain a `<verified_name>` child
/// whose content is a serialized `VerifiedNameCertificate` protobuf.
///
/// Returns `Ok(None)` if the node is not a `<business>` tag or has no
/// `<verified_name>` child. Returns `Err` if protobuf decoding fails.
///
/// Mirrors whatsmeow's `parseVerifiedName`.
pub fn parse_verified_name(business_node: &Node) -> Result<Option<VerifiedName>, anyhow::Error> {
    if business_node.tag != "business" {
        return Ok(None);
    }
    let verified_name_node = match business_node.get_optional_child("verified_name") {
        Some(node) => node,
        None => return Ok(None),
    };
    parse_verified_name_content(verified_name_node)
}

/// Decode a `VerifiedNameCertificate` protobuf from a `<verified_name>` node.
///
/// The node's content must be raw bytes containing a serialized
/// `VerifiedNameCertificate`. Returns `Ok(None)` if the node has no binary
/// content.
///
/// Mirrors whatsmeow's `parseVerifiedNameContent`.
pub fn parse_verified_name_content(
    verified_name_node: &Node,
) -> Result<Option<VerifiedName>, anyhow::Error> {
    let raw_cert = match &verified_name_node.content {
        Some(NodeContent::Bytes(b)) => b,
        _ => return Ok(None),
    };

    let cert = wa::VerifiedNameCertificate::decode(raw_cert.as_slice())?;
    let details_bytes = cert.details.as_deref().unwrap_or(&[]);
    let details = wa::verified_name_certificate::Details::decode(details_bytes)?;

    Ok(Some(VerifiedName {
        certificate: Box::new(cert),
        details: Box::new(details),
    }))
}

#[derive(Debug, Clone, Default)]
pub struct LocalChatSettings {
    pub found: bool,
    pub muted_until: Option<DateTime<Utc>>,
    pub pinned: bool,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacySetting {
    All,
    Contacts,
    ContactBlacklist,
    ContactAllowlist,
    MatchLastSeen,
    Known,
    None,
    OnStandard,
    Off,
    #[serde(other)]
    Undefined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacySettingType {
    GroupAdd,
    Last,
    Status,
    Profile,
    ReadReceipts,
    Online,
    CallAdd,
    Messages,
    Defense,
    Stickers,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrivacySettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_add: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_receipts: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_add: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defense: Option<PrivacySetting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stickers: Option<PrivacySetting>,
}

#[derive(Debug, Clone)]
pub struct BusinessHoursConfig {
    pub day_of_week: String,
    pub mode: String,
    pub open_time: String,
    pub close_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct BusinessProfile {
    pub jid: Jid,
    pub address: Option<String>,
    pub email: Option<String>,
    pub categories: Vec<Category>,
    pub profile_options: HashMap<String, String>,
    pub business_hours_time_zone: Option<String>,
    pub business_hours: Vec<BusinessHoursConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wacore_binary::builder::NodeBuilder;

    #[test]
    fn test_parse_verified_name_valid_cert() {
        let details = wa::verified_name_certificate::Details {
            serial: Some(42),
            issuer: Some("WhatsApp".to_string()),
            verified_name: Some("Test Corp".to_string()),
            ..Default::default()
        };
        let mut details_bytes = Vec::new();
        details.encode(&mut details_bytes).unwrap();

        let cert = wa::VerifiedNameCertificate {
            details: Some(details_bytes),
            signature: Some(vec![0x01, 0x02]),
            server_signature: None,
        };
        let mut cert_bytes = Vec::new();
        cert.encode(&mut cert_bytes).unwrap();

        let node = NodeBuilder::new("business")
            .children([NodeBuilder::new("verified_name")
                .bytes(cert_bytes)
                .build()])
            .build();

        let result = parse_verified_name(&node).unwrap();
        assert!(result.is_some());
        let vname = result.unwrap();
        assert_eq!(vname.details.verified_name, Some("Test Corp".to_string()));
        assert_eq!(vname.details.serial, Some(42));
        assert_eq!(vname.details.issuer, Some("WhatsApp".to_string()));
    }

    #[test]
    fn test_parse_verified_name_wrong_tag() {
        let node = NodeBuilder::new("not-business").build();
        let result = parse_verified_name(&node).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_verified_name_no_verified_name_child() {
        let node = NodeBuilder::new("business").build();
        let result = parse_verified_name(&node).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_verified_name_content_no_bytes() {
        let node = NodeBuilder::new("verified_name")
            .string_content("not bytes")
            .build();
        let result = parse_verified_name_content(&node).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_verified_name_content_empty_node() {
        let node = NodeBuilder::new("verified_name").build();
        let result = parse_verified_name_content(&node).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_verified_name_content_valid() {
        let details = wa::verified_name_certificate::Details {
            serial: Some(99),
            verified_name: Some("Shop ABC".to_string()),
            ..Default::default()
        };
        let mut details_bytes = Vec::new();
        details.encode(&mut details_bytes).unwrap();

        let cert = wa::VerifiedNameCertificate {
            details: Some(details_bytes),
            signature: None,
            server_signature: None,
        };
        let mut cert_bytes = Vec::new();
        cert.encode(&mut cert_bytes).unwrap();

        let node = NodeBuilder::new("verified_name")
            .bytes(cert_bytes)
            .build();
        let result = parse_verified_name_content(&node).unwrap();
        assert!(result.is_some());
        let vname = result.unwrap();
        assert_eq!(vname.details.verified_name, Some("Shop ABC".to_string()));
        assert_eq!(vname.details.serial, Some(99));
    }
}
