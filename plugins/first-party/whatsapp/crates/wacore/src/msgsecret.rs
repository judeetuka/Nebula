//! Message secret cryptography for WhatsApp encrypted message types.
//!
//! Ports whatsmeow/msgsecret.go -- handles key derivation, encryption, and
//! decryption for poll votes, encrypted reactions, encrypted comments, event
//! responses/edits, bot messages, and report tokens.
//!
//! ## Secret message types
//!
//! WhatsApp uses per-message secrets (stored in `MessageContextInfo.message_secret`)
//! to derive encryption keys for various message "add-ons" such as poll votes,
//! encrypted reactions in announcement groups, encrypted comments, event responses,
//! and event edits. Each use case concatenates context strings (original message ID,
//! original sender, modification sender, and the use-case label) into an HKDF info
//! parameter to derive a unique AES-256-GCM key.
//!
//! ## Crypto flow
//!
//! 1. Retrieve the original message's secret (`message_secret` from `MessageContextInfo`).
//! 2. Call [`generate_msg_secret_key`] with the use case, participants, and secret.
//! 3. Encrypt/decrypt with AES-256-GCM using the derived key. Some use cases include
//!    additional authenticated data (AAD) in the GCM tag.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::Aes256Gcm;
use hkdf::Hkdf;
use prost::Message;
use sha2::{Digest, Sha256};
use wacore_binary::jid::{Jid, JidExt, DEFAULT_USER_SERVER, HIDDEN_USER_SERVER};
use waproto::whatsapp as wa;
use waproto::whatsapp::message;

// ── AES-GCM constants ──────────────────────────────────────────────────────

/// AES-GCM nonce (IV) length in bytes.
const GCM_IV_LENGTH: usize = 12;

// ── Message secret type ────────────────────────────────────────────────────

/// Identifies the use case for a secret-encrypted message.
///
/// Each variant's string representation is concatenated into the HKDF info
/// during key derivation. The byte representations match whatsmeow exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MsgSecretType {
    /// Poll vote encryption/decryption.
    PollVote,
    /// Encrypted reaction in announcement groups.
    EncReaction,
    /// Encrypted comment in announcement groups.
    EncComment,
    /// Report token generation.
    ReportToken,
    /// Event RSVP response.
    EventResponse,
    /// Event edit (e.g., changing event details).
    EventEdit,
    /// Bot message secret derivation.
    BotMessage,
}

impl MsgSecretType {
    /// Returns the wire-format string used in HKDF info concatenation.
    ///
    /// These strings must match the Go constants exactly for interoperability.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PollVote => "Poll Vote",
            Self::EncReaction => "Enc Reaction",
            Self::EncComment => "Enc Comment",
            Self::ReportToken => "Report Token",
            Self::EventResponse => "Event Response",
            Self::EventEdit => "Event Edit",
            Self::BotMessage => "Bot Message",
        }
    }

    /// Returns the byte slice representation for HKDF concatenation.
    #[must_use]
    pub fn as_bytes(&self) -> &'static [u8] {
        self.as_str().as_bytes()
    }
}

// ── Errors ─────────────────────────────────────────────────────────────────

/// Errors specific to message secret operations.
#[derive(Debug, thiserror::Error)]
pub enum MsgSecretError {
    #[error("original message secret not found")]
    OriginalSecretNotFound,

    #[error("not an encrypted reaction message")]
    NotEncryptedReactionMessage,

    #[error("not an encrypted comment message")]
    NotEncryptedCommentMessage,

    #[error("not a poll update message")]
    NotPollUpdateMessage,

    #[error("not a secret encrypted message")]
    NotSecretEncryptedMessage,

    #[error("unsupported secret enc type: {0}")]
    UnsupportedSecretEncType(String),

    #[error("missing encrypted payload")]
    MissingEncPayload,

    #[error("missing encrypted IV")]
    MissingEncIv,

    #[error("missing target message key")]
    MissingTargetMessageKey,

    #[error("missing poll creation message key")]
    MissingPollCreationKey,

    #[error("missing vote in poll update")]
    MissingVote,

    #[error("AES-GCM encryption failed")]
    EncryptionFailed,

    #[error("AES-GCM decryption failed")]
    DecryptionFailed,

    #[error("protobuf decode failed: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("protobuf encode failed: {0}")]
    ProtobufEncode(#[from] prost::EncodeError),

    #[error("failed to parse JID {jid:?}: {reason}")]
    InvalidJid { jid: String, reason: String },
}

// ── Encrypted secret trait ─────────────────────────────────────────────────

/// Trait abstracting over protobuf types that carry encrypted payload + IV.
///
/// Mirrors whatsmeow's `messageEncryptedSecret` interface. Implemented for
/// `PollEncValue`, `EncReactionMessage`, `EncCommentMessage`,
/// `EncEventResponseMessage`, and `SecretEncryptedMessage`.
pub trait EncryptedSecret {
    /// Returns the encrypted IV, if present.
    fn enc_iv(&self) -> Option<&[u8]>;
    /// Returns the encrypted payload, if present.
    fn enc_payload(&self) -> Option<&[u8]>;
}

impl EncryptedSecret for message::PollEncValue {
    fn enc_iv(&self) -> Option<&[u8]> {
        self.enc_iv.as_deref()
    }
    fn enc_payload(&self) -> Option<&[u8]> {
        self.enc_payload.as_deref()
    }
}

impl EncryptedSecret for message::EncReactionMessage {
    fn enc_iv(&self) -> Option<&[u8]> {
        self.enc_iv.as_deref()
    }
    fn enc_payload(&self) -> Option<&[u8]> {
        self.enc_payload.as_deref()
    }
}

impl EncryptedSecret for message::EncCommentMessage {
    fn enc_iv(&self) -> Option<&[u8]> {
        self.enc_iv.as_deref()
    }
    fn enc_payload(&self) -> Option<&[u8]> {
        self.enc_payload.as_deref()
    }
}

impl EncryptedSecret for message::EncEventResponseMessage {
    fn enc_iv(&self) -> Option<&[u8]> {
        self.enc_iv.as_deref()
    }
    fn enc_payload(&self) -> Option<&[u8]> {
        self.enc_payload.as_deref()
    }
}

impl EncryptedSecret for message::SecretEncryptedMessage {
    fn enc_iv(&self) -> Option<&[u8]> {
        self.enc_iv.as_deref()
    }
    fn enc_payload(&self) -> Option<&[u8]> {
        self.enc_payload.as_deref()
    }
}

impl EncryptedSecret for wa::MessageSecretMessage {
    fn enc_iv(&self) -> Option<&[u8]> {
        self.enc_iv.as_deref()
    }
    fn enc_payload(&self) -> Option<&[u8]> {
        self.enc_payload.as_deref()
    }
}

// ── Core key derivation ────────────────────────────────────────────────────

/// HKDF-SHA256 helper: expand `ikm` with optional `salt` and `info` into `len` bytes.
fn hkdf_sha256(ikm: &[u8], salt: Option<&[u8]>, info: &[u8], len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::new(salt, ikm);
    let mut output = vec![0u8; len];
    hk.expand(info, &mut output)
        .expect("HKDF expand: output length is valid");
    output
}

/// Derive the bot-message HKDF key from a raw message secret.
///
/// This is applied before `generate_msg_secret_key` when processing bot messages.
/// Equivalent to whatsmeow's `applyBotMessageHKDF`.
#[must_use]
pub fn apply_bot_message_hkdf(message_secret: &[u8]) -> Vec<u8> {
    hkdf_sha256(
        message_secret,
        None,
        MsgSecretType::BotMessage.as_bytes(),
        32,
    )
}

/// Derive the AES-256-GCM secret key (and optional AAD) for a secret message operation.
///
/// The HKDF info is the concatenation of:
///   `orig_msg_id || orig_msg_sender.to_non_ad() || modification_sender.to_non_ad() || modification_type`
///
/// For `PollVote`, `EventResponse`, and the empty-string bot-message use case,
/// additional authenticated data (AAD) is also produced:
///   `orig_msg_id \x00 modification_sender.to_non_ad()`
///
/// Returns `(secret_key, additional_data)` where `additional_data` may be empty.
///
/// Equivalent to whatsmeow's `generateMsgSecretKey`.
pub fn generate_msg_secret_key(
    modification_type: Option<MsgSecretType>,
    modification_sender: &Jid,
    orig_msg_id: &str,
    orig_msg_sender: &Jid,
    orig_msg_secret: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    let orig_sender_str = orig_msg_sender.to_non_ad().to_string();
    let mod_sender_str = modification_sender.to_non_ad().to_string();
    let type_bytes = modification_type.map(|t| t.as_bytes()).unwrap_or_default();

    // Build use-case secret: origMsgID || origMsgSender || modificationSender || modificationType
    let mut use_case_secret = Vec::with_capacity(
        orig_msg_id.len() + orig_sender_str.len() + mod_sender_str.len() + type_bytes.len(),
    );
    use_case_secret.extend_from_slice(orig_msg_id.as_bytes());
    use_case_secret.extend_from_slice(orig_sender_str.as_bytes());
    use_case_secret.extend_from_slice(mod_sender_str.as_bytes());
    use_case_secret.extend_from_slice(type_bytes);

    let secret_key = hkdf_sha256(orig_msg_secret, None, &use_case_secret, 32);

    // Produce AAD for PollVote, EventResponse, and the empty (bot message) case
    let additional_data = match modification_type {
        Some(MsgSecretType::PollVote | MsgSecretType::EventResponse) | None => {
            // format: "{orig_msg_id}\x00{modification_sender}"
            let mut aad = Vec::with_capacity(orig_msg_id.len() + 1 + mod_sender_str.len());
            aad.extend_from_slice(orig_msg_id.as_bytes());
            aad.push(0x00);
            aad.extend_from_slice(mod_sender_str.as_bytes());
            aad
        }
        _ => Vec::new(),
    };

    (secret_key, additional_data)
}

// ── GCM encrypt / decrypt ──────────────────────────────────────────────────

/// Encrypt `plaintext` with AES-256-GCM using the given key, IV, and AAD.
///
/// Returns the ciphertext (including the 16-byte GCM tag appended).
fn gcm_encrypt(
    key: &[u8],
    iv: &[u8],
    plaintext: &[u8],
    additional_data: &[u8],
) -> Result<Vec<u8>, MsgSecretError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| MsgSecretError::EncryptionFailed)?;
    let nonce = aes_gcm::Nonce::from_slice(iv);
    let payload = Payload {
        msg: plaintext,
        aad: additional_data,
    };
    cipher
        .encrypt(nonce, payload)
        .map_err(|_| MsgSecretError::EncryptionFailed)
}

