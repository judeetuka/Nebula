//! Notification dispatch and parsing for WhatsApp notification stanzas.
//!
//! Ports whatsmeow/notification.go — the central `handleNotification` router
//! that dispatches by `type` attribute, plus per-type parsing functions.
//!
//! ## Wire Format
//!
//! All notifications arrive as `<notification type="..." ...>` stanzas.
//! The `type` attribute determines which sub-handler is invoked:
//!
//! ```xml
//! <notification type="encrypt" from="s.whatsapp.net" t="1713100000">
//!   <count value="5"/>
//! </notification>
//!
//! <notification type="server_sync" t="1713100000">
//!   <collection name="regular_high" version="42"/>
//!   <collection name="regular_low" version="17"/>
//! </notification>
//!
//! <notification type="picture" t="1713100000">
//!   <add jid="user@s.whatsapp.net" id="pic_id_123"/>
//! </notification>
//! ```

use chrono::{DateTime, TimeZone, Utc};
use serde::Serialize;
use wacore_binary::jid::{Jid, SERVER_JID};
use wacore_binary::node::{Node, NodeContent};

use crate::mediaretry::{self, MediaRetry};
use crate::types::events::{
    Blocklist, BlocklistAction, BlocklistChange, BlocklistChangeAction, IdentityChange,
    NewsletterJoin, NewsletterLeave, NewsletterLiveUpdate, NewsletterMuteChange, PictureUpdate,
};

// ── Notification action enum ──────────────────────────────────────────────

/// Result of parsing a notification stanza.
///
/// Each variant corresponds to one notification `type` value from the
/// WhatsApp protocol. The handler layer (in `src/handlers/notification.rs`)
/// converts these into [`Event`] dispatches and any required side-effects
/// (store updates, key uploads, etc.).
#[derive(Debug, Clone)]
pub enum NotificationAction {
    /// Encrypt notification from server: prekey count below threshold.
    /// The handler should upload pre-keys if `otks_left < MIN_PREKEY_COUNT`.
    EncryptPreKeyCount {
        otks_left: u32,
    },

    /// Encrypt notification from a user: identity key changed.
    /// All sessions/identities for that user should be deleted.
    EncryptIdentityChange(IdentityChange),

    /// Server sync notification: app state collections that need re-syncing.
    /// Each entry is `(collection_name, version)`.
    ServerSync {
        collections: Vec<ServerSyncCollection>,
    },

    /// Account sync notification: contains one or more sub-items.
    AccountSync(Vec<AccountSyncItem>),

    /// Device list changed for a user (add/remove/update).
    DeviceListChange(DeviceChange),

    /// Profile picture changed.
    PictureUpdate(Vec<PictureUpdate>),

    /// Media retry notification (encrypted re-upload response).
    MediaRetry(MediaRetry),

    /// Privacy token update from a contact.
    PrivacyToken(PrivacyTokenUpdate),

    /// Pair code companion registration (stage 2).
    LinkCodeCompanionReg,

    /// Newsletter live update (new messages in a channel).
    NewsletterLiveUpdate(NewsletterLiveUpdate),

    /// MEX push notification (newsletter join/leave/mute).
    MexUpdate(Vec<MexEvent>),

    /// User status/about text changed.
    StatusUpdate(StatusUpdate),

    /// LID migration mapping update.
    LidMigration {
        mappings: Vec<LidPnMapping>,
    },

    /// Group info change notification (w:gp2).
    GroupInfoChange(GroupNotification),

    /// Blocklist change notification (from account_sync).
    Blocklist(Blocklist),

    /// Unknown or unhandled notification type.
    Ignored {
        notification_type: String,
    },
}

// ── Sub-types ─────────────────────────────────────────────────────────────

/// A single app state collection that needs re-syncing.
#[derive(Debug, Clone, Serialize)]
pub struct ServerSyncCollection {
    pub name: String,
    pub version: u64,
}

/// Sub-items within an account_sync notification.
#[derive(Debug, Clone)]
pub enum AccountSyncItem {
    /// Privacy settings changed.
    Privacy(Node),
    /// Own device list changed.
    Devices(Node),
    /// Own profile picture changed.
    Picture { timestamp: DateTime<Utc> },
    /// Blocklist update.
    Blocklist(Blocklist),
    /// Unknown sub-item.
    Unknown { tag: String },
}

/// Device change info from a `devices` notification.
#[derive(Debug, Clone)]
pub struct DeviceChange {
    /// The user whose device list changed.
    pub from: Jid,
    /// Optional LID for the user (for LID-PN mapping).
    pub lid: Option<Jid>,
    /// The raw children (add/remove/update operations).
    pub operations: Vec<DeviceOperation>,
}

/// A single device operation within a device notification.
#[derive(Debug, Clone)]
pub enum DeviceOperation {
    Add {
        device_jid: Jid,
        device_lid: Option<Jid>,
        device_hash: String,
        device_lid_hash: Option<String>,
    },
    Remove {
        device_jid: Jid,
        device_lid: Option<Jid>,
        device_hash: String,
        device_lid_hash: Option<String>,
    },
    Update,
}

/// Privacy token received from a contact.
#[derive(Debug, Clone)]
pub struct PrivacyTokenUpdate {
    /// Sender of the token (from notification's `from` attribute).
    pub sender: Jid,
    /// Optional sender LID (from `sender_lid` attribute).
    pub sender_lid: Option<String>,
    /// The raw tokens node for downstream parsing via `tctoken::parse_privacy_token_notification`.
    pub tokens_node: Node,
}

/// Newsletter event from MEX push notification.
#[derive(Debug, Clone)]
pub enum MexEvent {
    Join(NewsletterJoin),
    Leave(NewsletterLeave),
    MuteChange(NewsletterMuteChange),
}

/// User status/about text update.
#[derive(Debug, Clone, Serialize)]
pub struct StatusUpdate {
    pub jid: Jid,
    pub timestamp: DateTime<Utc>,
    pub status: String,
}

/// LID-PN mapping entry from lid_migration notification.
#[derive(Debug, Clone, Serialize)]
pub struct LidPnMapping {
    pub lid: String,
    pub pn: String,
}

/// Group notification info from w:gp2 type notifications.
#[derive(Debug, Clone)]
pub struct GroupNotification {
    /// The group JID.
    pub group_jid: Jid,
    /// The actor who made the change.
    pub participant: Option<Jid>,
    /// Timestamp of the notification.
    pub timestamp: DateTime<Utc>,
    /// The specific group change.
    pub change: GroupChange,
}

