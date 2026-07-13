//! Receipt handling for WhatsApp message delivery, read, and played receipts.
//!
//! Ports whatsmeow/receipt.go — handles incoming receipt parsing, outgoing read
//! receipts (MarkRead), message acknowledgements (ack/nack), and delivery receipt
//! generation after message decryption.
//!
//! Wire format:
//! ```xml
//! <!-- Incoming receipt -->
//! <receipt id="MSG_ID" from="CHAT_JID" type="read" t="1713100000"
//!          participant="SENDER_JID" recipient="RECIPIENT_JID">
//!   <list>
//!     <item id="MSG_ID_2"/>
//!     <item id="MSG_ID_3"/>
//!   </list>
//! </receipt>
//!
//! <!-- Outgoing read receipt -->
//! <receipt id="MSG_ID" type="read" to="CHAT_JID" t="1713100000"
//!          participant="SENDER_JID">
//!   <list>
//!     <item id="MSG_ID_2"/>
//!   </list>
//! </receipt>
//!
//! <!-- Acknowledgement -->
//! <ack class="receipt" id="MSG_ID" to="FROM_JID" participant="..." type="..."/>
//! ```

use crate::types::events::Receipt;
use crate::types::message::MessageSource;
use crate::types::presence::ReceiptType;
use anyhow::{Result, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, JidExt, MessageId};
use wacore_binary::node::Node;

// ── Nack error codes ────────────────────────────────────────────────────────

/// Error codes sent in `<ack error="...">` to indicate why a stanza was rejected.
/// Matches whatsmeow's nack constants.
pub mod nack {
    pub const PARSING_ERROR: u16 = 487;
    pub const UNRECOGNIZED_STANZA: u16 = 488;
    pub const UNRECOGNIZED_STANZA_CLASS: u16 = 489;
    pub const UNRECOGNIZED_STANZA_TYPE: u16 = 490;
    pub const INVALID_PROTOBUF: u16 = 491;
    pub const INVALID_HOSTED_COMPANION_STANZA: u16 = 493;
    pub const MISSING_MESSAGE_SECRET: u16 = 495;
    pub const SIGNAL_ERROR_OLD_COUNTER: u16 = 496;
    pub const MESSAGE_DELETED_ON_PEER: u16 = 499;
    pub const UNHANDLED_ERROR: u16 = 500;
    pub const UNSUPPORTED_ADMIN_REVOKE: u16 = 550;
    pub const UNSUPPORTED_LID_GROUP: u16 = 551;
    pub const DB_OPERATION_FAILED: u16 = 552;
}

// ── Ack/Nack building ───────────────────────────────────────────────────────

/// Build an acknowledgement node for an incoming stanza.
///
/// Mirrors whatsmeow's `sendAck`. The `error` field is `0` for a normal ack,
/// or one of the `nack::*` constants for a negative ack.
pub fn build_ack(node: &Node, error: u16) -> Node {
    let mut builder = NodeBuilder::new("ack")
        .attr("class", node.tag.as_str())
        .attr(
            "id",
            node.attrs
                .get("id")
                .map(|v| v.to_string_value())
                .unwrap_or_default(),
        )
        .attr(
            "to",
            node.attrs
                .get("from")
                .map(|v| v.to_string_value())
                .unwrap_or_default(),
        );

    if let Some(participant) = node.attrs.get("participant") {
        builder = builder.attr("participant", participant.to_string_value());
    }

    if let Some(recipient) = node.attrs.get("recipient") {
        builder = builder.attr("recipient", recipient.to_string_value());
    }

    // receipt-type acks carry the type forward; message acks do not.
    if node.tag != "message" {
        if let Some(receipt_type) = node.attrs.get("type") {
            builder = builder.attr("type", receipt_type.to_string_value());
        }
    }

    if error != 0 {
        builder = builder.attr("error", error.to_string());
    }

    builder.build()
}

