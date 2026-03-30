//! WhatsApp call media pipeline: SRTP key derivation, relay binding, and audio streaming.

use crate::call::RelayEndpoint;
use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use aes::Aes128;
use aes_gcm::{Aes128Gcm, Nonce};
use aes_gcm::aead::Aead;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use log::{debug, info, warn};
use sha1::Sha1;
use sha2::Sha256;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

/// Derived SRTP keying material for one direction.
#[derive(Debug, Clone)]
pub struct SrtpKeys {
    /// 16-byte AES-128-ICM master key.
    pub master_key: [u8; 16],
    /// 14-byte SRTP master salt.
    pub master_salt: [u8; 14],
}

/// SFrame key material derived from bytes 30-45 of the HKDF output.
/// Used for AES-128-GCM media frame encryption inside the SRTP E2E layer.
#[derive(Debug, Clone)]
pub struct SFrameKeys {
    /// 16-byte AES-128-GCM key for SFrame encrypt/decrypt.
    pub key: [u8; 16],
}

/// Expanded SRTP session keys derived from master key + salt per RFC 3711 §4.3.1.
#[derive(Clone)]
pub struct SrtpSessionKeys {
    /// 16-byte cipher key (k_e) for AES-128-ICM.
    pub cipher_key: [u8; 16],
    /// 20-byte authentication key (k_a) for HMAC-SHA1.
    pub auth_key: [u8; 20],
    /// 14-byte session salt (k_s) for IV construction.
    pub session_salt: [u8; 14],
}

/// A complete SRTP context for one direction (send or receive).
pub struct SrtpContext {
    pub session_keys: SrtpSessionKeys,
    pub ssrc: u32,
    /// Rollover counter — incremented each time SEQ wraps from 0xFFFF to 0.
    pub roc: u32,
    /// Highest SEQ seen (for receive) or next SEQ (for send).
    pub seq: u16,
    /// Auth tag length in bytes (10 for HBH, 4 for E2E).
    pub auth_tag_len: usize,
}

impl SrtpContext {
    pub fn new(keys: &SrtpKeys, ssrc: u32, auth_tag_len: usize) -> Self {
        let session_keys = derive_session_keys(&keys.master_key, &keys.master_salt);
        Self {
            session_keys,
            ssrc,
            roc: 0,
            seq: 0,
            auth_tag_len,
        }
    }

    /// Create context using HKDF output DIRECTLY as AES-ICM key/salt (no RFC 3711 KDF).
    /// Hypothesis: WhatsApp may skip the session key derivation step.
    pub fn new_direct(keys: &SrtpKeys, ssrc: u32, auth_tag_len: usize) -> Self {
        // Use master key directly as cipher key, master salt as session salt
        // Still derive auth key via KDF for HMAC computation
        let auth_key_vec = kdf(&keys.master_key, &keys.master_salt, 0x01, 20);
        let mut k_a = [0u8; 20];
        k_a.copy_from_slice(&auth_key_vec);
        Self {
            session_keys: SrtpSessionKeys {
                cipher_key: keys.master_key,
                auth_key: k_a,
                session_salt: keys.master_salt,
            },
            ssrc,
            roc: 0,
            seq: 0,
            auth_tag_len,
        }
    }

    /// Encrypt an RTP payload with zero auth tag (E2E layer).
    /// Schirrmacher 2020: WhatsApp E2E uses constant zeros for auth.
    pub fn protect(&mut self, rtp_packet: &[u8]) -> Vec<u8> {
        let header_len = rtp_header_len(rtp_packet);
        let seq = u16::from_be_bytes([rtp_packet[2], rtp_packet[3]]);

        // Update ROC on sequence wrap
        if seq < self.seq && (self.seq - seq) > 0x8000 {
            self.roc = self.roc.wrapping_add(1);
        }
        self.seq = seq;

        let packet_index = ((self.roc as u64) << 16) | (seq as u64);

        // Encrypt payload with AES-128-ICM
        let mut out = rtp_packet.to_vec();
        aes_icm_crypt(
            &self.session_keys.cipher_key,
            &self.session_keys.session_salt,
            self.ssrc,
            packet_index,
            &mut out[header_len..],
        );

        // E2E: zero auth tag (Schirrmacher 2020)
        out.extend_from_slice(&vec![0u8; self.auth_tag_len]);
        out
    }

    /// Encrypt an RTP payload with real HMAC-SHA1 auth tag (HBH layer).
    /// The WASP relay verifies HBH integrity before stripping and forwarding.
    pub fn protect_with_hmac(&mut self, rtp_packet: &[u8]) -> Vec<u8> {
        let header_len = rtp_header_len(rtp_packet);
        let seq = u16::from_be_bytes([rtp_packet[2], rtp_packet[3]]);

        if seq < self.seq && (self.seq - seq) > 0x8000 {
            self.roc = self.roc.wrapping_add(1);
        }
        self.seq = seq;

        let packet_index = ((self.roc as u64) << 16) | (seq as u64);

        let mut out = rtp_packet.to_vec();
        aes_icm_crypt(
            &self.session_keys.cipher_key,
            &self.session_keys.session_salt,
            self.ssrc,
            packet_index,
            &mut out[header_len..],
        );

        // HBH: real HMAC-SHA1 auth tag (relay verifies this)
        let tag = compute_auth_tag(
            &self.session_keys.auth_key,
            &out,
            self.roc,
            self.auth_tag_len,
        );
        out.extend_from_slice(&tag);
        out
    }