/// Type of group change.
#[derive(Debug, Clone)]
pub enum GroupChange {
    /// Participants added to the group.
    ParticipantsAdd { participants: Vec<Jid> },
    /// Participants removed from the group.
    ParticipantsRemove { participants: Vec<Jid> },
    /// Participants promoted to admin.
    ParticipantsPromote { participants: Vec<Jid> },
    /// Participants demoted from admin.
    ParticipantsDemote { participants: Vec<Jid> },
    /// Group subject (name) changed.
    SubjectChange {
        new_subject: String,
        old_subject: Option<String>,
    },
    /// Group description changed.
    DescriptionChange { new_description: Option<String> },
    /// Group locked/unlocked (restrict settings changes to admins).
    Locked(bool),
    /// Group announcement mode changed (restrict messages to admins).
    Announce(bool),
    /// Group ephemeral timer changed.
    Ephemeral { expiration: u64 },
    /// Group invite link reset.
    InviteLinkReset,
    /// Group was created (the creating participant's info).
    GroupCreated,
    /// Unknown/unhandled group change.
    Unknown { tag: String },
}

// ── Main router ───────────────────────────────────────────────────────────

/// Parse a notification stanza and return the appropriate action.
///
/// This is the pure-parsing counterpart to whatsmeow's `handleNotification`.
/// The caller is responsible for acting on the returned `NotificationAction`
/// (dispatching events, updating stores, etc.).
pub fn parse_notification(node: &Node) -> NotificationAction {
    let notification_type = node.attrs().optional_string("type").unwrap_or_default();

    match notification_type {
        "encrypt" => parse_encrypt_notification(node),
        "server_sync" => parse_server_sync_notification(node),
        "account_sync" => parse_account_sync_notification(node),
        "devices" => parse_device_notification(node),
        "w:gp2" => parse_group_notification(node),
        "picture" => parse_picture_notification(node),
        "mediaretry" => parse_media_retry_notification(node),
        "privacy_token" => parse_privacy_token_notification(node),
        "link_code_companion_reg" => NotificationAction::LinkCodeCompanionReg,
        "newsletter" => parse_newsletter_notification(node),
        "mex" => parse_mex_notification(node),
        "status" => parse_status_notification(node),
        "lid_migration" => parse_lid_migration_notification(node),
        other => NotificationAction::Ignored {
            notification_type: other.to_string(),
        },
    }
}

// ── Per-type parsers ──────────────────────────────────────────────────────

/// Parse an `encrypt` notification.
///
/// Two cases (mirrors whatsmeow `handleEncryptNotification`):
/// 1. From server: `<count value="N"/>` — prekey count update
/// 2. From user: contains `<identity>` — identity key changed
pub fn parse_encrypt_notification(node: &Node) -> NotificationAction {
    let from = node
        .attrs()
        .optional_string("from")
        .unwrap_or_default();

    if from == SERVER_JID {
        // Server telling us our prekey count
        let otks_left = node
            .get_optional_child("count")
            .and_then(|count| count.attrs().optional_u64("value"))
            .map(|v| v as u32)
            .unwrap_or(0);

        NotificationAction::EncryptPreKeyCount { otks_left }
    } else if node.get_optional_child("identity").is_some() {
        // Identity key change for a user
        let jid = node
            .attrs()
            .optional_jid("from")
            .unwrap_or_default();

        let timestamp = node
            .attrs()
            .optional_u64("t")
            .and_then(|t| Utc.timestamp_opt(t as i64, 0).single())
            .unwrap_or_else(Utc::now);

        NotificationAction::EncryptIdentityChange(IdentityChange { jid, timestamp })
    } else {
        NotificationAction::Ignored {
            notification_type: "encrypt".to_string(),
        }
    }
}

/// Parse a `server_sync` notification.
///
/// Contains `<collection name="..." version="N"/>` children indicating
/// which app state collections need re-syncing.
///
/// Mirrors whatsmeow `handleAppStateNotification`.
pub fn parse_server_sync_notification(node: &Node) -> NotificationAction {
    let collections = node
        .get_children_by_tag("collection")
        .filter_map(|child| {
            let name = child.attrs().optional_string("name")?.to_string();
            let version = child.attrs().optional_u64("version").unwrap_or(0);
            Some(ServerSyncCollection { name, version })
        })
        .collect();

    NotificationAction::ServerSync { collections }
}

/// Parse an `account_sync` notification.
///
/// Contains sub-items: `<privacy>`, `<devices>`, `<picture>`, `<blocklist>`.
/// Mirrors whatsmeow `handleAccountSyncNotification`.
pub fn parse_account_sync_notification(node: &Node) -> NotificationAction {
    let timestamp = node
        .attrs()
        .optional_u64("t")
        .and_then(|t| Utc.timestamp_opt(t as i64, 0).single())
        .unwrap_or_else(Utc::now);

    let mut items = Vec::new();

    if let Some(children) = node.children() {
        for child in children {
            match child.tag.as_str() {
                "privacy" => {
                    items.push(AccountSyncItem::Privacy(child.clone()));
                }
                "devices" => {
                    items.push(AccountSyncItem::Devices(child.clone()));
                }
                "picture" => {
                    items.push(AccountSyncItem::Picture { timestamp });
                }
                "blocklist" => {
                    let blocklist = parse_blocklist_node(child);
                    items.push(AccountSyncItem::Blocklist(blocklist));
                }
                other => {
                    items.push(AccountSyncItem::Unknown {
                        tag: other.to_string(),
                    });
                }
            }
        }
    }

    NotificationAction::AccountSync(items)
}

/// Parse a blocklist node into a Blocklist event.
///
/// Mirrors whatsmeow `handleBlocklist`.
fn parse_blocklist_node(node: &Node) -> Blocklist {
    let action = BlocklistAction::from(
        node.attrs()
            .optional_string("action")
            .unwrap_or(""),
    );
    let dhash = node
        .attrs()
        .optional_string("dhash")
        .unwrap_or("")
        .to_string();
    let prev_dhash = node
        .attrs()
        .optional_string("prev_dhash")
        .map(String::from);

    let mut changes = Vec::new();
    if let Some(children) = node.children() {
        for child in children {
            let jid = child.attrs().optional_jid("jid");
            let action_str = child
                .attrs()
                .optional_string("action")
                .unwrap_or("");
            if let Some(jid) = jid {
                changes.push(BlocklistChange {
                    jid,
                    action: BlocklistChangeAction::from(action_str),
                });
            }
        }
    }

    Blocklist {
        action,
        dhash,
        prev_dhash,
        changes,
    }
}