// ── Incoming receipt parsing ────────────────────────────────────────────────

/// Parse a `<receipt>` node into a `Receipt` event.
///
/// For group receipts without a sender, the node contains `<participants>`
/// children — this function returns `None` and the caller should use
/// `parse_grouped_receipt` to extract individual per-sender receipts.
pub fn parse_receipt(
    node: &Node,
    own_jid: &Jid,
) -> Result<Option<Receipt>> {
    let parser = wacore_binary::attrs::AttrParser::new(node);

    let source = parse_message_source(node, own_jid, false)?;

    let timestamp = parser
        .optional_i64("t")
        .map(|t| Utc.timestamp_opt(t, 0).single())
        .flatten()
        .unwrap_or_else(Utc::now);

    let receipt_type: ReceiptType = parser
        .optional_string("type")
        .unwrap_or("")
        .to_string()
        .into();

    let message_sender = parser
        .optional_string("recipient")
        .and_then(|s| Jid::try_from(s).ok())
        .unwrap_or_default();

    // Grouped receipt: group chat without explicit sender
    if source.is_group && source.sender.is_empty() {
        let participant_nodes = node.get_children_by_tag("participants");
        if participant_nodes.is_empty() {
            return Err(anyhow!("missing <participants> in grouped receipt"));
        }
        // Caller should call parse_grouped_receipt for each participants node.
        return Ok(None);
    }

    let main_message_id: MessageId = parser
        .optional_string("id")
        .ok_or_else(|| anyhow!("missing id in receipt"))?
        .to_string();

    // Build message ID list from optional <list> children
    let mut message_ids = vec![main_message_id];

    if let Some(children) = node.children() {
        if children.len() == 1 && children[0].tag == "list" {
            if let Some(list_children) = children[0].children() {
                for item in list_children {
                    if item.tag == "item" {
                        if let Some(id) = item.attrs.get("id") {
                            message_ids.push(id.to_string_value());
                        }
                    }
                }
            }
        }
    }

    Ok(Some(Receipt {
        source,
        message_ids,
        timestamp,
        r#type: receipt_type,
        message_sender: message_sender,
    }))
}

/// Parse individual per-sender receipts from a `<participants>` node
/// within a grouped receipt.
///
/// Returns a list of receipts, one per `<user>` child node.
pub fn parse_grouped_receipt(
    base_source: &MessageSource,
    base_type: &ReceiptType,
    participants_node: &Node,
) -> Vec<Receipt> {
    let mut receipts = Vec::new();

    let key = participants_node
        .attrs
        .get("key")
        .map(|v| v.to_string_value())
        .unwrap_or_default();

    if let Some(children) = participants_node.children() {
        for child in children {
            if child.tag != "user" {
                log::warn!("unexpected node in grouped receipt participants: {}", child.tag);
                continue;
            }

            let parser = wacore_binary::attrs::AttrParser::new(child);

            let timestamp = parser
                .optional_i64("t")
                .map(|t| Utc.timestamp_opt(t, 0).single())
                .flatten()
                .unwrap_or_else(Utc::now);

            let sender = parser
                .optional_string("jid")
                .and_then(|s| Jid::try_from(s).ok())
                .unwrap_or_default();

            if sender.is_empty() {
                log::warn!("missing jid in grouped receipt user node");
                continue;
            }

            let mut source = base_source.clone();
            source.sender = sender;

            receipts.push(Receipt {
                source,
                message_ids: vec![key.clone()],
                timestamp,
                r#type: base_type.clone(),
                message_sender: Jid::default(),
            });
        }
    }

    receipts
}

// ── Outgoing read receipt (MarkRead) ────────────────────────────────────────

