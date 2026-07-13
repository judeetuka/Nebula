use crate::download::{DownloadUtils, MediaConnection, MediaType};
use crate::libsignal::crypto::{CryptographicHash, CryptographicMac, aes_256_cbc_encrypt_into};
use anyhow::{Result, anyhow};
use base64::Engine as _;
use base64::prelude::*;
use rand::Rng;
use rand::rng;
use sha2::Sha256;
use std::io::{Read, Write};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAC_SIZE: usize = 10;
const AES_BLOCK: usize = 16;
const STREAM_BUF: usize = 8 * 1024;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Result of encrypting media for upload.
pub struct EncryptedMedia {
    pub data_to_upload: Vec<u8>,
    pub media_key: [u8; 32],
    pub file_sha256: [u8; 32],
    pub file_enc_sha256: [u8; 32],
}

/// Result of a streaming encryption: hashes are computed incrementally
/// and the encrypted output is written to a temp file / writer.
pub struct StreamEncryptResult {
    pub media_key: [u8; 32],
    pub file_sha256: [u8; 32],
    pub file_enc_sha256: [u8; 32],
    /// Original plaintext length.
    pub file_length: u64,
    /// Total bytes written (ciphertext + 10-byte MAC).
    pub upload_size: u64,
}

/// Parsed JSON response from the WhatsApp media upload endpoint.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct UploadResponse {
    pub url: String,
    pub direct_path: String,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub object_id: String,
}

/// Complete upload result combining crypto metadata with server response.
#[derive(Debug, Clone)]
pub struct UploadResult {
    pub url: String,
    pub direct_path: String,
    pub handle: String,
    pub media_key: Vec<u8>,
    pub file_sha256: Vec<u8>,
    pub file_enc_sha256: Vec<u8>,
    pub file_length: u64,
}

/// HTTP request descriptor for the upload. The caller (client layer) is
/// responsible for executing the actual HTTP POST.
#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub url: String,
    pub content_length: u64,
    pub origin: &'static str,
}

/// WhatsApp web origin used in upload/download request headers.
const WA_ORIGIN: &str = "https://web.whatsapp.com";

// ---------------------------------------------------------------------------
// Buffer-based encryption (existing)
// ---------------------------------------------------------------------------

pub fn encrypt_media(plaintext: &[u8], media_type: MediaType) -> Result<EncryptedMedia> {
    let file_sha256 = {
        let mut hasher = CryptographicHash::new("SHA-256").map_err(|e| anyhow::anyhow!(e))?;
        hasher.update(plaintext);
        hasher
            .finalize_sha256_array()
            .map_err(|e| anyhow::anyhow!(e))?
    };

    let mut media_key = [0u8; 32];
    rng().fill(&mut media_key);
    let (iv, cipher_key, mac_key) = DownloadUtils::get_media_keys(&media_key, media_type)?;

    let mut data = Vec::new();
    aes_256_cbc_encrypt_into(plaintext, &cipher_key, &iv, &mut data)?;

    let mac_full = {
        let mut mac =
            CryptographicMac::new("HmacSha256", &mac_key).map_err(|e| anyhow::anyhow!(e))?;
        mac.update(&iv);
        mac.update(&data);
        mac.finalize_sha256_array()
            .map_err(|e| anyhow::anyhow!(e))?
    };

    let mut upload = data;
    upload.extend_from_slice(&mac_full[..10]);

    let file_enc_sha256 = {
        let mut hasher = CryptographicHash::new("SHA-256").map_err(|e| anyhow::anyhow!(e))?;
        hasher.update(&upload);
        hasher
            .finalize_sha256_array()
            .map_err(|e| anyhow::anyhow!(e))?
    };

    Ok(EncryptedMedia {
        data_to_upload: upload,
        media_key,
        file_sha256,
        file_enc_sha256,
    })
}

// ---------------------------------------------------------------------------
// Streaming encryption (new)
// ---------------------------------------------------------------------------