/// Parse a `devices` notification.
///
/// Mirrors whatsmeow `handleDeviceNotification`. Extracts the from JID,
/// optional LID, and device add/remove/update operations.
pub fn parse_device_notification(node: &Node) -> NotificationAction {
    let from = node.attrs().optional_jid("from").unwrap_or_default();
    let lid = node.attrs().optional_jid("lid");

    let mut operations = Vec::new();

    if let Some(children) = node.children() {
        for child in children {
            let device_hash = child
                .attrs()
                .optional_string("device_hash")
                .unwrap_or("")
                .to_string();
            let device_lid_hash = child
                .attrs()
                .optional_string("device_lid_hash")
                .map(String::from);

            // Extract device JID from <device> child
            let device_child = child.get_optional_child("device");
            let device_jid = device_child
                .and_then(|d| d.attrs().optional_jid("jid"))
                .unwrap_or_default();
            let device_lid = device_child.and_then(|d| d.attrs().optional_jid("lid"));

            match child.tag.as_str() {
                "add" => {
                    operations.push(DeviceOperation::Add {
                        device_jid,
                        device_lid,
                        device_hash,
                        device_lid_hash,
                    });
                }
                "remove" => {
                    operations.push(DeviceOperation::Remove {
                        device_jid,
                        device_lid,
                        device_hash,
                        device_lid_hash,
                    });
                }
                "update" => {
                    operations.push(DeviceOperation::Update);
                }
                _ => {}
            }
        }
    }

    NotificationAction::DeviceListChange(DeviceChange {
        from,
        lid,
        operations,
    })
}

/// Parse a `w:gp2` (group) notification.
///
/// Mirrors whatsmeow `parseGroupNotification`. The actual child tag
/// determines the specific group change.
pub fn parse_group_notification(node: &Node) -> NotificationAction {
    let group_jid = node.attrs().optional_jid("from").unwrap_or_default();
    let participant = node.attrs().optional_jid("participant");
    let timestamp = node
        .attrs()
        .optional_u64("t")
        .and_then(|t| Utc.timestamp_opt(t as i64, 0).single())
        .unwrap_or_else(Utc::now);

    let change = if let Some(children) = node.children() {
        parse_group_change(children)
    } else {
        GroupChange::Unknown {
            tag: String::new(),
        }
    };

    NotificationAction::GroupInfoChange(GroupNotification {
        group_jid,
        participant,
        timestamp,
        change,
    })
}

/// Determine the group change type from notification children.
fn parse_group_change(children: &[Node]) -> GroupChange {
    for child in children {
        match child.tag.as_str() {
            "add" | "invite" => {
                let participants = collect_participant_jids(child);
                return GroupChange::ParticipantsAdd { participants };
            }
            "remove" => {
                let participants = collect_participant_jids(child);
                return GroupChange::ParticipantsRemove { participants };
            }
            "promote" => {
                let participants = collect_participant_jids(child);
                return GroupChange::ParticipantsPromote { participants };
            }
            "demote" => {
                let participants = collect_participant_jids(child);
                return GroupChange::ParticipantsDemote { participants };
            }
            "subject" => {
                let new_subject = child
                    .attrs()
                    .optional_string("subject")
                    .unwrap_or("")
                    .to_string();
                let old_subject = child
                    .attrs()
                    .optional_string("s_o")
                    .map(String::from);
                return GroupChange::SubjectChange {
                    new_subject,
                    old_subject,
                };
            }
            "description" => {
                let new_description = match &child.content {
                    Some(NodeContent::String(s)) => Some(s.clone()),
                    _ => child
                        .get_optional_child("body")
                        .and_then(|body| match &body.content {
                            Some(NodeContent::String(s)) => Some(s.clone()),
                            _ => None,
                        }),
                };
                return GroupChange::DescriptionChange { new_description };
            }
            "locked" => return GroupChange::Locked(true),
            "unlocked" => return GroupChange::Locked(false),
            "announcement" => return GroupChange::Announce(true),
            "not_announcement" => return GroupChange::Announce(false),
            "ephemeral" => {
                let expiration = child
                    .attrs()
                    .optional_u64("expiration")
                    .unwrap_or(0);
                return GroupChange::Ephemeral { expiration };
            }
            "revoke" => return GroupChange::InviteLinkReset,
            "create" => return GroupChange::GroupCreated,
            _ => continue,
        }
    }

    GroupChange::Unknown {
        tag: children
            .first()
            .map(|c| c.tag.clone())
            .unwrap_or_default(),
    }
}

/// Collect participant JIDs from a group notification child.
fn collect_participant_jids(node: &Node) -> Vec<Jid> {
    node.get_children_by_tag("participant")
        .filter_map(|p| p.attrs().optional_jid("jid"))
        .collect()
}

/// Parse a `picture` notification.
///
/// Contains `<add>`, `<set>`, or `<delete>` children with JID and optional
/// picture ID. Mirrors whatsmeow `handlePictureNotification`.
pub fn parse_picture_notification(node: &Node) -> NotificationAction {
    let timestamp = node
        .attrs()
        .optional_u64("t")
        .and_then(|t| Utc.timestamp_opt(t as i64, 0).single())
        .unwrap_or_else(Utc::now);

    let mut updates = Vec::new();

    if let Some(children) = node.children() {
        for child in children {
            let jid = match child.attrs().optional_jid("jid") {
                Some(j) => j,
                None => continue,
            };
            let author = child.attrs().optional_jid("author").unwrap_or_default();

            let photo_change = match child.tag.as_str() {
                "delete" => Some(waproto::whatsapp::PhotoChange {
                    old_photo: None,
                    new_photo: None,
                    new_photo_id: None,
                }),
                "add" | "set" => {
                    let pic_id = child
                        .attrs()
                        .optional_string("id")
                        .unwrap_or("")
                        .to_string();
                    Some(waproto::whatsapp::PhotoChange {
                        old_photo: None,
                        new_photo: Some(pic_id.into_bytes()),
                        new_photo_id: None,
                    })
                }
                _ => continue,
            };

            updates.push(PictureUpdate {
                jid,
                author,
                timestamp,
                photo_change,
            });
        }
    }

    NotificationAction::PictureUpdate(updates)
}

/// Parse a `mediaretry` notification.
///
/// Delegates to [`mediaretry::parse_media_retry_notification`].
pub fn parse_media_retry_notification(node: &Node) -> NotificationAction {
    match mediaretry::parse_media_retry_notification(node) {
        Ok(retry) => NotificationAction::MediaRetry(retry),
        Err(_) => NotificationAction::Ignored {
            notification_type: "mediaretry".to_string(),
        },
    }
}

/// Parse a `privacy_token` notification.
///
/// Extracts the sender and tokens node for downstream processing.
/// The actual token parsing is deferred to `iq::tctoken::parse_privacy_token_notification`.
pub fn parse_privacy_token_notification(node: &Node) -> NotificationAction {
    let sender = node.attrs().optional_jid("from").unwrap_or_default();
    let sender_lid = node
        .attrs()
        .optional_string("sender_lid")
        .map(String::from);

    // Clone the tokens child node for downstream processing
    let tokens_node = match node.get_optional_child("tokens") {
        Some(n) => n.clone(),
        None => {
            return NotificationAction::Ignored {
                notification_type: "privacy_token".to_string(),
            };
        }
    };

    NotificationAction::PrivacyToken(PrivacyTokenUpdate {
        sender,
        sender_lid,
        tokens_node,
    })
}