    /// Decrypt an SRTP packet. Returns the RTP packet (header + plaintext payload).
    pub fn unprotect(&mut self, srtp_packet: &[u8]) -> Result<Vec<u8>, &'static str> {
        if srtp_packet.len() < 12 + self.auth_tag_len {
            return Err("SRTP packet too short");
        }

        let auth_boundary = srtp_packet.len() - self.auth_tag_len;
        let encrypted_portion = &srtp_packet[..auth_boundary];
        let received_tag = &srtp_packet[auth_boundary..];

        let header_len = rtp_header_len(encrypted_portion);
        let seq = u16::from_be_bytes([encrypted_portion[2], encrypted_portion[3]]);

        // Estimate ROC
        let roc = if seq < self.seq && (self.seq - seq) > 0x8000 {
            self.roc.wrapping_add(1)
        } else {
            self.roc
        };

        // Verify auth tag
        let expected_tag = compute_auth_tag(
            &self.session_keys.auth_key,
            encrypted_portion,
            roc,
            self.auth_tag_len,
        );
        if received_tag != expected_tag.as_slice() {
            return Err("SRTP auth tag mismatch");
        }

        // Update state after successful auth
        if seq < self.seq && (self.seq - seq) > 0x8000 {
            self.roc = self.roc.wrapping_add(1);
        }
        self.seq = seq;

        let packet_index = ((roc as u64) << 16) | (seq as u64);

        // Decrypt payload (ICM is symmetric — same operation as encrypt)
        let mut out = encrypted_portion.to_vec();
        aes_icm_crypt(
            &self.session_keys.cipher_key,
            &self.session_keys.session_salt,
            self.ssrc,
            packet_index,
            &mut out[header_len..],
        );

        Ok(out)
    }

    /// Decrypt payload-only using AES-ICM, skipping auth verification entirely.
    /// Schirrmacher (2020) found WhatsApp E2E 4-byte auth uses constant zeros as HMAC input,
    /// meaning standard auth verification will always fail.
    /// This strips the auth tag, decrypts the payload, and returns (rtp_header + decrypted_payload).
    pub fn decrypt_no_auth(&self, srtp_packet: &[u8]) -> Vec<u8> {
        let auth_boundary = srtp_packet.len().saturating_sub(self.auth_tag_len);
        let encrypted_portion = &srtp_packet[..auth_boundary];

        let header_len = rtp_header_len(encrypted_portion);
        let seq = u16::from_be_bytes([encrypted_portion[2], encrypted_portion[3]]);
        let packet_index = seq as u64; // ROC = 0 for simplicity

        let mut out = encrypted_portion.to_vec();
        aes_icm_crypt(
            &self.session_keys.cipher_key,
            &self.session_keys.session_salt,
            self.ssrc,
            packet_index,
            &mut out[header_len..],
        );

        out
    }
}

// === RFC 3711 Session Key Derivation (§4.3.1) ===

/// Derive session keys (k_e, k_a, k_s) from master key and salt using AES-CM PRF.
/// Public wrapper for use by call_sframe.rs.
pub fn derive_session_keys_pub(master_key: &[u8; 16], master_salt: &[u8; 14]) -> SrtpSessionKeys {
    derive_session_keys(master_key, master_salt)
}

/// Derive session keys (k_e, k_a, k_s) from master key and salt using AES-CM PRF.
fn derive_session_keys(master_key: &[u8; 16], master_salt: &[u8; 14]) -> SrtpSessionKeys {
    let cipher_key = kdf(master_key, master_salt, 0x00, 16);
    let auth_key = kdf(master_key, master_salt, 0x01, 20);
    let salt_key = kdf(master_key, master_salt, 0x02, 14);

    let mut k_e = [0u8; 16];
    let mut k_a = [0u8; 20];
    let mut k_s = [0u8; 14];
    k_e.copy_from_slice(&cipher_key);
    k_a.copy_from_slice(&auth_key);
    k_s.copy_from_slice(&salt_key);

    SrtpSessionKeys {
        cipher_key: k_e,
        auth_key: k_a,
        session_salt: k_s,
    }
}