/// Decrypt `ciphertext` with AES-256-GCM using the given key, IV, and AAD.
///
/// The ciphertext must include the trailing 16-byte GCM auth tag.
fn gcm_decrypt(
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
    additional_data: &[u8],
) -> Result<Vec<u8>, MsgSecretError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| MsgSecretError::DecryptionFailed)?;
    let nonce = aes_gcm::Nonce::from_slice(iv);
    let payload = Payload {
        msg: ciphertext,
        aad: additional_data,
    };
    cipher
        .decrypt(nonce, payload)
        .map_err(|_| MsgSecretError::DecryptionFailed)
}

// ── Helper: resolve original sender from a MessageKey ──────────────────────

/// Resolve the original message sender JID from a protobuf `MessageKey`.
///
/// Logic mirrors whatsmeow's `getOrigSenderFromKey`:
/// - `from_me == true`: the sender is the modification sender (same user).
/// - DM chat (s.whatsapp.net / lid): sender is `remote_jid`.
/// - Group chat: sender is `participant`.
pub fn get_orig_sender_from_key(
    key: &wa::MessageKey,
    modification_sender: &Jid,
    chat: &Jid,
) -> Result<Jid, MsgSecretError> {
    if key.from_me.unwrap_or(false) {
        return Ok(modification_sender.clone());
    }

    if chat.server() == DEFAULT_USER_SERVER || chat.server() == HIDDEN_USER_SERVER {
        // DM: sender is the remote JID from the key
        let raw = key.remote_jid.as_deref().unwrap_or("");
        Jid::try_from(raw).map_err(|e| MsgSecretError::InvalidJid {
            jid: raw.to_string(),
            reason: e.to_string(),
        })
    } else {
        // Group: sender is the participant
        let raw = key.participant.as_deref().unwrap_or("");
        let jid = Jid::try_from(raw).map_err(|e| MsgSecretError::InvalidJid {
            jid: raw.to_string(),
            reason: e.to_string(),
        })?;
        if jid.server() != DEFAULT_USER_SERVER && jid.server() != HIDDEN_USER_SERVER {
            return Err(MsgSecretError::InvalidJid {
                jid: raw.to_string(),
                reason: "unexpected server for group participant".to_string(),
            });
        }
        Ok(jid)
    }
}

// ── Low-level decrypt / encrypt ────────────────────────────────────────────

/// Decrypt an encrypted message secret payload.
///
/// This is the core decryption routine used by all `decrypt_*` functions.
/// The caller must supply the original message secret (retrieved from the store).
///
/// Equivalent to whatsmeow's `Client.decryptMsgSecret`, but without store access.
///
/// # Arguments
///
/// * `use_case` - The [`MsgSecretType`] for key derivation.
/// * `modification_sender` - The JID of whoever created the modification (e.g., voter).
/// * `orig_msg_id` - The message ID of the original message (e.g., poll creation).
/// * `orig_msg_sender` - The JID of the original message sender.
/// * `orig_msg_secret` - The 32-byte secret from the original message's `MessageContextInfo`.
/// * `encrypted` - Any type implementing [`EncryptedSecret`] (provides IV + payload).
pub fn decrypt_msg_secret(
    use_case: MsgSecretType,
    modification_sender: &Jid,
    orig_msg_id: &str,
    orig_msg_sender: &Jid,
    orig_msg_secret: &[u8],
    encrypted: &dyn EncryptedSecret,
) -> Result<Vec<u8>, MsgSecretError> {
    let iv = encrypted.enc_iv().ok_or(MsgSecretError::MissingEncIv)?;
    let ciphertext = encrypted
        .enc_payload()
        .ok_or(MsgSecretError::MissingEncPayload)?;

    let (secret_key, additional_data) = generate_msg_secret_key(
        Some(use_case),
        modification_sender,
        orig_msg_id,
        orig_msg_sender,
        orig_msg_secret,
    );

    gcm_decrypt(&secret_key, iv, ciphertext, &additional_data)
}

/// Encrypt a plaintext payload into a secret message.
///
/// Returns `(ciphertext, iv)` where the ciphertext includes the GCM auth tag.
/// The caller is responsible for storing the IV alongside the ciphertext.
///
/// Equivalent to whatsmeow's `Client.encryptMsgSecret`, but without store access.
///
/// # Arguments
///
/// * `use_case` - The [`MsgSecretType`] for key derivation.
/// * `own_id` - The current user's JID (the modification sender).
/// * `orig_msg_id` - The message ID of the original message.
/// * `orig_msg_sender` - The JID of the original message sender.
/// * `orig_msg_secret` - The 32-byte secret from the original message.
/// * `plaintext` - The protobuf-encoded payload to encrypt.
pub fn encrypt_msg_secret(
    use_case: MsgSecretType,
    own_id: &Jid,
    orig_msg_id: &str,
    orig_msg_sender: &Jid,
    orig_msg_secret: &[u8],
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), MsgSecretError> {
    let (secret_key, additional_data) = generate_msg_secret_key(
        Some(use_case),
        own_id,
        orig_msg_id,
        orig_msg_sender,
        orig_msg_secret,
    );

    let mut iv = [0u8; GCM_IV_LENGTH];
    rand::fill(&mut iv).map_err(|_| MsgSecretError::EncryptionFailed)?;

    let ciphertext = gcm_encrypt(&secret_key, &iv, plaintext, &additional_data)?;

    Ok((ciphertext, iv.to_vec()))
}

