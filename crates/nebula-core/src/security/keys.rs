use anyhow::{Context, Result};
use hkdf::Hkdf;
use sha2::Sha256;

/// HMAC signing key (32 bytes).
#[derive(Clone)]
pub struct HmacKey(pub [u8; 32]);

/// AES-256-GCM encryption key (32 bytes).
#[derive(Clone)]
pub struct AesKey(pub [u8; 32]);

/// A derived key pair for the two-layer security envelope.
#[derive(Clone)]
pub struct KeyPair {
    pub hmac: HmacKey,
    pub aes: AesKey,
}

impl KeyPair {
    /// Derive HMAC and AES keys from a master secret using HKDF-SHA256.
    ///
    /// Two separate derivation paths ensure key independence:
    /// - Path 1: HKDF(info="nebula-hmac-v1") → 32-byte HMAC key
    /// - Path 2: HKDF(info="nebula-aes-v1")  → 32-byte AES key
    pub fn derive_from_secret(master_secret: &[u8]) -> Result<Self> {
        let hk = Hkdf::<Sha256>::new(None, master_secret);

        let mut hmac_bytes = [0u8; 32];
        hk.expand(b"nebula-hmac-v1", &mut hmac_bytes)
            .map_err(|e| anyhow::anyhow!("HKDF expand for HMAC failed: {}", e))
            .context("deriving HMAC key")?;

        let mut aes_bytes = [0u8; 32];
        hk.expand(b"nebula-aes-v1", &mut aes_bytes)
            .map_err(|e| anyhow::anyhow!("HKDF expand for AES failed: {}", e))
            .context("deriving AES key")?;

        Ok(Self {
            hmac: HmacKey(hmac_bytes),
            aes: AesKey(aes_bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_produces_different_keys() {
        let kp = KeyPair::derive_from_secret(b"test").unwrap();
        assert_ne!(kp.hmac.0, kp.aes.0); // HMAC and AES keys must differ
    }

    #[test]
    fn test_derive_deterministic() {
        let k1 = KeyPair::derive_from_secret(b"same-secret").unwrap();
        let k2 = KeyPair::derive_from_secret(b"same-secret").unwrap();
        assert_eq!(k1.hmac.0, k2.hmac.0);
        assert_eq!(k1.aes.0, k2.aes.0);
    }

    #[test]
    fn test_different_secrets_different_keys() {
        let k1 = KeyPair::derive_from_secret(b"secret-a").unwrap();
        let k2 = KeyPair::derive_from_secret(b"secret-b").unwrap();
        assert_ne!(k1.hmac.0, k2.hmac.0);
        assert_ne!(k1.aes.0, k2.aes.0);
    }

    #[test]
    fn test_empty_secret() {
        let kp = KeyPair::derive_from_secret(b"").unwrap();
        assert_ne!(kp.hmac.0, [0u8; 32]); // Even empty secret produces real keys
    }
}
