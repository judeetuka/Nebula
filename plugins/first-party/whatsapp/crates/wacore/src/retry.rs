//! Message retry handling for the WhatsApp protocol.
//!
//! Ports whatsmeow/retry.go — handles outgoing retry receipts (when we cannot
//! decrypt an incoming message), incoming retry receipt handling (re-encrypting
//! and resending a previously sent message), and delayed re-request from phone.
//!
//! This module provides the protocol-layer data structures, node builders, and
//! retry logic. Actual message sending and Signal session operations are
//! performed by the client layer.
//!
//! Wire format:
//! ```xml
//! <!-- Outgoing retry receipt (we failed to decrypt) -->
//! <receipt id="MSG_ID" type="retry" to="FROM_JID" participant="...">
//!   <retry count="1" id="MSG_ID" t="TIMESTAMP" v="1"/>
//!   <registration>REG_ID_BYTES</registration>
//!   <!-- Optional prekey bundle on retry count > 1 -->
//!   <keys>
//!     <type>05</type>
//!     <identity>IDENTITY_KEY</identity>
//!     <key><id>PRE_KEY_ID</id><value>PRE_KEY_PUB</value></key>
//!     <skey><id>SIGNED_ID</id><value>SIGNED_PUB</value><signature>SIG</signature></skey>
//!     <device-identity>DEVICE_IDENTITY</device-identity>
//!   </keys>
//! </receipt>
//!
//! <!-- Incoming retry receipt (peer failed to decrypt our message) -->
//! <receipt id="MSG_ID" type="retry" from="PEER_JID" ...>
//!   <retry count="N" id="MSG_ID" t="TIMESTAMP" v="1"/>
//!   <registration>REG_ID_BYTES</registration>
//!   <!-- Optional prekey bundle from peer -->
//!   <keys>...</keys>
//! </receipt>
//! ```

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::Serialize;
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, MessageId};
use wacore_binary::node::Node;

// ── Constants ───────────────────────────────────────────────────────────────

/// Number of sent messages to cache in the ring buffer for handling retry
/// receipts. Matches whatsmeow's `recentMessagesSize`.
pub const RECENT_MESSAGES_SIZE: usize = 256;

/// If a retry comes after this duration since the last session recreation,
/// the Signal session will be recreated. Matches whatsmeow's
/// `recreateSessionTimeout` (1 hour).
pub const RECREATE_SESSION_TIMEOUT: chrono::Duration = chrono::Duration::hours(1);

/// Maximum number of retry receipts we will send for a single inbound
/// message before giving up. Matches whatsmeow's cap of 5.
pub const MAX_RETRY_COUNT: u32 = 5;

/// Maximum number of incoming retry requests we accept per message before
/// dropping further requests. Anti-flood protection.
pub const MAX_INCOMING_RETRY: u32 = 10;

/// How long to wait before requesting re-delivery from the paired phone,
/// giving the original sender time to resend first.
pub const REQUEST_FROM_PHONE_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

/// Protocol version for the `<retry>` node's `v` attribute.
const RETRY_VERSION: u32 = 1;

// ── Recent message cache ────────────────────────────────────────────────────

/// Key for the recent-message ring buffer, scoping cached messages by
/// destination JID and message ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecentMessageKey {
    pub to: Jid,
    pub id: MessageId,
}

impl RecentMessageKey {
    pub fn new(to: Jid, id: MessageId) -> Self {
        Self { to, id }
    }
}

/// Format discriminator for serialised message payloads in the retry store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MessageFormat {
    /// Standard WhatsApp E2E message (waE2E.Message).
    Wa,
    /// Facebook Messenger application message (waMsgApplication).
    Fb,
}

impl MessageFormat {
    /// Wire-format string used for persistent storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wa => "wa",
            Self::Fb => "fb",
        }
    }

    /// Parse from the wire-format string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "wa" => Some(Self::Wa),
            "fb" => Some(Self::Fb),
            _ => None,
        }
    }
}

/// A cached copy of a recently sent message, stored for retry handling.
///
/// The payload is kept as serialised protobuf bytes together with a format
/// discriminator so that the client layer can deserialise with the correct
/// proto type.
#[derive(Debug, Clone, Serialize)]
pub struct RecentMessage {
    /// Serialised protobuf payload.
    pub payload: Vec<u8>,
    /// Whether this is a `wa` or `fb` format message.
    pub format: MessageFormat,
}

impl RecentMessage {
    /// Create a new cached message.
    pub fn new(payload: Vec<u8>, format: MessageFormat) -> Self {
        Self { payload, format }
    }

    /// Returns `true` if the payload is empty (no usable message data).
    pub fn is_empty(&self) -> bool {
        self.payload.is_empty()
    }
}

/// Thread-safe ring-buffer cache of recently sent messages.
///
/// Uses a fixed-size array as a circular buffer with a `HashMap` overlay for
/// O(1) lookups. When the buffer wraps, the oldest entry is evicted from both
/// the array and the map.
///
/// This mirrors whatsmeow's `recentMessagesList` / `recentMessagesMap` pair.
pub struct RecentMessagesCache {
    inner: Mutex<RecentMessagesCacheInner>,
}