/// RFC 3711 key derivation function using AES-CM (counter mode) as PRF.
/// `label` is 0x00 for cipher key, 0x01 for auth key, 0x02 for salt key.
fn kdf(master_key: &[u8; 16], master_salt: &[u8; 14], label: u8, length: usize) -> Vec<u8> {
    // x = label || r (where r = 0 for key_derivation_rate = 0)
    // key_id = label << 48 (label in the 7th byte of a 14-byte value, counting from 0)
    let mut x = [0u8; 14];
    x[7] = label;

    // IV = (master_salt XOR x) || 0x0000 (padded to 16 bytes)
    let mut iv = [0u8; 16];
    for i in 0..14 {
        iv[i] = master_salt[i] ^ x[i];
    }

    // Generate keystream using AES-CM (AES in counter mode)
    let aes = Aes128::new(GenericArray::from_slice(master_key));
    let mut output = Vec::with_capacity(length);
    let blocks_needed = (length + 15) / 16;

    for counter in 0..blocks_needed {
        let mut block = iv;
        // Add counter to the last 2 bytes (big-endian)
        let c = counter as u16;
        block[14] = (c >> 8) as u8;
        block[15] = c as u8;

        let mut ga = GenericArray::clone_from_slice(&block);
        aes.encrypt_block(&mut ga);
        output.extend_from_slice(&ga);
    }

    output.truncate(length);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_kdf_rfc3711_b3() {
        // RFC 3711 Appendix B.3 test vector
        let mk = <[u8; 16]>::try_from(hex::decode("E1F97A0D3E018BE0D64FA32C06DE4139").unwrap().as_slice()).unwrap();
        let ms = <[u8; 14]>::try_from(hex::decode("0EC675AD498AFEEBB6960B3AABE6").unwrap().as_slice()).unwrap();
        let ck = kdf(&mk, &ms, 0x00, 16);
        let ak = kdf(&mk, &ms, 0x01, 20);
        let sk = kdf(&mk, &ms, 0x02, 14);
        assert_eq!(hex::encode(&ck).to_uppercase(), "C61E7A93744F39EE10734AFE3FF7A087", "cipher key mismatch");
        assert_eq!(hex::encode(&ak).to_uppercase(), "CEBE321F6FF7716B6FD4AB49AF256A156D38BAA4", "auth key mismatch");
        assert_eq!(hex::encode(&sk).to_uppercase(), "30CBBC08863D8C85D49DB34A9AE1", "salt key mismatch");
    }
}

// === AES-128-ICM Encryption (RFC 3711 §4.1.1) ===

/// Encrypt/decrypt payload in-place using AES-128-ICM (counter mode).
fn aes_icm_crypt(
    cipher_key: &[u8; 16],
    session_salt: &[u8; 14],
    ssrc: u32,
    packet_index: u64,
    payload: &mut [u8],
) {
    // IV = (session_salt padded to 16 bytes) XOR (0x0000 || SSRC || packet_index || 0x0000)
    let mut iv = [0u8; 16];
    iv[..14].copy_from_slice(session_salt);

    // XOR SSRC into bytes 4..8
    let ssrc_bytes = ssrc.to_be_bytes();
    for i in 0..4 {
        iv[4 + i] ^= ssrc_bytes[i];
    }

    // XOR packet_index (48-bit) into bytes 8..14
    let pi_bytes = packet_index.to_be_bytes(); // 8 bytes, we use lower 6
    for i in 0..6 {
        iv[8 + i] ^= pi_bytes[2 + i];
    }

    // Generate keystream and XOR with payload
    let aes = Aes128::new(GenericArray::from_slice(cipher_key));
    let blocks_needed = (payload.len() + 15) / 16;
    let mut offset = 0;

    for counter in 0..blocks_needed {
        let mut block = iv;
        // Add counter to last 2 bytes
        let prev = u16::from_be_bytes([block[14], block[15]]);
        let new_counter = prev.wrapping_add(counter as u16);
        block[14] = (new_counter >> 8) as u8;
        block[15] = new_counter as u8;

        let mut ga = GenericArray::clone_from_slice(&block);
        aes.encrypt_block(&mut ga);

        let remaining = payload.len() - offset;
        let to_xor = remaining.min(16);
        for i in 0..to_xor {
            payload[offset + i] ^= ga[i];
        }
        offset += to_xor;
    }
}

// === HMAC-SHA1 Authentication (RFC 3711 §4.2) ===

/// Compute truncated HMAC-SHA1 auth tag over (rtp_data || ROC).
fn compute_auth_tag(auth_key: &[u8; 20], rtp_data: &[u8], roc: u32, tag_len: usize) -> Vec<u8> {
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(auth_key).expect("HMAC key length is valid");
    mac.update(rtp_data);
    mac.update(&roc.to_be_bytes());
    let result = mac.finalize().into_bytes();
    result[..tag_len].to_vec()
}

// === Double Encryption (E2E + HBH) ===

/// Protect an RTP packet with double SRTP encryption (E2E inner + HBH outer).
/// WhatsApp relays strip the HBH layer and forward the E2E layer.
/// E2E uses zero auth (Schirrmacher 2020), HBH uses real HMAC-SHA1 (relay verifies).
pub fn double_protect(
    e2e_ctx: &mut SrtpContext,
    hbh_ctx: &mut SrtpContext,
    rtp_packet: &[u8],
) -> Vec<u8> {
    // First: E2E encrypt (4-byte zero auth tag)
    let e2e_protected = e2e_ctx.protect(rtp_packet);
    // Second: HBH encrypt with real HMAC-SHA1 (10-byte auth tag)
    // Relay verifies HBH integrity before stripping and forwarding E2E layer.
    hbh_ctx.protect_with_hmac(&e2e_protected)
}

/// Unprotect a double-encrypted SRTP packet (strip HBH outer, then E2E inner).
pub fn double_unprotect(
    e2e_ctx: &mut SrtpContext,
    hbh_ctx: &mut SrtpContext,
    srtp_packet: &[u8],
) -> Result<Vec<u8>, &'static str> {
    // First: strip HBH layer
    let after_hbh = hbh_ctx.unprotect(srtp_packet)?;
    // Second: strip E2E layer
    e2e_ctx.unprotect(&after_hbh)
}

