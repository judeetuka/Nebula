//! Broadcast list and status broadcast support for WhatsApp.
//!
//! Ports whatsmeow/broadcast.go — handles broadcast list participant resolution,
//! status broadcast recipient discovery, and status privacy settings queries.
//!
//! Wire format:
//! ```xml
//! <!-- Status privacy query -->
//! <iq xmlns="status" type="get" to="s.whatsapp.net">
//!   <privacy/>
//! </iq>
//!
//! <!-- Status privacy response -->
//! <privacy>
//!   <list type="contacts" default="true">
//!     <user jid="1234@s.whatsapp.net"/>
//!   </list>
//! </privacy>
//! ```

use crate::iq::spec::IqSpec;
use crate::request::InfoQuery;
use crate::StringEnum;
use anyhow::{anyhow, Result};
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, JidExt, BROADCAST_SERVER, STATUS_BROADCAST_USER};
use wacore_binary::node::{Node, NodeContent};

// ── Constants ──────────────────────────────────────────────────────────────

/// Returns the status broadcast JID (`status@broadcast`).
///
/// Uses a runtime constructor since `Jid` fields are heap-allocated strings
/// and cannot be constructed in a `const` context.
pub fn status_broadcast_jid() -> Jid {
    Jid::new(STATUS_BROADCAST_USER, BROADCAST_SERVER)
}

// ── Status privacy types ───────────────────────────────────────────────────

/// Privacy type controlling who receives status broadcasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum StatusPrivacyType {
    /// Only specific contacts on a whitelist.
    #[str = "whitelist"]
    Whitelist,
    /// All contacts except those on a blacklist.
    #[str = "blacklist"]
    Blacklist,
    /// All contacts.
    #[str = "contacts"]
    Contacts,
    /// Everyone (no restrictions).
    #[string_default]
    #[str = "all"]
    All,
}

/// A single status privacy setting entry.
///
/// WhatsApp can store multiple privacy entries; the first one with
/// `is_default == true` is the active setting.
#[derive(Debug, Clone)]
pub struct StatusPrivacy {
    /// The privacy type (whitelist, blacklist, contacts, or all).
    pub privacy_type: StatusPrivacyType,
    /// JIDs in the allow/deny list (only populated for whitelist/blacklist).
    pub list: Vec<Jid>,
    /// Whether this is the default (active) privacy setting.
    pub is_default: bool,
}

/// Default status privacy: contacts-only, no explicit list.
pub fn default_status_privacy() -> Vec<StatusPrivacy> {
    vec![StatusPrivacy {
        privacy_type: StatusPrivacyType::Contacts,
        list: Vec::new(),
        is_default: true,
    }]
}

// ── IQ: GetStatusPrivacy ───────────────────────────────────────────────────

/// IQ specification for fetching status privacy settings.
///
/// Sends:
/// ```xml
/// <iq xmlns="status" type="get" to="s.whatsapp.net">
///   <privacy/>
/// </iq>
/// ```
#[derive(Debug, Clone, Default)]
pub struct StatusPrivacyIq;

impl StatusPrivacyIq {
    pub fn new() -> Self {
        Self
    }
}

impl IqSpec for StatusPrivacyIq {
    type Response = Vec<StatusPrivacy>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let privacy_node = NodeBuilder::new("privacy").build();

        InfoQuery::get(
            "status",
            Jid::new("", wacore_binary::jid::SERVER_JID),
            Some(NodeContent::Nodes(vec![privacy_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let privacy_node = match response.get_optional_child("privacy") {
            Some(node) => node,
            None => return Ok(default_status_privacy()),
        };

        let mut outputs = Vec::new();

        for list_node in privacy_node.get_children_by_tag("list") {
            let mut attrs = list_node.attrs();

            let type_str = attrs.optional_string("type").unwrap_or("all");

            let privacy_type =
                StatusPrivacyType::try_from(type_str).unwrap_or(StatusPrivacyType::All);

            let is_default = attrs.optional_bool("default");

            // Collect <user jid="..."/> children
            let list: Vec<Jid> = list_node
                .get_children_by_tag("user")
                .filter_map(|user_node| user_node.attrs().optional_jid("jid"))
                .collect();

            let entry = StatusPrivacy {
                privacy_type,
                list,
                is_default,
            };

            outputs.push(entry);
        }

        // Move the default entry to the front, matching whatsmeow behavior.
        if let Some(default_idx) = outputs.iter().position(|e| e.is_default) {
            if default_idx > 0 {
                outputs.swap(0, default_idx);
            }
        }

        if outputs.is_empty() {
            return Ok(default_status_privacy());
        }

        Ok(outputs)
    }
}

// ── Broadcast list participant helpers ─────────────────────────────────────

/// Error returned when a non-status broadcast list is queried.
///
/// WhatsApp only supports resolving participants for `status@broadcast`;
/// custom broadcast lists are not queryable via usync.
#[derive(Debug, Clone)]
pub struct BroadcastListUnsupported;

impl std::fmt::Display for BroadcastListUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "non-status broadcast lists are not supported")
    }
}