struct RecentMessagesCacheInner {
    /// Ring buffer of keys in insertion order.
    keys: Vec<Option<RecentMessageKey>>,
    /// Fast lookup from key to cached message.
    map: HashMap<RecentMessageKey, RecentMessage>,
    /// Current write position in the ring buffer.
    ptr: usize,
}

impl RecentMessagesCache {
    /// Create a new cache with the default capacity ([`RECENT_MESSAGES_SIZE`]).
    pub fn new() -> Self {
        Self::with_capacity(RECENT_MESSAGES_SIZE)
    }

    /// Create a new cache with a custom capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let mut keys = Vec::with_capacity(capacity);
        keys.resize_with(capacity, || None);
        Self {
            inner: Mutex::new(RecentMessagesCacheInner {
                keys,
                map: HashMap::with_capacity(capacity),
                ptr: 0,
            }),
        }
    }

    /// Insert a message into the cache.
    ///
    /// If the ring buffer is full, the oldest entry is evicted.
    pub fn add(&self, to: Jid, id: MessageId, message: RecentMessage) {
        let mut inner = self.inner.lock().expect("recent messages lock poisoned");
        let key = RecentMessageKey::new(to, id);
        let capacity = inner.keys.len();

        // Evict the entry at the current pointer if it exists.
        if let Some(old_key) = inner.keys[inner.ptr].take() {
            inner.map.remove(&old_key);
        }

        inner.map.insert(key.clone(), message);
        inner.keys[inner.ptr] = Some(key);
        inner.ptr = (inner.ptr + 1) % capacity;
    }

    /// Look up a cached message by destination JID and message ID.
    ///
    /// Returns `None` if not found or if the message has been evicted.
    pub fn get(&self, to: &Jid, id: &MessageId) -> Option<RecentMessage> {
        let inner = self.inner.lock().expect("recent messages lock poisoned");
        let key = RecentMessageKey::new(to.clone(), id.clone());
        inner.map.get(&key).cloned()
    }

    /// Number of messages currently in the cache.
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().expect("recent messages lock poisoned");
        inner.map.len()
    }

    /// Returns `true` if the cache contains no messages.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for RecentMessagesCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── Session recreation tracking ─────────────────────────────────────────────

/// Tracks when Signal sessions were last recreated, used by
/// [`should_recreate_session`] to rate-limit session teardown.
pub struct SessionRecreateHistory {
    inner: Mutex<HashMap<Jid, DateTime<Utc>>>,
}

impl SessionRecreateHistory {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Record that a session was recreated for `jid` at the current time.
    pub fn mark_recreated(&self, jid: &Jid) {
        let mut inner = self.inner.lock().expect("session recreate lock poisoned");
        inner.insert(jid.clone(), Utc::now());
    }

    /// Get the last recreation time for a JID, if any.
    pub fn last_recreated(&self, jid: &Jid) -> Option<DateTime<Utc>> {
        let inner = self.inner.lock().expect("session recreate lock poisoned");
        inner.get(jid).copied()
    }
}

impl Default for SessionRecreateHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine whether a Signal session should be recreated before handling a
/// retry receipt.
///
/// Returns `Some(reason)` if the session should be recreated, or `None` if it
/// should not.
///
/// Logic (mirrors whatsmeow's `shouldRecreateSession`):
/// 1. If we have no session at all, always recreate.
/// 2. If retry count < 2, never recreate.
/// 3. Otherwise, recreate if over [`RECREATE_SESSION_TIMEOUT`] has passed
///    since the last recreation.
///
/// The `has_session` parameter should be resolved by the caller using the
/// Signal session store.
pub fn should_recreate_session(
    history: &SessionRecreateHistory,
    retry_count: u32,
    jid: &Jid,
    has_session: bool,
) -> Option<&'static str> {
    if !has_session {
        history.mark_recreated(jid);
        return Some("we don't have a Signal session with them");
    }

    if retry_count < 2 {
        return None;
    }

    let last = history.last_recreated(jid);
    let should_recreate = match last {
        None => true,
        Some(prev) => {
            let elapsed = Utc::now().signed_duration_since(prev);
            elapsed > RECREATE_SESSION_TIMEOUT
        }
    };

    if should_recreate {
        history.mark_recreated(jid);
        Some("retry count > 1 and over an hour since last recreation")
    } else {
        None
    }
}

// ── Incoming retry flood protection ─────────────────────────────────────────

/// Key for the incoming retry request counter, scoped per sender and message.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IncomingRetryKey {
    pub jid: Jid,
    pub message_id: MessageId,
}

impl IncomingRetryKey {
    pub fn new(jid: Jid, message_id: MessageId) -> Self {
        Self { jid, message_id }
    }
}

/// Thread-safe counter for incoming retry requests, providing anti-flood
/// protection. Each (sender, message_id) pair is tracked independently.
pub struct IncomingRetryCounter {
    inner: Mutex<HashMap<IncomingRetryKey, u32>>,
}

