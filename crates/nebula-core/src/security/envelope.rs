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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::keys::KeyPair;

    #[test]
    fn test_seal_open_roundtrip() {
        let keys = KeyPair::derive_from_secret(b"test-secret").unwrap();
        let plaintext = b"Hello, NEBULA!";
        let sealed = SecurityEnvelope::seal(plaintext, &keys.hmac, &keys.aes).unwrap();
        let opened = SecurityEnvelope::open(&sealed, &keys.hmac, &keys.aes).unwrap();
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn test_hmac_fast_rejection() {
        let keys = KeyPair::derive_from_secret(b"test-secret").unwrap();
        let plaintext = b"test data";
        let mut sealed = SecurityEnvelope::seal(plaintext, &keys.hmac, &keys.aes).unwrap();
        // Tamper with the ciphertext (after HMAC tag)
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        assert!(!SecurityEnvelope::verify_hmac(&sealed, &keys.hmac));
    }

    #[test]
    fn test_wrong_key_fails() {
        let keys1 = KeyPair::derive_from_secret(b"secret-1").unwrap();
        let keys2 = KeyPair::derive_from_secret(b"secret-2").unwrap();
        let sealed = SecurityEnvelope::seal(b"data", &keys1.hmac, &keys1.aes).unwrap();
        assert!(SecurityEnvelope::open(&sealed, &keys2.hmac, &keys2.aes).is_err());
    }

    #[test]
    fn test_key_derivation_deterministic() {
        let k1 = KeyPair::derive_from_secret(b"same").unwrap();
        let k2 = KeyPair::derive_from_secret(b"same").unwrap();
        assert_eq!(k1.hmac.0, k2.hmac.0);
        assert_eq!(k1.aes.0, k2.aes.0);
    }

    #[test]
    fn test_sealed_too_short() {
        let keys = KeyPair::derive_from_secret(b"x").unwrap();
        assert!(SecurityEnvelope::open(&[0u8; 10], &keys.hmac, &keys.aes).is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let keys = KeyPair::derive_from_secret(b"x").unwrap();
        let sealed = SecurityEnvelope::seal(b"", &keys.hmac, &keys.aes).unwrap();
        let opened = SecurityEnvelope::open(&sealed, &keys.hmac, &keys.aes).unwrap();
        assert_eq!(opened, b"");
    }

    #[test]
    fn test_large_plaintext() {
        let keys = KeyPair::derive_from_secret(b"x").unwrap();
        let large = vec![0xABu8; 100_000];
        let sealed = SecurityEnvelope::seal(&large, &keys.hmac, &keys.aes).unwrap();
        let opened = SecurityEnvelope::open(&sealed, &keys.hmac, &keys.aes).unwrap();
        assert_eq!(opened, large);
    }

    #[test]
    fn test_verify_hmac_valid() {
        let keys = KeyPair::derive_from_secret(b"x").unwrap();
        let sealed = SecurityEnvelope::seal(b"data", &keys.hmac, &keys.aes).unwrap();
        assert!(SecurityEnvelope::verify_hmac(&sealed, &keys.hmac));
    }

    #[test]
    fn test_different_plaintexts_different_ciphertexts() {
        let keys = KeyPair::derive_from_secret(b"x").unwrap();
        let s1 = SecurityEnvelope::seal(b"aaa", &keys.hmac, &keys.aes).unwrap();
        let s2 = SecurityEnvelope::seal(b"bbb", &keys.hmac, &keys.aes).unwrap();
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_same_plaintext_different_nonces() {
        let keys = KeyPair::derive_from_secret(b"x").unwrap();
        let s1 = SecurityEnvelope::seal(b"same", &keys.hmac, &keys.aes).unwrap();
        let s2 = SecurityEnvelope::seal(b"same", &keys.hmac, &keys.aes).unwrap();
        // Different random nonces -> different ciphertext
        assert_ne!(s1, s2);
        // But both decrypt to the same plaintext
        let p1 = SecurityEnvelope::open(&s1, &keys.hmac, &keys.aes).unwrap();
        let p2 = SecurityEnvelope::open(&s2, &keys.hmac, &keys.aes).unwrap();
        assert_eq!(p1, p2);
    }
}