/// Encrypt media from a reader, writing ciphertext to a writer.
///
/// This mirrors whatsmeow's `UploadReader` — it reads plaintext in chunks,
/// encrypts with AES-256-CBC, computes HMAC-SHA256, and writes the result
/// to `output`. Unlike `encrypt_media`, this never loads the full file into
/// memory.
///
/// The caller typically passes a temp file as `output`, then seeks back to 0
/// and streams it to the upload endpoint.
pub fn encrypt_media_streaming<R: Read, W: Write>(
    mut reader: R,
    media_type: MediaType,
    output: &mut W,
) -> Result<StreamEncryptResult> {
    use aes::Aes256;
    #[allow(deprecated)]
    use aes::cipher::generic_array::GenericArray;
    use aes::cipher::{BlockEncrypt, KeyInit};
    use sha2::Digest;

    let mut media_key = [0u8; 32];
    rng().fill(&mut media_key);
    let (iv, cipher_key, mac_key) = DownloadUtils::get_media_keys(&media_key, media_type)?;

    let cipher =
        Aes256::new_from_slice(&cipher_key).map_err(|_| anyhow!("Bad AES key length"))?;

    // Hashers for plaintext SHA-256 and upload SHA-256.
    let mut plain_hasher = Sha256::new();
    let mut enc_hasher = Sha256::new();

    // HMAC covers IV + ciphertext (no MAC bytes).
    let mut hmac = <hmac::Hmac<Sha256> as hmac::Mac>::new_from_slice(&mac_key)
        .map_err(|_| anyhow!("Failed to init HMAC"))?;
    hmac::Mac::update(&mut hmac, &iv);

    let mut prev_block: [u8; AES_BLOCK] = iv;
    let mut file_length: u64 = 0;
    let mut upload_size: u64 = 0;

    // Accumulate partial blocks from reads that aren't block-aligned.
    let mut partial = Vec::with_capacity(AES_BLOCK);
    let mut read_buf = [0u8; STREAM_BUF];

    loop {
        let n = reader.read(&mut read_buf)?;
        if n == 0 {
            break;
        }
        let chunk = &read_buf[..n];
        Digest::update(&mut plain_hasher, chunk);
        file_length += n as u64;

        partial.extend_from_slice(chunk);

        // Encrypt all complete blocks.
        while partial.len() >= AES_BLOCK {
            let mut block_bytes = [0u8; AES_BLOCK];
            block_bytes.copy_from_slice(&partial[..AES_BLOCK]);
            partial.drain(..AES_BLOCK);

            // XOR with previous ciphertext block (CBC).
            for (b, p) in block_bytes.iter_mut().zip(prev_block.iter()) {
                *b ^= *p;
            }

            #[allow(deprecated)]
            let mut block = GenericArray::clone_from_slice(&block_bytes);
            cipher.encrypt_block(&mut block);

            let ct_block: [u8; AES_BLOCK] = block.into();
            hmac::Mac::update(&mut hmac, &ct_block);
            Digest::update(&mut enc_hasher, &ct_block);
            output.write_all(&ct_block)?;
            upload_size += AES_BLOCK as u64;

            prev_block = ct_block;
        }
    }

    // PKCS7 pad the final partial block.
    let pad_len = AES_BLOCK - partial.len();
    partial.resize(AES_BLOCK, pad_len as u8);

    // XOR + encrypt the final padded block.
    {
        let mut block_bytes = [0u8; AES_BLOCK];
        block_bytes.copy_from_slice(&partial);
        for (b, p) in block_bytes.iter_mut().zip(prev_block.iter()) {
            *b ^= *p;
        }
        #[allow(deprecated)]
        let mut block = GenericArray::clone_from_slice(&block_bytes);
        cipher.encrypt_block(&mut block);

        let ct_block: [u8; AES_BLOCK] = block.into();
        hmac::Mac::update(&mut hmac, &ct_block);
        Digest::update(&mut enc_hasher, &ct_block);
        output.write_all(&ct_block)?;
        upload_size += AES_BLOCK as u64;
    }

    // Append truncated HMAC (10 bytes).
    let mac_full = hmac::Mac::finalize(hmac).into_bytes();
    let mac_trunc = &mac_full[..MAC_SIZE];
    Digest::update(&mut enc_hasher, mac_trunc);
    output.write_all(mac_trunc)?;
    upload_size += MAC_SIZE as u64;

    let file_sha256: [u8; 32] = plain_hasher.finalize().into();
    let file_enc_sha256: [u8; 32] = enc_hasher.finalize().into();

    Ok(StreamEncryptResult {
        media_key,
        file_sha256,
        file_enc_sha256,
        file_length,
        upload_size,
    })
}