/// Get the RTP header length (minimum 12, plus CSRC and extension).
pub fn rtp_header_len(pkt: &[u8]) -> usize {
    if pkt.len() < 12 {
        return pkt.len();
    }
    let cc = (pkt[0] & 0x0F) as usize;
    let mut len = 12 + cc * 4;
    // Check extension bit
    if pkt[0] & 0x10 != 0 && pkt.len() >= len + 4 {
        let ext_len = u16::from_be_bytes([pkt[len + 2], pkt[len + 3]]) as usize;
        len += 4 + ext_len * 4;
    }
    len.min(pkt.len())
}

/// Full call media session keys (both directions + HBH).
#[derive(Debug, Clone)]
pub struct CallMediaKeys {
    /// Keys for the stream FROM the caller (caller → callee).
    pub caller_to_callee: SrtpKeys,
    /// Keys for the stream FROM the callee (callee → caller).
    pub callee_to_caller: SrtpKeys,
    /// SFrame keys per direction (bytes 30-45 of HKDF output).
    pub sframe_caller: SFrameKeys,
    pub sframe_callee: SFrameKeys,
    /// SSRC for our outgoing audio stream.
    pub our_ssrc: u32,
    /// SSRC for the remote peer's audio stream.
    pub peer_ssrc: u32,
    /// Hop-by-hop keys (pre-split from hbh_key: 16 key + 14 salt).
    pub hbh: Option<SrtpKeys>,
}

/// Derive SRTP keys from the 32-byte CallKey using HKDF-SHA256.
///
/// Per the WhatsApp protocol (keygen=2):
/// - `HKDF(ikm=CallKey, salt=nil, info=peer_jid, length=46)` per direction
/// - First 16 bytes = SRTP master key, next 14 = SRTP master salt
/// - SSRC derived via `HKDF(ikm=call_id_hex, salt=<00000000>, info=peer_jid, length=4)`
pub fn derive_call_media_keys(
    call_key: &[u8],
    caller_jid: &str,
    callee_jid: &str,
    callee_jid_for_ssrc: &str,
    call_id: &str,
    hbh_key_bytes: Option<&[u8]>,
) -> CallMediaKeys {
    // SRTP keys use device :0 JID (caller always uses primary device for expandCallKey)
    let (caller_to_callee, sframe_caller) = derive_srtp_direction(call_key, caller_jid);
    let (callee_to_caller, sframe_callee) = derive_srtp_direction(call_key, callee_jid);

    // SSRCs use the actual device JID (caller generates SSRCs per-device)
    let our_ssrc = derive_ssrc(call_id, callee_jid_for_ssrc);
    let peer_ssrc = derive_ssrc(call_id, caller_jid);

    let hbh = hbh_key_bytes.and_then(|bytes| {
        if bytes.len() >= 30 {
            let mut key = [0u8; 16];
            let mut salt = [0u8; 14];
            key.copy_from_slice(&bytes[0..16]);
            salt.copy_from_slice(&bytes[16..30]);
            Some(SrtpKeys {
                master_key: key,
                master_salt: salt,
            })
        } else {
            warn!("HBH key too short: {} bytes (need 30)", bytes.len());
            None
        }
    });

    info!(
        "Derived keys: our_ssrc=0x{:08x}, peer_ssrc=0x{:08x}, hbh={}, sframe_caller={}, sframe_callee={}",
        our_ssrc, peer_ssrc, hbh.is_some(),
        hex::encode(&sframe_caller.key), hex::encode(&sframe_callee.key)
    );

    CallMediaKeys {
        caller_to_callee,
        callee_to_caller,
        sframe_caller,
        sframe_callee,
        our_ssrc,
        peer_ssrc,
        hbh,
    }
}

fn derive_srtp_direction(call_key: &[u8], jid: &str) -> (SrtpKeys, SFrameKeys) {
    let hk = Hkdf::<Sha256>::new(None, call_key);
    let mut okm = [0u8; 46];
    hk.expand(jid.as_bytes(), &mut okm)
        .expect("HKDF expand should not fail for 46 bytes");

    let mut master_key = [0u8; 16];
    let mut master_salt = [0u8; 14];
    let mut sframe_key = [0u8; 16];
    master_key.copy_from_slice(&okm[0..16]);
    master_salt.copy_from_slice(&okm[16..30]);
    sframe_key.copy_from_slice(&okm[30..46]);

    debug!(
        "Keys for {}: srtp_key={}, srtp_salt={}, sframe_key={}",
        jid,
        hex::encode(&master_key),
        hex::encode(&master_salt),
        hex::encode(&sframe_key),
    );

    (
        SrtpKeys { master_key, master_salt },
        SFrameKeys { key: sframe_key },
    )
}