impl std::error::Error for BroadcastListUnsupported {}

/// Resolve the participant list for a broadcast JID.
///
/// Currently only `status@broadcast` is supported. For the status broadcast,
/// this delegates to privacy settings to determine the recipient list, then
/// ensures the owner's own JID is included.
///
/// The `privacy_settings` parameter should be obtained from [`StatusPrivacyIq`].
/// The `own_jid` is the logged-in user's JID (used to ensure self-inclusion).
pub fn resolve_broadcast_participants(
    broadcast_jid: &Jid,
    privacy_settings: &[StatusPrivacy],
    own_jid: &Jid,
) -> Result<Vec<Jid>> {
    if !broadcast_jid.is_status_broadcast() {
        return Err(BroadcastListUnsupported.into());
    }

    if privacy_settings.is_empty() {
        return Err(anyhow!("empty status privacy settings"));
    }

    // Use the first (default) privacy setting, matching Go implementation.
    let privacy = &privacy_settings[0];
    let mut recipients = resolve_status_recipients(privacy);

    // Ensure own JID is in the list (whatsmeow always includes self).
    let own_non_ad = own_jid.to_non_ad();
    let self_present = recipients.iter().any(|jid| jid.to_non_ad() == own_non_ad);
    if !self_present {
        recipients.push(own_non_ad);
    }

    Ok(recipients)
}

/// Determine the recipient JID list from a single privacy setting.
///
/// - **Whitelist**: returns the explicit list directly.
/// - **Blacklist / Contacts / All**: returns an empty vec. The caller is
///   responsible for fetching the full contact list from the store and
///   filtering out blacklisted JIDs. This function cannot access the contact
///   store, so it returns the blacklist for the caller to subtract.
///
/// For full recipient resolution with contact store access, use the higher-level
/// `resolve_broadcast_participants` in the client layer.
fn resolve_status_recipients(privacy: &StatusPrivacy) -> Vec<Jid> {
    match privacy.privacy_type {
        StatusPrivacyType::Whitelist => privacy.list.clone(),
        // Blacklist/Contacts/All need the contact store which we don't have
        // at this layer. Return empty; the client fills in from its store.
        StatusPrivacyType::Blacklist | StatusPrivacyType::Contacts | StatusPrivacyType::All => {
            Vec::new()
        }
    }
}