impl IncomingRetryCounter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Increment the counter for a given key and return the new value.
    ///
    /// Returns `None` if the counter has reached [`MAX_INCOMING_RETRY`],
    /// indicating the request should be dropped.
    pub fn increment(&self, key: IncomingRetryKey) -> Option<u32> {
        let mut inner = self.inner.lock().expect("incoming retry lock poisoned");
        let count = inner.entry(key).or_insert(0);
        *count += 1;
        if *count >= MAX_INCOMING_RETRY {
            None
        } else {
            Some(*count)
        }
    }

    /// Get the current count for a key without incrementing.
    pub fn get(&self, key: &IncomingRetryKey) -> u32 {
        let inner = self.inner.lock().expect("incoming retry lock poisoned");
        inner.get(key).copied().unwrap_or(0)
    }

    /// Reset the counter for a specific key.
    pub fn reset(&self, key: &IncomingRetryKey) {
        let mut inner = self.inner.lock().expect("incoming retry lock poisoned");
        inner.remove(key);
    }

    /// Clear all counters.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().expect("incoming retry lock poisoned");
        inner.clear();
    }
}

impl Default for IncomingRetryCounter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Outgoing retry receipt counter ──────────────────────────────────────────

/// Thread-safe counter for outgoing retry receipts we have sent per message.
/// Once the count reaches [`MAX_RETRY_COUNT`], no further retries are sent.
pub struct OutgoingRetryCounter {
    inner: Mutex<HashMap<MessageId, u32>>,
}

impl OutgoingRetryCounter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Increment the retry count for a message, accounting for a retry count
    /// that may have been embedded in the original message's `<enc>` node.
    ///
    /// Returns `Some(count)` if a retry should be sent, or `None` if the
    /// maximum has been reached.
    pub fn increment(&self, message_id: &MessageId, retry_count_in_msg: u32) -> Option<u32> {
        let mut inner = self.inner.lock().expect("outgoing retry lock poisoned");
        let count = inner.entry(message_id.clone()).or_insert(0);
        *count += 1;

        // If this is our first retry and the message itself had a count,
        // inherit that count (the sender may have already retried before
        // we restarted).
        if *count == 1 && retry_count_in_msg > 0 {
            *count = retry_count_in_msg + 1;
        }

        if *count >= MAX_RETRY_COUNT {
            None
        } else {
            Some(*count)
        }
    }

    /// Get the current count for a message without incrementing.
    pub fn get(&self, message_id: &MessageId) -> u32 {
        let inner = self.inner.lock().expect("outgoing retry lock poisoned");
        inner.get(message_id).copied().unwrap_or(0)
    }
}

impl Default for OutgoingRetryCounter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Retry receipt node parsing ──────────────────────────────────────────────

/// Parsed contents of an incoming `<retry>` child node.
#[derive(Debug, Clone, Serialize)]
pub struct RetryInfo {
    /// The message ID being retried.
    pub message_id: MessageId,
    /// Unix timestamp of the original message.
    pub timestamp: i64,
    /// How many times the peer has retried this message.
    pub count: u32,
}

/// Parse the `<retry>` child node from a retry receipt.
///
/// Returns `None` if the node has no `<retry>` child.
pub fn parse_retry_child(node: &Node) -> Option<RetryInfo> {
    let retry_node = node.get_optional_child("retry")?;
    let mut parser = wacore_binary::attrs::AttrParser::new(retry_node);

    let message_id = parser
        .optional_string("id")
        .map(|s| s.to_string())
        .unwrap_or_default();

    let timestamp = parser.optional_unix_time("t").unwrap_or(0);

    let count = parser
        .optional_u64("count")
        .map(|v| v as u32)
        .unwrap_or(1);

    Some(RetryInfo {
        message_id,
        timestamp,
        count,
    })
}

/// Check whether the retry receipt node contains a `<keys>` child (prekey
/// bundle from the peer).
pub fn has_prekey_bundle(node: &Node) -> bool {
    node.get_optional_child("keys").is_some()
}

/// Extract the retry count from the first `<enc>` child of a message node,
/// used to seed the outgoing retry counter when we restart mid-conversation.
pub fn extract_enc_retry_count(node: &Node) -> u32 {
    let children = match node.children() {
        Some(c) => c,
        None => return 0,
    };

    if children.len() == 1 && children[0].tag == "enc" {
        let mut parser = wacore_binary::attrs::AttrParser::new(&children[0]);
        parser.optional_u64("count").map(|v| v as u32).unwrap_or(0)
    } else {
        0
    }
}

// ── Outgoing retry receipt node building ────────────────────────────────────

