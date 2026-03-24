//! Peer-to-peer message protocol for direct TCP mesh communication.
//!
//! This module defines the messages exchanged between NEBULA nodes over
//! direct TCP connections (bypassing the MQTT broker), plus wire-format
//! helpers that serialize with bincode and encrypt with the SecurityEnvelope.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use nebula_core::security::envelope::SecurityEnvelope;
use nebula_core::security::keys::{AesKey, HmacKey};

/// Maximum peer message size after encryption (4 MB).
/// Larger messages are rejected to prevent allocation attacks.
const MAX_PEER_MESSAGE_SIZE: u32 = 4 * 1024 * 1024;

/// Messages exchanged between peers over direct TCP connections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PeerMessage {
    /// Periodic health status exchange.
    Heartbeat {
        node_id: String,
        score: f32,
        battery_level: u8,
        cpu_load: f32,
        memory_available_mb: u32,
        active_tasks: u16,
        timestamp: i64,
    },

    /// Master publishes the current succession line to all peers.
    SuccessionLine {
        ordered_nodes: Vec<SuccessionEntry>,
        computed_at: i64,
    },

    /// A worker reports it detected master timeout.
    MasterTimeout {
        reporter_node_id: String,
        last_master_heartbeat: i64,
    },

    /// A worker claims promotion based on succession rank.
    PromotionClaim {
        node_id: String,
        succession_rank: u32,
        health_score: f32,
    },

    /// Acknowledge a promotion claim.
    PromotionAck { node_id: String, ack_from: String },

    /// New master announces its MQTT broker address.
    NewMasterBroker {
        node_id: String,
        mqtt_host: String,
        mqtt_port: u16,
    },

    /// Direct data request (for DB queries, file requests, etc.).
    DataRequest {
        request_id: String,
        action: String,
        payload: Vec<u8>,
    },

    /// Response to a data request.
    DataResponse {
        request_id: String,
        success: bool,
        payload: Vec<u8>,
        error: Option<String>,
    },

    /// Ping for connection liveness.
    Ping { timestamp: i64 },

    /// Pong response.
    Pong { timestamp: i64 },
}

/// A single entry in the master-published succession line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuccessionEntry {
    pub node_id: String,
    pub score: f32,
    pub rank: u32,
    pub eligible: bool,
}

/// Serialize a `PeerMessage` to encrypted bytes using `SecurityEnvelope`.
///
/// The returned bytes are the sealed ciphertext (HMAC tag + nonce + ciphertext).
/// The caller (connection layer) is responsible for adding the 4-byte big-endian
/// length prefix when writing to the wire.
pub fn encode_message(msg: &PeerMessage, hmac_key: &HmacKey, aes_key: &AesKey) -> Result<Vec<u8>> {
    let plaintext =
        bincode::serialize(msg).map_err(|e| anyhow::anyhow!("bincode serialize: {}", e))?;
    SecurityEnvelope::seal(&plaintext, hmac_key, aes_key)
}