fn derive_ssrc(call_id: &str, jid: &str) -> u32 {
    // IKM = call_id as hex ASCII, salt = 0x00000000, info = peer JID
    let salt = [0u8; 4];
    let hk = Hkdf::<Sha256>::new(Some(&salt), call_id.as_bytes());
    let mut ssrc_bytes = [0u8; 4];
    hk.expand(jid.as_bytes(), &mut ssrc_bytes)
        .expect("HKDF expand should not fail for 4 bytes");
    // WhatsApp interprets HKDF output as little-endian (confirmed by Frida:
    // derived 0x2264659e with BE but actual packet SSRC is 0x9e656422 = same bytes LE)
    u32::from_le_bytes(ssrc_bytes)
}

/// Attempt to bind to the best relay server using STUN with the WASP token.
///
/// Sends a STUN Binding Request with the 182-byte token to the relay on port 3478.
/// Returns the bound socket if successful.
pub async fn bind_to_relay(
    relay_endpoints: &[RelayEndpoint],
    tokens: &[Vec<u8>],
    relay_key: &[u8],
) -> Result<(UdpSocket, SocketAddr), anyhow::Error> {
    // Sort by RTT — prefer the closest relay
    let mut sorted: Vec<&RelayEndpoint> = relay_endpoints.iter().collect();
    sorted.sort_by_key(|e| e.c2r_rtt);

    // Filter to IPv4 UDP endpoints only (protocol=0 means UDP, protocol=1 means TCP)
    let udp_ipv4_endpoints: Vec<&&RelayEndpoint> = sorted
        .iter()
        .filter(|e| e.addr.is_ipv4() && e.protocol == 0)
        .collect();

    info!(
        "Relay binding: {} total endpoints, {} UDP IPv4 candidates",
        relay_endpoints.len(),
        udp_ipv4_endpoints.len()
    );

    if udp_ipv4_endpoints.is_empty() {
        return Err(anyhow::anyhow!("No UDP IPv4 relay endpoints available"));
    }

    for ep in &udp_ipv4_endpoints {
        let token_idx = ep.token_id as usize;
        let token = tokens.get(token_idx).ok_or_else(|| {
            anyhow::anyhow!("Token index {} out of range (have {})", token_idx, tokens.len())
        })?;

        info!(
            "Attempting WASP relay binding to {} ({}, rtt={}ms, token_id={}, UDP)",
            ep.addr, ep.relay_name, ep.c2r_rtt, ep.token_id
        );

        match attempt_stun_bind(ep.addr, token, relay_key).await {
            Ok((socket, mapped_addr)) => {
                info!(
                    "Relay binding succeeded: {} -> mapped={}",
                    ep.addr, mapped_addr
                );
                return Ok((socket, mapped_addr));
            }
            Err(e) => {
                warn!("Relay binding to {} failed: {:?}", ep.addr, e);
                continue;
            }
        }
    }

    Err(anyhow::anyhow!("All relay binding attempts failed"))
}

/// Send a WASP relay binding request: TURN Allocate with token + MESSAGE-INTEGRITY.
///
/// No challenge-response — relay uses short-term credential style.
/// The 16-byte relay key is used directly as the HMAC-SHA1 key.
/// We send ONLY token + MI (minimal attributes).
async fn attempt_stun_bind(
    relay_addr: SocketAddr,
    token: &[u8],
    relay_key: &[u8],
) -> Result<(UdpSocket, SocketAddr), anyhow::Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(relay_addr).await?;

    let txn_id: [u8; 12] = rand::random();

    // Build minimal request: token (0x4000) + MESSAGE-INTEGRITY (0x0008)
    let token_attr_len = token.len();
    let padded_token_len = (token_attr_len + 3) & !3;
    let token_attr_total = 4 + padded_token_len;
    let mi_attr_total = 4 + 20;

    let final_msg_len = token_attr_total + mi_attr_total;
    let mut buf = Vec::with_capacity(20 + final_msg_len);

    // STUN header: TURN Allocate (0x0003)
    buf.extend_from_slice(&0x0003u16.to_be_bytes());
    buf.extend_from_slice(&(final_msg_len as u16).to_be_bytes());
    buf.extend_from_slice(&0x2112A442u32.to_be_bytes());
    buf.extend_from_slice(&txn_id);

    // Attribute: WASP token (0x4000)
    buf.extend_from_slice(&0x4000u16.to_be_bytes());
    buf.extend_from_slice(&(token_attr_len as u16).to_be_bytes());
    buf.extend_from_slice(token);
    for _ in 0..(padded_token_len - token_attr_len) {
        buf.push(0);
    }

    // MESSAGE-INTEGRITY (0x0008) — HMAC-SHA1 with relay key (short-term, key used directly)
    {
        let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(relay_key)
            .expect("HMAC key length is valid");
        mac.update(&buf);
        let hmac_result = mac.finalize().into_bytes();

        buf.extend_from_slice(&0x0008u16.to_be_bytes());
        buf.extend_from_slice(&20u16.to_be_bytes());
        buf.extend_from_slice(&hmac_result);
    }

    debug!(
        "WASP Allocate: {} bytes to {}, token={} bytes, key={} bytes",
        buf.len(), relay_addr, token.len(), relay_key.len()
    );

    socket.send(&buf).await?;

    let mut resp_buf = [0u8; 1024];
    let resp_len = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        socket.recv(&mut resp_buf),
    ).await
    .map_err(|_| anyhow::anyhow!("WASP request timed out"))??;

    if resp_len < 20 {
        return Err(anyhow::anyhow!("WASP response too short: {} bytes", resp_len));
    }

    let resp_type = u16::from_be_bytes([resp_buf[0], resp_buf[1]]);
    let attrs = &resp_buf[20..resp_len];

    info!("WASP response: type=0x{:04x}, {} bytes", resp_type, resp_len);
    dump_stun_attrs(attrs);

    let is_success = resp_type == 0x0103 || resp_type == 0x0101 || resp_type == 0x0801;
    if is_success {
        if let Ok(addr) = parse_xor_mapped_address(attrs, &txn_id) {
            return Ok((socket, addr));
        }
        return Err(anyhow::anyhow!("Success response but no mapped address"));
    }

    let error_code = parse_error_code(attrs);
    Err(anyhow::anyhow!(
        "WASP error: type=0x{:04x}, error={}",
        resp_type,
        error_code.map(|(c, r)| format!("{} ({})", c, r)).unwrap_or("unknown".into()),
    ))
}


