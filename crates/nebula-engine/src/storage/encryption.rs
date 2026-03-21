use anyhow::Result;
use nebula_core::security::envelope::SecurityEnvelope;
use nebula_core::security::keys::KeyPair;

/// Encryption wrapper for storage operations.
///
/// Uses nebula-core's `SecurityEnvelope` (AES-256-GCM + HMAC-SHA256) to
/// encrypt and decrypt data at rest. Keys are derived from a master secret
/// via HKDF-SHA256.
pub struct StorageEncryption {
    keys: KeyPair,
}

impl StorageEncryption {
    /// Create a new encryption context from a master secret.
    ///
    /// The secret is fed through HKDF-SHA256 to derive independent HMAC and
    /// AES keys used by `SecurityEnvelope`.
    pub fn new(secret: &[u8]) -> Result<Self> {
        let keys = KeyPair::derive_from_secret(secret)?;
        Ok(Self { keys })
    }

    /// Encrypt raw bytes, returning a sealed envelope.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        SecurityEnvelope::seal(plaintext, &self.keys.hmac, &self.keys.aes)
    }

    /// Decrypt a sealed envelope back to raw bytes.
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        SecurityEnvelope::open(ciphertext, &self.keys.hmac, &self.keys.aes)
    }

    /// Convenience: encrypt a UTF-8 string.
    pub fn encrypt_string(&self, plaintext: &str) -> Result<Vec<u8>> {
        self.encrypt(plaintext.as_bytes())
    }

    /// Convenience: decrypt a sealed envelope and interpret as UTF-8.
    pub fn decrypt_to_string(&self, ciphertext: &[u8]) -> Result<String> {
        let bytes = self.decrypt(ciphertext)?;
        String::from_utf8(bytes)
            .map_err(|e| anyhow::anyhow!("decrypted data is not valid UTF-8: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let enc = StorageEncryption::new(b"test-secret-key").unwrap();
        let plaintext = b"Hello, NEBULA storage!";
        let ciphertext = enc.encrypt(plaintext).unwrap();
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_string_roundtrip() {
        let enc = StorageEncryption::new(b"my-secret").unwrap();
        let plaintext = "sensitive configuration data";
        let ciphertext = enc.encrypt_string(plaintext).unwrap();
        let decrypted = enc.decrypt_to_string(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_key_fails_decryption() {
        let enc1 = StorageEncryption::new(b"key-alpha").unwrap();
        let enc2 = StorageEncryption::new(b"key-beta").unwrap();
        let ciphertext = enc1.encrypt(b"secret data").unwrap();
        let result = enc2.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let enc = StorageEncryption::new(b"key").unwrap();
        let ciphertext = enc.encrypt(b"").unwrap();
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, b"");
    }

    #[test]
    fn test_large_plaintext() {
        let enc = StorageEncryption::new(b"key").unwrap();
        let plaintext = vec![0xABu8; 100_000];
        let ciphertext = enc.encrypt(&plaintext).unwrap();
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_encryptions_produce_different_ciphertexts() {
        let enc = StorageEncryption::new(b"key").unwrap();
        let c1 = enc.encrypt(b"same data").unwrap();
        let c2 = enc.encrypt(b"same data").unwrap();
        // Random nonce means different ciphertext each time
        assert_ne!(c1, c2);
        // But both decrypt to the same plaintext
        assert_eq!(enc.decrypt(&c1).unwrap(), enc.decrypt(&c2).unwrap());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let enc = StorageEncryption::new(b"key").unwrap();
        let mut ciphertext = enc.encrypt(b"important data").unwrap();
        let last = ciphertext.len() - 1;
        ciphertext[last] ^= 0xFF;
        assert!(enc.decrypt(&ciphertext).is_err());
    }
}
