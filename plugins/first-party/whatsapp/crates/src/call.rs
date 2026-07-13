use crate::client::Client;
use crate::store::signal_adapter::SignalProtocolStoreAdapter;
use crate::types::events::{CallOffer, CallTerminate, Event};
use log::{debug, info, warn};
use prost::Message as ProtoMessage;
use rand::TryRngCore;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, watch};
use wacore::libsignal::protocol::{
    PreKeySignalMessage, SignalMessage, UsePQRatchet, message_decrypt,
};
use wacore::messages::MessageUtils;
use wacore::types::jid::JidExt;
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::Jid;
use wacore_binary::node::{Node, NodeContent};
use waproto::whatsapp as wa;

/// A parsed transport candidate (ICE candidate).
#[derive(Debug, Clone)]
pub struct TransportCandidate {
    pub addr: SocketAddr,
    pub priority: u32,
}

/// Relay info parsed from the offer's <relay> node.
#[derive(Debug, Clone)]
pub struct RelayInfo {
    pub tokens: Vec<Vec<u8>>,
    pub key: Vec<u8>,
    /// Raw key bytes before base64 decode (for HMAC — relay may use raw form).
    pub key_raw: Vec<u8>,
    pub hbh_key: Vec<u8>,
    pub endpoints: Vec<RelayEndpoint>,
}

/// A relay server endpoint from <te2>.
#[derive(Debug, Clone)]
pub struct RelayEndpoint {
    pub relay_name: String,
    pub relay_id: u32,
    pub token_id: u32,
    pub c2r_rtt: u32,
    pub addr: SocketAddr,
    /// Transport protocol: 0 = UDP (default), 1 = TCP.
    pub protocol: u32,
}

/// State for an active/pending call.
#[derive(Debug, Clone)]
pub struct ActiveCall {
    pub call_id: String,
    pub caller: Jid,
    pub is_video: bool,
    pub timestamp: i64,
    /// The raw <offer> node — needed for CallKey decryption and transport setup.
    pub offer_node: Arc<Node>,
    /// Decrypted SRTP master secret (32 bytes), set after CallKey decryption.
    pub call_key: Option<Vec<u8>>,
    /// Relay info from the offer.
    pub relay_info: Option<RelayInfo>,
    /// Transport candidates from the caller (set when <transport> arrives).
    pub transport_candidates: Vec<TransportCandidate>,
    /// Sender half of a cancellation channel. Send `true` to stop the media loop.
    pub cancel_tx: Option<watch::Sender<bool>>,
}

/// In-memory store for active calls, keyed by call_id.
pub struct CallStore {
    calls: RwLock<HashMap<String, ActiveCall>>,
}

impl CallStore {
    pub fn new() -> Self {
        Self {
            calls: RwLock::new(HashMap::new()),
        }
    }

    pub async fn insert(&self, call: ActiveCall) {
        self.calls.write().await.insert(call.call_id.clone(), call);
    }

    pub async fn remove(&self, call_id: &str) -> Option<ActiveCall> {
        self.calls.write().await.remove(call_id)
    }

    pub async fn get(&self, call_id: &str) -> Option<ActiveCall> {
        self.calls.read().await.get(call_id).cloned()
    }

    pub async fn update(&self, call: ActiveCall) {
        self.calls.write().await.insert(call.call_id.clone(), call);
    }
}

// === Parsing helpers ===

/// Parse a <te> node's binary content as a socket address.
/// 6 bytes = IPv4 (4) + port (2 BE), 18 bytes = IPv6 (16) + port (2 BE).
fn parse_te_candidate(bytes: &[u8], priority: u32) -> Option<TransportCandidate> {
    match bytes.len() {
        6 => {
            let ip = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
            let port = u16::from_be_bytes([bytes[4], bytes[5]]);
            Some(TransportCandidate {
                addr: SocketAddr::new(IpAddr::V4(ip), port),
                priority,
            })
        }
        18 => {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&bytes[0..16]);
            let ip = Ipv6Addr::from(octets);
            let port = u16::from_be_bytes([bytes[16], bytes[17]]);
            Some(TransportCandidate {
                addr: SocketAddr::new(IpAddr::V6(ip), port),
                priority,
            })
        }
        n => {
            warn!("Unknown <te> candidate size: {} bytes", n);
            None
        }
    }
}

/// Parse transport candidates from a <transport> node.
fn parse_transport_candidates(transport: &Node) -> Vec<TransportCandidate> {
    let mut candidates = Vec::new();

    for te in transport.get_children_by_tag("te") {
        let priority = te
            .attrs()
            .optional_string("priority")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        if let Some(NodeContent::Bytes(bytes)) = &te.content {
            if let Some(c) = parse_te_candidate(bytes, priority) {
                candidates.push(c);
            }
        }
    }

    // Also parse <rte> (relay transport endpoint)
    for rte in transport.get_children_by_tag("rte") {
        if let Some(NodeContent::Bytes(bytes)) = &rte.content {
            if let Some(c) = parse_te_candidate(bytes, 0) {
                candidates.push(c);
            }
        }
    }

    candidates
}