/// Options for sending a read receipt.
#[derive(Debug, Clone)]
pub struct MarkReadOptions {
    /// The message IDs to mark as read. Must be non-empty.
    pub ids: Vec<MessageId>,
    /// Timestamp to use as the read-at time.
    pub timestamp: DateTime<Utc>,
    /// Chat JID (user JID in DMs, group JID in groups).
    pub chat: Jid,
    /// Sender JID — must be set in group chats (the user who sent the messages).
    pub sender: Option<Jid>,
    /// Receipt type. Defaults to `Read`. Use `Played` for voice messages.
    pub receipt_type: ReceiptType,
    /// If true, force read-self instead of read (for privacy or newsletter).
    pub force_read_self: bool,
}

/// Build a `<receipt>` node for marking messages as read.
///
/// Mirrors whatsmeow's `MarkRead`. If `force_read_self` is true or the chat
/// is a newsletter, the type is downgraded to `read-self`.
pub fn build_mark_read(opts: &MarkReadOptions) -> Result<Node> {
    if opts.ids.is_empty() {
        return Err(anyhow!("no message IDs specified"));
    }

    let type_str = if opts.force_read_self {
        match &opts.receipt_type {
            ReceiptType::Read => "read-self",
            ReceiptType::Played => "played-self",
            other => receipt_type_to_str(other),
        }
    } else {
        receipt_type_to_str(&opts.receipt_type)
    };

    let mut builder = NodeBuilder::new("receipt")
        .attr("id", opts.ids[0].as_str())
        .attr("type", type_str)
        .attr("to", opts.chat.to_string())
        .attr("t", opts.timestamp.timestamp().to_string());

    // In group chats, include the participant (sender) attribute.
    if let Some(ref sender) = opts.sender {
        if !sender.is_empty()
            && !opts.chat.is_user()
            && !opts.chat.is_hidden_user()
        {
            builder = builder.attr("participant", sender.to_non_ad().to_string());
        }
    }

    // Additional message IDs go in a <list> child.
    if opts.ids.len() > 1 {
        let items: Vec<Node> = opts.ids[1..]
            .iter()
            .map(|id| NodeBuilder::new("item").attr("id", id.as_str()).build())
            .collect();

        builder = builder.children([NodeBuilder::new("list").children(items).build()]);
    }

    Ok(builder.build())
}

// ── Delivery receipt (after message decryption) ─────────────────────────────

/// Receipt type to use when acknowledging a successfully received message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryReceiptMode {
    /// Active receipt — shows two gray ticks on sender's side.
    Active,
    /// Inactive receipt — transmitted but not rendered in official apps.
    Inactive,
    /// Sender receipt — for messages we ourselves sent from another device.
    Sender,
    /// Peer message receipt — for internal protocol messages.
    PeerMsg,
}

/// Build a delivery receipt for an incoming message.
///
/// Mirrors whatsmeow's `sendMessageReceipt`. The mode determines the receipt
/// type attribute.
pub fn build_delivery_receipt(
    message_id: &str,
    incoming_node: &Node,
    is_from_me: bool,
    is_peer_msg: bool,
    force_active: bool,
) -> Node {
    let mode = if is_from_me {
        if is_peer_msg {
            DeliveryReceiptMode::PeerMsg
        } else {
            DeliveryReceiptMode::Sender
        }
    } else if force_active {
        DeliveryReceiptMode::Active
    } else {
        DeliveryReceiptMode::Inactive
    };

    let mut builder = NodeBuilder::new("receipt")
        .attr("id", message_id)
        .attr(
            "to",
            incoming_node
                .attrs
                .get("from")
                .map(|v| v.to_string_value())
                .unwrap_or_default(),
        );

    if let Some(recipient) = incoming_node.attrs.get("recipient") {
        builder = builder.attr("recipient", recipient.to_string_value());
    }

    if let Some(participant) = incoming_node.attrs.get("participant") {
        builder = builder.attr("participant", participant.to_string_value());
    }

    match mode {
        DeliveryReceiptMode::Active => {} // no type attribute = active delivery
        DeliveryReceiptMode::Inactive => {
            builder = builder.attr("type", "inactive");
        }
        DeliveryReceiptMode::Sender => {
            builder = builder.attr("type", "sender");
        }
        DeliveryReceiptMode::PeerMsg => {
            builder = builder.attr("type", "peer_msg");
        }
    }

    builder.build()
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn receipt_type_to_str(rt: &ReceiptType) -> &'static str {
    match rt {
        ReceiptType::Delivered => "",
        ReceiptType::Sender => "sender",
        ReceiptType::Retry => "retry",
        ReceiptType::Read => "read",
        ReceiptType::ReadSelf => "read-self",
        ReceiptType::Played => "played",
        ReceiptType::PlayedSelf => "played-self",
        ReceiptType::ServerError => "server-error",
        ReceiptType::Inactive => "inactive",
        ReceiptType::PeerMsg => "peer_msg",
        ReceiptType::HistorySync => "hist_sync",
        ReceiptType::Other(_) => "",
    }
}

