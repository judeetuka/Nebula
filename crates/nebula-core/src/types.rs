/// Width of SHA-256 digest in bytes.
pub const HASH_WIDTH_IN_BYTES: usize = 32;

/// A SHA-256 digest.
pub type Digest = [u8; HASH_WIDTH_IN_BYTES];

/// A cryptographic nonce (32 bytes).
pub type Nonce = [u8; HASH_WIDTH_IN_BYTES];
