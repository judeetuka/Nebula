use sha2::{Digest, Sha256};

use crate::types;

/// Compute authentication digest: SHA256(token + nonce).
/// Compatible with rathole's challenge-response pattern.
pub fn compute_auth_digest(token: &str, nonce: &types::Nonce) -> types::Digest {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.update(nonce);
    hasher.finalize().into()
}

/// Verify an authentication digest against expected token and nonce.
pub fn verify_auth(received: &types::Digest, token: &str, nonce: &types::Nonce) -> bool {
    let expected = compute_auth_digest(token, nonce);
    // Constant-time comparison to prevent timing attacks
    constant_time_eq(received, &expected)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}
