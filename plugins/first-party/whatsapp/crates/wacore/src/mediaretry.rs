//! Media retry receipt handling for WhatsApp re-upload requests.
//!
//! Ports whatsmeow/mediaretry.go — handles sending encrypted retry receipts
//! to request media re-upload from the phone, and decrypting the notification
//! response containing the updated download path.
//!
//! ## Protocol overview
//!
//! When a media download fails (404/410), the client sends a `server-error`
//! receipt containing an encrypted `ServerErrorReceipt` protobuf. The phone
//! then re-uploads the media and sends back a notification with an encrypted
//! `MediaRetryNotification` containing the new `direct_path`.
//!
//! ## Wire format
//!
//! ```xml
//! <!-- Outgoing retry receipt -->
//! <receipt id="MSG_ID" to="OWN_JID" type="server-error">
//!   <encrypt>
//!     <enc_p>CIPHERTEXT</enc_p>
//!     <enc_iv>IV</enc_iv>
//!   </encrypt>
//!   <rmr jid="CHAT_JID" from_me="true|false" participant="SENDER_JID"/>
//! </receipt>
//!
//! <!-- Incoming retry notification (same shape, different direction) -->
//! <notification id="MSG_ID" t="TIMESTAMP" ...>
//!   <encrypt>
//!     <enc_p>CIPHERTEXT</enc_p>
//!     <enc_iv>IV</enc_iv>
//!   </encrypt>
//!   <rmr jid="CHAT_JID" from_me="true|false" participant="SENDER_JID"/>
//! </notification>
//! ```

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::Aes256Gcm;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use hkdf::Hkdf;
use prost::Message;
use rand::Rng;
use sha2::Sha256;
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, JidExt, MessageId};
use wacore_binary::node::{Node, NodeContent};
use waproto::whatsapp as wa;

use crate::types::message::MessageInfo;

/// HKDF info string used by WhatsApp for deriving the media retry encryption key.
const MEDIA_RETRY_HKDF_INFO: &[u8] = b"WhatsApp Media Retry Notification";

/// AES-GCM IV (nonce) length in bytes.
const GCM_IV_LENGTH: usize = 12;

// ── Error types ────────────────────────────────────────────────────────────

/// Error returned when the phone reports media is no longer available.
#[derive(Debug, Clone, thiserror::Error)]
#[error("media not available on phone")]
pub struct MediaNotAvailableOnPhone;

/// Error returned for unrecognized media retry error codes.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unknown media retry error (code: {code})")]
pub struct UnknownMediaRetryError {
    pub code: i64,
}

/// Error detail from an unencrypted `<error>` node in a media retry notification.
#[derive(Debug, Clone)]
pub struct MediaRetryError {
    pub code: i64,
}

// ── Event ──────────────────────────────────────────────────────────────────

/// Parsed media retry notification event.
///
/// Contains either encrypted ciphertext+IV (on success/pending) or an
/// unencrypted error. Decrypt with [`decrypt_media_retry_notification`] using
/// the original media key.
#[derive(Debug, Clone)]
pub struct MediaRetry {
    /// Encrypted payload from `<enc_p>`.
    pub ciphertext: Option<Vec<u8>>,
    /// AES-GCM IV from `<enc_iv>`.
    pub iv: Option<Vec<u8>>,
    /// Unencrypted error, if present instead of ciphertext.
    pub error: Option<MediaRetryError>,
    /// Timestamp of the notification.
    pub timestamp: DateTime<Utc>,
    /// The message ID of the original message.
    pub message_id: MessageId,
    /// The chat where the message was sent.
    pub chat_id: Jid,
    /// The sender of the original message. Only set in group chats.
    pub sender_id: Jid,
    /// Whether the message was sent by the current user.
    pub from_me: bool,
}

// ── Key derivation ─────────────────────────────────────────────────────────

