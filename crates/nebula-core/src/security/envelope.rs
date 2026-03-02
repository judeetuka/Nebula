use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce as GcmNonce};
use anyhow::{bail, Result};
use hmac::Mac;
use sha2::Sha256;

use super::keys::{AesKey, HmacKey};

type HmacSha256 = hmac::Hmac<Sha256>;

const HMAC_TAG_LEN: usize = 32;
const GCM_NONCE_LEN: usize = 12;

/// Two-layer security envelope for DoS resistance + authenticated encryption.
///
/// Wire format:
/// ```text
/// [HMAC-SHA256 tag (32 bytes)][AES-GCM nonce (12 bytes)][ciphertext + GCM auth tag (16 bytes)]
/// ```
///
/// The HMAC outer layer allows fast rejection of invalid packets without
/// paying the cost of AES-GCM decryption — key for DoS resistance.
pub struct SecurityEnvelope;

impl SecurityEnvelope {
    /// Seal plaintext with both layers: encrypt with AES-GCM, then HMAC the result.
    pub fn seal(
        plaintext: &[u8],
        hmac_key: &HmacKey,
        aes_key: &AesKey,
    ) -> Result<Vec<u8>> {
        // Generate random nonce for AES-GCM
        let nonce_bytes: [u8; GCM_NONCE_LEN] = rand::random();
        let nonce = GcmNonce::from_slice(&nonce_bytes);

        // Inner layer: AES-256-GCM encryption
        let cipher = Aes256Gcm::new_from_slice(&aes_key.0)
            .map_err(|e| anyhow::anyhow!("AES key init: {}", e))?;
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("AES-GCM encrypt: {}", e))?;

        // Build inner payload: [nonce][ciphertext+tag]
        let mut inner = Vec::with_capacity(GCM_NONCE_LEN + ciphertext.len());
        inner.extend_from_slice(&nonce_bytes);
        inner.extend_from_slice(&ciphertext);

        // Outer layer: HMAC-SHA256 over the inner payload
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&hmac_key.0)
            .map_err(|e| anyhow::anyhow!("HMAC key init: {}", e))?;
        Mac::update(&mut mac, &inner);
        let tag = mac.finalize().into_bytes();

        // Final: [HMAC tag][nonce][ciphertext+tag]
        let mut sealed = Vec::with_capacity(HMAC_TAG_LEN + inner.len());
        sealed.extend_from_slice(&tag);
        sealed.extend_from_slice(&inner);

        Ok(sealed)
    }

    /// Verify HMAC first (fast rejection), then decrypt AES-GCM.
    pub fn open(
        sealed: &[u8],
        hmac_key: &HmacKey,
        aes_key: &AesKey,
    ) -> Result<Vec<u8>> {
        if sealed.len() < HMAC_TAG_LEN + GCM_NONCE_LEN + 16 {
            bail!("sealed data too short");
        }

        let (tag_bytes, inner) = sealed.split_at(HMAC_TAG_LEN);

        // Outer layer: verify HMAC (fast rejection)
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&hmac_key.0)
            .map_err(|e| anyhow::anyhow!("HMAC key init: {}", e))?;
        Mac::update(&mut mac, inner);
        mac.verify_slice(tag_bytes)
            .map_err(|_| anyhow::anyhow!("HMAC verification failed"))?;

        // Inner layer: decrypt AES-GCM
        let (nonce_bytes, ciphertext) = inner.split_at(GCM_NONCE_LEN);
        let nonce = GcmNonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&aes_key.0)
            .map_err(|e| anyhow::anyhow!("AES key init: {}", e))?;
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("AES-GCM decrypt: {}", e))?;

        Ok(plaintext)
    }

    /// Fast HMAC-only check for DoS resistance. Does not decrypt.
    pub fn verify_hmac(sealed: &[u8], hmac_key: &HmacKey) -> bool {
        if sealed.len() < HMAC_TAG_LEN + GCM_NONCE_LEN + 16 {
            return false;
        }

        let (tag_bytes, inner) = sealed.split_at(HMAC_TAG_LEN);

        let Ok(mut mac) = <HmacSha256 as Mac>::new_from_slice(&hmac_key.0) else {
            return false;
        };
        Mac::update(&mut mac, inner);
        mac.verify_slice(tag_bytes).is_ok()
    }
}
