//! Storage traits for the WhatsApp client.
//!
//! This module defines 6 domain-grouped traits that together form the `Backend` trait:
//!
//! - [`SignalStore`]: Signal protocol cryptographic operations (identity, sessions, keys)
//! - [`AppSyncStore`]: WhatsApp app state synchronization
//! - [`ProtocolStore`]: WhatsApp Web protocol alignment (SKDM, LID mapping, device registry)
//! - [`DeviceStore`]: Device persistence operations
//! - [`MsgSecretStore`]: Per-message encryption secret storage
//! - [`ChatSettingsStore`]: Local chat settings (mute, pin, archive)

use crate::appstate::hash::HashState;
use crate::store::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use wacore_appstate::processor::AppStateMutationMAC;
use wacore_binary::jid::Jid;

/// App state synchronization key for WhatsApp's app state protocol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppStateSyncKey {
    pub key_data: Vec<u8>,
    pub fingerprint: Vec<u8>,
    pub timestamp: i64,
}

/// Entry representing a LID to Phone Number mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LidPnMappingEntry {
    /// The LID user part (e.g., "100000012345678")
    pub lid: String,
    /// The phone number user part (e.g., "559980000001")
    pub phone_number: String,
    /// Unix timestamp when the mapping was first learned
    pub created_at: i64,
    /// Unix timestamp when the mapping was last updated
    pub updated_at: i64,
    /// The source from which this mapping was learned (e.g., "usync", "peer_pn_message")
    pub learning_source: String,
}

/// Trusted contact privacy token entry.
///
/// Matches WhatsApp Web's Chat.tcToken / tcTokenTimestamp / tcTokenSenderTimestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcTokenEntry {
    /// Raw token bytes received from the server.
    pub token: Vec<u8>,
    /// Unix timestamp (seconds) when the token was received.
    pub token_timestamp: i64,
    /// Unix timestamp (seconds) when we last issued our token to this contact.
    pub sender_timestamp: Option<i64>,
}

/// Device information for registry tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// The device ID (0 = primary device, 1+ = companion devices)
    pub device_id: u32,
    /// The key index, if known
    pub key_index: Option<u32>,
}

/// Device list record matching WhatsApp Web's DeviceListRecord structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceListRecord {
    /// The user part of the JID (phone number or LID)
    pub user: String,
    /// List of known devices for this user
    pub devices: Vec<DeviceInfo>,
    /// Timestamp when this record was last updated
    pub timestamp: i64,
    /// Participant hash from usync, if available
    pub phash: Option<String>,
}

/// A saved WhatsApp contact from app state sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactEntry {
    /// The JID (e.g., "2347012856059@s.whatsapp.net")
    pub jid: String,
    /// Full name (from phone's address book)
    pub full_name: String,
    /// First name (from phone's address book)
    pub first_name: String,
    /// Push name (self-chosen WhatsApp display name)
    pub push_name: String,
}

/// A batch insert entry for message secrets.
///
/// Used by [`MsgSecretStore::put_message_secrets`] to store multiple secrets
/// in a single operation.
#[derive(Debug, Clone)]
pub struct MessageSecretInsert {
    /// The chat JID (e.g., "1234567890@s.whatsapp.net" or "group@g.us")
    pub chat: String,
    /// The sender JID
    pub sender: String,
    /// The message ID
    pub id: String,
    /// The raw encryption secret bytes from MessageContextInfo
    pub secret: Vec<u8>,
}

/// Local chat settings for per-chat preferences.
///
/// Matches whatsmeow's `LocalChatSettings` structure used to track mute/pin/archive
/// state synced from the app state protocol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatSettings {
    /// Unix timestamp (seconds) until which the chat is muted, or None if not muted.
    pub muted_until: Option<i64>,
    /// Whether the chat is pinned.
    pub pinned: bool,
    /// Whether the chat is archived.
    pub archived: bool,
}

/// Signal protocol cryptographic storage operations.
///
/// Handles identity keys, sessions, pre-keys, signed pre-keys, and sender keys
/// for end-to-end encryption.
#[async_trait]
pub trait SignalStore: Send + Sync {
    // --- Identity Operations ---