// ── Bot message decrypt ────────────────────────────────────────────────────

/// Decrypt a bot message using the bot message secret.
///
/// The bot message secret is first processed through [`apply_bot_message_hkdf`],
/// then the empty-string use case of [`generate_msg_secret_key`] is applied.
///
/// Equivalent to whatsmeow's `Client.decryptBotMessage`.
///
/// # Arguments
///
/// * `message_secret` - The `bot_message_secret` from `MessageContextInfo`.
/// * `encrypted` - The encrypted payload (IV + ciphertext).
/// * `message_id` - The message ID of the bot message.
/// * `target_sender` - The JID of the target sender.
/// * `info_sender` - The JID from the message info's sender field.
pub fn decrypt_bot_message(
    message_secret: &[u8],
    encrypted: &dyn EncryptedSecret,
    message_id: &str,
    target_sender: &Jid,
    info_sender: &Jid,
) -> Result<Vec<u8>, MsgSecretError> {
    let iv = encrypted.enc_iv().ok_or(MsgSecretError::MissingEncIv)?;
    let ciphertext = encrypted
        .enc_payload()
        .ok_or(MsgSecretError::MissingEncPayload)?;

    let derived_secret = apply_bot_message_hkdf(message_secret);
    let (new_key, additional_data) = generate_msg_secret_key(
        None, // empty string use case for bot messages
        info_sender,
        message_id,
        target_sender,
        &derived_secret,
    );

    gcm_decrypt(&new_key, iv, ciphertext, &additional_data)
}

// ── Poll utilities ─────────────────────────────────────────────────────────

/// Hash poll option names with SHA-256.
///
/// Each option name is independently hashed. The resulting 32-byte hashes are
/// used in poll vote messages to identify selected options without revealing
/// the option text to observers who do not have the poll creation message.
///
/// # Example
///
/// ```
/// use wacore::msgsecret::hash_poll_options;
///
/// let hashes = hash_poll_options(&["Yes", "No", "Maybe"]);
/// assert_eq!(hashes.len(), 3);
/// assert_eq!(hashes[0].len(), 32);
/// ```
#[must_use]
pub fn hash_poll_options(option_names: &[&str]) -> Vec<Vec<u8>> {
    option_names
        .iter()
        .map(|name| {
            let mut hasher = Sha256::new();
            hasher.update(name.as_bytes());
            hasher.finalize().to_vec()
        })
        .collect()
}

/// Hash poll option names from owned strings.
///
/// Convenience variant of [`hash_poll_options`] that accepts `String` slices.
#[must_use]
pub fn hash_poll_options_owned(option_names: &[String]) -> Vec<Vec<u8>> {
    option_names
        .iter()
        .map(|name| {
            let mut hasher = Sha256::new();
            hasher.update(name.as_bytes());
            hasher.finalize().to_vec()
        })
        .collect()
}

// ── Build poll creation ────────────────────────────────────────────────────

