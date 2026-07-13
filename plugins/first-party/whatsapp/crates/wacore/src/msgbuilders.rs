//! Protocol-layer message builders for edits, revokes, reactions, resend
//! requests, and disappearing timer changes.
//!
//! Each function constructs the protobuf [`wa::Message`] wrapper exactly as
//! whatsmeow's `send.go` does — no I/O, no client state, pure data
//! construction.  The caller is responsible for encrypting and transmitting the
//! resulting message through the normal send path.
//!
//! # Examples
//!
//! ```
//! use wacore::msgbuilders;
//! use wacore_binary::jid::Jid;
//! use waproto::whatsapp as wa;
//!
//! let chat = Jid::new("120363000000000000", "g.us");
//! let msg  = wa::Message { conversation: Some("hello".into()), ..Default::default() };
//! let edit = msgbuilders::build_edit(&chat, "3EB0ABCD1234", msg);
//! assert!(edit.edited_message.is_some());
//! ```

use chrono::Utc;
use std::time::Duration;
use wacore_binary::jid::{Jid, JidExt as _};
use waproto::whatsapp as wa;

/// Build a message-edit wrapper.
///
/// Wraps `new_msg` inside `EditedMessage > FutureProofMessage > ProtocolMessage`
/// with type `MESSAGE_EDIT`, referencing the original message by `(chat, id)`.
///
/// The edit is always `from_me = true` because only the original sender may
/// edit their own message.
pub fn build_edit(chat: &Jid, id: &str, new_msg: wa::Message) -> wa::Message {
    wa::Message {
        edited_message: Some(Box::new(wa::message::FutureProofMessage {
            message: Some(Box::new(wa::Message {
                protocol_message: Some(Box::new(wa::message::ProtocolMessage {
                    key: Some(wa::MessageKey {
                        remote_jid: Some(chat.to_string()),
                        from_me: Some(true),
                        id: Some(id.to_owned()),
                        ..Default::default()
                    }),
                    r#type: Some(wa::message::protocol_message::Type::MessageEdit as i32),
                    edited_message: Some(Box::new(new_msg)),
                    timestamp_ms: Some(Utc::now().timestamp_millis()),
                    ..Default::default()
                })),
                ..Default::default()
            })),
        })),
        ..Default::default()
    }
}