// ---------------------------------------------------------------------------
// Upload URL construction
// ---------------------------------------------------------------------------

pub struct UploadUtils;

impl UploadUtils {
    /// Build the HTTP upload request for encrypted media.
    ///
    /// URL format (matches whatsmeow `rawUpload`):
    /// `https://{host}/mms/{mms_type}/{token}?auth={auth}&token={token}`
    ///
    /// The caller streams the encrypted data as the POST body.
    pub fn build_upload_request(
        file_enc_sha256: &[u8],
        media_type: MediaType,
        media_conn: &MediaConnection,
    ) -> Result<UploadRequest> {
        let host = media_conn
            .hosts
            .first()
            .ok_or_else(|| anyhow!("No media hosts available"))?;

        let token = BASE64_URL_SAFE.encode(file_enc_sha256);
        let mms_type = media_type.mms_type();

        let url = format!(
            "https://{}/mms/{}/{}?auth={}&token={}",
            host.hostname, mms_type, token, media_conn.auth, token,
        );

        Ok(UploadRequest {
            url,
            content_length: 0, // caller sets after encryption
            origin: WA_ORIGIN,
        })
    }

    /// Build the HTTP upload request for newsletter (unencrypted) media.
    ///
    /// Newsletter uploads are NOT encrypted. The token is the SHA-256 of
    /// the raw data, and the MMS type is prefixed with `newsletter-`.
    pub fn build_newsletter_upload_request(
        file_sha256: &[u8],
        media_type: MediaType,
        media_conn: &MediaConnection,
    ) -> Result<UploadRequest> {
        let host = media_conn
            .hosts
            .first()
            .ok_or_else(|| anyhow!("No media hosts available"))?;

        let token = BASE64_URL_SAFE.encode(file_sha256);
        let mms_type = format!("newsletter-{}", media_type.mms_type());

        let url = format!(
            "https://{}/newsletter/{}/{}?auth={}&token={}",
            host.hostname, mms_type, token, media_conn.auth, token,
        );

        Ok(UploadRequest {
            url,
            content_length: 0,
            origin: WA_ORIGIN,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::{DownloadUtils, MediaConnection, MediaHost};

    #[test]
    fn roundtrip_decrypt_stream() {
        let msg = b"Roundtrip encryption test payload.";
        let enc = encrypt_media(msg, MediaType::Image).expect("media operation should succeed");
        use std::io::Cursor;
        let cursor = Cursor::new(enc.data_to_upload.clone());
        let plain = DownloadUtils::decrypt_stream(cursor, &enc.media_key, MediaType::Image)
            .expect("media operation should succeed");
        assert_eq!(plain, msg);
    }

    #[test]
    fn streaming_encrypt_roundtrip() {
        let msg = b"Streaming encryption roundtrip test with enough data to span multiple blocks.";
        let mut encrypted = Vec::new();
        let result = encrypt_media_streaming(
            &msg[..],
            MediaType::Video,
            &mut encrypted,
        )
        .expect("streaming encrypt should succeed");

        assert_eq!(result.file_length, msg.len() as u64);
        assert!(result.upload_size > result.file_length); // ciphertext + MAC
        assert_eq!(encrypted.len() as u64, result.upload_size);

        // Verify file_sha256 matches plaintext.
        use sha2::Digest;
        let expected_sha = Sha256::digest(msg);
        assert_eq!(result.file_sha256, <[u8; 32]>::from(expected_sha));

        // Verify file_enc_sha256 matches the encrypted blob.
        let enc_sha = Sha256::digest(&encrypted);
        assert_eq!(result.file_enc_sha256, <[u8; 32]>::from(enc_sha));

        // Decrypt and verify roundtrip.
        use std::io::Cursor;
        let plain = DownloadUtils::decrypt_stream(
            Cursor::new(encrypted),
            &result.media_key,
            MediaType::Video,
        )
        .expect("decrypt should succeed");
        assert_eq!(plain, msg);
    }

    #[test]
    fn streaming_encrypt_single_byte() {
        // Edge case: single byte plaintext.
        let msg = b"X";
        let mut encrypted = Vec::new();
        let result = encrypt_media_streaming(&msg[..], MediaType::Audio, &mut encrypted)
            .expect("streaming encrypt should succeed");
        assert_eq!(result.file_length, 1);

        use std::io::Cursor;
        let plain = DownloadUtils::decrypt_stream(
            Cursor::new(encrypted),
            &result.media_key,
            MediaType::Audio,
        )
        .expect("decrypt should succeed");
        assert_eq!(plain, msg);
    }

    #[test]
    fn streaming_encrypt_exact_block_boundary() {
        // 32 bytes = exactly 2 AES blocks, triggers full padding block.
        let msg = [0xABu8; 32];
        let mut encrypted = Vec::new();
        let result = encrypt_media_streaming(&msg[..], MediaType::Document, &mut encrypted)
            .expect("streaming encrypt should succeed");
        assert_eq!(result.file_length, 32);

        use std::io::Cursor;
        let plain = DownloadUtils::decrypt_stream(
            Cursor::new(encrypted),
            &result.media_key,
            MediaType::Document,
        )
        .expect("decrypt should succeed");
        assert_eq!(plain.as_slice(), &msg);
    }

    #[test]
    fn streaming_encrypt_large_payload() {
        // 100KB: spans many 8KB read chunks.
        let msg = vec![0x42u8; 100 * 1024];
        let mut encrypted = Vec::new();
        let result = encrypt_media_streaming(&msg[..], MediaType::Image, &mut encrypted)
            .expect("streaming encrypt should succeed");
        assert_eq!(result.file_length, msg.len() as u64);

        use std::io::Cursor;
        let plain = DownloadUtils::decrypt_stream(
            Cursor::new(encrypted),
            &result.media_key,
            MediaType::Image,
        )
        .expect("decrypt should succeed");
        assert_eq!(plain, msg);
    }

    fn mock_media_conn() -> MediaConnection {
        MediaConnection {
            hosts: vec![
                MediaHost {
                    hostname: "mmg.whatsapp.net".into(),
                },
                MediaHost {
                    hostname: "mmg-fallback.whatsapp.net".into(),
                },
            ],
            auth: "test-auth-token-123".into(),
        }
    }

    #[test]
    fn build_upload_request_url_format() {
        let enc_hash = [0xAAu8; 32];
        let req = UploadUtils::build_upload_request(
            &enc_hash,
            MediaType::Image,
            &mock_media_conn(),
        )
        .unwrap();

        let token = BASE64_URL_SAFE.encode(&enc_hash);
        assert!(req.url.starts_with("https://mmg.whatsapp.net/mms/image/"));
        assert!(req.url.contains(&format!("auth=test-auth-token-123")));
        assert!(req.url.contains(&format!("token={}", token)));
        assert_eq!(req.origin, WA_ORIGIN);
    }

    #[test]
    fn build_upload_request_document_type() {
        let enc_hash = [0xBBu8; 32];
        let req = UploadUtils::build_upload_request(
            &enc_hash,
            MediaType::Document,
            &mock_media_conn(),
        )
        .unwrap();
        assert!(req.url.contains("/mms/document/"));
    }

    #[test]
    fn build_newsletter_upload_request_format() {
        let sha256 = [0xCCu8; 32];
        let req = UploadUtils::build_newsletter_upload_request(
            &sha256,
            MediaType::Image,
            &mock_media_conn(),
        )
        .unwrap();

        assert!(req.url.starts_with("https://mmg.whatsapp.net/newsletter/newsletter-image/"));
        assert!(req.url.contains("auth=test-auth-token-123"));
    }

    #[test]
    fn build_upload_request_no_hosts_errors() {
        let conn = MediaConnection {
            hosts: vec![],
            auth: "tok".into(),
        };
        let err = UploadUtils::build_upload_request(&[0u8; 32], MediaType::Image, &conn);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("No media hosts"));
    }
}