    /// Store an identity key for a remote address.
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> Result<()>;

    /// Load an identity key for a remote address.
    async fn load_identity(&self, address: &str) -> Result<Option<Vec<u8>>>;

    /// Delete an identity key.
    async fn delete_identity(&self, address: &str) -> Result<()>;

    // --- Session Operations ---

    /// Get an encrypted session for an address.
    async fn get_session(&self, address: &str) -> Result<Option<Vec<u8>>>;

    /// Store an encrypted session.
    async fn put_session(&self, address: &str, session: &[u8]) -> Result<()>;

    /// Delete a session.
    async fn delete_session(&self, address: &str) -> Result<()>;

    /// Check if a session exists. Default implementation uses `get_session`.
    async fn has_session(&self, address: &str) -> Result<bool> {
        Ok(self.get_session(address).await?.is_some())
    }

    // --- PreKey Operations ---

    /// Store a pre-key.
    async fn store_prekey(&self, id: u32, record: &[u8], uploaded: bool) -> Result<()>;

    /// Load a pre-key by ID.
    async fn load_prekey(&self, id: u32) -> Result<Option<Vec<u8>>>;

    /// Remove a pre-key.
    async fn remove_prekey(&self, id: u32) -> Result<()>;

    // --- Signed PreKey Operations ---

    /// Store a signed pre-key.
    async fn store_signed_prekey(&self, id: u32, record: &[u8]) -> Result<()>;

    /// Load a signed pre-key by ID.
    async fn load_signed_prekey(&self, id: u32) -> Result<Option<Vec<u8>>>;

    /// Load all signed pre-keys. Returns (id, record) pairs.
    async fn load_all_signed_prekeys(&self) -> Result<Vec<(u32, Vec<u8>)>>;

    /// Remove a signed pre-key.
    async fn remove_signed_prekey(&self, id: u32) -> Result<()>;

    // --- Sender Key Operations ---

    /// Store a sender key for group messaging.
    async fn put_sender_key(&self, address: &str, record: &[u8]) -> Result<()>;

    /// Get a sender key.
    async fn get_sender_key(&self, address: &str) -> Result<Option<Vec<u8>>>;

    /// Delete a sender key.
    async fn delete_sender_key(&self, address: &str) -> Result<()>;

    // --- PN → LID Migration ---

    /// Migrate Signal sessions, identity keys, and sender keys from phone number
    /// (PN) addressing to LID addressing. Called when a PN-to-LID mapping is
    /// discovered and existing crypto sessions need to move to the new address
    /// format.
    ///
    /// The address column in sessions/identities/sender_keys tables uses the
    /// format `"user@server:device"`. This method replaces occurrences of
    /// `pn_user` in the user part with `lid_user`.
    ///
    /// Default implementation is a no-op for backends that don't persist Signal
    /// state or don't need migration.
    async fn migrate_pn_to_lid(&self, _pn_user: &str, _lid_user: &str) -> Result<()> {
        Ok(())
    }
}

/// WhatsApp app state synchronization storage.
///
/// Handles sync keys, version tracking, and mutation MACs for the app state protocol.
#[async_trait]
pub trait AppSyncStore: Send + Sync {
    /// Get an app state sync key by ID.
    async fn get_sync_key(&self, key_id: &[u8]) -> Result<Option<AppStateSyncKey>>;

    /// Set an app state sync key.
    async fn set_sync_key(&self, key_id: &[u8], key: AppStateSyncKey) -> Result<()>;

    /// Get the most recently stored app state sync key.
    /// Returns (key_id, key) or None if no keys exist.
    async fn get_latest_sync_key(&self) -> Result<Option<(Vec<u8>, AppStateSyncKey)>>;

    /// Get the app state version for a collection.
    async fn get_version(&self, name: &str) -> Result<HashState>;

    /// Set the app state version for a collection.
    async fn set_version(&self, name: &str, state: HashState) -> Result<()>;

    /// Store mutation MACs for a version.
    async fn put_mutation_macs(
        &self,
        name: &str,
        version: u64,
        mutations: &[AppStateMutationMAC],
    ) -> Result<()>;

