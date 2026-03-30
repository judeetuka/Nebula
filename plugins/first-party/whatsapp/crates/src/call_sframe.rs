//! WhatsApp SFrame E2E encryption for voice calls.
//!
//! Uses SFrame-like encryption with AES-128-CTR + HMAC-SHA256-32 (4-byte auth tag).
//! This matches cipher suite 0x0005 (AEAD_AES_128_CTR_HMAC_SHA256_32) from RFC 9605.
//!
//! Also supports AES-128-GCM with truncated 4-byte tag as an alternative.
//!
//! PCap evidence shows:
//! - No HBH SRTP layer — packets are SFrame-only between phone and relay
//! - 4-byte auth tag (minimum 6-byte payloads = 2B Opus DTX + 4B tag)
//! - SFrame metadata in 0xDEBE RTP extension (IDs 3 and 13), NOT in payload
//! - PT=121 for audio, extension always present (X=1)

use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use aes_gcm::aead::Aead;
use aes_gcm::{Aes128Gcm, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use log::debug;
use sha2::Sha256;

/// Auth tag length: 4 bytes (HMAC-SHA256 truncated to 32 bits).
pub const AUTH_TAG_LEN: usize = 4;

/// SFrame cipher suite identifier for key derivation labels.
const CIPHER_SUITE: u16 = 0x0005; // AEAD_AES_128_CTR_HMAC_SHA256_32

/// SFrame encryption/decryption context for one direction.
pub struct SFrameContext {
    /// AES-128-CTR encryption key (16 bytes).
    pub enc_key: [u8; 16],
    /// HMAC-SHA256 authentication key (32 bytes).
    pub auth_key: [u8; 32],
    /// Nonce salt (12 bytes), XOR'd with counter for per-packet nonce.
    pub salt: [u8; 12],
}

impl SFrameContext {
    /// Create SFrame context using standard RFC 9605 §4.3 key derivation.
    ///
    /// `base_key` = 32-byte CallKey, `kid` = key ID (typically 0).
    pub fn new(base_key: &[u8], kid: u64) -> Self {
        let hk = Hkdf::<Sha256>::new(None, base_key);
        let kid_bytes = kid.to_be_bytes();
        let cs_bytes = CIPHER_SUITE.to_be_bytes();

        // Derive key material: 48 bytes = 16 enc_key + 32 auth_key
        let mut key_info = Vec::with_capacity(34);
        key_info.extend_from_slice(b"SFrame 1.0 Secret key ");
        key_info.extend_from_slice(&kid_bytes);
        key_info.extend_from_slice(&cs_bytes);
        let mut key_material = [0u8; 48];
        hk.expand(&key_info, &mut key_material)
            .expect("HKDF expand for SFrame key");

        let mut enc_key = [0u8; 16];
        let mut auth_key = [0u8; 32];
        enc_key.copy_from_slice(&key_material[..16]);
        auth_key.copy_from_slice(&key_material[16..48]);

        // Derive salt: 12 bytes
        let mut salt_info = Vec::with_capacity(34);
        salt_info.extend_from_slice(b"SFrame 1.0 Secret salt ");
        salt_info.extend_from_slice(&kid_bytes);
        salt_info.extend_from_slice(&cs_bytes);
        let mut salt = [0u8; 12];
        hk.expand(&salt_info, &mut salt)
            .expect("HKDF expand for SFrame salt");

        debug!(
            "SFrame ctx (RFC9605 suite=0x{:04x}): kid={}, enc={}, salt={}",
            CIPHER_SUITE,
            kid,
            hex::encode(&enc_key),
            hex::encode(&salt)
        );

        Self {
            enc_key,
            auth_key,
            salt,
        }
    }

    /// Create SFrame context using SRTP-style HKDF derivation (call_key + JID as info).
    ///
    /// Derives 60 bytes: 16 enc + 32 auth + 12 salt.
    pub fn from_srtp_hkdf(call_key: &[u8], jid: &str) -> Self {
        let hk = Hkdf::<Sha256>::new(None, call_key);
        let mut okm = [0u8; 60];
        hk.expand(jid.as_bytes(), &mut okm)
            .expect("HKDF expand for SFrame SRTP-style");

        let mut enc_key = [0u8; 16];
        let mut auth_key = [0u8; 32];
        let mut salt = [0u8; 12];
        enc_key.copy_from_slice(&okm[..16]);
        auth_key.copy_from_slice(&okm[16..48]);
        salt.copy_from_slice(&okm[48..60]);

        debug!(
            "SFrame ctx (SRTP-HKDF): jid={}, enc={}, salt={}",
            jid,
            hex::encode(&enc_key),
            hex::encode(&salt)
        );

        Self {
            enc_key,
            auth_key,
            salt,
        }
    }

    fn make_nonce(&self, ctr: u64) -> [u8; 12] {
        let mut nonce = self.salt;
        let ctr_bytes = ctr.to_be_bytes();
        // XOR counter into last 8 bytes of 12-byte salt
        for i in 0..8 {
            nonce[4 + i] ^= ctr_bytes[i];
        }
        nonce
    }

    /// Encrypt plaintext → ciphertext || 4-byte auth tag.
    pub fn encrypt(&self, ctr: u64, aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let nonce = self.make_nonce(ctr);

        // AES-CTR encrypt
        let mut ciphertext = plaintext.to_vec();
        aes_ctr_crypt(&self.enc_key, &nonce, &mut ciphertext);

        // HMAC-SHA256 auth tag over (AAD || ciphertext), truncated to 4 bytes
        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(&self.auth_key).expect("HMAC key valid");
        mac.update(aad);
        mac.update(&ciphertext);
        let tag = mac.finalize().into_bytes();

        ciphertext.extend_from_slice(&tag[..AUTH_TAG_LEN]);
        ciphertext
    }

    /// Decrypt (ciphertext || 4-byte auth tag) → plaintext.
    pub fn decrypt(&self, ctr: u64, aad: &[u8], data: &[u8]) -> Result<Vec<u8>, &'static str> {
        if data.len() < AUTH_TAG_LEN {
            return Err("SFrame data too short for auth tag");
        }

        let ciphertext = &data[..data.len() - AUTH_TAG_LEN];
        let received_tag = &data[data.len() - AUTH_TAG_LEN..];

        // Verify HMAC-SHA256 truncated to 4 bytes
        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(&self.auth_key).expect("HMAC key valid");
        mac.update(aad);
        mac.update(ciphertext);
        let computed = mac.finalize().into_bytes();

        if &computed[..AUTH_TAG_LEN] != received_tag {
            return Err("SFrame HMAC mismatch");
        }

        // AES-CTR decrypt
        let nonce = self.make_nonce(ctr);
        let mut plaintext = ciphertext.to_vec();
        aes_ctr_crypt(&self.enc_key, &nonce, &mut plaintext);

        Ok(plaintext)
    }

    /// Create SFrame context from WhatsApp-style HKDF with descriptive info string.
    ///
    /// WhatsApp uses `"WhatsApp <Type> Keys"` pattern for media key expansion.
    /// For calls, it might be `"WhatsApp Call Keys"` or `"WhatsApp Voice Keys"`.
    pub fn from_wa_label(call_key: &[u8], label: &str) -> Self {
        let hk = Hkdf::<Sha256>::new(None, call_key);
        let mut okm = [0u8; 60]; // 16 enc + 32 auth + 12 salt
        hk.expand(label.as_bytes(), &mut okm)
            .expect("HKDF expand for WA-label SFrame");

        let mut enc_key = [0u8; 16];
        let mut auth_key = [0u8; 32];
        let mut salt = [0u8; 12];
        enc_key.copy_from_slice(&okm[..16]);
        auth_key.copy_from_slice(&okm[16..48]);
        salt.copy_from_slice(&okm[48..60]);

        debug!(
            "SFrame ctx (WA-label): label={}, enc={}, salt={}",
            label,
            hex::encode(&enc_key),
            hex::encode(&salt)
        );

        Self {
            enc_key,
            auth_key,
            salt,
        }
    }

    /// Create SFrame context using SRTP session keys (k_e as enc, k_a as auth, k_s as salt).
    /// This tests the theory that the payload is encrypted with SRTP session keys
    /// but without SRTP packet framing (just raw payload encryption).
    pub fn from_srtp_session_keys(srtp_keys: &super::call_media::SrtpKeys) -> Self {
        let session = super::call_media::derive_session_keys_pub(
            &srtp_keys.master_key,
            &srtp_keys.master_salt,
        );
        let mut salt = [0u8; 12];
        salt.copy_from_slice(&session.session_salt[..12]);

        debug!(
            "SFrame ctx (SRTP-session): enc={}, salt={}",
            hex::encode(&session.cipher_key),
            hex::encode(&salt)
        );

        Self {
            enc_key: session.cipher_key,
            // Use auth_key from SRTP (20 bytes HMAC-SHA1 key, zero-padded to 32)
            auth_key: {
                let mut ak = [0u8; 32];
                ak[..20].copy_from_slice(&session.auth_key);
                ak
            },
            salt,
        }
    }

    /// Create SFrame context from call_id-based HKDF (Frida trace from Schirrmacher research).
    ///
    /// The 2020 Frida trace shows WhatsApp derives 4-byte values using:
    ///   HKDF(ikm=call_id_hex_ascii, salt=LE32(purpose), info=jid, length=4)
    /// with purpose 0=SSRC, 1=unknown, 4=unknown.
    /// For SFrame, the key material might use a similar pattern but with larger output.
    pub fn from_call_id_hkdf(call_id: &str, jid: &str, salt_purpose: u32) -> Self {
        let salt = salt_purpose.to_le_bytes();
        let hk = Hkdf::<Sha256>::new(Some(&salt), call_id.as_bytes());
        let mut okm = [0u8; 60]; // 16 enc + 32 auth + 12 salt
        hk.expand(jid.as_bytes(), &mut okm)
            .expect("HKDF expand for call_id-based SFrame");

        let mut enc_key = [0u8; 16];
        let mut auth_key = [0u8; 32];
        let mut nonce_salt = [0u8; 12];
        enc_key.copy_from_slice(&okm[..16]);
        auth_key.copy_from_slice(&okm[16..48]);
        nonce_salt.copy_from_slice(&okm[48..60]);

        debug!(
            "SFrame ctx (callid-HKDF): purpose={}, jid={}, enc={}, salt={}",
            salt_purpose,
            jid,
            hex::encode(&enc_key),
            hex::encode(&nonce_salt)
        );

        Self {
            enc_key,
            auth_key,
            salt: nonce_salt,
        }
    }

    /// Create SFrame context directly from raw bytes (no HKDF).
    /// `key` first 16 bytes = enc, bytes 16..32 or remaining = auth (zero-padded).
    /// `salt_bytes` = 12-byte nonce salt.
    pub fn from_raw(key_bytes: &[u8], salt_bytes: &[u8; 12]) -> Self {
        let mut enc_key = [0u8; 16];
        let mut auth_key = [0u8; 32];
        let copy_enc = key_bytes.len().min(16);
        enc_key[..copy_enc].copy_from_slice(&key_bytes[..copy_enc]);
        if key_bytes.len() > 16 {
            let copy_auth = (key_bytes.len() - 16).min(32);
            auth_key[..copy_auth].copy_from_slice(&key_bytes[16..16 + copy_auth]);
        }

        debug!(
            "SFrame ctx (raw): enc={}, salt={}",
            hex::encode(&enc_key),
            hex::encode(salt_bytes)
        );

        Self {
            enc_key,
            auth_key,
            salt: *salt_bytes,
        }
    }
}