/// Parse relay info from the offer's <relay> node.
fn parse_relay_info(offer: &Node) -> Option<RelayInfo> {
    let relay = offer.get_optional_child("relay")?;

    // Parse tokens
    let tokens: Vec<Vec<u8>> = relay
        .get_children_by_tag("token")
        .filter_map(|t| match &t.content {
            Some(NodeContent::Bytes(b)) => Some(b.clone()),
            _ => None,
        })
        .collect();

    // Parse relay key (base64-encoded, may arrive as String or Bytes)
    // Save both raw and decoded versions — relay HMAC might use either
    let (key, key_raw) = relay
        .get_optional_child("key")
        .map(|k| {
            use base64::prelude::*;
            match &k.content {
                Some(NodeContent::String(s)) => {
                    let raw = s.as_bytes().to_vec();
                    let decoded = BASE64_STANDARD.decode(s).unwrap_or_else(|_| raw.clone());
                    (decoded, raw)
                }
                Some(NodeContent::Bytes(b)) => {
                    let raw = b.clone();
                    let decoded = BASE64_STANDARD.decode(b).unwrap_or_else(|_| raw.clone());
                    (decoded, raw)
                }
                _ => (vec![], vec![]),
            }
        })
        .unwrap_or_default();

    // Parse hbh_key (hop-by-hop encryption key, base64-encoded)
    let hbh_key = relay
        .get_optional_child("hbh_key")
        .and_then(|k| {
            use base64::prelude::*;
            match &k.content {
                Some(NodeContent::String(s)) => BASE64_STANDARD.decode(s).ok(),
                Some(NodeContent::Bytes(b)) => {
                    if let Ok(decoded) = BASE64_STANDARD.decode(b) {
                        Some(decoded)
                    } else {
                        Some(b.clone())
                    }
                }
                _ => None,
            }
        })
        .unwrap_or_default();

    // Parse <te2> relay endpoints
    let endpoints: Vec<RelayEndpoint> = relay
        .get_children_by_tag("te2")
        .filter_map(|te2| {
            let mut attrs = te2.attrs();
            let relay_name = attrs.optional_string("relay_name")?.to_string();
            let relay_id = attrs
                .optional_string("relay_id")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let token_id = attrs
                .optional_string("token_id")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let c2r_rtt = attrs
                .optional_string("c2r_rtt")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let protocol = attrs
                .optional_string("protocol")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let addr = match &te2.content {
                Some(NodeContent::Bytes(bytes)) => {
                    parse_te_candidate(bytes, 0).map(|c| c.addr)
                }
                _ => None,
            }?;

            Some(RelayEndpoint {
                relay_name,
                relay_id,
                token_id,
                c2r_rtt,
                addr,
                protocol,
            })
        })
        .collect();

    info!(
        "Parsed relay info: {} tokens, key={} bytes, hbh_key={} bytes, {} endpoints",
        tokens.len(),
        key.len(),
        hbh_key.len(),
        endpoints.len()
    );
    for ep in &endpoints {
        info!(
            "  Relay endpoint: {} (id={}, token={}, rtt={}ms, proto={}) -> {}",
            ep.relay_name, ep.relay_id, ep.token_id, ep.c2r_rtt,
            if ep.protocol == 1 { "TCP" } else { "UDP" }, ep.addr
        );
    }

    Some(RelayInfo {
        tokens,
        key,
        key_raw,
        hbh_key,
        endpoints,
    })
}