/// Derive the AES-256-GCM key for media retry encryption/decryption.
///
/// Uses HKDF-SHA256 with no salt and info `"WhatsApp Media Retry Notification"`
/// to expand the media key into a 32-byte encryption key.
fn get_media_retry_key(media_key: &[u8]) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(None, media_key);
    let mut key = [0u8; 32];
    hk.expand(MEDIA_RETRY_HKDF_INFO, &mut key)
        .map_err(|_| anyhow!("HKDF expand failed for media retry key"))?;
    Ok(key)
}

// ── Encrypt ────────────────────────────────────────────────────────────────

/// Encrypt a `ServerErrorReceipt` for the media retry receipt.
///
/// Returns `(ciphertext, iv)` where the ciphertext includes the GCM auth tag.
/// The message ID is used as AES-GCM additional authenticated data (AAD).
fn encrypt_media_retry_receipt(
    message_id: &str,
    media_key: &[u8],
) -> Result<(Vec<u8>, Vec<u8>)> {
    let receipt = wa::ServerErrorReceipt {
        stanza_id: Some(message_id.to_string()),
    };
    let plaintext = receipt.encode_to_vec();

    let key = get_media_retry_key(media_key)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| anyhow!("invalid key size for AES-256-GCM"))?;

    let mut iv = [0u8; GCM_IV_LENGTH];
    rand::rng().fill(&mut iv);

    let payload = Payload {
        msg: &plaintext,
        aad: message_id.as_bytes(),
    };
    let nonce = aes_gcm::Nonce::from_slice(&iv);
    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|_| anyhow!("AES-GCM encryption failed for media retry receipt"))?;

    Ok((ciphertext, iv.to_vec()))
}

// ── Build outgoing receipt node ────────────────────────────────────────────

/// Build a `<receipt type="server-error">` node to request media re-upload.
///
/// The receipt contains an encrypted `ServerErrorReceipt` and metadata about
/// the original message in a `<rmr>` child node.
///
/// # Arguments
///
/// * `message_info` - Info about the message whose media download failed.
/// * `media_key` - The media key from the original media message.
/// * `own_jid` - The client's own JID (used as the `to` attribute).
pub fn build_media_retry_receipt(
    message_info: &MessageInfo,
    media_key: &[u8],
    own_jid: &Jid,
) -> Result<Node> {
    let (ciphertext, iv) = encrypt_media_retry_receipt(&message_info.id, media_key)
        .context("failed to prepare encrypted retry receipt")?;

    let own_jid_non_ad = own_jid.to_non_ad();
    if own_jid_non_ad.is_empty() {
        return Err(anyhow!("own JID is empty (not logged in)"));
    }

    // Build <rmr> attributes
    let mut rmr_builder = NodeBuilder::new("rmr")
        .jid_attr("jid", message_info.source.chat.clone())
        .attr(
            "from_me",
            if message_info.source.is_from_me {
                "true"
            } else {
                "false"
            },
        );

    // In group chats, include the participant (sender) attribute.
    if message_info.source.is_group {
        rmr_builder = rmr_builder.jid_attr("participant", message_info.source.sender.clone());
    }

    // Build <encrypt> with <enc_p> and <enc_iv> children
    let encrypt_node = NodeBuilder::new("encrypt")
        .children([
            NodeBuilder::new("enc_p").bytes(ciphertext).build(),
            NodeBuilder::new("enc_iv").bytes(iv).build(),
        ])
        .build();

    let receipt = NodeBuilder::new("receipt")
        .attr("id", message_info.id.as_str())
        .jid_attr("to", own_jid_non_ad)
        .attr("type", "server-error")
        .children([encrypt_node, rmr_builder.build()])
        .build();

    Ok(receipt)
}

// ── Decrypt ────────────────────────────────────────────────────────────────