    /// Get a mutation MAC by index.
    async fn get_mutation_mac(&self, name: &str, index_mac: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Delete mutation MACs by their index MACs.
    async fn delete_mutation_macs(&self, name: &str, index_macs: &[Vec<u8>]) -> Result<()>;
}

/// WhatsApp Web protocol alignment storage.
///
/// Handles SKDM tracking, LID-PN mapping, base key collision detection,
/// device registry, and sender key status.
#[async_trait]
pub trait ProtocolStore: Send + Sync {
    // --- SKDM Tracking ---

    /// Get device JIDs that have received SKDM for a group.
    async fn get_skdm_recipients(&self, group_jid: &str) -> Result<Vec<Jid>>;

    /// Record devices that have received SKDM for a group.
    async fn add_skdm_recipients(&self, group_jid: &str, device_jids: &[Jid]) -> Result<()>;

    /// Clear SKDM recipients for a group (call when sender key is rotated).
    async fn clear_skdm_recipients(&self, group_jid: &str) -> Result<()>;

    // --- LID-PN Mapping ---

    /// Get a mapping by LID.
    async fn get_lid_mapping(&self, lid: &str) -> Result<Option<LidPnMappingEntry>>;

    /// Get a mapping by phone number (returns the most recent LID for that phone).
    async fn get_pn_mapping(&self, phone: &str) -> Result<Option<LidPnMappingEntry>>;

    /// Store or update a LID-PN mapping.
    async fn put_lid_mapping(&self, entry: &LidPnMappingEntry) -> Result<()>;

    /// Get all LID-PN mappings (for cache warm-up).
    async fn get_all_lid_mappings(&self) -> Result<Vec<LidPnMappingEntry>>;

    // --- Base Key Collision Detection ---

    /// Save the base key for a session address during retry collision detection.
    async fn save_base_key(&self, address: &str, message_id: &str, base_key: &[u8]) -> Result<()>;

    /// Check if the current session has the same base key as the saved one.
    async fn has_same_base_key(
        &self,
        address: &str,
        message_id: &str,
        current_base_key: &[u8],
    ) -> Result<bool>;

    /// Delete a base key entry.
    async fn delete_base_key(&self, address: &str, message_id: &str) -> Result<()>;

    // --- Device Registry ---

    /// Update the device list for a user (called after usync responses).
    async fn update_device_list(&self, record: DeviceListRecord) -> Result<()>;

    /// Get all known devices for a user.
    async fn get_devices(&self, user: &str) -> Result<Option<DeviceListRecord>>;

    // --- Sender Key Status (Lazy Deletion) ---

    /// Mark a participant's sender key as needing regeneration for a group.
    async fn mark_forget_sender_key(&self, group_jid: &str, participant: &str) -> Result<()>;

    /// Get participants that need fresh SKDM (marked for forget).
    /// Consumes the marks (deletes them after reading).
    async fn consume_forget_marks(&self, group_jid: &str) -> Result<Vec<String>>;

    // --- TcToken Storage ---

    /// Get a trusted contact token for a JID (stored under LID).
    async fn get_tc_token(&self, jid: &str) -> Result<Option<TcTokenEntry>>;

    /// Store or update a trusted contact token for a JID.
    async fn put_tc_token(&self, jid: &str, entry: &TcTokenEntry) -> Result<()>;

    /// Delete a trusted contact token for a JID.
    async fn delete_tc_token(&self, jid: &str) -> Result<()>;

    /// Get all JIDs that have stored tc tokens.
    async fn get_all_tc_token_jids(&self) -> Result<Vec<String>>;

    /// Delete tc tokens with token_timestamp older than cutoff. Returns count deleted.
    async fn delete_expired_tc_tokens(&self, cutoff_timestamp: i64) -> Result<u32>;

    // --- Contact Storage ---

    /// Store or update a contact from app state sync.
    async fn put_contact(&self, jid: &str, full_name: &str, first_name: &str) -> Result<()>;

    /// Update the push name for a contact (from incoming messages).
    ///
    /// Returns `(changed, old_push_name)` where `changed` is true if the push name
    /// was different from the previously stored one. This matches whatsmeow's
    /// `PutPushName` behavior which returns `(bool, string, error)` so the caller
    /// can emit `PushNameUpdate` events when the name changes.
    async fn put_contact_push_name(&self, jid: &str, push_name: &str) -> Result<(bool, String)>;