impl Client {
    pub(crate) async fn handle_call(self: &Arc<Self>, node: &Node) {
        let mut attrs = node.attrs();
        let from = attrs.jid("from");
        let timestamp = attrs
            .optional_string("t")
            .and_then(|t| t.parse::<i64>().ok())
            .unwrap_or(0);
        let push_name = attrs
            .optional_string("notify")
            .unwrap_or("")
            .to_string();
        let platform = attrs
            .optional_string("platform")
            .unwrap_or("")
            .to_string();
        let stanza_id = attrs
            .optional_string("id")
            .unwrap_or("")
            .to_string();

        // Check for <offer> child — incoming call
        if let Some(offer) = node.get_optional_child("offer") {
            let mut offer_attrs = offer.attrs();
            let call_id = offer_attrs
                .optional_string("call-id")
                .unwrap_or("")
                .to_string();
            let caller_pn = offer_attrs
                .optional_string("caller_pn")
                .map(|s| s.to_string());

            let is_video = offer.get_optional_child("video").is_some();

            let call_type = if is_video { "video" } else { "audio" };
            info!(
                "Incoming {} call from {} ({}), call_id={}",
                call_type, from, push_name, call_id
            );

            // Cache LID-to-PN mapping if available
            if let Some(ref pn) = caller_pn {
                if let Some(user) = pn.strip_suffix("@s.whatsapp.net") {
                    let _ = self
                        .add_lid_pn_mapping(
                            &from.user,
                            user,
                            crate::lid_pn_cache::LearningSource::CallOffer,
                        )
                        .await;
                }
            }

            // Parse relay info from the offer
            let relay_info = parse_relay_info(offer);

            // Store active call state
            self.call_store
                .insert(ActiveCall {
                    call_id: call_id.clone(),
                    caller: from.clone(),
                    is_video,
                    timestamp,
                    offer_node: Arc::new(offer.clone()),
                    call_key: None,
                    relay_info,
                    transport_candidates: Vec::new(),
                    cancel_tx: None,
                })
                .await;

            // Attempt to decrypt the CallKey from the <enc> node
            let client = self.clone();
            let caller_clone = from.clone();
            let call_id_clone = call_id.clone();
            tokio::spawn(async move {
                match client
                    .decrypt_call_key(&call_id_clone, &caller_clone)
                    .await
                {
                    Ok(key) => {
                        info!(
                            "Decrypted CallKey for call {}: {} bytes, hex={}",
                            call_id_clone,
                            key.len(),
                            hex::encode(&key)
                        );
                        // Update the stored call with the decrypted key
                        if let Some(mut call) = client.call_store.get(&call_id_clone).await {
                            call.call_key = Some(key);
                            client.call_store.update(call).await;
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to decrypt CallKey for call {}: {:?}",
                            call_id_clone, e
                        );
                    }
                }
            });

            self.core.event_bus.dispatch(&Event::CallOffer(CallOffer {
                id: stanza_id,
                call_id,
                caller: from,
                caller_pn,
                push_name,
                is_video,
                timestamp,
                platform,
            }));
            return;
        }

        // Check for <terminate> child — call ended
        if let Some(terminate) = node.get_optional_child("terminate") {
            let mut term_attrs = terminate.attrs();
            let call_id = term_attrs
                .optional_string("call-id")
                .unwrap_or("")
                .to_string();
            let reason = term_attrs
                .optional_string("reason")
                .map(|s| s.to_string());

            info!(
                "Call terminated from {}, call_id={}, reason={:?}",
                from, call_id, reason
            );

            // Signal cancellation to stop the media loop before removing
            if let Some(call) = self.call_store.get(&call_id).await {
                if let Some(tx) = &call.cancel_tx {
                    let _ = tx.send(true);
                }
            }
            self.call_store.remove(&call_id).await;

            self.core
                .event_bus
                .dispatch(&Event::CallTerminate(CallTerminate {
                    call_id,
                    caller: from,
                    timestamp,
                    reason,
                }));
            return;
        }

        // Check for <transport> child — relay/connection info after accept
        if let Some(transport) = node.get_optional_child("transport") {
            let mut transport_attrs = transport.attrs();
            let call_id = transport_attrs
                .optional_string("call-id")
                .unwrap_or("")
                .to_string();

            let candidates = parse_transport_candidates(transport);

            info!(
                "Call transport received for call_id={}: {} candidates",
                call_id,
                candidates.len()
            );
            for c in &candidates {
                info!("  Candidate: {} (priority={})", c.addr, c.priority);
            }

            // Update active call with transport candidates
            if let Some(mut call) = self.call_store.get(&call_id).await {
                call.transport_candidates = candidates;
                self.call_store.update(call.clone()).await;

                info!(
                    "Call {} state: call_key={}, relay={}, candidates={}",
                    call_id,
                    call.call_key.as_ref().map(|k| format!("{} bytes", k.len())).unwrap_or("none".into()),
                    call.relay_info.as_ref().map(|r| format!("{} endpoints", r.endpoints.len())).unwrap_or("none".into()),
                    call.transport_candidates.len()
                );

                // If we have everything, start the media pipeline
                if call.call_key.is_some() && call.relay_info.is_some() {
                    // Create cancellation channel
                    let (cancel_tx, cancel_rx) = watch::channel(false);
                    call.cancel_tx = Some(cancel_tx);
                    self.call_store.update(call.clone()).await;

                    let client = self.clone();
                    let call_clone = call.clone();
                    let own_jid = self.get_lid().await
                        .map(|j| format!("{}:{}@{}", j.user, j.device, j.server))
                        .unwrap_or_default();
                    tokio::spawn(async move {
                        if let Err(e) = client.start_call_media(&call_clone, &own_jid, cancel_rx).await {
                            warn!("Call media pipeline failed for {}: {:?}", call_clone.call_id, e);
                        }
                    });
                }
            }
            return;
        }

        // <relaylatency> — relay measurements, just log
        if node.get_optional_child("relaylatency").is_some() {
            debug!("Call relay latency measurement from {}", from);
            return;
        }

        debug!("Unhandled <call> child type from {}", from);
    }

