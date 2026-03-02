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