    /// Get all saved contacts (contacts with non-empty full_name or first_name).
    /// These represent the phone's actual address book contacts on WhatsApp.
    async fn get_saved_contacts(&self) -> Result<Vec<ContactEntry>>;

    /// Get all contacts (including push-name-only entries).
    async fn get_all_contacts(&self) -> Result<Vec<ContactEntry>>;

    /// Update the business name for a contact (from verified name certificate).
    ///
    /// Returns `(changed, old_business_name)` where `changed` is true if the
    /// business name was different from the previously stored one. This matches
    /// whatsmeow's `PutBusinessName` behavior so the caller can emit
    /// `BusinessNameUpdate` events when the name changes.
    async fn put_business_name(&self, jid: &str, business_name: &str) -> Result<(bool, String)>;

    /// Bulk insert contacts from app state sync.
    ///
    /// Mirrors whatsmeow's `PutAllContactNames` — used during full sync of
    /// `critical_unblock_low` to batch-insert all contact mutations in a single
    /// transaction instead of dispatching them individually.
    ///
    /// The default implementation falls back to calling `put_contact` in a loop.
    async fn put_all_contact_names(&self, contacts: &[ContactEntry]) -> Result<()> {
        for c in contacts {
            self.put_contact(&c.jid, &c.full_name, &c.first_name).await?;
        }
        Ok(())
    }
}

/// Device data persistence operations.
#[async_trait]
pub trait DeviceStore: Send + Sync {
    /// Save device data.
    async fn save(&self, device: &crate::store::Device) -> Result<()>;

    /// Load device data.
    async fn load(&self) -> Result<Option<crate::store::Device>>;

    /// Check if a device exists.
    async fn exists(&self) -> Result<bool>;

    /// Create a new device row and return its generated device_id.
    async fn create(&self) -> Result<i32>;

    /// Create a snapshot of the database state.
    /// The argument `name` can be used to label the snapshot file.
    /// `extra_content` can be used to save a related binary blob (e.g. the message that caused the failure).
    async fn snapshot_db(&self, _name: &str, _extra_content: Option<&[u8]>) -> Result<()> {
        Ok(())
    }
}

/// Per-message encryption secret storage.
///
/// Stores secrets from `MessageContextInfo.message_secret` for later decryption of
/// poll votes, encrypted reactions, comments, and bot messages. Maps whatsmeow's
/// `MsgSecretStore` interface.
#[async_trait]
pub trait MsgSecretStore: Send + Sync {
    /// Store a single message secret.
    async fn put_message_secret(
        &self,
        chat: &str,
        sender: &str,
        id: &str,
        secret: &[u8],
    ) -> Result<()>;

    /// Store multiple message secrets in a single batch operation.
    async fn put_message_secrets(&self, inserts: &[MessageSecretInsert]) -> Result<()>;

    /// Retrieve a message secret.
    ///
    /// Returns `(secret_bytes, sender_jid)` if found, or `None` if no secret
    /// exists for the given chat/sender/id combination.
    async fn get_message_secret(
        &self,
        chat: &str,
        sender: &str,
        id: &str,
    ) -> Result<Option<(Vec<u8>, String)>>;
}

/// Local chat settings storage.
///
/// Stores per-chat preferences (mute, pin, archive) synced from the app state
/// protocol. Maps whatsmeow's `ChatSettingsStore` interface.
#[async_trait]
pub trait ChatSettingsStore: Send + Sync {
    /// Set the mute-until timestamp for a chat.
    async fn put_muted_until(&self, chat: &str, muted_until: i64) -> Result<()>;

    /// Set the pinned state for a chat.
    async fn put_pinned(&self, chat: &str, pinned: bool) -> Result<()>;

    /// Set the archived state for a chat.
    async fn put_archived(&self, chat: &str, archived: bool) -> Result<()>;