/// Parse a `newsletter` notification (live updates).
///
/// Contains `<live_updates>` with message children.
/// Mirrors whatsmeow `handleNewsletterNotification`.
pub fn parse_newsletter_notification(node: &Node) -> NotificationAction {
    let jid = node.attrs().optional_jid("from").unwrap_or_default();
    let time = node
        .attrs()
        .optional_u64("t")
        .and_then(|t| Utc.timestamp_opt(t as i64, 0).single())
        .unwrap_or_else(Utc::now);

    let live_updates = match node.get_optional_child("live_updates") {
        Some(lu) => lu,
        None => {
            return NotificationAction::Ignored {
                notification_type: "newsletter".to_string(),
            };
        }
    };

    let messages = crate::newsletter::parse_newsletter_messages(live_updates);

    NotificationAction::NewsletterLiveUpdate(NewsletterLiveUpdate {
        jid,
        time,
        messages,
    })
}

/// Parse a `mex` notification (newsletter join/leave/mute via GraphQL push).
///
/// Contains `<update>` children with JSON payloads.
/// Mirrors whatsmeow `handleMexNotification`.
pub fn parse_mex_notification(node: &Node) -> NotificationAction {
    let mut events = Vec::new();

    if let Some(children) = node.children() {
        for child in children {
            if child.tag != "update" {
                continue;
            }

            let json_bytes: &[u8] = match &child.content {
                Some(NodeContent::Bytes(b)) => b.as_slice(),
                Some(NodeContent::String(s)) => s.as_bytes(),
                _ => continue,
            };

            // Deserialize the MEX GraphQL push event wrapper
            if let Ok(wrapper) = serde_json::from_slice::<MexEventWrapper>(json_bytes) {
                if let Some(holder) = wrapper.data.join {
                    events.push(MexEvent::Join(NewsletterJoin { jid: holder.jid }));
                } else if let Some(holder) = wrapper.data.leave {
                    events.push(MexEvent::Leave(NewsletterLeave { jid: holder.jid }));
                } else if let Some(holder) = wrapper.data.mute_change {
                    events.push(MexEvent::MuteChange(NewsletterMuteChange {
                        jid: holder.jid,
                        mute: holder.mute,
                    }));
                }
            }
        }
    }

    NotificationAction::MexUpdate(events)
}