    /// Decrypt the CallKey from the <enc> node in the stored offer.
    /// The <enc> contains a Signal-encrypted protobuf Message { call: Call { call_key: [32 bytes] } }.
    async fn decrypt_call_key(
        self: &Arc<Self>,
        call_id: &str,
        caller: &Jid,
    ) -> Result<Vec<u8>, anyhow::Error> {
        let call = self
            .call_store
            .get(call_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("No active call with id {}", call_id))?;

        // Find the <enc> node in the offer
        let enc_node = call
            .offer_node
            .get_optional_child("enc")
            .ok_or_else(|| anyhow::anyhow!("No <enc> node in offer for call {}", call_id))?;

        let ciphertext = match &enc_node.content {
            Some(NodeContent::Bytes(b)) => b.as_slice(),
            _ => return Err(anyhow::anyhow!("Enc node has no byte content")),
        };

        let enc_type = enc_node
            .attrs()
            .optional_string("type")
            .unwrap_or("pkmsg")
            .to_string();
        let padding_version = enc_node.attrs().optional_u64("v").unwrap_or(2) as u8;

        info!(
            "Decrypting CallKey: enc_type={}, {} bytes, caller={}",
            enc_type,
            ciphertext.len(),
            caller
        );

        // Resolve encryption JID — use caller's LID or PN
        let sender_encryption_jid = caller.clone();

        // Acquire session lock for this sender
        let signal_addr_str = sender_encryption_jid.to_protocol_address().to_string();
        let session_mutex = self
            .session_locks
            .get_with(signal_addr_str.clone(), async {
                Arc::new(tokio::sync::Mutex::new(()))
            })
            .await;
        let _session_guard = session_mutex.lock().await;

        let mut adapter =
            SignalProtocolStoreAdapter::new(self.persistence_manager.get_device_arc().await);
        let rng = rand::rngs::OsRng;

        let parsed = if enc_type == "pkmsg" {
            wacore::libsignal::protocol::CiphertextMessage::PreKeySignalMessage(
                PreKeySignalMessage::try_from(ciphertext)
                    .map_err(|e| anyhow::anyhow!("Failed to parse PreKeySignalMessage: {:?}", e))?,
            )
        } else {
            wacore::libsignal::protocol::CiphertextMessage::SignalMessage(
                SignalMessage::try_from(ciphertext)
                    .map_err(|e| anyhow::anyhow!("Failed to parse SignalMessage: {:?}", e))?,
            )
        };

        let signal_address = sender_encryption_jid.to_protocol_address();

        let padded_plaintext = message_decrypt(
            &parsed,
            &signal_address,
            &mut adapter.session_store,
            &mut adapter.identity_store,
            &mut adapter.pre_key_store,
            &adapter.signed_pre_key_store,
            &mut rng.unwrap_err(),
            UsePQRatchet::No,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Signal decrypt failed: {:?}", e))?;

        let plaintext = MessageUtils::unpad_message_ref(&padded_plaintext, padding_version)
            .map_err(|e| anyhow::anyhow!("Unpad failed: {:?}", e))?;

        // Decode as wa::Message and extract Call.call_key
        let msg = wa::Message::decode(plaintext)
            .map_err(|e| anyhow::anyhow!("Protobuf decode failed: {:?}", e))?;

        let call_key = msg
            .call
            .as_ref()
            .and_then(|c| c.call_key.clone())
            .ok_or_else(|| anyhow::anyhow!("No call_key in decrypted Call message"))?;

        if call_key.len() != 32 {
            return Err(anyhow::anyhow!(
                "CallKey has unexpected length: {} (expected 32)",
                call_key.len()
            ));
        }

        Ok(call_key)
    }

    /// Start the call media pipeline: derive keys, bind relay, send/receive audio.
    async fn start_call_media(
        &self,
        call: &ActiveCall,
        own_jid: &str,
        mut cancel_rx: watch::Receiver<bool>,
    ) -> Result<(), anyhow::Error> {
        let call_key = call
            .call_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No CallKey"))?;
        let relay_info = call
            .relay_info
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No relay info"))?;

        // expandCallKey uses LID:DEVICE@lid format.
        // Use actual device number for both keys and SSRC — this is the only config
        // where the caller accepts our packets (garbled but present).
        let caller_jid = format!("{}:{}@{}", call.caller.user, call.caller.device, call.caller.server);
        info!("Call {} key derivation: caller_jid={}, own_jid={}", call.call_id, caller_jid, own_jid);

        // Step 1: Derive SRTP + SFrame keys
        let keys = crate::call_media::derive_call_media_keys(
            call_key,
            &caller_jid,
            own_jid,    // device :36 for SRTP keys
            own_jid,    // device :36 for SSRC
            &call.call_id,
            Some(&relay_info.hbh_key),
        );

        // Step 2: Bind to the closest relay (use raw key for HMAC — relay may expect base64 ASCII)
        let (socket, mapped_addr) = crate::call_media::bind_to_relay(
            &relay_info.endpoints,
            &relay_info.tokens,
            &relay_info.key_raw,
        )
        .await?;

        info!(
            "Call {} media: relay bound at {}, our_ssrc=0x{:08x}",
            call.call_id, mapped_addr, keys.our_ssrc
        );

        // Step 3: Send our transport candidates back to caller
        let own_full_jid = self
            .get_pn()
            .await
            .ok_or_else(|| anyhow::anyhow!("Not logged in"))?
            .to_non_ad();
        let msg_id = self.generate_message_id().await;

        let mut our_te = Vec::new();
        match mapped_addr {
            SocketAddr::V4(v4) => {
                our_te.extend_from_slice(&v4.ip().octets());
                our_te.extend_from_slice(&v4.port().to_be_bytes());
            }
            SocketAddr::V6(v6) => {
                our_te.extend_from_slice(&v6.ip().octets());
                our_te.extend_from_slice(&v6.port().to_be_bytes());
            }
        }

        let transport_node = NodeBuilder::new("call")
            .attr("id", &msg_id)
            .attr("from", own_full_jid.to_string())
            .attr("to", call.caller.to_non_ad().to_string())
            .children(vec![NodeBuilder::new("transport")
                .attr("call-id", &call.call_id)
                .attr("call-creator", call.caller.to_non_ad().to_string())
                .children(vec![
                    NodeBuilder::new("te")
                        .attr("priority", "32")
                        .bytes(our_te)
                        .build(),
                    NodeBuilder::new("net")
                        .attr("medium", "3")
                        .attr("protocol", "0")
                        .build(),
                ])
                .build()])
            .build();

        self.send_node(transport_node)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send transport: {}", e))?;

        info!("Call {} transport sent with our relay candidate", call.call_id);

        // Media flows through the relay — the socket is already connected to the relay address.
        // No need to resolve peer candidates; the relay forwards our packets to the peer.

        // Step 4: SRTP contexts for sending and receiving.
        // Frida + no-auth decryption confirmed: standard SRTP (AES-128-ICM, 4-byte auth tag).
        // Send requires double encryption: E2E (4-byte tag) + HBH (10-byte tag).
        // Relay strips HBH and forwards E2E-only to the peer.
        let send_ssrc = keys.our_ssrc;
        let send_keys = keys.callee_to_caller.clone();
        let send_sframe_key = keys.sframe_callee.key;
        let send_hbh_keys = keys.hbh.clone();
        let recv_caller_keys = keys.caller_to_callee.clone();

        // Step 5: Bidirectional media loop (through relay)
        let socket = Arc::new(socket);
        let send_socket = socket.clone();
        let recv_socket = socket;
        let call_id = call.call_id.clone();

        // Sender: 60ms Opus frames (440Hz sine tone) with SRTP encryption.
        // We are the callee — encrypt with callee→caller keys, 4-byte E2E auth tag.
        let mut send_cancel_rx = cancel_rx.clone();
        let send_call_id = call_id.clone();
        let send_handle = tokio::spawn(async move {
            let mut seq: u16 = 0;
            let mut timestamp: u32 = 0;
            // WhatsApp Frida capture: ts increment = 960 per packet (20ms at 48kHz).
            let frame_samples: u32 = 960;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(20));

            // E2E SRTP context with standard RFC 3711 KDF.
            let mut e2e_send = crate::call_media::SrtpContext::new(&send_keys, send_ssrc, 4);
            // SFrame counter — caller expects SFrame-wrapped Opus (PCAP confirmed)
            let mut sframe_counter: u64 = 0;
            // HBH disabled — causes relay to drop packets.
            let mut hbh_send: Option<crate::call_media::SrtpContext> = None;
            let _ = send_hbh_keys; // suppress unused warning
            let _hbh_send_unused = send_hbh_keys.as_ref().map(|hbh| {
                crate::call_media::SrtpContext::new(hbh, send_ssrc, 10)
            });

            // Load TTS Opus frames (Hybrid SWB 20ms)
            let tone_frames = crate::call_media::sine_tone_frames();
            let mut frame_idx: usize = 0;

            // Base counter for ext ID=13 periodic values (observed starting ~0x4380)
            let ext_counter_base: u32 = 0x4380;

            // WASP relay keepalive: type 0x0801 every ~1s (Frida capture confirmed).
            // WhatsApp uses 0x0801 with incrementing txn ID counter, NOT 0x0011.
            let mut last_stun_ping = tokio::time::Instant::now();
            let stun_ping_interval = std::time::Duration::from_millis(1000);
            let ping_txn_base: [u8; 11] = rand::random();
            let mut ping_counter: u8 = 0;

            loop {
                tokio::select! {
                    _ = send_cancel_rx.changed() => {
                        info!("Call {} send loop cancelled", send_call_id);
                        break;
                    }
                    _ = interval.tick() => {
                        // WASP 0x0801 keepalive (~every 1 second)
                        if last_stun_ping.elapsed() >= stun_ping_interval {
                            let mut stun_ping = [0u8; 20];
                            // Type: 0x0801 (WASP keepalive, confirmed by Frida)
                            stun_ping[0] = 0x08;
                            stun_ping[1] = 0x01;
                            // Length: 0
                            // Magic cookie: 0x2112A442
                            stun_ping[4] = 0x21;
                            stun_ping[5] = 0x12;
                            stun_ping[6] = 0xA4;
                            stun_ping[7] = 0x42;
                            // Transaction ID: fixed base + incrementing last byte
                            stun_ping[8..19].copy_from_slice(&ping_txn_base);
                            stun_ping[19] = ping_counter;
                            ping_counter = ping_counter.wrapping_add(1);

                            if let Err(e) = send_socket.send(&stun_ping).await {
                                warn!("Call {} WASP keepalive error: {:?}", send_call_id, e);
                            }
                            last_stun_ping = tokio::time::Instant::now();
                        }

                        let opus_payload = if tone_frames.is_empty() {
                            crate::call_media::OPUS_SILENCE_60MS
                        } else {
                            let frame = tone_frames[frame_idx % tone_frames.len()];
                            frame_idx += 1;
                            frame
                        };

                        // Determine if this is a DTX/silence frame
                        let is_dtx = opus_payload.len() <= 3;

                        // Build 0xDEBE extension
                        let periodic = if seq % 9 == 0 {
                            Some(ext_counter_base.wrapping_add(seq as u32 * 111))
                        } else {
                            None
                        };
                        let ext_data = crate::call_sframe::build_debe_extension(is_dtx, periodic);

                        let rtp = crate::call_media::build_rtp_packet_wa(
                            send_ssrc, seq, timestamp, &ext_data, opus_payload,
                        );

                        // E2E SRTP with real HMAC
                        let srtp = e2e_send.protect_with_hmac(&rtp);

                        // Self-test: encrypt then decrypt to verify round-trip
                        if seq < 3 {
                            let mut verify_ctx = crate::call_media::SrtpContext::new(&send_keys, send_ssrc, 4);
                            let decrypted = verify_ctx.decrypt_no_auth(&srtp);
                            let header_len = crate::call_media::rtp_header_len(&decrypted);
                            let orig_payload = &rtp[header_len..];
                            let dec_payload = &decrypted[header_len..];
                            let match_ok = orig_payload == dec_payload;
                            let opus_first = if orig_payload.len() > 0 { format!("0x{:02x}", orig_payload[0]) } else { "empty".into() };
                            info!("Call {} SEND seq={}: rtp={}B opus={}B srtp={}B round_trip={} opus_toc={}",
                                send_call_id, seq, rtp.len(), opus_payload.len(), srtp.len(),
                                if match_ok { "OK" } else { "MISMATCH!" }, opus_first);
                            if !match_ok {
                                let orig_hex: String = orig_payload.iter().take(16).map(|b| format!("{:02x}", b)).collect();
                                let dec_hex: String = dec_payload.iter().take(16).map(|b| format!("{:02x}", b)).collect();
                                warn!("  CRYPTO BUG! orig={} dec={}", orig_hex, dec_hex);
                            }
                        }

                        // Send to relay (socket is connected to relay address)
                        if let Err(e) = send_socket.send(&srtp).await {
                            warn!("Call {} send error: {:?}", send_call_id, e);
                            break;
                        }

                        seq = seq.wrapping_add(1);
                        timestamp = timestamp.wrapping_add(frame_samples);
                    }
                }
            }
        });

        // Receiver: NO-AUTH SRTP decryption with caller→callee keys.
        // Frida confirmed: expandCallKey→generateE2EKeysV2 → HKDF(CallKey, callerJID, 46B).
        // Schirrmacher (2020): E2E auth uses constant zeros → skip verification, AES-ICM only.
        let recv_call_id = call_id.clone();
        let recv_handle = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let mut pkt_count: u64 = 0;
            let mut success_count: u64 = 0;

            // Recording file — app cache dir (std::env::temp_dir on Android)
            let recording_dir = std::env::temp_dir().join("wa_call_recordings");
            let _ = tokio::fs::create_dir_all(&recording_dir).await;
            let recording_path = recording_dir
                .join(format!("wa_call_{}.opus_raw", recv_call_id))
                .to_string_lossy()
                .to_string();
            let mut recording_file: Option<tokio::fs::File> = match tokio::fs::File::create(&recording_path).await {
                Ok(f) => {
                    info!("Call {} recording to {}", recv_call_id, recording_path);
                    Some(f)
                }
                Err(e) => {
                    warn!("Call {} failed to create recording: {:?}", recv_call_id, e);
                    None
                }
            };

            // NO-AUTH SRTP context (lazy-init on first packet to capture actual SSRC).
            // Caller→callee direction keys, 4-byte auth tag (skipped on verify).
            let mut noauth_ctx: Option<crate::call_media::SrtpContext> = None;

            loop {
                tokio::select! {
                    _ = cancel_rx.changed() => {
                        info!("Call {} recv loop cancelled", recv_call_id);
                        break;
                    }
                    result = recv_socket.recv(&mut buf) => {
                        match result {
                            Ok(len) => {
                                let pkt = &buf[..len];

                                // Filter non-RTP and RTCP
                                if len < 12 || (pkt[0] & 0xC0) != 0x80 {
                                    continue;
                                }
                                // Strip marker bit before RTCP check (C-4 fix: marker+PT=121 = 0xF9 >= 200)
                                let pt = pkt[1] & 0x7F;
                                if pt >= 72 && pt <= 77 {
                                    continue; // RTCP SR/RR/SDES/BYE/APP/XR
                                }

                                pkt_count += 1;
                                let rtp_seq = u16::from_be_bytes([pkt[2], pkt[3]]);

                                // Hex dump first 3 packets
                                if pkt_count <= 3 {
                                    let hex_preview = pkt.iter().take(40).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                                    let pt = pkt[1] & 0x7F;
                                    let ssrc = u32::from_be_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]);
                                    info!(
                                        "Call {} pkt #{}: {} bytes, PT={} seq={} SSRC=0x{:08x}\n  hex: {}",
                                        recv_call_id, pkt_count, len, pt, rtp_seq, ssrc, hex_preview
                                    );
                                }

                                // Minimum: RTP header (12) + auth tag (4)
                                if len < 16 { continue; }

                                // Lazy-init NO-AUTH SRTP context with actual SSRC from first packet
                                let ctx = noauth_ctx.get_or_insert_with(|| {
                                    let actual_ssrc = u32::from_be_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]);
                                    info!("Call {} NO-AUTH-SRTP: init with caller keys, SSRC=0x{:08x}",
                                        recv_call_id, actual_ssrc);
                                    crate::call_media::SrtpContext::new(&recv_caller_keys, actual_ssrc, 4)
                                });

                                let header_len = crate::call_media::rtp_header_len(pkt);
                                if pkt.len() <= header_len { continue; }
                                let raw_payload = &pkt[header_len..];
                                let rtp_header = &pkt[..header_len];

                                // Pure SRTP decrypt (no SFrame — confirmed by Frida: GCM is only for signaling)
                                // Try with 4-byte auth tag first, then 0-byte (no tag)
                                let dec4 = ctx.decrypt_no_auth(pkt);
                                let hl4 = crate::call_media::rtp_header_len(&dec4);

                                // Also try without stripping any auth tag (auth_tag_len=0)
                                let nostrip_ctx = crate::call_media::SrtpContext::new(
                                    &recv_caller_keys, ctx.ssrc, 0);
                                let dec0 = nostrip_ctx.decrypt_no_auth(pkt);
                                let hl0 = crate::call_media::rtp_header_len(&dec0);

                                if pkt_count <= 5 {
                                    let p4 = if dec4.len() > hl4 { &dec4[hl4..] } else { &[] };
                                    let p0 = if dec0.len() > hl0 { &dec0[hl0..] } else { &[] };
                                    info!("Call {} pkt#{} SRTP auth=4: first=0x{:02x} len={}B | auth=0: first=0x{:02x} len={}B\n  raw_pl={}B hex={}\n  dec4_hex={}\n  dec0_hex={}",
                                        recv_call_id, pkt_count,
                                        p4.first().copied().unwrap_or(0), p4.len(),
                                        p0.first().copied().unwrap_or(0), p0.len(),
                                        raw_payload.len(),
                                        raw_payload.iter().take(24).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(""),
                                        p4.iter().take(24).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(""),
                                        p0.iter().take(24).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(""));
                                }

                                // Use auth=4 for recording (consistent with previous behavior)
                                let opus = if dec4.len() > hl4 { dec4[hl4..].to_vec() } else { continue };

                                success_count += 1;
                                if success_count % 500 == 0 {
                                    info!("Call {} decrypted {} packets",
                                        recv_call_id, success_count);
                                }

                                // Write to recording file: [4B timestamp][2B seq][2B len][opus data]
                                if let Some(ref mut file) = recording_file {
                                    let rtp_ts = u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
                                    let pl = opus.len() as u16;
                                    let mut fh = [0u8; 8];
                                    fh[0..4].copy_from_slice(&rtp_ts.to_be_bytes());
                                    fh[4..6].copy_from_slice(&rtp_seq.to_be_bytes());
                                    fh[6..8].copy_from_slice(&pl.to_be_bytes());
                                    let _ = AsyncWriteExt::write_all(file, &fh).await;
                                    let _ = AsyncWriteExt::write_all(file, &opus).await;
                                }
                            }
                            Err(e) => {
                                warn!("Call {} recv error: {:?}", recv_call_id, e);
                                break;
                            }
                        }
                    }
                }
            }
            if let Some(ref mut file) = recording_file {
                let _ = AsyncWriteExt::flush(file).await;
            }
            info!(
                "Call {} total={} pkts, decrypted={}, strategy=NO-AUTH-SRTP-caller, recording: {}",
                recv_call_id, pkt_count, success_count, recording_path
            );
        });

        // Wait for either task to finish (usually via cancellation)
        tokio::select! {
            _ = send_handle => {}
            _ = recv_handle => {}
        }

        info!("Call {} media loop ended", call_id);
        Ok(())
    }

    /// Terminate (hang up) an active call.
    pub async fn terminate_call(&self, call_id: &str) -> Result<(), anyhow::Error> {
        let call = self
            .call_store
            .get(call_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("No active call with id {}", call_id))?;

        // Signal media loop cancellation
        if let Some(tx) = &call.cancel_tx {
            let _ = tx.send(true);
        }

        let own_jid = self
            .get_pn()
            .await
            .ok_or_else(|| anyhow::anyhow!("Not logged in"))?
            .to_non_ad();
        let caller = call.caller.to_non_ad();

        let msg_id = self.generate_message_id().await;

        let node = NodeBuilder::new("call")
            .attr("id", &msg_id)
            .attr("from", own_jid.to_string())
            .attr("to", caller.to_string())
            .children(vec![NodeBuilder::new("terminate")
                .attr("call-id", &call.call_id)
                .attr("call-creator", caller.to_string())
                .attr("reason", "general-error")
                .build()])
            .build();

        info!("Terminating call {} to {}", call_id, caller);
        self.send_node(node)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        self.call_store.remove(call_id).await;
        Ok(())
    }

    /// Reject an incoming call.
    ///
    /// Removes the call from the store and sends a `<reject>` stanza to the
    /// caller. Uses `wacore::call::build_reject_call` for node construction.
    pub async fn reject_call(&self, call_id: &str) -> Result<(), anyhow::Error> {
        let call = self
            .call_store
            .remove(call_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("No active call with id {}", call_id))?;

        let own_jid = self
            .get_pn()
            .await
            .ok_or_else(|| anyhow::anyhow!("Not logged in"))?
            .to_non_ad();
        let caller = call.caller.to_non_ad();

        let msg_id = self.generate_message_id().await;

        let node = wacore::call::build_reject_call(&wacore::call::RejectCallParams {
            message_id: msg_id,
            own_jid,
            caller: caller.clone(),
            call_id: call.call_id.clone(),
        });

        info!("Rejecting call {} from {}", call_id, caller);
        self.send_node(node)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Accept an incoming call.
    pub async fn accept_call(&self, call_id: &str) -> Result<(), anyhow::Error> {
        let call = self
            .call_store
            .get(call_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("No active call with id {}", call_id))?;

        let own_jid = self
            .get_pn()
            .await
            .ok_or_else(|| anyhow::anyhow!("Not logged in"))?
            .to_non_ad();
        let caller = call.caller.to_non_ad();

        let msg_id = self.generate_message_id().await;

        let mut children = vec![
            NodeBuilder::new("audio")
                .attr("enc", "opus")
                .attr("rate", "16000")
                .build(),
            NodeBuilder::new("audio")
                .attr("enc", "opus")
                .attr("rate", "8000")
                .build(),
        ];

        if call.is_video {
            children.push(
                NodeBuilder::new("video")
                    .attr("enc", "vp8")
                    .attr("dec", "vp8")
                    .build(),
            );
        }

        children.push(NodeBuilder::new("net").attr("medium", "3").build());
        children.push(NodeBuilder::new("encopt").attr("keygen", "2").build());

        let node = NodeBuilder::new("call")
            .attr("id", &msg_id)
            .attr("from", own_jid.to_string())
            .attr("to", caller.to_string())
            .children(vec![NodeBuilder::new("accept")
                .attr("call-id", &call.call_id)
                .attr("call-creator", caller.to_string())
                .children(children)
                .build()])
            .build();

        info!("Accepting call {} from {}", call_id, caller);
        self.send_node(node)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}