/// AES-128-GCM context with truncated 4-byte tag.
/// Some SFrame implementations use GCM with short tags instead of CTR+HMAC.
pub struct GcmSFrameContext {
    pub key: [u8; 16],
    pub salt: [u8; 12],
}

impl GcmSFrameContext {
    /// Create from RFC 9605 HKDF with AES-128-GCM cipher suite (0x0001).
    pub fn new_rfc9605(base_key: &[u8], kid: u64) -> Self {
        let hk = Hkdf::<Sha256>::new(None, base_key);
        let kid_bytes = kid.to_be_bytes();
        let cs_bytes = 0x0001u16.to_be_bytes(); // AEAD_AES_128_GCM

        let mut key_info = Vec::with_capacity(34);
        key_info.extend_from_slice(b"SFrame 1.0 Secret key ");
        key_info.extend_from_slice(&kid_bytes);
        key_info.extend_from_slice(&cs_bytes);
        let mut key = [0u8; 16];
        hk.expand(&key_info, &mut key).expect("HKDF expand");

        let mut salt_info = Vec::with_capacity(34);
        salt_info.extend_from_slice(b"SFrame 1.0 Secret salt ");
        salt_info.extend_from_slice(&kid_bytes);
        salt_info.extend_from_slice(&cs_bytes);
        let mut salt = [0u8; 12];
        hk.expand(&salt_info, &mut salt).expect("HKDF expand");

        debug!(
            "GCM-SFrame ctx (RFC9605 GCM): kid={}, key={}, salt={}",
            kid,
            hex::encode(&key),
            hex::encode(&salt)
        );

        Self { key, salt }
    }