    /// Get all chat settings for a chat. Returns defaults if no settings exist.
    async fn get_chat_settings(&self, chat: &str) -> Result<ChatSettings>;
}

/// A buffered inbound event, keyed by ciphertext hash.
///
/// Stored after successful decryption so that a duplicate ciphertext can be
/// recognised (dedup) without re-decrypting. The plaintext is cleared after
/// processing to save space while the hash entry is kept for dedup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferedEvent {
    /// The decrypted plaintext of the event.
    pub plaintext: Vec<u8>,
    /// Unix timestamp (seconds) when this entry was inserted into the buffer.
    pub insert_time: i64,
    /// Unix timestamp (seconds) from the server's `t` attribute on the stanza.
    pub server_time: i64,
}

/// Buffered event storage for message deduplication and retry persistence.
///
/// Two purposes:
/// 1. **Inbound**: buffer decrypted events by ciphertext hash to prevent
///    double-processing when the server re-delivers a stanza.
/// 2. **Outbound**: store sent messages for re-encryption on retry receipt
///    so the client can fulfil a retry request without the caller re-sending.
///
/// Mirrors whatsmeow's `EventBuffer` interface from `store/store.go`.
///
/// **Note**: This trait is intentionally *not* added to the `Backend` super-trait
/// yet to avoid breaking existing store implementations. Consumers that need
/// event buffering should require `Backend + EventBuffer` explicitly.
#[async_trait]
pub trait EventBuffer: Send + Sync {
    /// Get a buffered inbound event by ciphertext hash.
    ///
    /// Returns `None` if no entry exists for the given hash.
    async fn get_buffered_event(
        &self,
        ciphertext_hash: &[u8; 32],
    ) -> Result<Option<BufferedEvent>>;

    /// Store a buffered inbound event.
    ///
    /// `ciphertext_hash` is the SHA-256 of the raw ciphertext bytes.
    /// `plaintext` is the decrypted protobuf payload.
    /// `server_timestamp` is the `t` attribute from the incoming stanza.
    async fn put_buffered_event(
        &self,
        ciphertext_hash: &[u8; 32],
        plaintext: &[u8],
        server_timestamp: i64,
    ) -> Result<()>;

    /// Clear the plaintext of a buffered event (keep hash for dedup).
    ///
    /// Called after the event has been fully processed. The hash entry remains
    /// so future duplicates can be detected without the plaintext.
    async fn clear_buffered_event_plaintext(
        &self,
        ciphertext_hash: &[u8; 32],
    ) -> Result<()>;

    /// Delete old buffered event hashes.
    ///
    /// Implementation should delete entries whose `insert_time` is older than
    /// a reasonable retention period (e.g., 7 days).
    async fn delete_old_buffered_hashes(&self) -> Result<()>;

    /// Get an outgoing event for retry re-encryption.
    ///
    /// Looks up a previously sent message by its chat JID and message ID.
    /// `alt_chat` is the alternative chat JID (e.g., LID variant) for
    /// cross-addressing lookups.
    ///
    /// Returns `(format, plaintext)` where `format` is the serialisation
    /// format identifier (e.g., "proto").
    async fn get_outgoing_event(
        &self,
        chat: &str,
        alt_chat: &str,
        id: &str,
    ) -> Result<Option<(String, Vec<u8>)>>;

    /// Store an outgoing event for potential retry.
    ///
    /// Called after a message is successfully encrypted and sent. If the
    /// server later sends a retry receipt, the stored plaintext can be
    /// re-encrypted without the caller re-sending.
    async fn add_outgoing_event(
        &self,
        chat: &str,
        id: &str,
        format: &str,
        plaintext: &[u8],
    ) -> Result<()>;

    /// Delete old outgoing events.
    ///
    /// Implementation should delete entries older than a reasonable retention
    /// period (e.g., 7 days).
    async fn delete_old_outgoing_events(&self) -> Result<()>;
}

/// Combined storage backend trait.
///
/// Any type implementing all six domain traits automatically implements `Backend`.
pub trait Backend:
    SignalStore
    + AppSyncStore
    + ProtocolStore
    + DeviceStore
    + MsgSecretStore
    + ChatSettingsStore
    + Send
    + Sync
{
}

impl<T> Backend for T where
    T: SignalStore
        + AppSyncStore
        + ProtocolStore
        + DeviceStore
        + MsgSecretStore
        + ChatSettingsStore
        + Send
        + Sync
{
}