/// Deserialize encrypted bytes back to a `PeerMessage`.
///
/// Expects the sealed ciphertext produced by `encode_message` (without the
/// length prefix -- the connection layer strips that before calling this).
pub fn decode_message(data: &[u8], hmac_key: &HmacKey, aes_key: &AesKey) -> Result<PeerMessage> {
    if data.len() as u64 > u64::from(MAX_PEER_MESSAGE_SIZE) {
        bail!(
            "Peer message too large: {} bytes (max {})",
            data.len(),
            MAX_PEER_MESSAGE_SIZE
        );
    }
    let plaintext = SecurityEnvelope::open(data, hmac_key, aes_key)?;
    let msg: PeerMessage = bincode::deserialize(&plaintext)
        .map_err(|e| anyhow::anyhow!("bincode deserialize: {}", e))?;
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::security::keys::KeyPair;

    fn test_keys() -> KeyPair {
        KeyPair::derive_from_secret(b"peer-test-secret").unwrap()
    }

    #[test]
    fn test_heartbeat_serde_roundtrip() {
        let msg = PeerMessage::Heartbeat {
            node_id: "node-1".into(),
            score: 0.85,
            battery_level: 90,
            cpu_load: 0.2,
            memory_available_mb: 4096,
            active_tasks: 3,
            timestamp: 1700000000,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: PeerMessage = bincode::deserialize(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_succession_line_serde_roundtrip() {
        let msg = PeerMessage::SuccessionLine {
            ordered_nodes: vec![
                SuccessionEntry {
                    node_id: "a".into(),
                    score: 0.95,
                    rank: 1,
                    eligible: true,
                },
                SuccessionEntry {
                    node_id: "b".into(),
                    score: 0.80,
                    rank: 2,
                    eligible: true,
                },
            ],
            computed_at: 1700000001,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        assert_eq!(msg, bincode::deserialize::<PeerMessage>(&bytes).unwrap());
    }

    #[test]
    fn test_data_request_serde_roundtrip() {
        let msg = PeerMessage::DataRequest {
            request_id: "req-001".into(),
            action: "get_config".into(),
            payload: vec![1, 2, 3, 4],
        };
        let bytes = bincode::serialize(&msg).unwrap();
        assert_eq!(msg, bincode::deserialize::<PeerMessage>(&bytes).unwrap());
    }

    #[test]
    fn test_ping_pong_serde_roundtrip() {
        let ping = PeerMessage::Ping { timestamp: 42 };
        let pong = PeerMessage::Pong { timestamp: 43 };
        assert_eq!(
            ping,
            bincode::deserialize(&bincode::serialize(&ping).unwrap()).unwrap()
        );
        assert_eq!(
            pong,
            bincode::deserialize(&bincode::serialize(&pong).unwrap()).unwrap()
        );
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let keys = test_keys();
        let msg = PeerMessage::Heartbeat {
            node_id: "node-1".into(),
            score: 0.85,
            battery_level: 90,
            cpu_load: 0.2,
            memory_available_mb: 4096,
            active_tasks: 3,
            timestamp: 1700000000,
        };
        let encoded = encode_message(&msg, &keys.hmac, &keys.aes).unwrap();
        let decoded = decode_message(&encoded, &keys.hmac, &keys.aes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_encode_produces_different_ciphertext_each_time() {
        let keys = test_keys();
        let msg = PeerMessage::Ping { timestamp: 42 };
        let enc1 = encode_message(&msg, &keys.hmac, &keys.aes).unwrap();
        let enc2 = encode_message(&msg, &keys.hmac, &keys.aes).unwrap();
        assert_ne!(enc1, enc2);
        assert_eq!(
            decode_message(&enc1, &keys.hmac, &keys.aes).unwrap(),
            decode_message(&enc2, &keys.hmac, &keys.aes).unwrap(),
        );
    }

    #[test]
    fn test_wrong_key_fails_decode() {
        let keys1 = KeyPair::derive_from_secret(b"secret-alpha").unwrap();
        let keys2 = KeyPair::derive_from_secret(b"secret-beta").unwrap();
        let msg = PeerMessage::Ping { timestamp: 42 };
        let encoded = encode_message(&msg, &keys1.hmac, &keys1.aes).unwrap();
        assert!(decode_message(&encoded, &keys2.hmac, &keys2.aes).is_err());
    }

    #[test]
    fn test_tampered_data_fails_decode() {
        let keys = test_keys();
        let msg = PeerMessage::Ping { timestamp: 42 };
        let mut encoded = encode_message(&msg, &keys.hmac, &keys.aes).unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xFF;
        assert!(decode_message(&encoded, &keys.hmac, &keys.aes).is_err());
    }

    #[test]
    fn test_succession_entry_fields() {
        let entry = SuccessionEntry {
            node_id: "z".into(),
            score: 0.77,
            rank: 3,
            eligible: false,
        };
        let bytes = bincode::serialize(&entry).unwrap();
        let decoded: SuccessionEntry = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.node_id, "z");
        assert!(!decoded.eligible);
    }
}