/// Build a poll creation message with the given name, options, and selection limit.
///
/// Generates a random 32-byte message secret and wraps it in `MessageContextInfo`.
/// The returned `wa::Message` is ready to send via the normal message send path.
///
/// If `selectable_option_count` is negative or exceeds the number of options,
/// it is clamped to 0 (meaning unlimited selection).
///
/// Equivalent to whatsmeow's `Client.BuildPollCreation`.
///
/// # Example
///
/// ```
/// use wacore::msgsecret::build_poll_creation;
///
/// let msg = build_poll_creation("Favorite color?", &["Red", "Blue", "Green"], 1);
/// assert!(msg.poll_creation_message.is_some());
/// assert!(msg.message_context_info.is_some());
/// ```
#[must_use]
pub fn build_poll_creation(
    name: &str,
    option_names: &[&str],
    selectable_option_count: i32,
) -> wa::Message {
    let mut msg_secret = [0u8; 32];
    rand::fill(&mut msg_secret).expect("RNG failure");

    let count =
        if selectable_option_count < 0 || selectable_option_count > option_names.len() as i32 {
            0u32
        } else {
            selectable_option_count as u32
        };

    let options: Vec<message::poll_creation_message::Option> = option_names
        .iter()
        .map(|&opt| message::poll_creation_message::Option {
            option_name: Some(opt.to_string()),
            option_hash: None,
        })
        .collect();

    wa::Message {
        poll_creation_message: Some(Box::new(message::PollCreationMessage {
            enc_key: None,
            name: Some(name.to_string()),
            options,
            selectable_options_count: Some(count),
            context_info: None,
            poll_content_type: None,
            poll_type: None,
            correct_answer: None,
        })),
        message_context_info: Some(wa::MessageContextInfo {
            message_secret: Some(msg_secret.to_vec()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// ── MessageKey builder ─────────────────────────────────────────────────────

/// Build a protobuf `MessageKey` from message metadata.
///
/// Equivalent to whatsmeow's `getKeyFromInfo`.
#[must_use]
pub fn message_key_from_info(
    chat: &Jid,
    sender: &Jid,
    message_id: &str,
    is_from_me: bool,
    is_group: bool,
) -> wa::MessageKey {
    let mut key = wa::MessageKey {
        remote_jid: Some(chat.to_string()),
        from_me: Some(is_from_me),
        id: Some(message_id.to_string()),
        participant: None,
    };
    if is_group {
        key.participant = Some(sender.to_string());
    }
    key
}

// ── Poll vote encrypt / decrypt ────────────────────────────────────────────

/// Decrypt a poll vote from a `PollUpdateMessage`.
///
/// Returns the decoded `PollVoteMessage` containing SHA-256 hashes of selected
/// option names. Compare these hashes against [`hash_poll_options`] output to
/// determine which options were selected.
///
/// # Arguments
///
/// * `poll_update` - The `PollUpdateMessage` from the incoming `wa::Message`.
/// * `modification_sender` - The JID of the voter.
/// * `chat` - The chat JID where the poll exists.
/// * `orig_msg_secret` - The 32-byte secret from the original poll creation message.
pub fn decrypt_poll_vote(
    poll_update: &message::PollUpdateMessage,
    modification_sender: &Jid,
    chat: &Jid,
    orig_msg_secret: &[u8],
) -> Result<message::PollVoteMessage, MsgSecretError> {
    let creation_key = poll_update
        .poll_creation_message_key
        .as_ref()
        .ok_or(MsgSecretError::MissingPollCreationKey)?;

    let vote = poll_update
        .vote
        .as_ref()
        .ok_or(MsgSecretError::MissingVote)?;

    let orig_msg_id = creation_key.id.as_deref().unwrap_or("");
    let orig_sender = get_orig_sender_from_key(creation_key, modification_sender, chat)?;

    let plaintext = decrypt_msg_secret(
        MsgSecretType::PollVote,
        modification_sender,
        orig_msg_id,
        &orig_sender,
        orig_msg_secret,
        vote,
    )?;

    message::PollVoteMessage::decode(plaintext.as_slice()).map_err(MsgSecretError::ProtobufDecode)
}

/// Encrypt a poll vote message.
///
/// Returns a `PollUpdateMessage` ready to embed in a `wa::Message` for sending.
///
/// # Arguments
///
/// * `own_id` - The current user's JID.
/// * `poll_chat` - The chat JID where the poll exists.
/// * `poll_sender` - The JID of the poll creator.
/// * `poll_msg_id` - The message ID of the poll creation message.
/// * `poll_is_group` - Whether the poll is in a group chat.
/// * `orig_msg_secret` - The 32-byte secret from the poll creation message.
/// * `vote` - The `PollVoteMessage` to encrypt (with selected option hashes).
/// * `sender_timestamp_ms` - Timestamp in milliseconds for the vote.
pub fn encrypt_poll_vote(
    own_id: &Jid,
    poll_chat: &Jid,
    poll_sender: &Jid,
    poll_msg_id: &str,
    poll_is_group: bool,
    orig_msg_secret: &[u8],
    vote: &message::PollVoteMessage,
    sender_timestamp_ms: i64,
) -> Result<message::PollUpdateMessage, MsgSecretError> {
    let plaintext = vote.encode_to_vec();

    let (ciphertext, iv) = encrypt_msg_secret(
        MsgSecretType::PollVote,
        own_id,
        poll_msg_id,
        poll_sender,
        orig_msg_secret,
        &plaintext,
    )?;

    Ok(message::PollUpdateMessage {
        poll_creation_message_key: Some(message_key_from_info(
            poll_chat,
            poll_sender,
            poll_msg_id,
            own_id.to_non_ad() == poll_sender.to_non_ad(),
            poll_is_group,
        )),
        vote: Some(message::PollEncValue {
            enc_payload: Some(ciphertext),
            enc_iv: Some(iv),
        }),
        metadata: None,
        sender_timestamp_ms: Some(sender_timestamp_ms),
    })
}

/// Build a complete poll vote `wa::Message` from option names.
///
/// Hashes the selected option names, builds a `PollVoteMessage`, encrypts it,
/// and wraps it in a `PollUpdateMessage` inside a `wa::Message`.
///
/// Equivalent to whatsmeow's `Client.BuildPollVote`.
pub fn build_poll_vote(
    own_id: &Jid,
    poll_chat: &Jid,
    poll_sender: &Jid,
    poll_msg_id: &str,
    poll_is_group: bool,
    orig_msg_secret: &[u8],
    selected_option_names: &[&str],
    sender_timestamp_ms: i64,
) -> Result<wa::Message, MsgSecretError> {
    let vote = message::PollVoteMessage {
        selected_options: hash_poll_options(selected_option_names),
    };

    let poll_update = encrypt_poll_vote(
        own_id,
        poll_chat,
        poll_sender,
        poll_msg_id,
        poll_is_group,
        orig_msg_secret,
        &vote,
        sender_timestamp_ms,
    )?;

    Ok(wa::Message {
        poll_update_message: Some(poll_update),
        ..Default::default()
    })
}

// ── Reaction encrypt / decrypt ─────────────────────────────────────────────

/// Decrypt an encrypted reaction in an announcement group.
///
/// Returns the decoded `ReactionMessage` with the reaction emoji and target key.
///
/// # Arguments
///
/// * `enc_reaction` - The `EncReactionMessage` from the incoming `wa::Message`.
/// * `modification_sender` - The JID of whoever sent the reaction.
/// * `chat` - The chat JID.
/// * `orig_msg_secret` - The 32-byte secret from the original message.
pub fn decrypt_reaction(
    enc_reaction: &message::EncReactionMessage,
    modification_sender: &Jid,
    chat: &Jid,
    orig_msg_secret: &[u8],
) -> Result<message::ReactionMessage, MsgSecretError> {
    let target_key = enc_reaction
        .target_message_key
        .as_ref()
        .ok_or(MsgSecretError::MissingTargetMessageKey)?;

    let orig_msg_id = target_key.id.as_deref().unwrap_or("");
    let orig_sender = get_orig_sender_from_key(target_key, modification_sender, chat)?;

    let plaintext = decrypt_msg_secret(
        MsgSecretType::EncReaction,
        modification_sender,
        orig_msg_id,
        &orig_sender,
        orig_msg_secret,
        enc_reaction,
    )?;

    message::ReactionMessage::decode(plaintext.as_slice()).map_err(MsgSecretError::ProtobufDecode)
}

/// Encrypt a reaction message for an announcement group.
///
/// The reaction's `key` field is extracted and used as the `target_message_key`
/// in the returned `EncReactionMessage`. The key is cleared from the reaction
/// before serialization (matching whatsmeow behavior).
///
/// # Arguments
///
/// * `own_id` - The current user's JID.
/// * `root_chat` - The chat JID.
/// * `root_sender` - The JID of the original message sender.
/// * `root_msg_id` - The message ID of the original message.
/// * `orig_msg_secret` - The 32-byte secret from the original message.
/// * `reaction` - The `ReactionMessage` to encrypt. Its `key` field will be
///   used as the target key and then excluded from the encrypted payload.
pub fn encrypt_reaction(
    own_id: &Jid,
    root_chat: &Jid,
    root_sender: &Jid,
    root_msg_id: &str,
    orig_msg_secret: &[u8],
    reaction: &message::ReactionMessage,
) -> Result<message::EncReactionMessage, MsgSecretError> {
    // Clone and strip the key from the reaction before serialization
    let reaction_key = reaction.key.clone();
    let mut stripped = reaction.clone();
    stripped.key = None;

    let plaintext = stripped.encode_to_vec();

    let (ciphertext, iv) = encrypt_msg_secret(
        MsgSecretType::EncReaction,
        own_id,
        root_msg_id,
        root_sender,
        orig_msg_secret,
        &plaintext,
    )?;

    Ok(message::EncReactionMessage {
        target_message_key: reaction_key,
        enc_payload: Some(ciphertext),
        enc_iv: Some(iv),
    })
}

// ── Comment encrypt / decrypt ──────────────────────────────────────────────

/// Decrypt an encrypted comment in an announcement group.
///
/// Returns the decoded inner `wa::Message`.
///
/// # Arguments
///
/// * `enc_comment` - The `EncCommentMessage` from the incoming `wa::Message`.
/// * `modification_sender` - The JID of whoever sent the comment.
/// * `chat` - The chat JID.
/// * `orig_msg_secret` - The 32-byte secret from the original message.
pub fn decrypt_comment(
    enc_comment: &message::EncCommentMessage,
    modification_sender: &Jid,
    chat: &Jid,
    orig_msg_secret: &[u8],
) -> Result<wa::Message, MsgSecretError> {
    let target_key = enc_comment
        .target_message_key
        .as_ref()
        .ok_or(MsgSecretError::MissingTargetMessageKey)?;

    let orig_msg_id = target_key.id.as_deref().unwrap_or("");
    let orig_sender = get_orig_sender_from_key(target_key, modification_sender, chat)?;

    let plaintext = decrypt_msg_secret(
        MsgSecretType::EncComment,
        modification_sender,
        orig_msg_id,
        &orig_sender,
        orig_msg_secret,
        enc_comment,
    )?;

    wa::Message::decode(plaintext.as_slice()).map_err(MsgSecretError::ProtobufDecode)
}

/// Encrypt a comment message for an announcement group.
///
/// Returns a `wa::Message` wrapping an `EncCommentMessage`.
///
/// # Arguments
///
/// * `own_id` - The current user's JID.
/// * `root_chat` - The chat JID.
/// * `root_sender` - The JID of the original message sender.
/// * `root_msg_id` - The message ID being commented on.
/// * `root_is_from_me` - Whether the original message was sent by the current user.
/// * `orig_msg_secret` - The 32-byte secret from the original message.
/// * `comment` - The `wa::Message` to encrypt as a comment.
pub fn encrypt_comment(
    own_id: &Jid,
    root_chat: &Jid,
    root_sender: &Jid,
    root_msg_id: &str,
    root_is_from_me: bool,
    orig_msg_secret: &[u8],
    comment: &wa::Message,
) -> Result<wa::Message, MsgSecretError> {
    let plaintext = comment.encode_to_vec();

    let (ciphertext, iv) = encrypt_msg_secret(
        MsgSecretType::EncComment,
        own_id,
        root_msg_id,
        root_sender,
        orig_msg_secret,
        &plaintext,
    )?;

    Ok(wa::Message {
        enc_comment_message: Some(message::EncCommentMessage {
            target_message_key: Some(wa::MessageKey {
                remote_jid: Some(root_chat.to_string()),
                participant: Some(root_sender.to_non_ad().to_string()),
                from_me: Some(root_is_from_me),
                id: Some(root_msg_id.to_string()),
            }),
            enc_payload: Some(ciphertext),
            enc_iv: Some(iv),
        }),
        ..Default::default()
    })
}

// ── Secret encrypted message (event edits) ─────────────────────────────────

/// Decrypt a `SecretEncryptedMessage` (currently only EVENT_EDIT is supported).
///
/// Returns the decoded inner `wa::Message`. If the outer message has
/// `MessageContextInfo` but the decrypted inner message does not, the outer
/// context info is carried over to the inner message.
///
/// # Arguments
///
/// * `outer_msg` - The full `wa::Message` containing the `secret_encrypted_message`.
/// * `modification_sender` - The JID of whoever sent the modification.
/// * `chat` - The chat JID.
/// * `orig_msg_secret` - The 32-byte secret from the original message.
pub fn decrypt_secret_encrypted_message(
    outer_msg: &wa::Message,
    modification_sender: &Jid,
    chat: &Jid,
    orig_msg_secret: &[u8],
) -> Result<wa::Message, MsgSecretError> {
    let enc_msg = outer_msg
        .secret_encrypted_message
        .as_ref()
        .ok_or(MsgSecretError::NotSecretEncryptedMessage)?;

    // Only EVENT_EDIT is supported
    let enc_type = enc_msg.secret_enc_type.unwrap_or(0);
    if enc_type != message::secret_encrypted_message::SecretEncType::EventEdit as i32 {
        let type_name = message::secret_encrypted_message::SecretEncType::try_from(enc_type)
            .map(|t| t.as_str_name().to_string())
            .unwrap_or_else(|_| format!("unknown({})", enc_type));
        return Err(MsgSecretError::UnsupportedSecretEncType(type_name));
    }

    let target_key = enc_msg
        .target_message_key
        .as_ref()
        .ok_or(MsgSecretError::MissingTargetMessageKey)?;

    let orig_msg_id = target_key.id.as_deref().unwrap_or("");
    let orig_sender = get_orig_sender_from_key(target_key, modification_sender, chat)?;

    let plaintext = decrypt_msg_secret(
        MsgSecretType::EventEdit,
        modification_sender,
        orig_msg_id,
        &orig_sender,
        orig_msg_secret,
        enc_msg,
    )?;

    let mut msg =
        wa::Message::decode(plaintext.as_slice()).map_err(MsgSecretError::ProtobufDecode)?;

    // Carry over MessageContextInfo from outer message if inner lacks it
    if outer_msg.message_context_info.is_some() && msg.message_context_info.is_none() {
        msg.message_context_info = outer_msg.message_context_info.clone();
    }

    Ok(msg)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wacore_binary::jid::Jid;

    // ── MsgSecretType ──────────────────────────────────────────────────

    #[test]
    fn test_msg_secret_type_strings() {
        assert_eq!(MsgSecretType::PollVote.as_str(), "Poll Vote");
        assert_eq!(MsgSecretType::EncReaction.as_str(), "Enc Reaction");
        assert_eq!(MsgSecretType::EncComment.as_str(), "Enc Comment");
        assert_eq!(MsgSecretType::ReportToken.as_str(), "Report Token");
        assert_eq!(MsgSecretType::EventResponse.as_str(), "Event Response");
        assert_eq!(MsgSecretType::EventEdit.as_str(), "Event Edit");
        assert_eq!(MsgSecretType::BotMessage.as_str(), "Bot Message");
    }

    #[test]
    fn test_msg_secret_type_bytes() {
        assert_eq!(MsgSecretType::PollVote.as_bytes(), b"Poll Vote");
        assert_eq!(MsgSecretType::BotMessage.as_bytes(), b"Bot Message");
    }

    // ── HKDF + key derivation ──────────────────────────────────────────

    #[test]
    fn test_hkdf_sha256_output_length() {
        let result = hkdf_sha256(b"secret", None, b"info", 32);
        assert_eq!(result.len(), 32);

        let result = hkdf_sha256(b"secret", None, b"info", 64);
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn test_hkdf_sha256_deterministic() {
        let a = hkdf_sha256(b"key", None, b"info", 32);
        let b = hkdf_sha256(b"key", None, b"info", 32);
        assert_eq!(a, b);

        let c = hkdf_sha256(b"different_key", None, b"info", 32);
        assert_ne!(a, c);
    }

    #[test]
    fn test_apply_bot_message_hkdf() {
        let secret = [0x42u8; 32];
        let derived = apply_bot_message_hkdf(&secret);
        assert_eq!(derived.len(), 32);

        // Deterministic
        let derived2 = apply_bot_message_hkdf(&secret);
        assert_eq!(derived, derived2);

        // Different input produces different output
        let other_secret = [0x43u8; 32];
        let derived3 = apply_bot_message_hkdf(&other_secret);
        assert_ne!(derived, derived3);
    }

    #[test]
    fn test_generate_msg_secret_key_produces_32_byte_key() {
        let sender = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let secret = [0xABu8; 32];

        let (key, _aad) = generate_msg_secret_key(
            Some(MsgSecretType::PollVote),
            &sender,
            "3EB0MSG001",
            &orig_sender,
            &secret,
        );

        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_generate_msg_secret_key_poll_vote_has_aad() {
        let sender = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let secret = [0xABu8; 32];

        let (_key, aad) = generate_msg_secret_key(
            Some(MsgSecretType::PollVote),
            &sender,
            "3EB0MSG001",
            &orig_sender,
            &secret,
        );

        assert!(!aad.is_empty(), "PollVote should produce AAD");
        // AAD format: "{msg_id}\x00{sender}"
        let sender_str = sender.to_non_ad().to_string();
        let expected_aad_prefix = b"3EB0MSG001\x00";
        assert!(aad.starts_with(expected_aad_prefix));
        assert!(aad.ends_with(sender_str.as_bytes()));
    }

    #[test]
    fn test_generate_msg_secret_key_event_response_has_aad() {
        let sender = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let secret = [0xABu8; 32];

        let (_key, aad) = generate_msg_secret_key(
            Some(MsgSecretType::EventResponse),
            &sender,
            "MSG_ID",
            &orig_sender,
            &secret,
        );

        assert!(!aad.is_empty(), "EventResponse should produce AAD");
    }

    #[test]
    fn test_generate_msg_secret_key_enc_reaction_no_aad() {
        let sender = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let secret = [0xABu8; 32];

        let (_key, aad) = generate_msg_secret_key(
            Some(MsgSecretType::EncReaction),
            &sender,
            "MSG_ID",
            &orig_sender,
            &secret,
        );

        assert!(aad.is_empty(), "EncReaction should not produce AAD");
    }

    #[test]
    fn test_generate_msg_secret_key_enc_comment_no_aad() {
        let sender = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let secret = [0xABu8; 32];

        let (_key, aad) = generate_msg_secret_key(
            Some(MsgSecretType::EncComment),
            &sender,
            "MSG_ID",
            &orig_sender,
            &secret,
        );

        assert!(aad.is_empty(), "EncComment should not produce AAD");
    }

    #[test]
    fn test_generate_msg_secret_key_none_type_has_aad() {
        // None (bot message use case) should produce AAD
        let sender = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let secret = [0xABu8; 32];

        let (_key, aad) = generate_msg_secret_key(None, &sender, "MSG_ID", &orig_sender, &secret);

        assert!(!aad.is_empty(), "None (bot message) should produce AAD");
    }

    #[test]
    fn test_generate_msg_secret_key_different_types_different_keys() {
        let sender = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let secret = [0xABu8; 32];

        let (key1, _) = generate_msg_secret_key(
            Some(MsgSecretType::PollVote),
            &sender,
            "MSG_ID",
            &orig_sender,
            &secret,
        );

        let (key2, _) = generate_msg_secret_key(
            Some(MsgSecretType::EncReaction),
            &sender,
            "MSG_ID",
            &orig_sender,
            &secret,
        );

        assert_ne!(key1, key2, "different types must produce different keys");
    }

    #[test]
    fn test_generate_msg_secret_key_different_senders_different_keys() {
        let sender_a = Jid::pn("1111111111");
        let sender_b = Jid::pn("2222222222");
        let orig_sender = Jid::pn("9876543210");
        let secret = [0xABu8; 32];

        let (key1, _) = generate_msg_secret_key(
            Some(MsgSecretType::PollVote),
            &sender_a,
            "MSG_ID",
            &orig_sender,
            &secret,
        );

        let (key2, _) = generate_msg_secret_key(
            Some(MsgSecretType::PollVote),
            &sender_b,
            "MSG_ID",
            &orig_sender,
            &secret,
        );

        assert_ne!(key1, key2, "different senders must produce different keys");
    }

    // ── GCM encrypt / decrypt roundtrip ────────────────────────────────

    #[test]
    fn test_gcm_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let iv = [0x01u8; 12];
        let plaintext = b"hello world";
        let aad = b"additional data";

        let ciphertext = gcm_encrypt(&key, &iv, plaintext, aad).expect("encryption should succeed");
        let decrypted =
            gcm_decrypt(&key, &iv, &ciphertext, aad).expect("decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_gcm_encrypt_decrypt_no_aad() {
        let key = [0x42u8; 32];
        let iv = [0x01u8; 12];
        let plaintext = b"no additional data";

        let ciphertext = gcm_encrypt(&key, &iv, plaintext, &[]).expect("encryption should succeed");
        let decrypted =
            gcm_decrypt(&key, &iv, &ciphertext, &[]).expect("decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_gcm_decrypt_wrong_key_fails() {
        let key = [0x42u8; 32];
        let wrong_key = [0x43u8; 32];
        let iv = [0x01u8; 12];
        let plaintext = b"secret data";

        let ciphertext = gcm_encrypt(&key, &iv, plaintext, &[]).expect("encryption should succeed");
        let result = gcm_decrypt(&wrong_key, &iv, &ciphertext, &[]);

        assert!(result.is_err());
    }

    #[test]
    fn test_gcm_decrypt_wrong_aad_fails() {
        let key = [0x42u8; 32];
        let iv = [0x01u8; 12];
        let plaintext = b"secret data";

        let ciphertext =
            gcm_encrypt(&key, &iv, plaintext, b"correct aad").expect("encryption should succeed");
        let result = gcm_decrypt(&key, &iv, &ciphertext, b"wrong aad");

        assert!(result.is_err());
    }

    #[test]
    fn test_gcm_ciphertext_includes_tag() {
        let key = [0x42u8; 32];
        let iv = [0x01u8; 12];
        let plaintext = b"hello";

        let ciphertext = gcm_encrypt(&key, &iv, plaintext, &[]).expect("encryption should succeed");

        // Ciphertext should be plaintext length + 16 bytes GCM tag
        assert_eq!(ciphertext.len(), plaintext.len() + 16);
    }

    // ── encrypt_msg_secret / decrypt_msg_secret roundtrip ──────────────

    #[test]
    fn test_encrypt_decrypt_msg_secret_roundtrip() {
        let own_id = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let orig_secret = [0xDE; 32];
        let msg_id = "3EB0TESTMSG";
        let plaintext = b"encrypted poll vote payload";

        let (ciphertext, iv) = encrypt_msg_secret(
            MsgSecretType::PollVote,
            &own_id,
            msg_id,
            &orig_sender,
            &orig_secret,
            plaintext,
        )
        .expect("encryption should succeed");

        // Build a mock EncryptedSecret
        let mock = message::PollEncValue {
            enc_payload: Some(ciphertext),
            enc_iv: Some(iv),
        };

        let decrypted = decrypt_msg_secret(
            MsgSecretType::PollVote,
            &own_id,
            msg_id,
            &orig_sender,
            &orig_secret,
            &mock,
        )
        .expect("decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_msg_secret_enc_reaction_roundtrip() {
        let own_id = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let orig_secret = [0xBB; 32];
        let msg_id = "3EB0REACT";
        let plaintext = b"reaction data";

        let (ciphertext, iv) = encrypt_msg_secret(
            MsgSecretType::EncReaction,
            &own_id,
            msg_id,
            &orig_sender,
            &orig_secret,
            plaintext,
        )
        .expect("encryption should succeed");

        let mock = message::EncReactionMessage {
            target_message_key: None,
            enc_payload: Some(ciphertext),
            enc_iv: Some(iv),
        };

        let decrypted = decrypt_msg_secret(
            MsgSecretType::EncReaction,
            &own_id,
            msg_id,
            &orig_sender,
            &orig_secret,
            &mock,
        )
        .expect("decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_msg_secret_produces_random_iv() {
        let own_id = Jid::pn("1234567890");
        let orig_sender = Jid::pn("9876543210");
        let orig_secret = [0xAA; 32];

        let (_, iv1) = encrypt_msg_secret(
            MsgSecretType::PollVote,
            &own_id,
            "MSG1",
            &orig_sender,
            &orig_secret,
            b"data",
        )
        .unwrap();

        let (_, iv2) = encrypt_msg_secret(
            MsgSecretType::PollVote,
            &own_id,
            "MSG1",
            &orig_sender,
            &orig_secret,
            b"data",
        )
        .unwrap();

        assert_eq!(iv1.len(), GCM_IV_LENGTH);
        assert_eq!(iv2.len(), GCM_IV_LENGTH);
        // Extremely unlikely to be equal with random IVs
        assert_ne!(iv1, iv2, "IVs should be random and differ between calls");
    }

    // ── decrypt_msg_secret error cases ─────────────────────────────────

    #[test]
    fn test_decrypt_msg_secret_missing_iv() {
        let mock = message::PollEncValue {
            enc_payload: Some(vec![0u8; 32]),
            enc_iv: None,
        };

        let result = decrypt_msg_secret(
            MsgSecretType::PollVote,
            &Jid::pn("1234"),
            "MSG",
            &Jid::pn("5678"),
            &[0u8; 32],
            &mock,
        );

        assert!(matches!(result, Err(MsgSecretError::MissingEncIv)));
    }

    #[test]
    fn test_decrypt_msg_secret_missing_payload() {
        let mock = message::PollEncValue {
            enc_payload: None,
            enc_iv: Some(vec![0u8; 12]),
        };

        let result = decrypt_msg_secret(
            MsgSecretType::PollVote,
            &Jid::pn("1234"),
            "MSG",
            &Jid::pn("5678"),
            &[0u8; 32],
            &mock,
        );

        assert!(matches!(result, Err(MsgSecretError::MissingEncPayload)));
    }

    // ── Hash poll options ──────────────────────────────────────────────

    #[test]
    fn test_hash_poll_options_correct_count() {
        let hashes = hash_poll_options(&["Yes", "No", "Maybe"]);
        assert_eq!(hashes.len(), 3);
    }

    #[test]
    fn test_hash_poll_options_sha256_length() {
        let hashes = hash_poll_options(&["Hello"]);
        assert_eq!(hashes[0].len(), 32);
    }

    #[test]
    fn test_hash_poll_options_deterministic() {
        let h1 = hash_poll_options(&["Yes", "No"]);
        let h2 = hash_poll_options(&["Yes", "No"]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_poll_options_different_text_different_hash() {
        let hashes = hash_poll_options(&["Yes", "No"]);
        assert_ne!(hashes[0], hashes[1]);
    }

    #[test]
    fn test_hash_poll_options_empty() {
        let hashes = hash_poll_options(&[]);
        assert!(hashes.is_empty());
    }

    #[test]
    fn test_hash_poll_options_known_value() {
        // SHA-256("Yes") is a known value we can verify
        let hashes = hash_poll_options(&["Yes"]);
        let expected = {
            let mut h = Sha256::new();
            h.update(b"Yes");
            h.finalize().to_vec()
        };
        assert_eq!(hashes[0], expected);
    }

    #[test]
    fn test_hash_poll_options_owned() {
        let names = vec!["Red".to_string(), "Blue".to_string()];
        let h1 = hash_poll_options_owned(&names);
        let h2 = hash_poll_options(&["Red", "Blue"]);
        assert_eq!(h1, h2);
    }

    // ── Build poll creation ────────────────────────────────────────────

    #[test]
    fn test_build_poll_creation_basic() {
        let msg = build_poll_creation("Favorite?", &["A", "B", "C"], 1);

        let poll = msg.poll_creation_message.as_ref().unwrap();
        assert_eq!(poll.name.as_deref(), Some("Favorite?"));
        assert_eq!(poll.options.len(), 3);
        assert_eq!(poll.selectable_options_count, Some(1));

        let ctx = msg.message_context_info.as_ref().unwrap();
        let secret = ctx.message_secret.as_ref().unwrap();
        assert_eq!(secret.len(), 32);
    }

    #[test]
    fn test_build_poll_creation_option_names() {
        let msg = build_poll_creation("Q?", &["Yes", "No"], 0);
        let poll = msg.poll_creation_message.as_ref().unwrap();

        assert_eq!(poll.options[0].option_name.as_deref(), Some("Yes"));
        assert_eq!(poll.options[1].option_name.as_deref(), Some("No"));
    }

    #[test]
    fn test_build_poll_creation_clamps_count() {
        // Negative should clamp to 0
        let msg = build_poll_creation("Q?", &["A", "B"], -1);
        let poll = msg.poll_creation_message.as_ref().unwrap();
        assert_eq!(poll.selectable_options_count, Some(0));

        // Exceeds options count should clamp to 0
        let msg = build_poll_creation("Q?", &["A", "B"], 5);
        let poll = msg.poll_creation_message.as_ref().unwrap();
        assert_eq!(poll.selectable_options_count, Some(0));
    }

    #[test]
    fn test_build_poll_creation_secret_is_random() {
        let msg1 = build_poll_creation("Q?", &["A"], 1);
        let msg2 = build_poll_creation("Q?", &["A"], 1);

        let s1 = msg1.message_context_info.unwrap().message_secret.unwrap();
        let s2 = msg2.message_context_info.unwrap().message_secret.unwrap();
        assert_ne!(s1, s2, "each poll creation should have a unique secret");
    }

    // ── message_key_from_info ──────────────────────────────────────────

    #[test]
    fn test_message_key_from_info_dm() {
        let chat = Jid::pn("1234567890");
        let sender = Jid::pn("1234567890");

        let key = message_key_from_info(&chat, &sender, "MSG001", true, false);

        assert_eq!(key.remote_jid.as_deref(), Some("1234567890@s.whatsapp.net"));
        assert_eq!(key.from_me, Some(true));
        assert_eq!(key.id.as_deref(), Some("MSG001"));
        assert!(key.participant.is_none());
    }

    #[test]
    fn test_message_key_from_info_group() {
        let chat = Jid::group("120363012345");
        let sender = Jid::pn("9876543210");

        let key = message_key_from_info(&chat, &sender, "MSG002", false, true);

        assert_eq!(key.remote_jid.as_deref(), Some("120363012345@g.us"));
        assert_eq!(key.from_me, Some(false));
        assert_eq!(key.id.as_deref(), Some("MSG002"));
        assert!(key.participant.is_some());
    }

    // ── get_orig_sender_from_key ───────────────────────────────────────

    #[test]
    fn test_get_orig_sender_from_key_from_me() {
        let key = wa::MessageKey {
            from_me: Some(true),
            ..Default::default()
        };
        let mod_sender = Jid::pn("1234567890");
        let chat = Jid::group("12345");

        let result = get_orig_sender_from_key(&key, &mod_sender, &chat).unwrap();
        assert_eq!(result, mod_sender);
    }

    #[test]
    fn test_get_orig_sender_from_key_dm() {
        let key = wa::MessageKey {
            from_me: Some(false),
            remote_jid: Some("9876543210@s.whatsapp.net".to_string()),
            ..Default::default()
        };
        let mod_sender = Jid::pn("1234567890");
        let chat = Jid::pn("9876543210"); // DM chat

        let result = get_orig_sender_from_key(&key, &mod_sender, &chat).unwrap();
        assert_eq!(result.user, "9876543210");
    }

    #[test]
    fn test_get_orig_sender_from_key_group() {
        let key = wa::MessageKey {
            from_me: Some(false),
            participant: Some("5555555555@s.whatsapp.net".to_string()),
            ..Default::default()
        };
        let mod_sender = Jid::pn("1234567890");
        let chat = Jid::group("12345");

        let result = get_orig_sender_from_key(&key, &mod_sender, &chat).unwrap();
        assert_eq!(result.user, "5555555555");
    }

    #[test]
    fn test_get_orig_sender_from_key_group_bad_server() {
        let key = wa::MessageKey {
            from_me: Some(false),
            participant: Some("12345@g.us".to_string()), // group JID as participant is invalid
            ..Default::default()
        };
        let mod_sender = Jid::pn("1234567890");
        let chat = Jid::group("12345");

        let result = get_orig_sender_from_key(&key, &mod_sender, &chat);
        assert!(result.is_err());
    }

    // ── Poll vote encrypt / decrypt roundtrip ──────────────────────────

    #[test]
    fn test_poll_vote_encrypt_decrypt_roundtrip() {
        let own_id = Jid::pn("1111111111");
        let poll_sender = Jid::pn("2222222222");
        let poll_chat = Jid::group("120363012345");
        let poll_msg_id = "3EB0POLL001";
        let orig_secret = [0xCC; 32];

        let vote = message::PollVoteMessage {
            selected_options: hash_poll_options(&["Option A", "Option C"]),
        };

        let poll_update = encrypt_poll_vote(
            &own_id,
            &poll_chat,
            &poll_sender,
            poll_msg_id,
            true,
            &orig_secret,
            &vote,
            1713100000000,
        )
        .expect("encryption should succeed");

        assert!(poll_update.vote.is_some());
        assert!(poll_update.poll_creation_message_key.is_some());
        assert_eq!(poll_update.sender_timestamp_ms, Some(1713100000000));

        let decrypted = decrypt_poll_vote(&poll_update, &own_id, &poll_chat, &orig_secret)
            .expect("decryption should succeed");

        assert_eq!(decrypted.selected_options, vote.selected_options);
    }

    // ── build_poll_vote ────────────────────────────────────────────────

    #[test]
    fn test_build_poll_vote() {
        let own_id = Jid::pn("1111111111");
        let poll_sender = Jid::pn("2222222222");
        let poll_chat = Jid::group("120363012345");
        let orig_secret = [0xDD; 32];

        let msg = build_poll_vote(
            &own_id,
            &poll_chat,
            &poll_sender,
            "3EB0POLL",
            true,
            &orig_secret,
            &["Yes", "Maybe"],
            1713100000000,
        )
        .expect("build should succeed");

        assert!(msg.poll_update_message.is_some());
        let update = msg.poll_update_message.unwrap();
        assert!(update.vote.is_some());
        assert!(update.poll_creation_message_key.is_some());
    }

    // ── Reaction encrypt / decrypt roundtrip ───────────────────────────

    #[test]
    fn test_reaction_encrypt_decrypt_roundtrip() {
        let own_id = Jid::pn("1111111111");
        let orig_sender = Jid::pn("2222222222");
        let chat = Jid::group("120363012345");
        let msg_id = "3EB0REACT001";
        let orig_secret = [0xEE; 32];

        let target_key = wa::MessageKey {
            remote_jid: Some(chat.to_string()),
            from_me: Some(false),
            id: Some(msg_id.to_string()),
            participant: Some(orig_sender.to_string()),
        };

        let reaction = message::ReactionMessage {
            key: Some(target_key.clone()),
            text: Some("\u{1F44D}".to_string()),
            grouping_key: None,
            sender_timestamp_ms: Some(1713100000000),
        };

        let enc = encrypt_reaction(
            &own_id,
            &chat,
            &orig_sender,
            msg_id,
            &orig_secret,
            &reaction,
        )
        .expect("encryption should succeed");

        assert!(enc.target_message_key.is_some());
        assert!(enc.enc_payload.is_some());
        assert!(enc.enc_iv.is_some());

        let decrypted = decrypt_reaction(&enc, &own_id, &chat, &orig_secret)
            .expect("decryption should succeed");

        assert_eq!(decrypted.text.as_deref(), Some("\u{1F44D}"));
        // The key should be stripped from the encrypted payload
        assert!(decrypted.key.is_none());
    }

    // ── Comment encrypt / decrypt roundtrip ────────────────────────────

    #[test]
    fn test_comment_encrypt_decrypt_roundtrip() {
        let own_id = Jid::pn("1111111111");
        let orig_sender = Jid::pn("2222222222");
        let chat = Jid::group("120363012345");
        let msg_id = "3EB0COMMENT001";
        let orig_secret = [0xFF; 32];

        let inner_comment = wa::Message {
            conversation: Some("This is a comment".to_string()),
            ..Default::default()
        };

        let encrypted_msg = encrypt_comment(
            &own_id,
            &chat,
            &orig_sender,
            msg_id,
            false,
            &orig_secret,
            &inner_comment,
        )
        .expect("encryption should succeed");

        assert!(encrypted_msg.enc_comment_message.is_some());
        let enc_comment = encrypted_msg.enc_comment_message.as_ref().unwrap();
        assert!(enc_comment.target_message_key.is_some());

        let decrypted = decrypt_comment(enc_comment, &own_id, &chat, &orig_secret)
            .expect("decryption should succeed");

        assert_eq!(decrypted.conversation.as_deref(), Some("This is a comment"));
    }

    // ── Secret encrypted message (event edit) ──────────────────────────

    #[test]
    fn test_decrypt_secret_encrypted_message_event_edit() {
        let mod_sender = Jid::pn("1111111111");
        let orig_sender = Jid::pn("2222222222");
        let chat = Jid::group("120363012345");
        let msg_id = "3EB0EVENT001";
        let orig_secret = [0xAA; 32];

        // Build the inner message
        let inner = wa::Message {
            conversation: Some("Updated event details".to_string()),
            ..Default::default()
        };
        let plaintext = inner.encode_to_vec();

        // Encrypt manually using the event edit key
        let (ciphertext, iv) = encrypt_msg_secret(
            MsgSecretType::EventEdit,
            &mod_sender,
            msg_id,
            &orig_sender,
            &orig_secret,
            &plaintext,
        )
        .unwrap();

        // Build the outer message with SecretEncryptedMessage
        let outer = wa::Message {
            secret_encrypted_message: Some(message::SecretEncryptedMessage {
                target_message_key: Some(wa::MessageKey {
                    remote_jid: Some(chat.to_string()),
                    from_me: Some(false),
                    id: Some(msg_id.to_string()),
                    participant: Some(orig_sender.to_string()),
                }),
                enc_payload: Some(ciphertext),
                enc_iv: Some(iv),
                secret_enc_type: Some(
                    message::secret_encrypted_message::SecretEncType::EventEdit as i32,
                ),
            }),
            message_context_info: Some(wa::MessageContextInfo {
                message_secret: Some(vec![0xBB; 32]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let decrypted = decrypt_secret_encrypted_message(&outer, &mod_sender, &chat, &orig_secret)
            .expect("decryption should succeed");

        assert_eq!(
            decrypted.conversation.as_deref(),
            Some("Updated event details")
        );
        // MessageContextInfo should be carried over
        assert!(decrypted.message_context_info.is_some());
    }

    #[test]
    fn test_decrypt_secret_encrypted_message_not_present() {
        let outer = wa::Message::default();
        let result = decrypt_secret_encrypted_message(
            &outer,
            &Jid::pn("1234"),
            &Jid::group("12345"),
            &[0u8; 32],
        );
        assert!(matches!(
            result,
            Err(MsgSecretError::NotSecretEncryptedMessage)
        ));
    }

    #[test]
    fn test_decrypt_secret_encrypted_message_unsupported_type() {
        let outer = wa::Message {
            secret_encrypted_message: Some(message::SecretEncryptedMessage {
                target_message_key: Some(wa::MessageKey::default()),
                enc_payload: Some(vec![0u8; 32]),
                enc_iv: Some(vec![0u8; 12]),
                secret_enc_type: Some(
                    message::secret_encrypted_message::SecretEncType::Unknown as i32,
                ),
            }),
            ..Default::default()
        };

        let result = decrypt_secret_encrypted_message(
            &outer,
            &Jid::pn("1234"),
            &Jid::group("12345"),
            &[0u8; 32],
        );

        assert!(matches!(
            result,
            Err(MsgSecretError::UnsupportedSecretEncType(_))
        ));
    }

    // ── Bot message decrypt ────────────────────────────────────────────

    #[test]
    fn test_decrypt_bot_message_roundtrip() {
        let info_sender = Jid::pn("1111111111");
        let target_sender = Jid::pn("2222222222");
        let message_id = "3EB0BOT001";
        let bot_secret = [0xBB; 32];
        let plaintext = b"bot response data";

        // Derive the same way the decrypt function does internally
        let derived = apply_bot_message_hkdf(&bot_secret);
        let (key, aad) =
            generate_msg_secret_key(None, &info_sender, message_id, &target_sender, &derived);

        let mut iv = [0u8; GCM_IV_LENGTH];
        rand::fill(&mut iv).unwrap();

        let ciphertext =
            gcm_encrypt(&key, &iv, plaintext, &aad).expect("encryption should succeed");

        let mock = message::PollEncValue {
            enc_payload: Some(ciphertext),
            enc_iv: Some(iv.to_vec()),
        };

        let decrypted =
            decrypt_bot_message(&bot_secret, &mock, message_id, &target_sender, &info_sender)
                .expect("decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    // ── Error display ──────────────────────────────────────────────────

    #[test]
    fn test_error_display() {
        assert_eq!(
            MsgSecretError::OriginalSecretNotFound.to_string(),
            "original message secret not found"
        );
        assert_eq!(
            MsgSecretError::NotPollUpdateMessage.to_string(),
            "not a poll update message"
        );
        assert_eq!(
            MsgSecretError::MissingEncIv.to_string(),
            "missing encrypted IV"
        );
        assert_eq!(
            MsgSecretError::UnsupportedSecretEncType("UNKNOWN".to_string()).to_string(),
            "unsupported secret enc type: UNKNOWN"
        );
    }
}