/// Dump all STUN attributes for debugging.
fn dump_stun_attrs(attrs: &[u8]) {
    let mut offset = 0;
    while offset + 4 <= attrs.len() {
        let attr_type = u16::from_be_bytes([attrs[offset], attrs[offset + 1]]);
        let attr_len = u16::from_be_bytes([attrs[offset + 2], attrs[offset + 3]]) as usize;
        let padded = (attr_len + 3) & !3;

        let value_preview = if offset + 4 + attr_len <= attrs.len() && attr_len > 0 {
            // Try as UTF-8 string first
            if let Ok(s) = std::str::from_utf8(&attrs[offset + 4..offset + 4 + attr_len.min(64)]) {
                format!("\"{}\"", s)
            } else {
                format!("{} bytes", attr_len)
            }
        } else {
            format!("{} bytes", attr_len)
        };

        info!("  STUN attr 0x{:04x}: len={}, value={}", attr_type, attr_len, value_preview);
        offset += 4 + padded;
    }
}

/// Parse ERROR-CODE (0x0009) attribute from a STUN error response.
/// Returns (error_code, reason_phrase).
fn parse_error_code(attrs: &[u8]) -> Option<(u32, String)> {
    let mut offset = 0;
    while offset + 4 <= attrs.len() {
        let attr_type = u16::from_be_bytes([attrs[offset], attrs[offset + 1]]);
        let attr_len = u16::from_be_bytes([attrs[offset + 2], attrs[offset + 3]]) as usize;
        let padded = (attr_len + 3) & !3;

        if attr_type == 0x0009 && attr_len >= 4 {
            // ERROR-CODE: 2 reserved bytes, 1 class byte, 1 number byte, then reason phrase
            let class = (attrs[offset + 6] & 0x07) as u32;
            let number = attrs[offset + 7] as u32;
            let code = class * 100 + number;
            let reason = if attr_len > 4 {
                String::from_utf8_lossy(&attrs[offset + 8..offset + 4 + attr_len]).to_string()
            } else {
                String::new()
            };
            return Some((code, reason));
        }

        offset += 4 + padded;
    }
    None
}

/// Parse XOR-MAPPED-ADDRESS from STUN response attributes.
fn parse_xor_mapped_address(
    attrs: &[u8],
    _txn_id: &[u8; 12],
) -> Result<SocketAddr, anyhow::Error> {
    let mut offset = 0;
    while offset + 4 <= attrs.len() {
        let attr_type = u16::from_be_bytes([attrs[offset], attrs[offset + 1]]);
        let attr_len = u16::from_be_bytes([attrs[offset + 2], attrs[offset + 3]]) as usize;
        let padded = (attr_len + 3) & !3;

        if (attr_type == 0x0020 || attr_type == 0x0016) && attr_len >= 8 {
            // XOR-MAPPED-ADDRESS (0x0020) or XOR-RELAYED-ADDRESS (0x0016)
            let family = attrs[offset + 5];
            let xport = u16::from_be_bytes([attrs[offset + 6], attrs[offset + 7]]) ^ 0x2112;

            if family == 0x01 && attr_len >= 8 {
                // IPv4
                let xip = [
                    attrs[offset + 8] ^ 0x21,
                    attrs[offset + 9] ^ 0x12,
                    attrs[offset + 10] ^ 0xA4,
                    attrs[offset + 11] ^ 0x42,
                ];
                let ip = std::net::Ipv4Addr::new(xip[0], xip[1], xip[2], xip[3]);
                return Ok(SocketAddr::new(std::net::IpAddr::V4(ip), xport));
            }
        }

        // Also check MAPPED-ADDRESS (0x0001) as fallback
        if attr_type == 0x0001 && attr_len >= 8 {
            let family = attrs[offset + 5];
            let port = u16::from_be_bytes([attrs[offset + 6], attrs[offset + 7]]);

            if family == 0x01 {
                let ip = std::net::Ipv4Addr::new(
                    attrs[offset + 8],
                    attrs[offset + 9],
                    attrs[offset + 10],
                    attrs[offset + 11],
                );
                return Ok(SocketAddr::new(std::net::IpAddr::V4(ip), port));
            }
        }

        offset += 4 + padded;
    }

    Err(anyhow::anyhow!("No MAPPED-ADDRESS found in STUN response"))
}