/// Build the base receipt attributes from an incoming message node.
///
/// Extracts `to` (from the incoming `from`), `participant`, and `recipient`
/// attributes, matching whatsmeow's `buildBaseReceipt`.
fn build_base_receipt_attrs(message_id: &str, node: &Node) -> NodeBuilder {
    let mut builder = NodeBuilder::new("receipt").attr("id", message_id).attr(
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

    builder
}

/// Optional prekey bundle to include in a retry receipt.
#[derive(Debug, Clone)]
pub struct PreKeyBundleData {
    /// DJB key type byte (0x05).
    pub key_type: u8,
    /// Our identity public key.
    pub identity_key: Vec<u8>,
    /// One-time prekey ID (big-endian, 3 bytes).
    pub pre_key_id: Vec<u8>,
    /// One-time prekey public value.
    pub pre_key_value: Vec<u8>,
    /// Signed prekey ID (big-endian, 3 bytes).
    pub signed_pre_key_id: Vec<u8>,
    /// Signed prekey public value.
    pub signed_pre_key_value: Vec<u8>,
    /// Signed prekey signature.
    pub signed_pre_key_signature: Vec<u8>,
    /// Serialised device identity protobuf.
    pub device_identity: Vec<u8>,
}

/// Build the `<keys>` node from a prekey bundle, used when retry count > 1.
fn build_keys_node(bundle: &PreKeyBundleData) -> Node {
    NodeBuilder::new("keys")
        .children([
            NodeBuilder::new("type")
                .bytes(vec![bundle.key_type])
                .build(),
            NodeBuilder::new("identity")
                .bytes(bundle.identity_key.clone())
                .build(),
            NodeBuilder::new("key")
                .children([
                    NodeBuilder::new("id")
                        .bytes(bundle.pre_key_id.clone())
                        .build(),
                    NodeBuilder::new("value")
                        .bytes(bundle.pre_key_value.clone())
                        .build(),
                ])
                .build(),
            NodeBuilder::new("skey")
                .children([
                    NodeBuilder::new("id")
                        .bytes(bundle.signed_pre_key_id.clone())
                        .build(),
                    NodeBuilder::new("value")
                        .bytes(bundle.signed_pre_key_value.clone())
                        .build(),
                    NodeBuilder::new("signature")
                        .bytes(bundle.signed_pre_key_signature.clone())
                        .build(),
                ])
                .build(),
            NodeBuilder::new("device-identity")
                .bytes(bundle.device_identity.clone())
                .build(),
        ])
        .build()
}

/// Build a retry receipt node to send when we fail to decrypt an incoming
/// message.
///
/// Mirrors whatsmeow's `sendRetryReceipt`. The caller is responsible for
/// tracking the retry count via [`OutgoingRetryCounter`] and passing the
/// resolved parameters.
///
/// # Parameters
///
/// - `message_id`: The stanza ID of the undecryptable message.
/// - `incoming_node`: The original incoming `<message>` node.
/// - `retry_count`: Current retry attempt (1-based).
/// - `registration_id`: Our local Signal registration ID.
/// - `is_peer_msg_from_me`: If the message type is `peer_msg` and is from us.
/// - `prekey_bundle`: Optional prekey bundle, included on retry count > 1.
pub fn build_retry_receipt(
    message_id: &str,
    incoming_node: &Node,
    retry_count: u32,
    registration_id: u32,
    is_peer_msg_from_me: bool,
    prekey_bundle: Option<&PreKeyBundleData>,
) -> Node {
    let mut builder = build_base_receipt_attrs(message_id, incoming_node);
    builder = builder.attr("type", "retry");

    if is_peer_msg_from_me {
        builder = builder.attr("category", "peer");
    }

    // Registration ID as 4-byte big-endian.
    let registration_bytes = registration_id.to_be_bytes().to_vec();

    let timestamp_str = incoming_node
        .attrs
        .get("t")
        .map(|v| v.to_string_value())
        .unwrap_or_else(|| Utc::now().timestamp().to_string());

    let mut children = vec![
        NodeBuilder::new("retry")
            .attr("count", retry_count.to_string())
            .attr("id", message_id)
            .attr("t", timestamp_str)
            .attr("v", RETRY_VERSION.to_string())
            .build(),
        NodeBuilder::new("registration")
            .bytes(registration_bytes)
            .build(),
    ];

    if let Some(bundle) = prekey_bundle {
        children.push(build_keys_node(bundle));
    }

    builder.children(children).build()
}

// ── Outgoing retry message node building ────────────────────────────────────

/// Build the `<message>` node for a retry response (re-encrypted message
/// sent back to a peer who sent us a retry receipt).
///
/// Mirrors the node construction in whatsmeow's `handleRetryReceipt`.
/// The actual encryption is performed by the client layer — this function
/// assembles the outer envelope.
///
/// # Parameters
///
/// - `message_id`: The original message ID being retried.
/// - `incoming_retry_node`: The retry receipt node we received.
/// - `retry_count`: The retry count from the receipt.
/// - `timestamp`: The original message timestamp.
/// - `msg_type`: The message type attribute (`"text"`, `"media"`, etc.).
/// - `media_type`: Optional media type attribute (e.g., `"image"`).
/// - `encrypted_node`: The `<enc>` node produced by re-encrypting the message.
/// - `extra_children`: Additional child nodes (e.g., `<franking>`).
/// - `is_group`: Whether this is a group message.
pub fn build_retry_response(
    message_id: &str,
    incoming_retry_node: &Node,
    retry_count: u32,
    timestamp: i64,
    msg_type: &str,
    media_type: Option<&str>,
    mut encrypted_node: Node,
    extra_children: Vec<Node>,
    is_group: bool,
) -> Node {
    // Stamp the retry count onto the encrypted node.
    encrypted_node
        .attrs
        .insert("count".to_string(), retry_count.to_string());

    let mut builder = NodeBuilder::new("message")
        .attr(
            "to",
            incoming_retry_node
                .attrs
                .get("from")
                .map(|v| v.to_string_value())
                .unwrap_or_default(),
        )
        .attr("type", msg_type)
        .attr("id", message_id)
        .attr("t", timestamp.to_string());

    if !is_group {
        builder = builder.attr("device_fanout", "false");
    }

    // Forward participant, recipient, and edit attributes from the retry receipt.
    for attr_name in &["participant", "recipient", "edit"] {
        if let Some(value) = incoming_retry_node.attrs.get(attr_name) {
            builder = builder.attr(*attr_name, value.to_string_value());
        }
    }

    // Assemble content: encrypted node + any extras (e.g., franking tag).
    let mut content = vec![encrypted_node];
    content.extend(extra_children);

    builder.children(content).build()
}

/// Build a `<franking>` node containing the franking tag, used when
/// retrying an `fb`-format message.
pub fn build_franking_node(franking_tag: &[u8]) -> Node {
    NodeBuilder::new("franking")
        .children([NodeBuilder::new("franking_tag")
            .bytes(franking_tag.to_vec())
            .build()])
        .build()
}

// ── Unavailable message request ─────────────────────────────────────────────

/// Metadata for requesting re-delivery of an unavailable message from the
/// paired phone device.
#[derive(Debug, Clone, Serialize)]
pub struct UnavailableMessageRequest {
    /// Chat where the message was sent.
    pub chat: Jid,
    /// Original sender of the message.
    pub sender: Jid,
    /// Message ID to request.
    pub message_id: MessageId,
}

impl UnavailableMessageRequest {
    pub fn new(chat: Jid, sender: Jid, message_id: MessageId) -> Self {
        Self {
            chat,
            sender,
            message_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RecentMessagesCache ────────────────────────────────────────────

    fn make_jid(user: &str) -> Jid {
        Jid::try_from(format!("{user}@s.whatsapp.net")).unwrap()
    }

    fn make_message(data: &str) -> RecentMessage {
        RecentMessage::new(data.as_bytes().to_vec(), MessageFormat::Wa)
    }

    #[test]
    fn test_cache_add_and_get() {
        let cache = RecentMessagesCache::with_capacity(4);
        let jid = make_jid("alice");
        let id = "MSG001".to_string();
        let msg = make_message("hello");

        cache.add(jid.clone(), id.clone(), msg);

        let retrieved = cache.get(&jid, &id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().payload, b"hello");
    }

    #[test]
    fn test_cache_miss() {
        let cache = RecentMessagesCache::with_capacity(4);
        let jid = make_jid("alice");
        assert!(cache.get(&jid, &"NONEXISTENT".to_string()).is_none());
    }

    #[test]
    fn test_cache_eviction_on_wrap() {
        let cache = RecentMessagesCache::with_capacity(4);
        let jid = make_jid("bob");

        for i in 0..4 {
            cache.add(
                jid.clone(),
                format!("MSG{i:03}"),
                make_message(&format!("body{i}")),
            );
        }
        assert_eq!(cache.len(), 4);

        // Adding a 5th message should evict MSG000.
        cache.add(
            jid.clone(),
            "MSG004".to_string(),
            make_message("body4"),
        );
        assert_eq!(cache.len(), 4);
        assert!(cache.get(&jid, &"MSG000".to_string()).is_none());
        assert!(cache.get(&jid, &"MSG004".to_string()).is_some());
    }

    #[test]
    fn test_cache_full_rotation() {
        let cache = RecentMessagesCache::with_capacity(3);
        let jid = make_jid("carol");

        // Fill completely and wrap around twice.
        for i in 0..9 {
            cache.add(
                jid.clone(),
                format!("M{i}"),
                make_message(&format!("d{i}")),
            );
        }

        // Only the last 3 should remain.
        assert_eq!(cache.len(), 3);
        for i in 6..9 {
            assert!(cache.get(&jid, &format!("M{i}")).is_some());
        }
        for i in 0..6 {
            assert!(cache.get(&jid, &format!("M{i}")).is_none());
        }
    }

    #[test]
    fn test_cache_different_jids() {
        let cache = RecentMessagesCache::with_capacity(4);
        let alice = make_jid("alice");
        let bob = make_jid("bob");
        let id = "MSG001".to_string();

        cache.add(alice.clone(), id.clone(), make_message("for alice"));
        cache.add(bob.clone(), id.clone(), make_message("for bob"));

        let alice_msg = cache.get(&alice, &id).unwrap();
        assert_eq!(alice_msg.payload, b"for alice");

        let bob_msg = cache.get(&bob, &id).unwrap();
        assert_eq!(bob_msg.payload, b"for bob");
    }

    #[test]
    fn test_cache_default_capacity() {
        let cache = RecentMessagesCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    // ── RecentMessage ──────────────────────────────────────────────────

    #[test]
    fn test_recent_message_is_empty() {
        let empty = RecentMessage::new(vec![], MessageFormat::Wa);
        assert!(empty.is_empty());

        let non_empty = RecentMessage::new(vec![1, 2, 3], MessageFormat::Fb);
        assert!(!non_empty.is_empty());
    }

    // ── MessageFormat ──────────────────────────────────────────────────

    #[test]
    fn test_message_format_roundtrip() {
        assert_eq!(MessageFormat::from_str(MessageFormat::Wa.as_str()), Some(MessageFormat::Wa));
        assert_eq!(MessageFormat::from_str(MessageFormat::Fb.as_str()), Some(MessageFormat::Fb));
        assert_eq!(MessageFormat::from_str("unknown"), None);
    }

    // ── IncomingRetryCounter ───────────────────────────────────────────

    #[test]
    fn test_incoming_retry_counter_basic() {
        let counter = IncomingRetryCounter::new();
        let key = IncomingRetryKey::new(make_jid("alice"), "MSG001".to_string());

        for i in 1..MAX_INCOMING_RETRY {
            assert_eq!(counter.increment(key.clone()), Some(i));
        }

        // At MAX_INCOMING_RETRY, should return None (flood protection).
        assert_eq!(counter.increment(key.clone()), None);
    }

    #[test]
    fn test_incoming_retry_counter_independent_keys() {
        let counter = IncomingRetryCounter::new();
        let key_a = IncomingRetryKey::new(make_jid("alice"), "MSG001".to_string());
        let key_b = IncomingRetryKey::new(make_jid("bob"), "MSG001".to_string());

        assert_eq!(counter.increment(key_a.clone()), Some(1));
        assert_eq!(counter.increment(key_b.clone()), Some(1));
        assert_eq!(counter.increment(key_a), Some(2));
        assert_eq!(counter.get(&key_b), 1);
    }

    #[test]
    fn test_incoming_retry_counter_reset() {
        let counter = IncomingRetryCounter::new();
        let key = IncomingRetryKey::new(make_jid("alice"), "MSG001".to_string());

        counter.increment(key.clone());
        counter.increment(key.clone());
        assert_eq!(counter.get(&key), 2);

        counter.reset(&key);
        assert_eq!(counter.get(&key), 0);
    }

    #[test]
    fn test_incoming_retry_counter_clear() {
        let counter = IncomingRetryCounter::new();
        counter.increment(IncomingRetryKey::new(make_jid("a"), "M1".to_string()));
        counter.increment(IncomingRetryKey::new(make_jid("b"), "M2".to_string()));

        counter.clear();
        assert_eq!(
            counter.get(&IncomingRetryKey::new(make_jid("a"), "M1".to_string())),
            0,
        );
    }

    // ── OutgoingRetryCounter ───────────────────────────────────────────

    #[test]
    fn test_outgoing_retry_counter_basic() {
        let counter = OutgoingRetryCounter::new();
        let id = "MSG001".to_string();

        for i in 1..MAX_RETRY_COUNT {
            assert_eq!(counter.increment(&id, 0), Some(i));
        }

        // At MAX_RETRY_COUNT, should return None.
        assert_eq!(counter.increment(&id, 0), None);
    }

    #[test]
    fn test_outgoing_retry_counter_inherits_msg_count() {
        let counter = OutgoingRetryCounter::new();
        let id = "MSG001".to_string();

        // First increment with retry_count_in_msg = 3 should jump to 4.
        assert_eq!(counter.increment(&id, 3), Some(4));

        // Next increment reaches MAX_RETRY_COUNT (5), returns None.
        assert_eq!(counter.increment(&id, 0), None);
    }

    #[test]
    fn test_outgoing_retry_counter_no_inherit_after_first() {
        let counter = OutgoingRetryCounter::new();
        let id = "MSG001".to_string();

        // First increment: count = 1, no inherit (retry_count_in_msg = 0).
        assert_eq!(counter.increment(&id, 0), Some(1));

        // Second increment: count = 2, inherit is ignored since count != 1.
        assert_eq!(counter.increment(&id, 3), Some(2));
    }

    // ── should_recreate_session ────────────────────────────────────────

    #[test]
    fn test_recreate_no_session() {
        let history = SessionRecreateHistory::new();
        let jid = make_jid("alice");

        let reason = should_recreate_session(&history, 0, &jid, false);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("don't have a Signal session"));
    }

    #[test]
    fn test_no_recreate_low_retry_count() {
        let history = SessionRecreateHistory::new();
        let jid = make_jid("alice");

        assert!(should_recreate_session(&history, 0, &jid, true).is_none());
        assert!(should_recreate_session(&history, 1, &jid, true).is_none());
    }

    #[test]
    fn test_recreate_high_retry_no_history() {
        let history = SessionRecreateHistory::new();
        let jid = make_jid("alice");

        let reason = should_recreate_session(&history, 2, &jid, true);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("over an hour"));
    }

    #[test]
    fn test_no_recreate_recent_history() {
        let history = SessionRecreateHistory::new();
        let jid = make_jid("alice");

        // First call recreates and records the timestamp.
        should_recreate_session(&history, 2, &jid, true);

        // Immediate second call should NOT recreate (within timeout).
        assert!(should_recreate_session(&history, 2, &jid, true).is_none());
    }

    // ── parse_retry_child ──────────────────────────────────────────────

    #[test]
    fn test_parse_retry_child_present() {
        let node = NodeBuilder::new("receipt")
            .children([NodeBuilder::new("retry")
                .attr("id", "3EB0MSG001")
                .attr("t", "1713100000")
                .attr("count", "2")
                .attr("v", "1")
                .build()])
            .build();

        let info = parse_retry_child(&node).unwrap();
        assert_eq!(info.message_id, "3EB0MSG001");
        assert_eq!(info.timestamp, 1713100000);
        assert_eq!(info.count, 2);
    }

    #[test]
    fn test_parse_retry_child_missing() {
        let node = NodeBuilder::new("receipt").build();
        assert!(parse_retry_child(&node).is_none());
    }

    #[test]
    fn test_parse_retry_child_defaults() {
        let node = NodeBuilder::new("receipt")
            .children([NodeBuilder::new("retry").build()])
            .build();

        let info = parse_retry_child(&node).unwrap();
        assert!(info.message_id.is_empty());
        assert_eq!(info.timestamp, 0);
        assert_eq!(info.count, 1);
    }

    // ── has_prekey_bundle ──────────────────────────────────────────────

    #[test]
    fn test_has_prekey_bundle_true() {
        let node = NodeBuilder::new("receipt")
            .children([NodeBuilder::new("keys").build()])
            .build();
        assert!(has_prekey_bundle(&node));
    }

    #[test]
    fn test_has_prekey_bundle_false() {
        let node = NodeBuilder::new("receipt").build();
        assert!(!has_prekey_bundle(&node));
    }

    // ── extract_enc_retry_count ────────────────────────────────────────

    #[test]
    fn test_extract_enc_retry_count_present() {
        let node = NodeBuilder::new("message")
            .children([NodeBuilder::new("enc").attr("count", "3").build()])
            .build();
        assert_eq!(extract_enc_retry_count(&node), 3);
    }

    #[test]
    fn test_extract_enc_retry_count_missing() {
        let node = NodeBuilder::new("message").build();
        assert_eq!(extract_enc_retry_count(&node), 0);
    }

    #[test]
    fn test_extract_enc_retry_count_multiple_children() {
        let node = NodeBuilder::new("message")
            .children([
                NodeBuilder::new("enc").attr("count", "2").build(),
                NodeBuilder::new("enc").attr("count", "5").build(),
            ])
            .build();
        // Only triggers when there is exactly one <enc> child.
        assert_eq!(extract_enc_retry_count(&node), 0);
    }

    // ── build_retry_receipt ────────────────────────────────────────────

    #[test]
    fn test_build_retry_receipt_basic() {
        let incoming = NodeBuilder::new("message")
            .attr("from", "sender@s.whatsapp.net")
            .attr("t", "1713100000")
            .build();

        let receipt = build_retry_receipt(
            "3EB0MSG001",
            &incoming,
            1,
            12345,
            false,
            None,
        );

        assert_eq!(receipt.tag, "receipt");
        assert_eq!(
            receipt.attrs.get("type").map(|v| v.to_string_value()),
            Some("retry".to_string()),
        );
        assert_eq!(
            receipt.attrs.get("id").map(|v| v.to_string_value()),
            Some("3EB0MSG001".to_string()),
        );
        assert_eq!(
            receipt.attrs.get("to").map(|v| v.to_string_value()),
            Some("sender@s.whatsapp.net".to_string()),
        );
        assert!(receipt.attrs.get("category").is_none());

        // Should have <retry> and <registration> children.
        let children = receipt.children().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].tag, "retry");
        assert_eq!(children[1].tag, "registration");

        // Verify registration ID bytes (big-endian 12345).
        let reg_bytes = match &children[1].content {
            Some(wacore_binary::node::NodeContent::Bytes(b)) => b.clone(),
            _ => panic!("expected bytes content in registration node"),
        };
        assert_eq!(reg_bytes, 12345u32.to_be_bytes().to_vec());
    }

    #[test]
    fn test_build_retry_receipt_peer_msg() {
        let incoming = NodeBuilder::new("message")
            .attr("from", "self@s.whatsapp.net")
            .attr("t", "1713100000")
            .build();

        let receipt = build_retry_receipt(
            "3EB0MSG001",
            &incoming,
            1,
            12345,
            true,
            None,
        );

        assert_eq!(
            receipt.attrs.get("category").map(|v| v.to_string_value()),
            Some("peer".to_string()),
        );
    }

    #[test]
    fn test_build_retry_receipt_with_prekey_bundle() {
        let incoming = NodeBuilder::new("message")
            .attr("from", "sender@s.whatsapp.net")
            .attr("t", "1713100000")
            .build();

        let bundle = PreKeyBundleData {
            key_type: 0x05,
            identity_key: vec![1; 32],
            pre_key_id: vec![0, 0, 1],
            pre_key_value: vec![2; 32],
            signed_pre_key_id: vec![0, 0, 2],
            signed_pre_key_value: vec![3; 32],
            signed_pre_key_signature: vec![4; 64],
            device_identity: vec![5; 16],
        };

        let receipt = build_retry_receipt(
            "3EB0MSG001",
            &incoming,
            2,
            12345,
            false,
            Some(&bundle),
        );

        let children = receipt.children().unwrap();
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].tag, "retry");
        assert_eq!(children[1].tag, "registration");
        assert_eq!(children[2].tag, "keys");

        // Verify <keys> children.
        let keys_children = children[2].children().unwrap();
        let key_tags: Vec<&str> = keys_children.iter().map(|n| n.tag.as_str()).collect();
        assert_eq!(key_tags, vec!["type", "identity", "key", "skey", "device-identity"]);
    }

    #[test]
    fn test_build_retry_receipt_forwards_participant() {
        let incoming = NodeBuilder::new("message")
            .attr("from", "group@g.us")
            .attr("participant", "sender@s.whatsapp.net")
            .attr("t", "1713100000")
            .build();

        let receipt = build_retry_receipt(
            "3EB0MSG001",
            &incoming,
            1,
            12345,
            false,
            None,
        );

        assert_eq!(
            receipt.attrs.get("participant").map(|v| v.to_string_value()),
            Some("sender@s.whatsapp.net".to_string()),
        );
    }

    // ── build_retry_response ───────────────────────────────────────────

    #[test]
    fn test_build_retry_response_dm() {
        let retry_node = NodeBuilder::new("receipt")
            .attr("from", "peer@s.whatsapp.net")
            .build();

        let enc_node = NodeBuilder::new("enc")
            .attr("v", "2")
            .bytes(vec![0xAB; 16])
            .build();

        let node = build_retry_response(
            "3EB0MSG001",
            &retry_node,
            2,
            1713100000,
            "text",
            None,
            enc_node,
            vec![],
            false,
        );

        assert_eq!(node.tag, "message");
        assert_eq!(
            node.attrs.get("to").map(|v| v.to_string_value()),
            Some("peer@s.whatsapp.net".to_string()),
        );
        assert_eq!(
            node.attrs.get("type").map(|v| v.to_string_value()),
            Some("text".to_string()),
        );
        assert_eq!(
            node.attrs.get("device_fanout").map(|v| v.to_string_value()),
            Some("false".to_string()),
        );

        // Enc node should have the retry count stamped.
        let children = node.children().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].tag, "enc");
        assert_eq!(
            children[0].attrs.get("count").map(|v| v.to_string_value()),
            Some("2".to_string()),
        );
    }

    #[test]
    fn test_build_retry_response_group() {
        let retry_node = NodeBuilder::new("receipt")
            .attr("from", "group@g.us")
            .attr("participant", "sender@s.whatsapp.net")
            .build();

        let enc_node = NodeBuilder::new("enc").build();

        let node = build_retry_response(
            "3EB0MSG001",
            &retry_node,
            1,
            1713100000,
            "text",
            None,
            enc_node,
            vec![],
            true,
        );

        // Group messages should NOT have device_fanout.
        assert!(node.attrs.get("device_fanout").is_none());

        // Should forward participant.
        assert_eq!(
            node.attrs.get("participant").map(|v| v.to_string_value()),
            Some("sender@s.whatsapp.net".to_string()),
        );
    }

    #[test]
    fn test_build_retry_response_with_franking() {
        let retry_node = NodeBuilder::new("receipt")
            .attr("from", "peer@s.whatsapp.net")
            .build();

        let enc_node = NodeBuilder::new("enc").build();
        let franking = build_franking_node(&[0xDE, 0xAD]);

        let node = build_retry_response(
            "3EB0MSG001",
            &retry_node,
            1,
            1713100000,
            "text",
            None,
            enc_node,
            vec![franking],
            false,
        );

        let children = node.children().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].tag, "enc");
        assert_eq!(children[1].tag, "franking");
    }

    #[test]
    fn test_build_retry_response_forwards_edit() {
        let retry_node = NodeBuilder::new("receipt")
            .attr("from", "peer@s.whatsapp.net")
            .attr("edit", "1")
            .build();

        let enc_node = NodeBuilder::new("enc").build();

        let node = build_retry_response(
            "3EB0MSG001",
            &retry_node,
            1,
            1713100000,
            "text",
            None,
            enc_node,
            vec![],
            false,
        );

        assert_eq!(
            node.attrs.get("edit").map(|v| v.to_string_value()),
            Some("1".to_string()),
        );
    }

    // ── build_franking_node ────────────────────────────────────────────

    #[test]
    fn test_build_franking_node() {
        let tag = vec![0xCA, 0xFE, 0xBA, 0xBE];
        let node = build_franking_node(&tag);

        assert_eq!(node.tag, "franking");
        let children = node.children().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].tag, "franking_tag");

        let content_bytes = match &children[0].content {
            Some(wacore_binary::node::NodeContent::Bytes(b)) => b.clone(),
            _ => panic!("expected bytes content in franking_tag"),
        };
        assert_eq!(content_bytes, tag);
    }

    // ── UnavailableMessageRequest ──────────────────────────────────────

    #[test]
    fn test_unavailable_message_request() {
        let req = UnavailableMessageRequest::new(
            make_jid("group"),
            make_jid("sender"),
            "MSG001".to_string(),
        );
        assert_eq!(req.chat, make_jid("group"));
        assert_eq!(req.sender, make_jid("sender"));
        assert_eq!(req.message_id, "MSG001");
    }
}