    /// Create from call_id-based HKDF with purpose salt (matching Frida trace pattern).
    pub fn from_call_id_hkdf(call_id: &str, jid: &str, salt_purpose: u32) -> Self {
        let salt = salt_purpose.to_le_bytes();
        let hk = Hkdf::<Sha256>::new(Some(&salt), call_id.as_bytes());
        let mut okm = [0u8; 28]; // 16 key + 12 salt
        hk.expand(jid.as_bytes(), &mut okm).expect("HKDF expand");

        let mut key = [0u8; 16];
        let mut nonce_salt = [0u8; 12];
        key.copy_from_slice(&okm[..16]);
        nonce_salt.copy_from_slice(&okm[16..28]);

        debug!(
            "GCM-SFrame ctx (callid-HKDF): purpose={}, jid={}, key={}, salt={}",
            salt_purpose,
            jid,
            hex::encode(&key),
            hex::encode(&nonce_salt)
        );

        Self {
            key,
            salt: nonce_salt,
        }
    }

    /// Create from raw key bytes (first 16 = key) and explicit salt.
    pub fn from_raw(key_bytes: &[u8], salt_bytes: &[u8; 12]) -> Self {
        let mut key = [0u8; 16];
        let n = key_bytes.len().min(16);
        key[..n].copy_from_slice(&key_bytes[..n]);

        debug!(
            "GCM-SFrame ctx (raw): key={}, salt={}",
            hex::encode(&key),
            hex::encode(salt_bytes)
        );

        Self {
            key,
            salt: *salt_bytes,
        }
    }