/// Build a minimal RTP packet with Opus silence.
///
/// RTP header (12 bytes) + Opus payload.
/// PT=60, clock rate=16000.
pub fn build_rtp_packet(
    ssrc: u32,
    seq: u16,
    timestamp: u32,
    opus_payload: &[u8],
) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(12 + opus_payload.len());
    // V=2, P=0, X=0, CC=0
    pkt.push(0x80);
    // M=0, PT=60
    pkt.push(60);
    // Sequence number
    pkt.extend_from_slice(&seq.to_be_bytes());
    // Timestamp
    pkt.extend_from_slice(&timestamp.to_be_bytes());
    // SSRC
    pkt.extend_from_slice(&ssrc.to_be_bytes());
    // Payload
    pkt.extend_from_slice(opus_payload);
    pkt
}

/// Build an RTP packet matching real WhatsApp format: PT=121, 0xDEBE extension.
///
/// The payload should already be SFrame-encrypted (ciphertext + 4-byte auth tag).
/// `ext_data` is the 0xDEBE extension content (already padded to 4-byte boundary).
pub fn build_rtp_packet_wa(
    ssrc: u32,
    seq: u16,
    timestamp: u32,
    ext_data: &[u8],
    encrypted_payload: &[u8],
) -> Vec<u8> {
    let ext_words = ext_data.len() / 4;
    let mut pkt = Vec::with_capacity(16 + ext_data.len() + encrypted_payload.len());

    // V=2, P=0, X=1 (extension present), CC=0
    pkt.push(0x90);
    // M=0, PT=121
    pkt.push(121);
    // Sequence number
    pkt.extend_from_slice(&seq.to_be_bytes());
    // Timestamp
    pkt.extend_from_slice(&timestamp.to_be_bytes());
    // SSRC
    pkt.extend_from_slice(&ssrc.to_be_bytes());
    // Extension header: profile 0xDEBE, length in 32-bit words
    pkt.extend_from_slice(&0xDEBEu16.to_be_bytes());
    pkt.extend_from_slice(&(ext_words as u16).to_be_bytes());
    // Extension data
    pkt.extend_from_slice(ext_data);
    // Payload (SFrame-encrypted audio)
    pkt.extend_from_slice(encrypted_payload);

    pkt
}

/// Opus silence/DTX frame (20ms CELT FB, TOC=0xF8).
pub const OPUS_SILENCE_60MS: &[u8] = &[0xF8, 0xFF, 0xFE];

/// Embedded TTS Opus frames (20ms each, CELT FB 16kHz content, 24kbps).
/// Binary format: [u16 LE frame_count][u16 LE len][frame bytes]...
static SINE_TONE_DATA: &[u8] = include_bytes!("sine_440hz_opus.bin");

/// Parse the embedded sine tone data and return individual Opus frames.
/// Loops ~1 second of 440Hz tone when cycled.
pub fn sine_tone_frames() -> Vec<&'static [u8]> {
    let count = u16::from_le_bytes([SINE_TONE_DATA[0], SINE_TONE_DATA[1]]) as usize;
    let mut frames = Vec::with_capacity(count);
    let mut offset = 2;
    for _ in 0..count {
        let len = u16::from_le_bytes([SINE_TONE_DATA[offset], SINE_TONE_DATA[offset + 1]]) as usize;
        offset += 2;
        frames.push(&SINE_TONE_DATA[offset..offset + len]);
        offset += len;
    }
    frames
}

// === SFrame AES-128-GCM (WhatsApp E2E media frame encryption) ===
//
// The 46-byte HKDF output from expandCallKey is split as:
//   bytes 0-15:  SRTP master key
//   bytes 16-29: SRTP master salt
//   bytes 30-45: SFrame AES-128-GCM key (16 bytes)
//
// After SRTP E2E decryption, the payload is:
//   [SFrame header (1+ bytes)] [GCM ciphertext] [GCM tag (variable)]
//
// SFrame header (RFC 9605 §4.3):
//   bit 7:   X (extended flag — if 1, more header bytes follow)
//   bits 4-6: KID (key ID, 0-7)
//   bits 0-3: CTR (counter, 0-15) or CTR length (if X=1)

/// Derive SFrame encryption key and salt from a base key per RFC 9605 §4.3.1.
/// base_key (16 bytes) → HKDF-Extract → HKDF-Expand with labels → (key, salt).
fn derive_sframe_key_salt(base_key: &[u8; 16]) -> ([u8; 16], [u8; 12]) {
    // Step 1: HKDF-Extract(salt="", IKM=base_key)
    let hk = Hkdf::<Sha256>::new(None, base_key);

    // Step 2: Expand for key and salt with RFC 9605 labels
    let mut key = [0u8; 16];
    let mut salt = [0u8; 12];
    hk.expand(b"SFrame 1.0 Secret key\x00", &mut key)
        .expect("HKDF expand for SFrame key");
    hk.expand(b"SFrame 1.0 Secret salt\x00", &mut salt)
        .expect("HKDF expand for SFrame salt");

    (key, salt)
}