/// Filter a full contact list against a blacklist privacy setting.
///
/// Call this when `StatusPrivacyType::Blacklist` is active. Pass in all
/// contacts from the store; blacklisted JIDs are removed.
pub fn filter_blacklisted_contacts(contacts: &[Jid], blacklist: &[Jid]) -> Vec<Jid> {
    let blackset: std::collections::HashSet<String> = blacklist
        .iter()
        .map(|j| j.to_non_ad().to_string())
        .collect();

    contacts
        .iter()
        .filter(|jid| !blackset.contains(&jid.to_non_ad().to_string()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wacore_binary::builder::NodeBuilder;

    // ── StatusPrivacyIq::build_iq ──────────────────────────────────────────

    #[test]
    fn test_build_status_privacy_iq() {
        let spec = StatusPrivacyIq::new();
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "status");
        assert_eq!(iq.to, Jid::new("", "s.whatsapp.net"));

        // Content should be a single <privacy/> node
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].tag, "privacy");
        } else {
            panic!("expected Nodes content with <privacy/>");
        }
    }

    // ── StatusPrivacyIq::parse_response ────────────────────────────────────

    fn build_privacy_response(lists: Vec<(&str, bool, Vec<&str>)>) -> Node {
        let list_nodes: Vec<Node> = lists
            .into_iter()
            .map(|(type_str, is_default, jids)| {
                let user_nodes: Vec<Node> = jids
                    .into_iter()
                    .map(|jid| NodeBuilder::new("user").attr("jid", jid).build())
                    .collect();

                let mut builder = NodeBuilder::new("list").attr("type", type_str);
                if is_default {
                    builder = builder.attr("default", "true");
                }
                builder.children(user_nodes).build()
            })
            .collect();

        NodeBuilder::new("iq")
            .children([NodeBuilder::new("privacy").children(list_nodes).build()])
            .build()
    }

    #[test]
    fn test_parse_whitelist_privacy() {
        let response = build_privacy_response(vec![(
            "whitelist",
            true,
            vec!["111@s.whatsapp.net", "222@s.whatsapp.net"],
        )]);

        let spec = StatusPrivacyIq::new();
        let result = spec.parse_response(&response).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].privacy_type, StatusPrivacyType::Whitelist);
        assert!(result[0].is_default);
        assert_eq!(result[0].list.len(), 2);
        assert_eq!(result[0].list[0].user, "111");
        assert_eq!(result[0].list[1].user, "222");
    }

    #[test]
    fn test_parse_blacklist_privacy() {
        let response =
            build_privacy_response(vec![("blacklist", true, vec!["999@s.whatsapp.net"])]);

        let spec = StatusPrivacyIq::new();
        let result = spec.parse_response(&response).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].privacy_type, StatusPrivacyType::Blacklist);
        assert_eq!(result[0].list.len(), 1);
        assert_eq!(result[0].list[0].user, "999");
    }

    #[test]
    fn test_parse_contacts_privacy_empty_list() {
        let response = build_privacy_response(vec![("contacts", true, vec![])]);

        let spec = StatusPrivacyIq::new();
        let result = spec.parse_response(&response).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].privacy_type, StatusPrivacyType::Contacts);
        assert!(result[0].list.is_empty());
    }

    #[test]
    fn test_parse_all_privacy() {
        let response = build_privacy_response(vec![("all", false, vec![])]);

        let spec = StatusPrivacyIq::new();
        let result = spec.parse_response(&response).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].privacy_type, StatusPrivacyType::All);
        assert!(!result[0].is_default);
    }

    #[test]
    fn test_parse_empty_response_returns_default() {
        // Response with <privacy> but no <list> children
        let response = NodeBuilder::new("iq")
            .children([NodeBuilder::new("privacy").build()])
            .build();

        let spec = StatusPrivacyIq::new();
        let result = spec.parse_response(&response).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].privacy_type, StatusPrivacyType::Contacts);
        assert!(result[0].is_default);
    }

    #[test]
    fn test_parse_missing_privacy_returns_default() {
        // Response without <privacy> node at all
        let response = NodeBuilder::new("iq").build();

        let spec = StatusPrivacyIq::new();
        let result = spec.parse_response(&response).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].privacy_type, StatusPrivacyType::Contacts);
        assert!(result[0].is_default);
    }

    #[test]
    fn test_parse_multiple_lists_default_moved_to_front() {
        let response = build_privacy_response(vec![
            ("blacklist", false, vec!["aaa@s.whatsapp.net"]),
            ("whitelist", true, vec!["bbb@s.whatsapp.net"]),
        ]);

        let spec = StatusPrivacyIq::new();
        let result = spec.parse_response(&response).unwrap();

        assert_eq!(result.len(), 2);
        // Default (whitelist) should be first
        assert_eq!(result[0].privacy_type, StatusPrivacyType::Whitelist);
        assert!(result[0].is_default);
        assert_eq!(result[1].privacy_type, StatusPrivacyType::Blacklist);
        assert!(!result[1].is_default);
    }

    // ── StatusPrivacyType enum ─────────────────────────────────────────────

    #[test]
    fn test_privacy_type_as_str() {
        assert_eq!(StatusPrivacyType::Whitelist.as_str(), "whitelist");
        assert_eq!(StatusPrivacyType::Blacklist.as_str(), "blacklist");
        assert_eq!(StatusPrivacyType::Contacts.as_str(), "contacts");
        assert_eq!(StatusPrivacyType::All.as_str(), "all");
    }

    #[test]
    fn test_privacy_type_try_from() {
        assert_eq!(
            StatusPrivacyType::try_from("whitelist").unwrap(),
            StatusPrivacyType::Whitelist,
        );
        assert_eq!(
            StatusPrivacyType::try_from("blacklist").unwrap(),
            StatusPrivacyType::Blacklist,
        );
        assert_eq!(
            StatusPrivacyType::try_from("contacts").unwrap(),
            StatusPrivacyType::Contacts,
        );
        assert_eq!(
            StatusPrivacyType::try_from("all").unwrap(),
            StatusPrivacyType::All,
        );
        assert!(StatusPrivacyType::try_from("invalid").is_err());
    }

    #[test]
    fn test_privacy_type_default_is_all() {
        assert_eq!(StatusPrivacyType::default(), StatusPrivacyType::All);
    }

    // ── resolve_broadcast_participants ──────────────────────────────────────

    #[test]
    fn test_resolve_status_broadcast_whitelist() {
        let privacy = vec![StatusPrivacy {
            privacy_type: StatusPrivacyType::Whitelist,
            list: vec![
                Jid::new("111", "s.whatsapp.net"),
                Jid::new("222", "s.whatsapp.net"),
            ],
            is_default: true,
        }];

        let own_jid = Jid::new("333", "s.whatsapp.net");
        let bcast_jid = status_broadcast_jid();

        let result = resolve_broadcast_participants(&bcast_jid, &privacy, &own_jid).unwrap();

        // Should contain the whitelist + own JID appended
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].user, "111");
        assert_eq!(result[1].user, "222");
        assert_eq!(result[2].user, "333");
    }

    #[test]
    fn test_resolve_status_broadcast_self_already_present() {
        let own_jid = Jid::new("111", "s.whatsapp.net");

        let privacy = vec![StatusPrivacy {
            privacy_type: StatusPrivacyType::Whitelist,
            list: vec![
                Jid::new("111", "s.whatsapp.net"),
                Jid::new("222", "s.whatsapp.net"),
            ],
            is_default: true,
        }];

        let bcast_jid = status_broadcast_jid();
        let result = resolve_broadcast_participants(&bcast_jid, &privacy, &own_jid).unwrap();

        // Self already in list, should not be duplicated
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_resolve_non_status_broadcast_unsupported() {
        let privacy = vec![StatusPrivacy {
            privacy_type: StatusPrivacyType::All,
            list: Vec::new(),
            is_default: true,
        }];

        // Custom broadcast list, not status@broadcast
        let bcast_jid = Jid::new("customlist", BROADCAST_SERVER);
        let own_jid = Jid::new("111", "s.whatsapp.net");

        let result = resolve_broadcast_participants(&bcast_jid, &privacy, &own_jid);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));
    }

    #[test]
    fn test_resolve_empty_privacy_error() {
        let bcast_jid = status_broadcast_jid();
        let own_jid = Jid::new("111", "s.whatsapp.net");

        let result = resolve_broadcast_participants(&bcast_jid, &[], &own_jid);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_blacklist_returns_self_only() {
        let privacy = vec![StatusPrivacy {
            privacy_type: StatusPrivacyType::Blacklist,
            list: vec![Jid::new("blocked", "s.whatsapp.net")],
            is_default: true,
        }];

        let own_jid = Jid::new("me", "s.whatsapp.net");
        let bcast_jid = status_broadcast_jid();

        let result = resolve_broadcast_participants(&bcast_jid, &privacy, &own_jid).unwrap();

        // Blacklist mode without contact store returns empty + self
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].user, "me");
    }

    // ── filter_blacklisted_contacts ────────────────────────────────────────

    #[test]
    fn test_filter_blacklisted_contacts() {
        let contacts = vec![
            Jid::new("111", "s.whatsapp.net"),
            Jid::new("222", "s.whatsapp.net"),
            Jid::new("333", "s.whatsapp.net"),
        ];
        let blacklist = vec![Jid::new("222", "s.whatsapp.net")];

        let filtered = filter_blacklisted_contacts(&contacts, &blacklist);

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].user, "111");
        assert_eq!(filtered[1].user, "333");
    }

    #[test]
    fn test_filter_empty_blacklist_returns_all() {
        let contacts = vec![
            Jid::new("111", "s.whatsapp.net"),
            Jid::new("222", "s.whatsapp.net"),
        ];

        let filtered = filter_blacklisted_contacts(&contacts, &[]);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_empty_contacts_returns_empty() {
        let blacklist = vec![Jid::new("111", "s.whatsapp.net")];
        let filtered = filter_blacklisted_contacts(&[], &blacklist);
        assert!(filtered.is_empty());
    }

    // ── status_broadcast_jid ───────────────────────────────────────────────

    #[test]
    fn test_status_broadcast_jid() {
        let jid = status_broadcast_jid();
        assert_eq!(jid.user, "status");
        assert_eq!(jid.server, "broadcast");
        assert!(jid.is_status_broadcast());
        assert!(!jid.is_broadcast_list());
    }
}