/// Decrypt a media retry notification response.
///
/// Takes a parsed [`MediaRetry`] event and the original media key, and returns
/// the decrypted `MediaRetryNotification` protobuf containing the updated
/// `direct_path` for re-downloading the media.
///
/// # Errors
///
/// - [`MediaNotAvailableOnPhone`] if the phone reported error code 2.
/// - [`UnknownMediaRetryError`] for other unencrypted error codes.
/// - Decryption or protobuf parsing errors.
pub fn decrypt_media_retry_notification(
    evt: &MediaRetry,
    media_key: &[u8],
) -> Result<wa::MediaRetryNotification> {
    // Handle unencrypted error responses
    if let Some(ref error) = evt.error {
        if evt.ciphertext.is_none() {
            if error.code == 2 {
                return Err(MediaNotAvailableOnPhone.into());
            }
            return Err(UnknownMediaRetryError { code: error.code }.into());
        }
    }

    let ciphertext = evt
        .ciphertext
        .as_ref()
        .ok_or_else(|| anyhow!("missing ciphertext in media retry notification"))?;
    let iv = evt
        .iv
        .as_ref()
        .ok_or_else(|| anyhow!("missing IV in media retry notification"))?;

    let key = get_media_retry_key(media_key)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| anyhow!("invalid key size for AES-256-GCM"))?;

    let nonce = aes_gcm::Nonce::from_slice(iv);
    let payload = Payload {
        msg: ciphertext,
        aad: evt.message_id.as_bytes(),
    };
    let plaintext = cipher
        .decrypt(nonce, payload)
        .map_err(|_| anyhow!("failed to decrypt media retry notification"))?;

    wa::MediaRetryNotification::decode(plaintext.as_slice())
        .context("failed to decode MediaRetryNotification protobuf (invalid encryption key?)")
}

// ── Parse incoming notification ────────────────────────────────────────────