    /// Create from SRTP-style HKDF (call_key + JID as info). Derives 28 bytes: 16 key + 12 salt.
    pub fn from_srtp_hkdf(call_key: &[u8], jid: &str) -> Self {
        let hk = Hkdf::<Sha256>::new(None, call_key);
        let mut okm = [0u8; 28];
        hk.expand(jid.as_bytes(), &mut okm).expect("HKDF expand");

        let mut key = [0u8; 16];
        let mut salt = [0u8; 12];
        key.copy_from_slice(&okm[..16]);
        salt.copy_from_slice(&okm[16..28]);

        debug!(
            "GCM-SFrame ctx (SRTP-HKDF): jid={}, key={}, salt={}",
            jid,
            hex::encode(&key),
            hex::encode(&salt)
        );

        Self { key, salt }
    }

    fn make_nonce(&self, ctr: u64) -> [u8; 12] {
        let mut nonce = self.salt;
        let ctr_bytes = ctr.to_be_bytes();
        for i in 0..8 {
            nonce[4 + i] ^= ctr_bytes[i];
        }
        nonce
    }

    /// Decrypt payload (ciphertext + 4-byte GCM tag) using AES-128-GCM.
    /// GCM normally uses 16-byte tags, so we reconstruct by zero-padding to 16.
    /// Also tries the full payload as a standard 16-byte-tag GCM decrypt.
    pub fn decrypt(&self, ctr: u64, aad: &[u8], data: &[u8]) -> Result<Vec<u8>, &'static str> {
        let nonce_bytes = self.make_nonce(ctr);
        let cipher =
            Aes128Gcm::new_from_slice(&self.key).map_err(|_| "GCM key init failed")?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Try 1: Treat last 4 bytes as a truncated GCM tag → pad to 16 for the library
        // (This is non-standard but some implementations support it)
        if data.len() >= 4 {
            let ct = &data[..data.len() - 4];
            let short_tag = &data[data.len() - 4..];
            // Build full GCM ciphertext with 16-byte tag (pad with zeros)
            let mut full_ct = ct.to_vec();
            full_ct.extend_from_slice(short_tag);
            full_ct.extend_from_slice(&[0u8; 12]); // pad to 16-byte tag

            let payload = aes_gcm::aead::Payload {
                msg: &full_ct,
                aad,
            };
            if let Ok(pt) = cipher.decrypt(nonce, payload) {
                return Ok(pt);
            }
        }