/// Build a revoke (delete-for-everyone) message.
///
/// If `sender` is `None` (or an empty JID), the message is treated as sent by
/// the current user (`from_me = true`).  When `sender` is provided, the revoke
/// targets someone else's message in a group (`from_me = false`,
/// `participant = sender`).
pub fn build_revoke(chat: &Jid, sender: Option<&Jid>, id: &str) -> wa::Message {
    let (from_me, participant) = match sender {
        Some(s) if !s.is_empty() => (false, Some(s.to_string())),
        _ => (true, None),
    };

    wa::Message {
        protocol_message: Some(Box::new(wa::message::ProtocolMessage {
            key: Some(wa::MessageKey {
                remote_jid: Some(chat.to_string()),
                from_me: Some(from_me),
                id: Some(id.to_owned()),
                participant,
            }),
            r#type: Some(wa::message::protocol_message::Type::Revoke as i32),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Build a reaction message.
///
/// `reaction` is the emoji string.  Pass an empty string (`""`) to remove a
/// previously sent reaction.
///
/// When `sender` is `None` (or empty), the reaction targets the current user's
/// own message (`from_me = true`).  Otherwise it targets another participant's
/// message in a group.
pub fn build_reaction(
    chat: &Jid,
    sender: Option<&Jid>,
    id: &str,
    reaction: &str,
) -> wa::Message {
    let (from_me, participant) = match sender {
        Some(s) if !s.is_empty() => (false, Some(s.to_string())),
        _ => (true, None),
    };

    wa::Message {
        reaction_message: Some(wa::message::ReactionMessage {
            key: Some(wa::MessageKey {
                remote_jid: Some(chat.to_string()),
                from_me: Some(from_me),
                id: Some(id.to_owned()),
                participant,
            }),
            text: Some(reaction.to_owned()),
            sender_timestamp_ms: Some(Utc::now().timestamp_millis()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Build a request to have the phone re-send an unavailable message.
///
/// Used for placeholder messages that the companion device could not decrypt.
/// The phone will re-encrypt and send the content.
pub fn build_unavailable_message_request(
    chat: &Jid,
    _sender: &Jid,
    id: &str,
) -> wa::Message {
    use wa::message::peer_data_operation_request_message::PlaceholderMessageResendRequest;

    wa::Message {
        protocol_message: Some(Box::new(wa::message::ProtocolMessage {
            r#type: Some(
                wa::message::protocol_message::Type::PeerDataOperationRequestMessage as i32,
            ),
            peer_data_operation_request_message: Some(
                wa::message::PeerDataOperationRequestMessage {
                    peer_data_operation_request_type: Some(
                        wa::message::PeerDataOperationRequestType::PlaceholderMessageResend
                            as i32,
                    ),
                    placeholder_message_resend_request: vec![
                        PlaceholderMessageResendRequest {
                            message_key: Some(wa::MessageKey {
                                remote_jid: Some(chat.to_string()),
                                from_me: Some(false),
                                id: Some(id.to_owned()),
                                ..Default::default()
                            }),
                        },
                    ],
                    ..Default::default()
                },
            ),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Build a disappearing-timer change message.
///
/// `timer` is the ephemeral expiration duration.  Pass `Duration::ZERO` to
/// disable disappearing messages.
pub fn build_set_disappearing_timer(timer: Duration) -> wa::Message {
    #[allow(clippy::cast_possible_truncation)]
    let seconds = timer.as_secs() as u32;

    wa::Message {
        protocol_message: Some(Box::new(wa::message::ProtocolMessage {
            r#type: Some(wa::message::protocol_message::Type::EphemeralSetting as i32),
            ephemeral_expiration: Some(seconds),
            ..Default::default()
        })),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group_chat() -> Jid {
        Jid::new("120363000000000000", "g.us")
    }

    fn dm_chat() -> Jid {
        Jid::new("5511999887766", "s.whatsapp.net")
    }

    fn other_sender() -> Jid {
        Jid::new("5511888776655", "s.whatsapp.net")
    }

    const MSG_ID: &str = "3EB0ABCD12345678";

    // ── build_edit ──────────────────────────────────────────────────────

    #[test]
    fn edit_wraps_in_future_proof_message() {
        let new_msg = wa::Message {
            conversation: Some("edited text".into()),
            ..Default::default()
        };
        let result = build_edit(&dm_chat(), MSG_ID, new_msg);

        let fpm = result.edited_message.as_ref().expect("edited_message set");
        let inner = fpm.message.as_ref().expect("inner message set");
        let proto = inner
            .protocol_message
            .as_ref()
            .expect("protocol_message set");

        assert_eq!(
            proto.r#type,
            Some(wa::message::protocol_message::Type::MessageEdit as i32)
        );

        let key = proto.key.as_ref().expect("key set");
        assert_eq!(key.from_me, Some(true));
        assert_eq!(key.id.as_deref(), Some(MSG_ID));
        assert!(key.remote_jid.is_some());

        let edited = proto.edited_message.as_ref().expect("edited_message set");
        assert_eq!(edited.conversation.as_deref(), Some("edited text"));

        assert!(proto.timestamp_ms.is_some());
    }

    #[test]
    fn edit_sets_chat_jid() {
        let chat = group_chat();
        let result = build_edit(&chat, MSG_ID, wa::Message::default());

        let key = result
            .edited_message
            .as_ref()
            .unwrap()
            .message
            .as_ref()
            .unwrap()
            .protocol_message
            .as_ref()
            .unwrap()
            .key
            .as_ref()
            .unwrap();

        assert_eq!(key.remote_jid.as_deref(), Some(&*chat.to_string()));
    }

    // ── build_revoke ────────────────────────────────────────────────────

    #[test]
    fn revoke_own_message() {
        let result = build_revoke(&dm_chat(), None, MSG_ID);

        let proto = result
            .protocol_message
            .as_ref()
            .expect("protocol_message set");
        assert_eq!(
            proto.r#type,
            Some(wa::message::protocol_message::Type::Revoke as i32)
        );

        let key = proto.key.as_ref().expect("key set");
        assert_eq!(key.from_me, Some(true));
        assert_eq!(key.id.as_deref(), Some(MSG_ID));
        assert!(key.participant.is_none());
    }

    #[test]
    fn revoke_other_message_in_group() {
        let sender = other_sender();
        let result = build_revoke(&group_chat(), Some(&sender), MSG_ID);

        let key = result
            .protocol_message
            .as_ref()
            .unwrap()
            .key
            .as_ref()
            .unwrap();

        assert_eq!(key.from_me, Some(false));
        assert_eq!(key.participant.as_deref(), Some(&*sender.to_string()));
    }

    #[test]
    fn revoke_with_empty_sender_is_own_message() {
        let empty = Jid::default();
        let result = build_revoke(&dm_chat(), Some(&empty), MSG_ID);

        let key = result
            .protocol_message
            .as_ref()
            .unwrap()
            .key
            .as_ref()
            .unwrap();
        assert_eq!(key.from_me, Some(true));
        assert!(key.participant.is_none());
    }

    // ── build_reaction ──────────────────────────────────────────────────

    #[test]
    fn reaction_on_own_message() {
        let result = build_reaction(&dm_chat(), None, MSG_ID, "\u{1F44D}");

        let rxn = result
            .reaction_message
            .as_ref()
            .expect("reaction_message set");
        assert_eq!(rxn.text.as_deref(), Some("\u{1F44D}"));
        assert!(rxn.sender_timestamp_ms.is_some());

        let key = rxn.key.as_ref().expect("key set");
        assert_eq!(key.from_me, Some(true));
        assert!(key.participant.is_none());
    }

    #[test]
    fn reaction_on_other_message() {
        let sender = other_sender();
        let result = build_reaction(&group_chat(), Some(&sender), MSG_ID, "\u{2764}\u{FE0F}");

        let key = result
            .reaction_message
            .as_ref()
            .unwrap()
            .key
            .as_ref()
            .unwrap();
        assert_eq!(key.from_me, Some(false));
        assert_eq!(key.participant.as_deref(), Some(&*sender.to_string()));
    }

    #[test]
    fn remove_reaction_uses_empty_string() {
        let result = build_reaction(&dm_chat(), None, MSG_ID, "");

        let rxn = result.reaction_message.as_ref().unwrap();
        assert_eq!(rxn.text.as_deref(), Some(""));
    }

    // ── build_unavailable_message_request ───────────────────────────────

    #[test]
    fn unavailable_request_structure() {
        let sender = other_sender();
        let result = build_unavailable_message_request(&dm_chat(), &sender, MSG_ID);

        let proto = result
            .protocol_message
            .as_ref()
            .expect("protocol_message set");
        assert_eq!(
            proto.r#type,
            Some(
                wa::message::protocol_message::Type::PeerDataOperationRequestMessage as i32
            )
        );

        let peer_req = proto
            .peer_data_operation_request_message
            .as_ref()
            .expect("peer request set");
        assert_eq!(
            peer_req.peer_data_operation_request_type,
            Some(
                wa::message::PeerDataOperationRequestType::PlaceholderMessageResend as i32
            )
        );

        let resend_list = &peer_req.placeholder_message_resend_request;
        assert_eq!(resend_list.len(), 1);

        let key = resend_list[0]
            .message_key
            .as_ref()
            .expect("message_key set");
        assert_eq!(key.from_me, Some(false));
        assert_eq!(key.id.as_deref(), Some(MSG_ID));
    }

    // ── build_set_disappearing_timer ────────────────────────────────────

    #[test]
    fn disappearing_timer_24h() {
        let result = build_set_disappearing_timer(Duration::from_secs(86400));

        let proto = result
            .protocol_message
            .as_ref()
            .expect("protocol_message set");
        assert_eq!(
            proto.r#type,
            Some(wa::message::protocol_message::Type::EphemeralSetting as i32)
        );
        assert_eq!(proto.ephemeral_expiration, Some(86400));
    }

    #[test]
    fn disappearing_timer_disabled() {
        let result = build_set_disappearing_timer(Duration::ZERO);

        let proto = result.protocol_message.as_ref().unwrap();
        assert_eq!(proto.ephemeral_expiration, Some(0));
    }

    #[test]
    fn disappearing_timer_7d() {
        let result = build_set_disappearing_timer(Duration::from_secs(7 * 86400));

        let proto = result.protocol_message.as_ref().unwrap();
        assert_eq!(proto.ephemeral_expiration, Some(604_800));
    }
}