/// Parse an incoming media retry notification node into a [`MediaRetry`] event.
///
/// Extracts the `<rmr>` metadata, optional `<error>` node, and encrypted
/// payload (`<enc_p>`, `<enc_iv>`) from the notification.
pub fn parse_media_retry_notification(node: &Node) -> Result<MediaRetry> {
    let mut parser = wacore_binary::attrs::AttrParser::new(node);

    let timestamp = parser
        .optional_unix_time("t")
        .map(|t| Utc.timestamp_opt(t, 0).single())
        .flatten()
        .unwrap_or_else(Utc::now);

    let message_id: MessageId = parser
        .optional_string("id")
        .ok_or_else(|| anyhow!("missing id in media retry notification"))?
        .to_string();

    // Parse <rmr> child
    let rmr_node = node
        .get_optional_child("rmr")
        .ok_or_else(|| anyhow!("missing <rmr> in media retry notification"))?;

    let mut rmr_parser = wacore_binary::attrs::AttrParser::new(rmr_node);
    let chat_id = rmr_parser.jid("jid");
    let from_me = rmr_parser.optional_bool("from_me");
    let sender_id = rmr_parser
        .optional_jid("participant")
        .unwrap_or_default();

    // Check for unencrypted <error> node
    if let Some(error_node) = node.get_optional_child("error") {
        let mut error_parser = wacore_binary::attrs::AttrParser::new(error_node);
        let code = error_parser
            .optional_unix_time("code")
            .unwrap_or(0);

        return Ok(MediaRetry {
            ciphertext: None,
            iv: None,
            error: Some(MediaRetryError { code }),
            timestamp,
            message_id,
            chat_id,
            sender_id,
            from_me,
        });
    }

    // Extract encrypted payload: <encrypt> -> <enc_p>, <enc_iv>
    let enc_p_bytes = node
        .get_optional_child_by_tag(&["encrypt", "enc_p"])
        .and_then(|n| match &n.content {
            Some(NodeContent::Bytes(b)) => Some(b.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            anyhow!(
                "missing <enc_p> in media retry notification {}",
                message_id
            )
        })?;

    let enc_iv_bytes = node
        .get_optional_child_by_tag(&["encrypt", "enc_iv"])
        .and_then(|n| match &n.content {
            Some(NodeContent::Bytes(b)) => Some(b.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            anyhow!(
                "missing <enc_iv> in media retry notification {}",
                message_id
            )
        })?;

    Ok(MediaRetry {
        ciphertext: Some(enc_p_bytes),
        iv: Some(enc_iv_bytes),
        error: None,
        timestamp,
        message_id,
        chat_id,
        sender_id,
        from_me,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // ── Key derivation ─────────────────────────────────────────────────

    #[test]
    fn key_derivation_produces_32_bytes() {
        let media_key = b"test-media-key-0123456789abcdef";
        let key = get_media_retry_key(media_key).unwrap();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn key_derivation_is_deterministic() {
        let media_key = b"deterministic-test-key-abcdef01";
        let key1 = get_media_retry_key(media_key).unwrap();
        let key2 = get_media_retry_key(media_key).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn different_media_keys_produce_different_derived_keys() {
        let key_a = get_media_retry_key(b"media-key-alpha").unwrap();
        let key_b = get_media_retry_key(b"media-key-bravo").unwrap();
        assert_ne!(key_a, key_b);
    }

    // ── Encrypt / decrypt round-trip ───────────────────────────────────

    #[test]
    fn encrypt_decrypt_round_trip() {
        let message_id = "3EB0ABCD1234";
        let media_key = b"round-trip-media-key-0123456789a";

        let (ciphertext, iv) =
            encrypt_media_retry_receipt(message_id, media_key).unwrap();

        // Ciphertext should be non-empty and longer than plaintext (includes tag)
        assert!(!ciphertext.is_empty());
        assert_eq!(iv.len(), GCM_IV_LENGTH);

        // Decrypt
        let key = get_media_retry_key(media_key).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let nonce = aes_gcm::Nonce::from_slice(&iv);
        let payload = Payload {
            msg: &ciphertext,
            aad: message_id.as_bytes(),
        };
        let plaintext = cipher.decrypt(nonce, payload).unwrap();

        let receipt = wa::ServerErrorReceipt::decode(plaintext.as_slice()).unwrap();
        assert_eq!(receipt.stanza_id.as_deref(), Some(message_id));
    }

    #[test]
    fn encrypt_with_wrong_aad_fails_decrypt() {
        let message_id = "3EB0WRONG_AAD";
        let media_key = b"wrong-aad-test-key-0123456789ab";

        let (ciphertext, iv) =
            encrypt_media_retry_receipt(message_id, media_key).unwrap();

        let key = get_media_retry_key(media_key).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let nonce = aes_gcm::Nonce::from_slice(&iv);
        let payload = Payload {
            msg: &ciphertext,
            aad: b"DIFFERENT_MSG_ID",
        };
        assert!(cipher.decrypt(nonce, payload).is_err());
    }

    // ── Build receipt node ─────────────────────────────────────────────

    fn make_test_message_info(is_group: bool) -> MessageInfo {
        use crate::types::message::MessageSource;

        let chat = if is_group {
            Jid::try_from("120363001234@g.us").unwrap()
        } else {
            Jid::try_from("5511999887766@s.whatsapp.net").unwrap()
        };

        let sender = if is_group {
            Jid::try_from("5511999887766@s.whatsapp.net").unwrap()
        } else {
            chat.clone()
        };

        MessageInfo {
            source: MessageSource {
                chat,
                sender,
                is_from_me: false,
                is_group,
                ..Default::default()
            },
            id: "3EB0MSG001".to_string(),
            timestamp: Utc.timestamp_opt(1713100000, 0).unwrap(),
            ..Default::default()
        }
    }

    #[test]
    fn build_receipt_dm_structure() {
        let info = make_test_message_info(false);
        let own_jid = Jid::try_from("myphone@s.whatsapp.net").unwrap();
        let media_key = b"dm-test-media-key-0123456789abc";

        let node = build_media_retry_receipt(&info, media_key, &own_jid).unwrap();

        assert_eq!(node.tag, "receipt");
        assert_eq!(
            node.attrs.get("id").map(|v| v.to_string_value()),
            Some("3EB0MSG001".to_string())
        );
        assert_eq!(
            node.attrs.get("type").map(|v| v.to_string_value()),
            Some("server-error".to_string())
        );
        assert_eq!(
            node.attrs.get("to").map(|v| v.to_string_value()),
            Some("myphone@s.whatsapp.net".to_string())
        );

        // Should have 2 children: <encrypt> and <rmr>
        let children = node.children().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].tag, "encrypt");
        assert_eq!(children[1].tag, "rmr");

        // <encrypt> should have <enc_p> and <enc_iv>
        let encrypt_children = children[0].children().unwrap();
        assert_eq!(encrypt_children.len(), 2);
        assert_eq!(encrypt_children[0].tag, "enc_p");
        assert_eq!(encrypt_children[1].tag, "enc_iv");

        // <rmr> should have jid and from_me, but no participant for DMs
        let rmr = &children[1];
        assert!(rmr.attrs.get("jid").is_some());
        assert_eq!(
            rmr.attrs.get("from_me").map(|v| v.to_string_value()),
            Some("false".to_string())
        );
        assert!(rmr.attrs.get("participant").is_none());
    }

    #[test]
    fn build_receipt_group_includes_participant() {
        let info = make_test_message_info(true);
        let own_jid = Jid::try_from("myphone@s.whatsapp.net").unwrap();
        let media_key = b"group-test-media-key-0123456789a";

        let node = build_media_retry_receipt(&info, media_key, &own_jid).unwrap();

        let children = node.children().unwrap();
        let rmr = &children[1];
        assert_eq!(rmr.tag, "rmr");

        // Group chats must include participant
        assert!(rmr.attrs.get("participant").is_some());
    }

    #[test]
    fn build_receipt_empty_jid_rejected() {
        let info = make_test_message_info(false);
        let empty_jid = Jid::default();
        let media_key = b"empty-jid-test-key-0123456789ab";

        let result = build_media_retry_receipt(&info, media_key, &empty_jid);
        assert!(result.is_err());
    }

    // ── Parse notification ─────────────────────────────────────────────

    fn build_encrypted_notification(
        message_id: &str,
        chat_jid: &str,
        from_me: bool,
        participant: Option<&str>,
    ) -> Node {
        let media_key = b"parse-test-media-key-0123456789a";
        let (ciphertext, iv) =
            encrypt_media_retry_receipt(message_id, media_key).unwrap();

        let encrypt_node = NodeBuilder::new("encrypt")
            .children([
                NodeBuilder::new("enc_p").bytes(ciphertext).build(),
                NodeBuilder::new("enc_iv").bytes(iv).build(),
            ])
            .build();

        let mut rmr_builder = NodeBuilder::new("rmr")
            .attr("jid", chat_jid)
            .attr("from_me", if from_me { "true" } else { "false" });

        if let Some(p) = participant {
            rmr_builder = rmr_builder.attr("participant", p);
        }

        NodeBuilder::new("notification")
            .attr("id", message_id)
            .attr("t", "1713100000")
            .children([encrypt_node, rmr_builder.build()])
            .build()
    }

    #[test]
    fn parse_encrypted_dm_notification() {
        let node = build_encrypted_notification(
            "3EB0PARSE01",
            "5511999887766@s.whatsapp.net",
            false,
            None,
        );

        let evt = parse_media_retry_notification(&node).unwrap();

        assert_eq!(evt.message_id, "3EB0PARSE01");
        assert_eq!(evt.chat_id.to_string(), "5511999887766@s.whatsapp.net");
        assert!(!evt.from_me);
        assert!(evt.sender_id.is_empty());
        assert!(evt.ciphertext.is_some());
        assert!(evt.iv.is_some());
        assert!(evt.error.is_none());
        assert_eq!(
            evt.timestamp,
            Utc.timestamp_opt(1713100000, 0).unwrap()
        );
    }

    #[test]
    fn parse_encrypted_group_notification() {
        let node = build_encrypted_notification(
            "3EB0PARSE02",
            "120363001234@g.us",
            true,
            Some("5511999887766@s.whatsapp.net"),
        );

        let evt = parse_media_retry_notification(&node).unwrap();

        assert_eq!(evt.message_id, "3EB0PARSE02");
        assert!(evt.from_me);
        assert_eq!(evt.sender_id.to_string(), "5511999887766@s.whatsapp.net");
    }

    #[test]
    fn parse_error_notification() {
        let rmr_node = NodeBuilder::new("rmr")
            .attr("jid", "5511999887766@s.whatsapp.net")
            .attr("from_me", "false")
            .build();

        let error_node = NodeBuilder::new("error")
            .attr("code", "2")
            .build();

        let node = NodeBuilder::new("notification")
            .attr("id", "3EB0ERR001")
            .attr("t", "1713100000")
            .children([error_node, rmr_node])
            .build();

        let evt = parse_media_retry_notification(&node).unwrap();

        assert_eq!(evt.message_id, "3EB0ERR001");
        assert!(evt.error.is_some());
        assert_eq!(evt.error.as_ref().unwrap().code, 2);
        assert!(evt.ciphertext.is_none());
        assert!(evt.iv.is_none());
    }

    #[test]
    fn parse_missing_rmr_fails() {
        let node = NodeBuilder::new("notification")
            .attr("id", "3EB0NOPE01")
            .attr("t", "1713100000")
            .build();

        let result = parse_media_retry_notification(&node);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("rmr"),
            "error should mention missing <rmr>"
        );
    }

    #[test]
    fn parse_missing_id_fails() {
        let rmr = NodeBuilder::new("rmr")
            .attr("jid", "5511999887766@s.whatsapp.net")
            .attr("from_me", "false")
            .build();

        let node = NodeBuilder::new("notification")
            .attr("t", "1713100000")
            .children([rmr])
            .build();

        let result = parse_media_retry_notification(&node);
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_enc_p_fails() {
        let encrypt_node = NodeBuilder::new("encrypt")
            .children([
                // Missing enc_p, only enc_iv present
                NodeBuilder::new("enc_iv").bytes(vec![0u8; 12]).build(),
            ])
            .build();

        let rmr = NodeBuilder::new("rmr")
            .attr("jid", "5511999887766@s.whatsapp.net")
            .attr("from_me", "false")
            .build();

        let node = NodeBuilder::new("notification")
            .attr("id", "3EB0MISS01")
            .attr("t", "1713100000")
            .children([encrypt_node, rmr])
            .build();

        let result = parse_media_retry_notification(&node);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("enc_p"));
    }

    // ── Full decrypt round-trip ────────────────────────────────────────

    #[test]
    fn full_encrypt_parse_decrypt_round_trip() {
        let media_key = b"parse-test-media-key-0123456789a";
        let message_id = "3EB0FULL_RT";

        let node = build_encrypted_notification(
            message_id,
            "5511999887766@s.whatsapp.net",
            false,
            None,
        );

        let evt = parse_media_retry_notification(&node).unwrap();
        assert_eq!(evt.message_id, message_id);

        // Build a fake MediaRetryNotification response with the same crypto
        // (In production, the phone sends back a different notification, but
        // we test the decrypt path with a known plaintext.)
        let notif = wa::MediaRetryNotification {
            stanza_id: Some(message_id.to_string()),
            direct_path: Some("/mms/image/new-path-abc123".to_string()),
            result: Some(
                wa::media_retry_notification::ResultType::Success as i32,
            ),
            message_secret: None,
        };
        let plaintext = notif.encode_to_vec();

        let key = get_media_retry_key(media_key).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let iv = vec![0xAA; GCM_IV_LENGTH];
        let nonce = aes_gcm::Nonce::from_slice(&iv);
        let payload = Payload {
            msg: &plaintext,
            aad: message_id.as_bytes(),
        };
        let encrypted = cipher.encrypt(nonce, payload).unwrap();

        let response_evt = MediaRetry {
            ciphertext: Some(encrypted),
            iv: Some(iv),
            error: None,
            timestamp: Utc::now(),
            message_id: message_id.to_string(),
            chat_id: Jid::try_from("5511999887766@s.whatsapp.net").unwrap(),
            sender_id: Jid::default(),
            from_me: false,
        };

        let decrypted =
            decrypt_media_retry_notification(&response_evt, media_key).unwrap();

        assert_eq!(decrypted.stanza_id.as_deref(), Some(message_id));
        assert_eq!(
            decrypted.direct_path.as_deref(),
            Some("/mms/image/new-path-abc123")
        );
        assert_eq!(
            decrypted.result,
            Some(wa::media_retry_notification::ResultType::Success as i32)
        );
    }

    #[test]
    fn decrypt_error_code_2_returns_not_available() {
        let evt = MediaRetry {
            ciphertext: None,
            iv: None,
            error: Some(MediaRetryError { code: 2 }),
            timestamp: Utc::now(),
            message_id: "3EB0ERR002".to_string(),
            chat_id: Jid::try_from("5511999887766@s.whatsapp.net").unwrap(),
            sender_id: Jid::default(),
            from_me: false,
        };

        let result = decrypt_media_retry_notification(&evt, b"any-key-here");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .downcast_ref::<MediaNotAvailableOnPhone>()
            .is_some());
    }

    #[test]
    fn decrypt_unknown_error_code_propagates() {
        let evt = MediaRetry {
            ciphertext: None,
            iv: None,
            error: Some(MediaRetryError { code: 99 }),
            timestamp: Utc::now(),
            message_id: "3EB0ERR099".to_string(),
            chat_id: Jid::try_from("5511999887766@s.whatsapp.net").unwrap(),
            sender_id: Jid::default(),
            from_me: false,
        };

        let result = decrypt_media_retry_notification(&evt, b"any-key-here");
        assert!(result.is_err());
        let err = result.unwrap_err();
        let retry_err = err.downcast_ref::<UnknownMediaRetryError>().unwrap();
        assert_eq!(retry_err.code, 99);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let media_key = b"correct-media-key-0123456789abcd";
        let wrong_key = b"wrong---media-key-0123456789abcd";
        let message_id = "3EB0WRONGKEY";

        // Encrypt with correct key
        let notif = wa::MediaRetryNotification {
            stanza_id: Some(message_id.to_string()),
            direct_path: Some("/mms/image/secret".to_string()),
            result: Some(
                wa::media_retry_notification::ResultType::Success as i32,
            ),
            message_secret: None,
        };
        let plaintext = notif.encode_to_vec();

        let key = get_media_retry_key(media_key).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let iv = vec![0xBB; GCM_IV_LENGTH];
        let nonce = aes_gcm::Nonce::from_slice(&iv);
        let payload = Payload {
            msg: &plaintext,
            aad: message_id.as_bytes(),
        };
        let encrypted = cipher.encrypt(nonce, payload).unwrap();

        let evt = MediaRetry {
            ciphertext: Some(encrypted),
            iv: Some(iv),
            error: None,
            timestamp: Utc::now(),
            message_id: message_id.to_string(),
            chat_id: Jid::try_from("5511999887766@s.whatsapp.net").unwrap(),
            sender_id: Jid::default(),
            from_me: false,
        };

        // Decrypt with wrong key should fail
        let result = decrypt_media_retry_notification(&evt, wrong_key);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to decrypt"));
    }
}