/// Try to decrypt an SFrame-encrypted payload using AES-128-GCM.
/// `sframe_base_key`: 16-byte base key from HKDF bytes 30-45.
/// `payload`: data after SRTP decryption (SFrame header + ciphertext + tag).
/// `rtp_header`: RTP header bytes used as AAD.
/// Returns decrypted Opus frame on success.
pub fn sframe_gcm_decrypt(
    sframe_base_key: &[u8; 16],
    payload: &[u8],
    rtp_header: &[u8],
) -> Option<Vec<u8>> {
    if payload.is_empty() {
        return None;
    }

    let first = payload[0];
    let extended = (first & 0x80) != 0;

    let (counter, header_len) = if !extended {
        ((first & 0x0F) as u64, 1)
    } else {
        let ctr_len = (first & 0x0F) as usize;
        if payload.len() < 1 + ctr_len {
            return None;
        }
        let mut ctr: u64 = 0;
        for i in 0..ctr_len {
            ctr = (ctr << 8) | (payload[1 + i] as u64);
        }
        (ctr, 1 + ctr_len)
    };

    let sframe_data = &payload[header_len..];
    let aad_header = &payload[..header_len];

    // Try both RFC 9605 derived key and raw base key
    let (derived_key, derived_salt) = derive_sframe_key_salt(sframe_base_key);

    // Two key variants: RFC 9605 derived, and raw base key
    let key_variants: [(&[u8; 16], &str); 2] = [
        (&derived_key, "rfc9605"),
        (sframe_base_key, "raw"),
    ];

    for (gcm_key, _label) in &key_variants {
        let cipher = <Aes128Gcm as aes_gcm::KeyInit>::new(GenericArray::from_slice(*gcm_key));

        // Two nonce variants: salt XOR counter (RFC 9605), and plain counter
        let mut nonce_rfc = [0u8; 12];
        let ctr_be = counter.to_be_bytes();
        // Pad counter to 12 bytes, then XOR with salt
        let mut ctr_padded = [0u8; 12];
        ctr_padded[4..12].copy_from_slice(&ctr_be);
        for i in 0..12 {
            nonce_rfc[i] = derived_salt[i] ^ ctr_padded[i];
        }
        let nonce_plain = ctr_padded; // No salt XOR

        let nonces: [&[u8; 12]; 2] = [&nonce_rfc, &nonce_plain];

        for nonce_bytes in &nonces {
            let nonce = Nonce::from_slice(*nonce_bytes);

            // Try GCM tag sizes: 16, 8, 4 bytes
            for tag_len in [16usize, 8, 4] {
                if sframe_data.len() < tag_len {
                    continue;
                }
                let ct_len = sframe_data.len() - tag_len;
                let ciphertext = &sframe_data[..ct_len];
                let tag_bytes = &sframe_data[ct_len..];

                let mut ct_with_tag = Vec::with_capacity(ciphertext.len() + 16);
                ct_with_tag.extend_from_slice(ciphertext);
                ct_with_tag.extend_from_slice(tag_bytes);
                if tag_len < 16 {
                    ct_with_tag.resize(ciphertext.len() + 16, 0);
                }

                // AAD variants: SFrame header, empty, RTP header
                for aad in [aad_header, &[] as &[u8], rtp_header] {
                    if let Ok(pt) = cipher.decrypt(nonce, aes_gcm::aead::Payload { msg: &ct_with_tag, aad }) {
                        return Some(pt);
                    }
                }
            }
        }
    }

    None
}

/// SFrame-encrypt an Opus frame with AES-128-GCM.
/// Uses raw base key directly (not RFC 9605 HKDF-derived).
/// Nonce: 12-byte counter (big-endian padded). AAD: SFrame header bytes.
/// Returns: [SFrame header] [ciphertext] [16-byte GCM tag]
pub fn sframe_gcm_encrypt(
    sframe_base_key: &[u8; 16],
    counter: u64,
    opus_frame: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();

    // SFrame header encoding (RFC 9605 §4.3)
    let (header_byte, extra_ctr) = if counter <= 15 {
        (counter as u8, vec![])
    } else if counter <= 0xFF {
        (0x81u8, vec![counter as u8])
    } else if counter <= 0xFFFF {
        (0x82u8, vec![(counter >> 8) as u8, counter as u8])
    } else {
        let ctr_bytes = counter.to_be_bytes();
        let start = ctr_bytes.iter().position(|&b| b != 0).unwrap_or(7);
        let len = 8 - start;
        (0x80 | (len as u8), ctr_bytes[start..].to_vec())
    };

    out.push(header_byte);
    out.extend_from_slice(&extra_ctr);
    let header_len = out.len();

    // Nonce: counter as 12-byte big-endian (no salt XOR — raw key mode)
    let mut nonce_bytes = [0u8; 12];
    let ctr_be = counter.to_be_bytes();
    nonce_bytes[4..12].copy_from_slice(&ctr_be);

    // Use raw base key directly as AES-128-GCM key
    let cipher = <Aes128Gcm as aes_gcm::KeyInit>::new(GenericArray::from_slice(sframe_base_key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    // AAD: SFrame header bytes
    let aad = &out[..header_len];
    let encrypted = cipher
        .encrypt(nonce, aes_gcm::aead::Payload { msg: opus_frame, aad })
        .expect("AES-GCM encrypt should not fail");

    out.extend_from_slice(&encrypted);
    out
}