/// Parse message source from a node's attributes.
///
/// This is a simplified version — the full version should be in the message
/// module. Used here to avoid circular dependencies.
fn parse_message_source(
    node: &Node,
    own_jid: &Jid,
    _require_participant: bool,
) -> Result<MessageSource> {
    let parser = wacore_binary::attrs::AttrParser::new(node);

    let from = parser
        .optional_string("from")
        .and_then(|s| Jid::try_from(s).ok())
        .unwrap_or_default();

    let participant = parser
        .optional_string("participant")
        .and_then(|s| Jid::try_from(s).ok());

    let recipient = parser
        .optional_string("recipient")
        .and_then(|s| Jid::try_from(s).ok());

    let is_group = from.is_group() || from.is_broadcast_list() || from.is_status_broadcast();

    let (chat, sender, is_from_me) = if is_group {
        let sender = participant.clone().unwrap_or_default();
        let is_from_me = sender.to_non_ad() == own_jid.to_non_ad();
        (from, sender, is_from_me)
    } else {
        let is_from_me = from.to_non_ad() == own_jid.to_non_ad();
        if is_from_me {
            // Outgoing DM — chat is the recipient
            let chat = recipient.clone().unwrap_or(from.clone());
            (chat, from.clone(), true)
        } else {
            (from.clone(), from.clone(), false)
        }
    };

    Ok(MessageSource {
        chat,
        sender,
        is_from_me,
        is_group,
        addressing_mode: None,
        sender_alt: None,
        recipient_alt: None,
        broadcast_list_owner: None,
        recipient,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ack_normal() {
        let incoming = NodeBuilder::new("receipt")
            .attr("id", "3EB0ABCD1234")
            .attr("from", "123456@s.whatsapp.net")
            .attr("type", "read")
            .build();

        let ack = build_ack(&incoming, 0);

        assert_eq!(ack.tag, "ack");
        assert_eq!(
            ack.attrs.get("class").map(|v| v.to_string_value()),
            Some("receipt".to_string())
        );
        assert_eq!(
            ack.attrs.get("id").map(|v| v.to_string_value()),
            Some("3EB0ABCD1234".to_string())
        );
        assert_eq!(
            ack.attrs.get("to").map(|v| v.to_string_value()),
            Some("123456@s.whatsapp.net".to_string())
        );
        assert_eq!(
            ack.attrs.get("type").map(|v| v.to_string_value()),
            Some("read".to_string())
        );
        assert!(ack.attrs.get("error").is_none());
    }

    #[test]
    fn test_build_ack_with_nack() {
        let incoming = NodeBuilder::new("message")
            .attr("id", "3EB0ABCD1234")
            .attr("from", "123456@s.whatsapp.net")
            .build();

        let ack = build_ack(&incoming, nack::INVALID_PROTOBUF);

        assert_eq!(
            ack.attrs.get("error").map(|v| v.to_string_value()),
            Some("491".to_string())
        );
        // Message acks don't carry receipt type
        assert!(ack.attrs.get("type").is_none());
    }

    #[test]
    fn test_build_mark_read_single() {
        let opts = MarkReadOptions {
            ids: vec!["3EB0MSG001".to_string()],
            timestamp: Utc.timestamp_opt(1713100000, 0).unwrap(),
            chat: Jid::try_from("123456@s.whatsapp.net").unwrap(),
            sender: None,
            receipt_type: ReceiptType::Read,
            force_read_self: false,
        };

        let node = build_mark_read(&opts).unwrap();

        assert_eq!(node.tag, "receipt");
        assert_eq!(
            node.attrs.get("type").map(|v| v.to_string_value()),
            Some("read".to_string())
        );
        // No <list> child for single message
        assert!(node.children().map_or(true, |c| c.is_empty()));
    }

    #[test]
    fn test_build_mark_read_multiple() {
        let opts = MarkReadOptions {
            ids: vec![
                "3EB0MSG001".to_string(),
                "3EB0MSG002".to_string(),
                "3EB0MSG003".to_string(),
            ],
            timestamp: Utc.timestamp_opt(1713100000, 0).unwrap(),
            chat: Jid::try_from("group@g.us").unwrap(),
            sender: Some(Jid::try_from("sender@s.whatsapp.net").unwrap()),
            receipt_type: ReceiptType::Read,
            force_read_self: false,
        };

        let node = build_mark_read(&opts).unwrap();

        // Should have participant attr for group chat
        assert!(node.attrs.get("participant").is_some());

        // Should have <list> child with 2 items (IDs 2 and 3)
        let children = node.children().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].tag, "list");
        let items = children[0].children().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_build_mark_read_force_self() {
        let opts = MarkReadOptions {
            ids: vec!["3EB0MSG001".to_string()],
            timestamp: Utc::now(),
            chat: Jid::try_from("newsletter@newsletter").unwrap(),
            sender: None,
            receipt_type: ReceiptType::Read,
            force_read_self: true,
        };

        let node = build_mark_read(&opts).unwrap();

        assert_eq!(
            node.attrs.get("type").map(|v| v.to_string_value()),
            Some("read-self".to_string())
        );
    }

    #[test]
    fn test_build_delivery_receipt_active() {
        let incoming = NodeBuilder::new("message")
            .attr("from", "sender@s.whatsapp.net")
            .build();

        let receipt = build_delivery_receipt("3EB0MSG001", &incoming, false, false, true);

        assert_eq!(receipt.tag, "receipt");
        // Active delivery = no type attribute
        assert!(receipt.attrs.get("type").is_none());
    }

    #[test]
    fn test_build_delivery_receipt_inactive() {
        let incoming = NodeBuilder::new("message")
            .attr("from", "sender@s.whatsapp.net")
            .build();

        let receipt = build_delivery_receipt("3EB0MSG001", &incoming, false, false, false);

        assert_eq!(
            receipt.attrs.get("type").map(|v| v.to_string_value()),
            Some("inactive".to_string())
        );
    }

    #[test]
    fn test_build_delivery_receipt_sender() {
        let incoming = NodeBuilder::new("message")
            .attr("from", "myphone@s.whatsapp.net")
            .build();

        let receipt = build_delivery_receipt("3EB0MSG001", &incoming, true, false, false);

        assert_eq!(
            receipt.attrs.get("type").map(|v| v.to_string_value()),
            Some("sender".to_string())
        );
    }

    #[test]
    fn test_receipt_type_to_str() {
        assert_eq!(receipt_type_to_str(&ReceiptType::Read), "read");
        assert_eq!(receipt_type_to_str(&ReceiptType::Delivered), "");
        assert_eq!(receipt_type_to_str(&ReceiptType::Played), "played");
        assert_eq!(receipt_type_to_str(&ReceiptType::Inactive), "inactive");
    }
}