/// Parse a `status` notification (user about/status text change).
///
/// Mirrors whatsmeow `handleStatusNotification`.
pub fn parse_status_notification(node: &Node) -> NotificationAction {
    let jid = node.attrs().optional_jid("from").unwrap_or_default();
    let timestamp = node
        .attrs()
        .optional_u64("t")
        .and_then(|t| Utc.timestamp_opt(t as i64, 0).single())
        .unwrap_or_else(Utc::now);

    let status = node
        .get_optional_child("set")
        .and_then(|set_node| match &set_node.content {
            Some(NodeContent::Bytes(b)) => String::from_utf8(b.clone()).ok(),
            Some(NodeContent::String(s)) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();

    NotificationAction::StatusUpdate(StatusUpdate {
        jid,
        timestamp,
        status,
    })
}

/// Parse a `lid_migration` notification.
///
/// Contains mappings from LID to PN (phone number) for contacts
/// migrating to the LID addressing scheme.
pub fn parse_lid_migration_notification(node: &Node) -> NotificationAction {
    let mut mappings = Vec::new();

    if let Some(children) = node.children() {
        for child in children {
            if child.tag != "mapping" {
                continue;
            }

            let lid = child
                .attrs()
                .optional_string("lid")
                .unwrap_or("")
                .to_string();
            let pn = child
                .attrs()
                .optional_string("pn")
                .unwrap_or("")
                .to_string();

            if !lid.is_empty() && !pn.is_empty() {
                mappings.push(LidPnMapping { lid, pn });
            }
        }
    }

    NotificationAction::LidMigration { mappings }
}

// ── MEX JSON types ────────────────────────────────────────────────────────

/// Wrapper for MEX push event JSON.
#[derive(Debug, serde::Deserialize)]
struct MexEventWrapper {
    data: MexEventData,
}

/// Intermediate JID container for MEX JSON deserialization.
///
/// MEX sends JIDs as plain strings (e.g. `"120363001234@newsletter"`),
/// but our `Jid` type derives serde from its struct fields. This wrapper
/// parses the string JID and converts to a proper `Jid`.
#[derive(Debug, serde::Deserialize)]
struct MexJidHolder {
    #[serde(deserialize_with = "deserialize_jid_from_string")]
    jid: Jid,
}

/// Intermediate mute-change container for MEX JSON.
#[derive(Debug, serde::Deserialize)]
struct MexMuteChangeHolder {
    #[serde(deserialize_with = "deserialize_jid_from_string")]
    jid: Jid,
    mute: crate::types::newsletter::NewsletterMuteState,
}

#[derive(Debug, serde::Deserialize)]
struct MexEventData {
    #[serde(rename = "xwa2_notify_newsletter_on_join")]
    join: Option<MexJidHolder>,
    #[serde(rename = "xwa2_notify_newsletter_on_leave")]
    leave: Option<MexJidHolder>,
    #[serde(rename = "xwa2_notify_newsletter_on_mute_change")]
    mute_change: Option<MexMuteChangeHolder>,
}

/// Deserialize a JID from a JSON string like `"120363001234@newsletter"`.
fn deserialize_jid_from_string<'de, D>(deserializer: D) -> Result<Jid, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    s.parse::<Jid>().map_err(serde::de::Error::custom)
}

// ── Privacy setting change ────────────────────────────────────────────────

/// A single privacy setting change within an account_sync notification.
///
/// This is used by the events module to represent what changed.
#[derive(Debug, Clone, Serialize)]
pub struct PrivacySettingChange {
    pub category: String,
    pub old_value: String,
    pub new_value: String,
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::newsletter::NewsletterMuteState;
    use wacore_binary::builder::NodeBuilder;

    // ── Router dispatch tests ─────────────────────────────────────────

    #[test]
    fn test_router_dispatches_encrypt_type() {
        let node = NodeBuilder::new("notification")
            .attr("type", "encrypt")
            .attr("from", SERVER_JID)
            .children([NodeBuilder::new("count").attr("value", "3").build()])
            .build();

        match parse_notification(&node) {
            NotificationAction::EncryptPreKeyCount { otks_left } => {
                assert_eq!(otks_left, 3);
            }
            other => panic!("expected EncryptPreKeyCount, got {other:?}"),
        }
    }

    #[test]
    fn test_router_dispatches_server_sync_type() {
        let node = NodeBuilder::new("notification")
            .attr("type", "server_sync")
            .children([
                NodeBuilder::new("collection")
                    .attr("name", "regular_high")
                    .attr("version", "42")
                    .build(),
                NodeBuilder::new("collection")
                    .attr("name", "regular_low")
                    .attr("version", "17")
                    .build(),
            ])
            .build();

        match parse_notification(&node) {
            NotificationAction::ServerSync { collections } => {
                assert_eq!(collections.len(), 2);
                assert_eq!(collections[0].name, "regular_high");
                assert_eq!(collections[0].version, 42);
                assert_eq!(collections[1].name, "regular_low");
                assert_eq!(collections[1].version, 17);
            }
            other => panic!("expected ServerSync, got {other:?}"),
        }
    }

    #[test]
    fn test_router_dispatches_picture_type() {
        let node = NodeBuilder::new("notification")
            .attr("type", "picture")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("add")
                .attr("jid", "123@s.whatsapp.net")
                .attr("id", "pic_abc")
                .build()])
            .build();

        match parse_notification(&node) {
            NotificationAction::PictureUpdate(updates) => {
                assert_eq!(updates.len(), 1);
                assert_eq!(updates[0].jid.user, "123");
            }
            other => panic!("expected PictureUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_router_dispatches_link_code_companion_reg() {
        let node = NodeBuilder::new("notification")
            .attr("type", "link_code_companion_reg")
            .build();

        assert!(matches!(
            parse_notification(&node),
            NotificationAction::LinkCodeCompanionReg
        ));
    }

    #[test]
    fn test_router_unknown_type_returns_ignored() {
        let node = NodeBuilder::new("notification")
            .attr("type", "some_future_type")
            .build();

        match parse_notification(&node) {
            NotificationAction::Ignored {
                notification_type, ..
            } => {
                assert_eq!(notification_type, "some_future_type");
            }
            other => panic!("expected Ignored, got {other:?}"),
        }
    }

    #[test]
    fn test_router_missing_type_returns_ignored() {
        let node = NodeBuilder::new("notification").build();

        assert!(matches!(
            parse_notification(&node),
            NotificationAction::Ignored { .. }
        ));
    }

    // ── Encrypt notification tests ────────────────────────────────────

    #[test]
    fn test_encrypt_server_prekey_count() {
        let node = NodeBuilder::new("notification")
            .attr("type", "encrypt")
            .attr("from", SERVER_JID)
            .children([NodeBuilder::new("count").attr("value", "7").build()])
            .build();

        match parse_encrypt_notification(&node) {
            NotificationAction::EncryptPreKeyCount { otks_left } => {
                assert_eq!(otks_left, 7);
            }
            other => panic!("expected EncryptPreKeyCount, got {other:?}"),
        }
    }

    #[test]
    fn test_encrypt_server_missing_count_defaults_to_zero() {
        let node = NodeBuilder::new("notification")
            .attr("type", "encrypt")
            .attr("from", SERVER_JID)
            .build();

        match parse_encrypt_notification(&node) {
            NotificationAction::EncryptPreKeyCount { otks_left } => {
                assert_eq!(otks_left, 0);
            }
            other => panic!("expected EncryptPreKeyCount, got {other:?}"),
        }
    }

    #[test]
    fn test_encrypt_identity_change() {
        let node = NodeBuilder::new("notification")
            .attr("type", "encrypt")
            .attr("from", "5511999887766@s.whatsapp.net")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("identity").build()])
            .build();

        match parse_encrypt_notification(&node) {
            NotificationAction::EncryptIdentityChange(change) => {
                assert_eq!(change.jid.user, "5511999887766");
                assert_eq!(
                    change.timestamp,
                    Utc.timestamp_opt(1713100000, 0).unwrap()
                );
            }
            other => panic!("expected EncryptIdentityChange, got {other:?}"),
        }
    }

    #[test]
    fn test_encrypt_unknown_from_user_no_identity() {
        let node = NodeBuilder::new("notification")
            .attr("type", "encrypt")
            .attr("from", "5511999887766@s.whatsapp.net")
            .build();

        assert!(matches!(
            parse_encrypt_notification(&node),
            NotificationAction::Ignored { .. }
        ));
    }

    // ── Server sync tests ─────────────────────────────────────────────

    #[test]
    fn test_server_sync_empty_collections() {
        let node = NodeBuilder::new("notification")
            .attr("type", "server_sync")
            .build();

        match parse_server_sync_notification(&node) {
            NotificationAction::ServerSync { collections } => {
                assert!(collections.is_empty());
            }
            other => panic!("expected ServerSync, got {other:?}"),
        }
    }

    #[test]
    fn test_server_sync_multiple_collections() {
        let node = NodeBuilder::new("notification")
            .attr("type", "server_sync")
            .children([
                NodeBuilder::new("collection")
                    .attr("name", "critical_block")
                    .attr("version", "100")
                    .build(),
                NodeBuilder::new("collection")
                    .attr("name", "regular")
                    .attr("version", "50")
                    .build(),
                NodeBuilder::new("collection")
                    .attr("name", "critical_unblock_low")
                    .build(), // no version
            ])
            .build();

        match parse_server_sync_notification(&node) {
            NotificationAction::ServerSync { collections } => {
                assert_eq!(collections.len(), 3);
                assert_eq!(collections[0].name, "critical_block");
                assert_eq!(collections[0].version, 100);
                assert_eq!(collections[2].version, 0); // default
            }
            other => panic!("expected ServerSync, got {other:?}"),
        }
    }

    // ── Account sync tests ────────────────────────────────────────────

    #[test]
    fn test_account_sync_privacy_child() {
        let node = NodeBuilder::new("notification")
            .attr("type", "account_sync")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("privacy")
                .children([NodeBuilder::new("category")
                    .attr("name", "last")
                    .attr("value", "contacts")
                    .build()])
                .build()])
            .build();

        match parse_account_sync_notification(&node) {
            NotificationAction::AccountSync(items) => {
                assert_eq!(items.len(), 1);
                assert!(matches!(&items[0], AccountSyncItem::Privacy(_)));
            }
            other => panic!("expected AccountSync, got {other:?}"),
        }
    }

    #[test]
    fn test_account_sync_blocklist_child() {
        let node = NodeBuilder::new("notification")
            .attr("type", "account_sync")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("blocklist")
                .attr("dhash", "abc123")
                .attr("action", "modify")
                .children([NodeBuilder::new("item")
                    .attr("jid", "5511999887766@s.whatsapp.net")
                    .attr("action", "block")
                    .build()])
                .build()])
            .build();

        match parse_account_sync_notification(&node) {
            NotificationAction::AccountSync(items) => {
                assert_eq!(items.len(), 1);
                if let AccountSyncItem::Blocklist(bl) = &items[0] {
                    assert_eq!(bl.dhash, "abc123");
                    assert_eq!(bl.action, BlocklistAction::Modify);
                    assert_eq!(bl.changes.len(), 1);
                    assert_eq!(bl.changes[0].action, BlocklistChangeAction::Block);
                } else {
                    panic!("expected Blocklist item");
                }
            }
            other => panic!("expected AccountSync, got {other:?}"),
        }
    }

    #[test]
    fn test_account_sync_multiple_children() {
        let node = NodeBuilder::new("notification")
            .attr("type", "account_sync")
            .attr("t", "1713100000")
            .children([
                NodeBuilder::new("privacy").build(),
                NodeBuilder::new("devices").build(),
                NodeBuilder::new("picture").build(),
                NodeBuilder::new("unknown_tag").build(),
            ])
            .build();

        match parse_account_sync_notification(&node) {
            NotificationAction::AccountSync(items) => {
                assert_eq!(items.len(), 4);
                assert!(matches!(&items[0], AccountSyncItem::Privacy(_)));
                assert!(matches!(&items[1], AccountSyncItem::Devices(_)));
                assert!(matches!(&items[2], AccountSyncItem::Picture { .. }));
                assert!(matches!(&items[3], AccountSyncItem::Unknown { tag } if tag == "unknown_tag"));
            }
            other => panic!("expected AccountSync, got {other:?}"),
        }
    }

    // ── Device notification tests ─────────────────────────────────────

    #[test]
    fn test_device_add_operation() {
        let node = NodeBuilder::new("notification")
            .attr("type", "devices")
            .attr("from", "1234567890@s.whatsapp.net")
            .children([NodeBuilder::new("add")
                .attr("device_hash", "abc123")
                .children([
                    NodeBuilder::new("device")
                        .attr("jid", "1234567890:1@s.whatsapp.net")
                        .build(),
                ])
                .build()])
            .build();

        match parse_device_notification(&node) {
            NotificationAction::DeviceListChange(change) => {
                assert_eq!(change.from.user, "1234567890");
                assert_eq!(change.operations.len(), 1);
                assert!(matches!(&change.operations[0], DeviceOperation::Add { device_hash, .. } if device_hash == "abc123"));
            }
            other => panic!("expected DeviceListChange, got {other:?}"),
        }
    }

    #[test]
    fn test_device_remove_operation() {
        let node = NodeBuilder::new("notification")
            .attr("type", "devices")
            .attr("from", "1234567890@s.whatsapp.net")
            .children([NodeBuilder::new("remove")
                .attr("device_hash", "def456")
                .children([
                    NodeBuilder::new("device")
                        .attr("jid", "1234567890:3@s.whatsapp.net")
                        .build(),
                ])
                .build()])
            .build();

        match parse_device_notification(&node) {
            NotificationAction::DeviceListChange(change) => {
                assert_eq!(change.operations.len(), 1);
                assert!(matches!(&change.operations[0], DeviceOperation::Remove { .. }));
            }
            other => panic!("expected DeviceListChange, got {other:?}"),
        }
    }

    #[test]
    fn test_device_update_operation() {
        let node = NodeBuilder::new("notification")
            .attr("type", "devices")
            .attr("from", "1234567890@s.whatsapp.net")
            .children([NodeBuilder::new("update")
                .attr("hash", "2:abcdef")
                .build()])
            .build();

        match parse_device_notification(&node) {
            NotificationAction::DeviceListChange(change) => {
                assert_eq!(change.operations.len(), 1);
                assert!(matches!(&change.operations[0], DeviceOperation::Update));
            }
            other => panic!("expected DeviceListChange, got {other:?}"),
        }
    }

    #[test]
    fn test_device_with_lid_mapping() {
        let node = NodeBuilder::new("notification")
            .attr("type", "devices")
            .attr("from", "1234567890@s.whatsapp.net")
            .attr("lid", "100000001@lid")
            .children([NodeBuilder::new("add")
                .attr("device_hash", "abc")
                .children([
                    NodeBuilder::new("device")
                        .attr("jid", "1234567890:1@s.whatsapp.net")
                        .attr("lid", "100000001:1@lid")
                        .build(),
                ])
                .build()])
            .build();

        match parse_device_notification(&node) {
            NotificationAction::DeviceListChange(change) => {
                assert!(change.lid.is_some());
                assert_eq!(change.lid.as_ref().unwrap().user, "100000001");
                if let DeviceOperation::Add {
                    device_lid,
                    ..
                } = &change.operations[0]
                {
                    assert!(device_lid.is_some());
                } else {
                    panic!("expected Add operation");
                }
            }
            other => panic!("expected DeviceListChange, got {other:?}"),
        }
    }

    // ── Group notification tests ──────────────────────────────────────

    #[test]
    fn test_group_participant_add() {
        let node = NodeBuilder::new("notification")
            .attr("type", "w:gp2")
            .attr("from", "120363001234@g.us")
            .attr("participant", "5511999887766@s.whatsapp.net")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("add")
                .children([
                    NodeBuilder::new("participant")
                        .attr("jid", "5511888776655@s.whatsapp.net")
                        .build(),
                    NodeBuilder::new("participant")
                        .attr("jid", "5511777665544@s.whatsapp.net")
                        .build(),
                ])
                .build()])
            .build();

        match parse_group_notification(&node) {
            NotificationAction::GroupInfoChange(gn) => {
                assert_eq!(gn.group_jid.user, "120363001234");
                assert!(gn.participant.is_some());
                if let GroupChange::ParticipantsAdd { participants } = &gn.change {
                    assert_eq!(participants.len(), 2);
                } else {
                    panic!("expected ParticipantsAdd");
                }
            }
            other => panic!("expected GroupInfoChange, got {other:?}"),
        }
    }

    #[test]
    fn test_group_participant_remove() {
        let node = NodeBuilder::new("notification")
            .attr("type", "w:gp2")
            .attr("from", "120363001234@g.us")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("remove")
                .children([NodeBuilder::new("participant")
                    .attr("jid", "5511888776655@s.whatsapp.net")
                    .build()])
                .build()])
            .build();

        match parse_group_notification(&node) {
            NotificationAction::GroupInfoChange(gn) => {
                if let GroupChange::ParticipantsRemove { participants } = &gn.change {
                    assert_eq!(participants.len(), 1);
                } else {
                    panic!("expected ParticipantsRemove");
                }
            }
            other => panic!("expected GroupInfoChange, got {other:?}"),
        }
    }

    #[test]
    fn test_group_subject_change() {
        let node = NodeBuilder::new("notification")
            .attr("type", "w:gp2")
            .attr("from", "120363001234@g.us")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("subject")
                .attr("subject", "New Group Name")
                .attr("s_o", "Old Group Name")
                .build()])
            .build();

        match parse_group_notification(&node) {
            NotificationAction::GroupInfoChange(gn) => {
                if let GroupChange::SubjectChange {
                    new_subject,
                    old_subject,
                } = &gn.change
                {
                    assert_eq!(new_subject, "New Group Name");
                    assert_eq!(old_subject.as_deref(), Some("Old Group Name"));
                } else {
                    panic!("expected SubjectChange");
                }
            }
            other => panic!("expected GroupInfoChange, got {other:?}"),
        }
    }

    #[test]
    fn test_group_announce_mode() {
        let node = NodeBuilder::new("notification")
            .attr("type", "w:gp2")
            .attr("from", "120363001234@g.us")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("announcement").build()])
            .build();

        match parse_group_notification(&node) {
            NotificationAction::GroupInfoChange(gn) => {
                assert!(matches!(&gn.change, GroupChange::Announce(true)));
            }
            other => panic!("expected GroupInfoChange, got {other:?}"),
        }
    }

    #[test]
    fn test_group_ephemeral_timer() {
        let node = NodeBuilder::new("notification")
            .attr("type", "w:gp2")
            .attr("from", "120363001234@g.us")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("ephemeral")
                .attr("expiration", "86400")
                .build()])
            .build();

        match parse_group_notification(&node) {
            NotificationAction::GroupInfoChange(gn) => {
                if let GroupChange::Ephemeral { expiration } = &gn.change {
                    assert_eq!(*expiration, 86400);
                } else {
                    panic!("expected Ephemeral");
                }
            }
            other => panic!("expected GroupInfoChange, got {other:?}"),
        }
    }

    // ── Picture notification tests ────────────────────────────────────

    #[test]
    fn test_picture_add() {
        let node = NodeBuilder::new("notification")
            .attr("type", "picture")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("add")
                .attr("jid", "5511999887766@s.whatsapp.net")
                .attr("id", "pic_id_123")
                .build()])
            .build();

        match parse_picture_notification(&node) {
            NotificationAction::PictureUpdate(updates) => {
                assert_eq!(updates.len(), 1);
                assert_eq!(updates[0].jid.user, "5511999887766");
                let photo = updates[0].photo_change.as_ref().unwrap();
                assert_eq!(
                    photo.new_photo.as_deref(),
                    Some(b"pic_id_123".as_slice())
                );
            }
            other => panic!("expected PictureUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_picture_delete() {
        let node = NodeBuilder::new("notification")
            .attr("type", "picture")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("delete")
                .attr("jid", "5511999887766@s.whatsapp.net")
                .build()])
            .build();

        match parse_picture_notification(&node) {
            NotificationAction::PictureUpdate(updates) => {
                assert_eq!(updates.len(), 1);
                let photo = updates[0].photo_change.as_ref().unwrap();
                assert!(photo.new_photo.is_none());
            }
            other => panic!("expected PictureUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_picture_set() {
        let node = NodeBuilder::new("notification")
            .attr("type", "picture")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("set")
                .attr("jid", "5511999887766@s.whatsapp.net")
                .attr("id", "pic_set_456")
                .build()])
            .build();

        match parse_picture_notification(&node) {
            NotificationAction::PictureUpdate(updates) => {
                assert_eq!(updates.len(), 1);
                let photo = updates[0].photo_change.as_ref().unwrap();
                assert_eq!(
                    photo.new_photo.as_deref(),
                    Some(b"pic_set_456".as_slice())
                );
            }
            other => panic!("expected PictureUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_picture_unknown_tag_skipped() {
        let node = NodeBuilder::new("notification")
            .attr("type", "picture")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("unknown_op")
                .attr("jid", "5511999887766@s.whatsapp.net")
                .build()])
            .build();

        match parse_picture_notification(&node) {
            NotificationAction::PictureUpdate(updates) => {
                assert!(updates.is_empty());
            }
            other => panic!("expected PictureUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_picture_multiple_changes() {
        let node = NodeBuilder::new("notification")
            .attr("type", "picture")
            .attr("t", "1713100000")
            .children([
                NodeBuilder::new("add")
                    .attr("jid", "111@s.whatsapp.net")
                    .attr("id", "pic1")
                    .build(),
                NodeBuilder::new("delete")
                    .attr("jid", "222@s.whatsapp.net")
                    .build(),
                NodeBuilder::new("set")
                    .attr("jid", "333@s.whatsapp.net")
                    .attr("id", "pic3")
                    .build(),
            ])
            .build();

        match parse_picture_notification(&node) {
            NotificationAction::PictureUpdate(updates) => {
                assert_eq!(updates.len(), 3);
            }
            other => panic!("expected PictureUpdate, got {other:?}"),
        }
    }

    // ── Privacy token tests ───────────────────────────────────────────

    #[test]
    fn test_privacy_token_with_tokens() {
        let node = NodeBuilder::new("notification")
            .attr("type", "privacy_token")
            .attr("from", "5511999887766@s.whatsapp.net")
            .attr("sender_lid", "100000001@lid")
            .children([NodeBuilder::new("tokens")
                .children([NodeBuilder::new("token")
                    .attr("type", "trusted_contact")
                    .attr("t", "1707000000")
                    .bytes(vec![0xCA, 0xFE])
                    .build()])
                .build()])
            .build();

        match parse_privacy_token_notification(&node) {
            NotificationAction::PrivacyToken(update) => {
                assert_eq!(update.sender.user, "5511999887766");
                assert_eq!(update.sender_lid.as_deref(), Some("100000001@lid"));
                assert_eq!(update.tokens_node.tag, "tokens");
            }
            other => panic!("expected PrivacyToken, got {other:?}"),
        }
    }

    #[test]
    fn test_privacy_token_missing_tokens_node() {
        let node = NodeBuilder::new("notification")
            .attr("type", "privacy_token")
            .attr("from", "5511999887766@s.whatsapp.net")
            .build();

        assert!(matches!(
            parse_privacy_token_notification(&node),
            NotificationAction::Ignored { .. }
        ));
    }

    // ── Newsletter notification tests ─────────────────────────────────

    #[test]
    fn test_newsletter_live_update() {
        let node = NodeBuilder::new("notification")
            .attr("type", "newsletter")
            .attr("from", "120363001234@newsletter")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("live_updates")
                .children([NodeBuilder::new("message")
                    .attr("server_id", "42")
                    .attr("id", "msg_abc")
                    .attr("type", "text")
                    .attr("t", "1713100000")
                    .build()])
                .build()])
            .build();

        match parse_newsletter_notification(&node) {
            NotificationAction::NewsletterLiveUpdate(update) => {
                assert_eq!(update.jid.user, "120363001234");
                assert_eq!(update.messages.len(), 1);
                assert_eq!(update.messages[0].message_server_id, 42);
            }
            other => panic!("expected NewsletterLiveUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_newsletter_missing_live_updates() {
        let node = NodeBuilder::new("notification")
            .attr("type", "newsletter")
            .attr("from", "120363001234@newsletter")
            .attr("t", "1713100000")
            .build();

        assert!(matches!(
            parse_newsletter_notification(&node),
            NotificationAction::Ignored { .. }
        ));
    }

    // ── MEX notification tests ────────────────────────────────────────

    #[test]
    fn test_mex_join_event() {
        let json_payload = r#"{"data":{"xwa2_notify_newsletter_on_join":{"jid":"120363001234@newsletter"}}}"#;

        let node = NodeBuilder::new("notification")
            .attr("type", "mex")
            .children([NodeBuilder::new("update")
                .bytes(json_payload.as_bytes().to_vec())
                .build()])
            .build();

        match parse_mex_notification(&node) {
            NotificationAction::MexUpdate(events) => {
                assert_eq!(events.len(), 1);
                assert!(matches!(&events[0], MexEvent::Join(_)));
            }
            other => panic!("expected MexUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_mex_leave_event() {
        let json_payload = r#"{"data":{"xwa2_notify_newsletter_on_leave":{"jid":"120363001234@newsletter"}}}"#;

        let node = NodeBuilder::new("notification")
            .attr("type", "mex")
            .children([NodeBuilder::new("update")
                .bytes(json_payload.as_bytes().to_vec())
                .build()])
            .build();

        match parse_mex_notification(&node) {
            NotificationAction::MexUpdate(events) => {
                assert_eq!(events.len(), 1);
                assert!(matches!(&events[0], MexEvent::Leave(_)));
            }
            other => panic!("expected MexUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_mex_mute_change_event() {
        let json_payload = r#"{"data":{"xwa2_notify_newsletter_on_mute_change":{"jid":"120363001234@newsletter","mute":"on"}}}"#;

        let node = NodeBuilder::new("notification")
            .attr("type", "mex")
            .children([NodeBuilder::new("update")
                .bytes(json_payload.as_bytes().to_vec())
                .build()])
            .build();

        match parse_mex_notification(&node) {
            NotificationAction::MexUpdate(events) => {
                assert_eq!(events.len(), 1);
                if let MexEvent::MuteChange(mc) = &events[0] {
                    assert_eq!(mc.mute, NewsletterMuteState::On);
                } else {
                    panic!("expected MuteChange");
                }
            }
            other => panic!("expected MexUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_mex_empty_update() {
        let node = NodeBuilder::new("notification")
            .attr("type", "mex")
            .build();

        match parse_mex_notification(&node) {
            NotificationAction::MexUpdate(events) => {
                assert!(events.is_empty());
            }
            other => panic!("expected MexUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_mex_skips_non_update_children() {
        let node = NodeBuilder::new("notification")
            .attr("type", "mex")
            .children([NodeBuilder::new("something_else").build()])
            .build();

        match parse_mex_notification(&node) {
            NotificationAction::MexUpdate(events) => {
                assert!(events.is_empty());
            }
            other => panic!("expected MexUpdate, got {other:?}"),
        }
    }

    // ── Status notification tests ─────────────────────────────────────

    #[test]
    fn test_status_notification_with_set() {
        let node = NodeBuilder::new("notification")
            .attr("type", "status")
            .attr("from", "5511999887766@s.whatsapp.net")
            .attr("t", "1713100000")
            .children([NodeBuilder::new("set")
                .bytes("Hello World!".as_bytes().to_vec())
                .build()])
            .build();

        match parse_status_notification(&node) {
            NotificationAction::StatusUpdate(update) => {
                assert_eq!(update.jid.user, "5511999887766");
                assert_eq!(update.status, "Hello World!");
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_status_notification_missing_set() {
        let node = NodeBuilder::new("notification")
            .attr("type", "status")
            .attr("from", "5511999887766@s.whatsapp.net")
            .attr("t", "1713100000")
            .build();

        match parse_status_notification(&node) {
            NotificationAction::StatusUpdate(update) => {
                assert!(update.status.is_empty());
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    // ── LID migration tests ──────────────────────────────────────────

    #[test]
    fn test_lid_migration_with_mappings() {
        let node = NodeBuilder::new("notification")
            .attr("type", "lid_migration")
            .children([
                NodeBuilder::new("mapping")
                    .attr("lid", "100000001")
                    .attr("pn", "5511999887766")
                    .build(),
                NodeBuilder::new("mapping")
                    .attr("lid", "100000002")
                    .attr("pn", "5511888776655")
                    .build(),
            ])
            .build();

        match parse_lid_migration_notification(&node) {
            NotificationAction::LidMigration { mappings } => {
                assert_eq!(mappings.len(), 2);
                assert_eq!(mappings[0].lid, "100000001");
                assert_eq!(mappings[0].pn, "5511999887766");
                assert_eq!(mappings[1].lid, "100000002");
                assert_eq!(mappings[1].pn, "5511888776655");
            }
            other => panic!("expected LidMigration, got {other:?}"),
        }
    }

    #[test]
    fn test_lid_migration_empty() {
        let node = NodeBuilder::new("notification")
            .attr("type", "lid_migration")
            .build();

        match parse_lid_migration_notification(&node) {
            NotificationAction::LidMigration { mappings } => {
                assert!(mappings.is_empty());
            }
            other => panic!("expected LidMigration, got {other:?}"),
        }
    }

    #[test]
    fn test_lid_migration_skips_incomplete_mappings() {
        let node = NodeBuilder::new("notification")
            .attr("type", "lid_migration")
            .children([
                NodeBuilder::new("mapping")
                    .attr("lid", "100000001")
                    // missing pn
                    .build(),
                NodeBuilder::new("mapping")
                    // missing lid
                    .attr("pn", "5511888776655")
                    .build(),
                NodeBuilder::new("mapping")
                    .attr("lid", "100000003")
                    .attr("pn", "5511777665544")
                    .build(),
            ])
            .build();

        match parse_lid_migration_notification(&node) {
            NotificationAction::LidMigration { mappings } => {
                assert_eq!(mappings.len(), 1);
                assert_eq!(mappings[0].lid, "100000003");
            }
            other => panic!("expected LidMigration, got {other:?}"),
        }
    }

    // ── Blocklist parsing tests ───────────────────────────────────────

    #[test]
    fn test_blocklist_default_action() {
        let node = NodeBuilder::new("blocklist")
            .attr("dhash", "hash123")
            .children([
                NodeBuilder::new("item")
                    .attr("jid", "111@s.whatsapp.net")
                    .attr("action", "block")
                    .build(),
                NodeBuilder::new("item")
                    .attr("jid", "222@s.whatsapp.net")
                    .attr("action", "unblock")
                    .build(),
            ])
            .build();

        let bl = parse_blocklist_node(&node);
        assert_eq!(bl.action, BlocklistAction::Default);
        assert_eq!(bl.dhash, "hash123");
        assert!(bl.prev_dhash.is_none());
        assert_eq!(bl.changes.len(), 2);
        assert_eq!(bl.changes[0].action, BlocklistChangeAction::Block);
        assert_eq!(bl.changes[1].action, BlocklistChangeAction::Unblock);
    }

    #[test]
    fn test_blocklist_modify_with_prev_dhash() {
        let node = NodeBuilder::new("blocklist")
            .attr("dhash", "new_hash")
            .attr("prev_dhash", "old_hash")
            .attr("action", "modify")
            .build();

        let bl = parse_blocklist_node(&node);
        assert_eq!(bl.action, BlocklistAction::Modify);
        assert_eq!(bl.prev_dhash.as_deref(), Some("old_hash"));
        assert!(bl.changes.is_empty());
    }
}