        // Try 2: Maybe the full payload IS GCM ciphertext with 16-byte tag
        if data.len() >= 16 {
            let payload = aes_gcm::aead::Payload { msg: data, aad };
            if let Ok(pt) = cipher.decrypt(nonce, payload) {
                return Ok(pt);
            }
        }

        Err("GCM decrypt failed")
    }
}

/// AES-128-CTR encryption/decryption (symmetric — same operation for both).
fn aes_ctr_crypt(key: &[u8; 16], nonce: &[u8; 12], data: &mut [u8]) {
    let aes = Aes128::new(GenericArray::from_slice(key));
    let blocks_needed = (data.len() + 15) / 16;
    let mut offset = 0;

    for counter in 0..blocks_needed {
        // IV = nonce (12B) || counter (4B big-endian)
        let mut block = [0u8; 16];
        block[..12].copy_from_slice(nonce);
        let c = counter as u32;
        block[12] = (c >> 24) as u8;
        block[13] = (c >> 16) as u8;
        block[14] = (c >> 8) as u8;
        block[15] = c as u8;

        let mut ga = GenericArray::clone_from_slice(&block);
        aes.encrypt_block(&mut ga);

        let remaining = data.len() - offset;
        let to_xor = remaining.min(16);
        for i in 0..to_xor {
            data[offset + i] ^= ga[i];
        }
        offset += to_xor;
    }
}

/// Build 0xDEBE RTP extension data using one-byte header format (RFC 8285).
///
/// Extension elements observed in real WhatsApp traffic:
/// - ID=3 (1 byte, value 0x01): Present on DTX/silence frames
/// - ID=13 (4 bytes): Periodic counter, sent every ~9 packets
pub fn build_debe_extension(is_dtx: bool, periodic_counter: Option<u32>) -> Vec<u8> {
    let mut data = Vec::new();

    if is_dtx {
        // ID=3, L=0 (L+1=1 byte of data per RFC 8285 one-byte header)
        data.push(0x30);
        data.push(0x01);
    }

    if let Some(counter) = periodic_counter {
        // ID=13 (0xD), L=3 (L+1=4 bytes of data)
        data.push(0xD3);
        data.extend_from_slice(&counter.to_be_bytes());
    }

    // Pad to 4-byte boundary with zeros
    while data.len() % 4 != 0 {
        data.push(0x00);
    }

    data
}
