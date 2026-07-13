use crate::libsignal::crypto::{CryptographicMac, aes_256_cbc_decrypt_into};
use anyhow::{Result, anyhow};
use base64::Engine as _;
use base64::prelude::*;
use hkdf::Hkdf;
use hmac::Hmac;
use hmac::Mac;
use sha2::Sha256;
use std::path::Path;
use waproto::whatsapp as wa;
use waproto::whatsapp::ExternalBlobReference;
use waproto::whatsapp::message::HistorySyncNotification;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Video,
    Audio,
    Document,
    History,
    AppState,
    Sticker,
    StickerPack,
    LinkThumbnail,
}

impl MediaType {
    pub fn app_info(&self) -> &'static str {
        match self {
            MediaType::Image => "WhatsApp Image Keys",
            MediaType::Video => "WhatsApp Video Keys",
            MediaType::Audio => "WhatsApp Audio Keys",
            MediaType::Document => "WhatsApp Document Keys",
            MediaType::History => "WhatsApp History Keys",
            MediaType::AppState => "WhatsApp App State Keys",
            MediaType::Sticker => "WhatsApp Image Keys",
            MediaType::StickerPack => "WhatsApp Sticker Pack Keys",
            MediaType::LinkThumbnail => "WhatsApp Link Thumbnail Keys",
        }
    }

    pub fn mms_type(&self) -> &'static str {
        match self {
            MediaType::Image | MediaType::Sticker => "image",
            MediaType::Video => "video",
            MediaType::Audio => "audio",
            MediaType::Document => "document",
            MediaType::History => "md-msg-hist",
            MediaType::AppState => "md-app-state",
            MediaType::StickerPack => "sticker-pack",
            MediaType::LinkThumbnail => "thumbnail-link",
        }
    }
}

/// Describes how downloaded media bytes should be processed after HTTP fetch.
///
/// Mirrors WhatsApp Web's `isMediaCryptoExpectedForMediaType()` pattern:
/// encrypted (E2EE) media requires AES-256-CBC decryption + HMAC verification,
/// while unencrypted media (newsletters/channels) only needs SHA-256 validation.
#[derive(Debug, Clone)]
pub enum MediaDecryption {
    /// E2E encrypted media: decrypt with AES-256-CBC using HKDF-expanded
    /// keys from the media key, then verify HMAC-SHA256 integrity.
    Encrypted {
        media_key: Vec<u8>,
        media_type: MediaType,
    },
    /// Unencrypted media (newsletter/channel): verify SHA-256 hash of
    /// the raw downloaded bytes. No decryption needed.
    Plaintext { file_sha256: Vec<u8> },
}

pub trait Downloadable: Sync + Send {
    fn direct_path(&self) -> Option<&str>;
    fn media_key(&self) -> Option<&[u8]>;
    fn file_enc_sha256(&self) -> Option<&[u8]>;
    fn file_sha256(&self) -> Option<&[u8]>;
    fn file_length(&self) -> Option<u64>;
    fn app_info(&self) -> MediaType;

    /// Static CDN URL for direct download, bypassing host construction.
    /// Present on some message types (ImageMessage, VideoMessage) when
    /// sent in newsletter/channel chats.
    fn static_url(&self) -> Option<&str> {
        None
    }

    /// Whether this media requires decryption.
    /// Returns `true` if `media_key` is present (E2EE media),
    /// `false` otherwise (newsletter/channel media).
    fn is_encrypted(&self) -> bool {
        self.media_key().is_some()
    }
}

macro_rules! impl_downloadable {
    (@common $file_length_field:ident, $media_type:expr) => {
        fn direct_path(&self) -> Option<&str> {
            self.direct_path.as_deref()
        }

        fn media_key(&self) -> Option<&[u8]> {
            self.media_key.as_deref()
        }

        fn file_enc_sha256(&self) -> Option<&[u8]> {
            self.file_enc_sha256.as_deref()
        }

        fn file_sha256(&self) -> Option<&[u8]> {
            self.file_sha256.as_deref()
        }

        fn file_length(&self) -> Option<u64> {
            self.$file_length_field
        }

        fn app_info(&self) -> MediaType {
            $media_type
        }
    };
    ($type:ty, $media_type:expr, $file_length_field:ident) => {
        impl Downloadable for $type {
            impl_downloadable!(@common $file_length_field, $media_type);
        }
    };
    ($type:ty, $media_type:expr, $file_length_field:ident, static_url) => {
        impl Downloadable for $type {
            impl_downloadable!(@common $file_length_field, $media_type);

            fn static_url(&self) -> Option<&str> {
                self.static_url.as_deref()
            }
        }
    };
}

impl_downloadable!(
    wa::message::ImageMessage,
    MediaType::Image,
    file_length,
    static_url
);
impl_downloadable!(
    wa::message::VideoMessage,
    MediaType::Video,
    file_length,
    static_url
);
impl_downloadable!(
    wa::message::DocumentMessage,
    MediaType::Document,
    file_length
);
impl_downloadable!(wa::message::AudioMessage, MediaType::Audio, file_length);
impl_downloadable!(wa::message::StickerMessage, MediaType::Sticker, file_length);
impl_downloadable!(ExternalBlobReference, MediaType::AppState, file_size_bytes);
impl_downloadable!(HistorySyncNotification, MediaType::History, file_length);

// ---------------------------------------------------------------------------
// DownloadableThumbnail trait
// ---------------------------------------------------------------------------

/// Trait for messages that carry an encrypted thumbnail attachment
/// (e.g., link preview thumbnails in ExtendedTextMessage, or document/image
/// thumbnails). Mirrors whatsmeow's `DownloadableThumbnail` interface.
pub trait DownloadableThumbnail: Sync + Send {
    fn thumbnail_direct_path(&self) -> Option<&str>;
    fn thumbnail_sha256(&self) -> Option<&[u8]>;
    fn thumbnail_enc_sha256(&self) -> Option<&[u8]>;
    fn media_key(&self) -> Option<&[u8]>;
    fn thumbnail_media_type(&self) -> MediaType;
}

/// ExtendedTextMessage thumbnails are link preview thumbnails.
impl DownloadableThumbnail for wa::message::ExtendedTextMessage {
    fn thumbnail_direct_path(&self) -> Option<&str> {
        self.thumbnail_direct_path.as_deref()
    }
    fn thumbnail_sha256(&self) -> Option<&[u8]> {
        self.thumbnail_sha256.as_deref()
    }
    fn thumbnail_enc_sha256(&self) -> Option<&[u8]> {
        self.thumbnail_enc_sha256.as_deref()
    }
    fn media_key(&self) -> Option<&[u8]> {
        self.media_key.as_deref()
    }
    fn thumbnail_media_type(&self) -> MediaType {
        MediaType::LinkThumbnail
    }
}

/// ImageMessage can carry mid-quality or thumbnail paths.
impl DownloadableThumbnail for wa::message::ImageMessage {
    fn thumbnail_direct_path(&self) -> Option<&str> {
        self.thumbnail_direct_path.as_deref()
    }
    fn thumbnail_sha256(&self) -> Option<&[u8]> {
        self.thumbnail_sha256.as_deref()
    }
    fn thumbnail_enc_sha256(&self) -> Option<&[u8]> {
        self.thumbnail_enc_sha256.as_deref()
    }
    fn media_key(&self) -> Option<&[u8]> {
        self.media_key.as_deref()
    }
    fn thumbnail_media_type(&self) -> MediaType {
        MediaType::Image
    }
}

/// VideoMessage thumbnails.
impl DownloadableThumbnail for wa::message::VideoMessage {
    fn thumbnail_direct_path(&self) -> Option<&str> {
        self.thumbnail_direct_path.as_deref()
    }
    fn thumbnail_sha256(&self) -> Option<&[u8]> {
        self.thumbnail_sha256.as_deref()
    }
    fn thumbnail_enc_sha256(&self) -> Option<&[u8]> {
        self.thumbnail_enc_sha256.as_deref()
    }
    fn media_key(&self) -> Option<&[u8]> {
        self.media_key.as_deref()
    }
    fn thumbnail_media_type(&self) -> MediaType {
        MediaType::Image
    }
}

/// DocumentMessage thumbnails.
impl DownloadableThumbnail for wa::message::DocumentMessage {
    fn thumbnail_direct_path(&self) -> Option<&str> {
        self.thumbnail_direct_path.as_deref()
    }
    fn thumbnail_sha256(&self) -> Option<&[u8]> {
        self.thumbnail_sha256.as_deref()
    }
    fn thumbnail_enc_sha256(&self) -> Option<&[u8]> {
        self.thumbnail_enc_sha256.as_deref()
    }
    fn media_key(&self) -> Option<&[u8]> {
        self.media_key.as_deref()
    }
    fn thumbnail_media_type(&self) -> MediaType {
        MediaType::Image
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub decryption: MediaDecryption,
}

pub struct MediaConnection {
    pub hosts: Vec<MediaHost>,
    pub auth: String,
}

pub struct MediaHost {
    pub hostname: String,
}

pub struct DownloadUtils;

impl DownloadUtils {
    pub fn prepare_download_requests(
        downloadable: &dyn Downloadable,
        media_conn: &MediaConnection,
    ) -> Result<Vec<DownloadRequest>> {
        let is_encrypted = downloadable.is_encrypted();
        let media_type = downloadable.app_info();

        let decryption = if is_encrypted {
            let media_key = downloadable
                .media_key()
                .ok_or_else(|| anyhow!("Missing media_key for encrypted media"))?
                .to_vec();
            MediaDecryption::Encrypted {
                media_key,
                media_type,
            }
        } else {
            let file_sha256 = downloadable
                .file_sha256()
                .ok_or_else(|| anyhow!("Missing file_sha256 for unencrypted media"))?
                .to_vec();
            MediaDecryption::Plaintext { file_sha256 }
        };

        // Static URL: use directly without host construction.
        // WhatsApp Web uses staticUrl for newsletter CDN media.
        if let Some(static_url) = downloadable.static_url() {
            return Ok(vec![DownloadRequest {
                url: static_url.to_string(),
                decryption,
            }]);
        }

        let direct_path = downloadable
            .direct_path()
            .ok_or_else(|| anyhow!("Missing direct_path"))?;

        // Encrypted media uses file_enc_sha256 as URL token,
        // unencrypted (newsletter) uses file_sha256 instead.
        let token = if is_encrypted {
            let hash = downloadable
                .file_enc_sha256()
                .ok_or_else(|| anyhow!("Missing file_enc_sha256"))?;
            BASE64_URL_SAFE_NO_PAD.encode(hash)
        } else {
            let hash = downloadable
                .file_sha256()
                .ok_or_else(|| anyhow!("Missing file_sha256 for unencrypted media"))?;
            BASE64_URL_SAFE_NO_PAD.encode(hash)
        };

        let requests = media_conn
            .hosts
            .iter()
            .map(|host| DownloadRequest {
                url: format!(
                    "https://{}{direct_path}?auth={}&token={token}",
                    host.hostname, media_conn.auth,
                ),
                decryption: decryption.clone(),
            })
            .collect();

        Ok(requests)
    }

    /// Validate SHA-256 hash of plaintext (unencrypted) media data.
    ///
    /// Used for newsletter/channel media which is not encrypted but
    /// still needs integrity verification (matches WhatsApp Web's
    /// `validateFilehash()` call for unencrypted downloads).
    pub fn validate_plaintext_sha256(data: &[u8], expected_sha256: &[u8]) -> Result<()> {
        use sha2::Digest;
        let actual = Sha256::digest(data);
        if actual.as_slice() != expected_sha256 {
            return Err(anyhow!(
                "SHA-256 mismatch for plaintext media: expected {}, got {}",
                hex::encode(expected_sha256),
                hex::encode(actual),
            ));
        }
        Ok(())
    }

    /// Stream plaintext (unencrypted) media to a writer while computing and
    /// validating the SHA-256 hash. Returns the number of bytes written.
    ///
    /// On hash mismatch, data has already been written to the writer;
    /// callers should discard writer contents on error.
    pub fn copy_and_validate_plaintext_to_writer<R: std::io::Read, W: std::io::Write>(
        mut reader: R,
        expected_sha256: &[u8],
        writer: &mut W,
    ) -> Result<u64> {
        use sha2::Digest;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            writer.write_all(&buf[..n])?;
            total += n as u64;
        }
        let actual = hasher.finalize();
        if actual.as_slice() != expected_sha256 {
            return Err(anyhow!("SHA-256 mismatch for plaintext media"));
        }
        Ok(total)
    }

    /// Decrypt a media stream, writing plaintext chunks to the given writer.
    ///
    /// Reads encrypted data in 8KB chunks from `reader`, decrypts with AES-256-CBC,
    /// verifies HMAC-SHA256 integrity, and writes decrypted plaintext to `writer`.
    /// Returns the number of plaintext bytes written.
    ///
    /// If MAC verification fails, an error is returned. Note that some data may
    /// already have been written to `writer` before the MAC is checked (the MAC
    /// covers the last 10 bytes of the stream). Callers should discard the writer
    /// contents on error.
    pub fn decrypt_stream_to_writer<R: std::io::Read, W: std::io::Write>(
        mut reader: R,
        media_key: &[u8],
        app_info: MediaType,
        writer: &mut W,
    ) -> Result<u64> {
        use aes::Aes256;
        #[allow(deprecated)]
        use aes::cipher::generic_array::GenericArray;
        use aes::cipher::{BlockDecrypt, KeyInit};

        const MAC_SIZE: usize = 10;
        const BLOCK: usize = 16;
        const CHUNK: usize = 8 * 1024;

        let (iv, cipher_key, mac_key) = Self::get_media_keys(media_key, app_info)?;

        let mut hmac = <Hmac<Sha256> as hmac::Mac>::new_from_slice(&mac_key)
            .map_err(|_| anyhow!("Failed to init HMAC"))?;
        hmac.update(&iv);

        let cipher =
            Aes256::new_from_slice(&cipher_key).map_err(|_| anyhow!("Bad AES key length"))?;

        let mut bytes_written: u64 = 0;
        let mut tail: Vec<u8> = Vec::with_capacity(BLOCK + MAC_SIZE);
        let mut prev_block = iv;

        let mut read_buf = [0u8; CHUNK];

        loop {
            let n = reader.read(&mut read_buf)?;
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&read_buf[..n]);

            if tail.len() > MAC_SIZE + BLOCK {
                let mut processable_len = tail.len() - (MAC_SIZE + BLOCK);
                processable_len -= processable_len % BLOCK;
                if processable_len >= BLOCK {
                    let (to_process, rest) = tail.split_at(processable_len);
                    hmac.update(to_process);
                    for cblock in to_process.chunks_exact(BLOCK) {
                        #[allow(deprecated)]
                        let mut block = GenericArray::clone_from_slice(cblock);
                        cipher.decrypt_block(&mut block);
                        for (b, p) in block.iter_mut().zip(prev_block.iter()) {
                            *b ^= *p;
                        }
                        writer.write_all(&block)?;
                        bytes_written += BLOCK as u64;
                        prev_block = match <[u8; BLOCK]>::try_from(cblock) {
                            Ok(arr) => arr,
                            Err(_) => return Err(anyhow!("Failed to convert block to array")),
                        };
                    }
                    tail = rest.to_vec();
                }
            }
        }

        if tail.len() < MAC_SIZE + BLOCK || !(tail.len() - MAC_SIZE).is_multiple_of(BLOCK) {
            return Err(anyhow!("Invalid final media size"));
        }
        let mac_index = tail.len() - MAC_SIZE;
        let (final_ciphertext, mac_bytes) = tail.split_at(mac_index);
        hmac.update(final_ciphertext);
        let expected_mac_full = hmac.finalize().into_bytes();
        let expected_mac = &expected_mac_full[..MAC_SIZE];
        if mac_bytes != expected_mac {
            return Err(anyhow!("MAC mismatch"));
        }

        let mut final_plain = Vec::with_capacity(final_ciphertext.len());
        for cblock in final_ciphertext.chunks_exact(BLOCK) {
            #[allow(deprecated)]
            let mut block = GenericArray::clone_from_slice(cblock);
            cipher.decrypt_block(&mut block);
            for (b, p) in block.iter_mut().zip(prev_block.iter()) {
                *b ^= *p;
            }
            final_plain.extend_from_slice(&block);
        }
        if final_plain.is_empty() {
            return Err(anyhow!("Empty plaintext after decrypt"));
        }
        let pad_len = match final_plain.last() {
            Some(&v) => v as usize,
            None => return Err(anyhow!("Empty plaintext after decrypt")),
        };
        if pad_len == 0 || pad_len > BLOCK || pad_len > final_plain.len() {
            return Err(anyhow!("Invalid PKCS7 padding"));
        }
        if !final_plain[final_plain.len() - pad_len..]
            .iter()
            .all(|&b| b as usize == pad_len)
        {
            return Err(anyhow!("Bad PKCS7 padding bytes"));
        }
        final_plain.truncate(final_plain.len() - pad_len);
        writer.write_all(&final_plain)?;
        bytes_written += final_plain.len() as u64;

        Ok(bytes_written)
    }

    /// Decrypt a media stream, returning the plaintext as a `Vec<u8>`.
    ///
    /// This is a convenience wrapper around [`decrypt_stream_to_writer`] that
    /// accumulates output in memory.
    pub fn decrypt_stream<R: std::io::Read>(
        reader: R,
        media_key: &[u8],
        app_info: MediaType,
    ) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        Self::decrypt_stream_to_writer(reader, media_key, app_info, &mut buf)?;
        Ok(buf)
    }

    pub fn get_media_keys(
        media_key: &[u8],
        app_info: MediaType,
    ) -> Result<([u8; 16], [u8; 32], [u8; 32])> {
        let hk = Hkdf::<Sha256>::new(None, media_key);
        let mut expanded = vec![0u8; 112];
        hk.expand(app_info.app_info().as_bytes(), &mut expanded)
            .map_err(|e| anyhow!("HKDF expand failed: {e}"))?;
        let iv: [u8; 16] = expanded[0..16]
            .try_into()
            .map_err(|_| anyhow!("HKDF output has unexpected length for IV"))?;
        let cipher_key: [u8; 32] = expanded[16..48]
            .try_into()
            .map_err(|_| anyhow!("HKDF output has unexpected length for cipher key"))?;
        let mac_key: [u8; 32] = expanded[48..80]
            .try_into()
            .map_err(|_| anyhow!("HKDF output has unexpected length for MAC key"))?;
        Ok((iv, cipher_key, mac_key))
    }

    pub fn decrypt_cbc(cipher_key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        aes_256_cbc_decrypt_into(ciphertext, cipher_key, iv, &mut output)
            .map_err(|e| anyhow!(e.to_string()))?;
        Ok(output)
    }

    pub fn verify_and_decrypt(
        encrypted_payload: &[u8],
        media_key: &[u8],
        media_type: MediaType,
    ) -> Result<Vec<u8>> {
        const MAC_SIZE: usize = 10;
        if encrypted_payload.len() <= MAC_SIZE {
            return Err(anyhow!("Downloaded file is too short to contain MAC"));
        }

        let (ciphertext, received_mac) =
            encrypted_payload.split_at(encrypted_payload.len() - MAC_SIZE);

        let (iv, cipher_key, mac_key) = Self::get_media_keys(media_key, media_type)?;

        let computed_mac_full = {
            let mut mac = CryptographicMac::new("HmacSha256", &mac_key)
                .map_err(|e| anyhow!(e.to_string()))?;
            mac.update(&iv);
            mac.update(ciphertext);
            mac.finalize()
        };
        if &computed_mac_full[..MAC_SIZE] != received_mac {
            return Err(anyhow!("Invalid MAC signature"));
        }

        let mut output = Vec::new();
        aes_256_cbc_decrypt_into(ciphertext, &cipher_key, &iv, &mut output)
            .map_err(|e| anyhow!(e.to_string()))?;
        Ok(output)
    }

    // -------------------------------------------------------------------
    // Download URL construction
    // -------------------------------------------------------------------

    /// Build a download URL from a direct path and media connection.
    ///
    /// Mirrors whatsmeow's `DownloadMediaWithPath` URL construction:
    /// `https://{host}{direct_path}&hash={base64url(enc_sha256)}&mms-type={type}&__wa-mms=`
    ///
    /// Returns one URL per host in `media_conn`, to be tried in order.
    pub fn build_download_urls(
        direct_path: &str,
        enc_file_hash: &[u8],
        media_type: MediaType,
        media_conn: &MediaConnection,
    ) -> Result<Vec<String>> {
        if !direct_path.starts_with('/') {
            return Err(anyhow!(
                "Media download path does not start with slash: {}",
                direct_path
            ));
        }
        let hash_token = BASE64_URL_SAFE.encode(enc_file_hash);
        let mms_type = media_type.mms_type();

        let urls: Vec<String> = media_conn
            .hosts
            .iter()
            .map(|host| {
                format!(
                    "https://{}{}&hash={}&mms-type={}&__wa-mms=",
                    host.hostname, direct_path, hash_token, mms_type,
                )
            })
            .collect();

        if urls.is_empty() {
            return Err(anyhow!("No media hosts available"));
        }
        Ok(urls)
    }

    /// Build download URL from a [`Downloadable`] message + media connection.
    ///
    /// This is a higher-level wrapper around [`build_download_urls`] that
    /// extracts the needed fields from the downloadable trait object.
    pub fn build_download_url_from_msg(
        downloadable: &dyn Downloadable,
        media_conn: &MediaConnection,
    ) -> Result<Vec<String>> {
        // Prefer static URL if available.
        if let Some(static_url) = downloadable.static_url() {
            return Ok(vec![static_url.to_string()]);
        }

        let direct_path = downloadable
            .direct_path()
            .ok_or_else(|| anyhow!("Missing direct_path"))?;

        let enc_hash = if downloadable.is_encrypted() {
            downloadable
                .file_enc_sha256()
                .ok_or_else(|| anyhow!("Missing file_enc_sha256"))?
        } else {
            downloadable
                .file_sha256()
                .ok_or_else(|| anyhow!("Missing file_sha256 for unencrypted media"))?
        };

        Self::build_download_urls(direct_path, enc_hash, downloadable.app_info(), media_conn)
    }

    /// Build download URLs for a thumbnail attachment.
    pub fn build_thumbnail_download_urls(
        thumb: &dyn DownloadableThumbnail,
        media_conn: &MediaConnection,
    ) -> Result<Vec<String>> {
        let direct_path = thumb
            .thumbnail_direct_path()
            .ok_or_else(|| anyhow!("Missing thumbnail_direct_path"))?;
        let enc_hash = thumb
            .thumbnail_enc_sha256()
            .ok_or_else(|| anyhow!("Missing thumbnail_enc_sha256"))?;

        Self::build_download_urls(
            direct_path,
            enc_hash,
            thumb.thumbnail_media_type(),
            media_conn,
        )
    }

    // -------------------------------------------------------------------
    // Streaming decrypt to file
    // -------------------------------------------------------------------

    /// Streaming download-and-decrypt to a file path.
    ///
    /// Reads encrypted data from `reader`, decrypts with AES-256-CBC,
    /// verifies HMAC-SHA256, and writes decrypted plaintext to `output_path`.
    /// Returns the number of plaintext bytes written.
    ///
    /// This mirrors whatsmeow's `DownloadToFile` approach: the encrypted
    /// stream is processed in chunks without buffering the entire payload.
    pub fn decrypt_stream_to_file<R: std::io::Read>(
        reader: R,
        media_key: &[u8],
        media_type: MediaType,
        output_path: &Path,
    ) -> Result<u64> {
        let file = std::fs::File::create(output_path)
            .map_err(|e| anyhow!("Failed to create output file: {}", e))?;
        let mut writer = std::io::BufWriter::new(file);
        let bytes = Self::decrypt_stream_to_writer(reader, media_key, media_type, &mut writer)?;
        std::io::Write::flush(&mut writer)?;
        Ok(bytes)
    }

    /// Streaming download-and-decrypt for plaintext (newsletter) media to a file.
    ///
    /// Reads unencrypted data from `reader`, validates SHA-256 hash,
    /// and writes to `output_path`.
    pub fn copy_plaintext_to_file<R: std::io::Read>(
        reader: R,
        expected_sha256: &[u8],
        output_path: &Path,
    ) -> Result<u64> {
        let file = std::fs::File::create(output_path)
            .map_err(|e| anyhow!("Failed to create output file: {}", e))?;
        let mut writer = std::io::BufWriter::new(file);
        let bytes =
            Self::copy_and_validate_plaintext_to_writer(reader, expected_sha256, &mut writer)?;
        std::io::Write::flush(&mut writer)?;
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// DownloadAny: convenience dispatcher
// ---------------------------------------------------------------------------

/// Identifies which media sub-message is present in a `wa::Message` and
/// returns a reference to it as a `&dyn Downloadable`.
///
/// Mirrors whatsmeow's `DownloadAny` — inspects the message and returns the
/// first non-nil downloadable attachment. The caller then feeds the
/// downloadable to `DownloadUtils::prepare_download_requests` and
/// `DownloadUtils::decrypt_stream`.
pub fn download_any_ref(msg: &wa::Message) -> Result<&dyn Downloadable> {
    if let Some(ref m) = msg.image_message {
        return Ok(m.as_ref());
    }
    if let Some(ref m) = msg.video_message {
        return Ok(m.as_ref());
    }
    if let Some(ref m) = msg.audio_message {
        return Ok(m.as_ref());
    }
    if let Some(ref m) = msg.document_message {
        return Ok(m.as_ref());
    }
    if let Some(ref m) = msg.sticker_message {
        return Ok(m.as_ref());
    }
    Err(anyhow!("No downloadable media found in message"))
}

/// Returns the [`MediaType`] for the first downloadable sub-message.
pub fn media_type_of(msg: &wa::Message) -> Option<MediaType> {
    download_any_ref(msg).ok().map(|d| d.app_info())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDownloadable {
        direct_path: Option<String>,
        static_url: Option<String>,
        media_key: Option<Vec<u8>>,
        file_sha256: Option<Vec<u8>>,
        file_enc_sha256: Option<Vec<u8>>,
        media_type: MediaType,
    }

    impl Downloadable for MockDownloadable {
        fn direct_path(&self) -> Option<&str> {
            self.direct_path.as_deref()
        }
        fn media_key(&self) -> Option<&[u8]> {
            self.media_key.as_deref()
        }
        fn file_enc_sha256(&self) -> Option<&[u8]> {
            self.file_enc_sha256.as_deref()
        }
        fn file_sha256(&self) -> Option<&[u8]> {
            self.file_sha256.as_deref()
        }
        fn file_length(&self) -> Option<u64> {
            Some(1024)
        }
        fn app_info(&self) -> MediaType {
            self.media_type
        }
        fn static_url(&self) -> Option<&str> {
            self.static_url.as_deref()
        }
    }

    fn mock_media_conn() -> MediaConnection {
        MediaConnection {
            hosts: vec![
                MediaHost {
                    hostname: "cdn1.example.com".into(),
                },
                MediaHost {
                    hostname: "cdn2.example.com".into(),
                },
            ],
            auth: "test-auth-token".into(),
        }
    }

    #[test]
    fn prepare_requests_encrypted() {
        let d = MockDownloadable {
            direct_path: Some("/v/t1/media.enc".into()),
            static_url: None,
            media_key: Some(vec![1; 32]),
            file_sha256: Some(vec![2; 32]),
            file_enc_sha256: Some(vec![3; 32]),
            media_type: MediaType::Image,
        };
        let reqs = DownloadUtils::prepare_download_requests(&d, &mock_media_conn()).unwrap();
        assert_eq!(reqs.len(), 2);
        assert!(matches!(
            &reqs[0].decryption,
            MediaDecryption::Encrypted { media_type, .. } if *media_type == MediaType::Image
        ));
        let expected_token = BASE64_URL_SAFE_NO_PAD.encode([3u8; 32]);
        assert!(reqs[0].url.contains(&expected_token));
        assert!(reqs[0].url.starts_with("https://cdn1.example.com"));
        assert!(reqs[1].url.starts_with("https://cdn2.example.com"));
    }

    #[test]
    fn prepare_requests_plaintext_newsletter() {
        let d = MockDownloadable {
            direct_path: Some("/newsletter/newsletter-image/abc".into()),
            static_url: None,
            media_key: None,
            file_sha256: Some(vec![4; 32]),
            file_enc_sha256: None,
            media_type: MediaType::Image,
        };
        let reqs = DownloadUtils::prepare_download_requests(&d, &mock_media_conn()).unwrap();
        assert_eq!(reqs.len(), 2);
        assert!(matches!(
            &reqs[0].decryption,
            MediaDecryption::Plaintext { file_sha256 } if file_sha256 == &vec![4u8; 32]
        ));
        // Token should be base64url of file_sha256 (not file_enc_sha256)
        let expected_token = BASE64_URL_SAFE_NO_PAD.encode([4u8; 32]);
        assert!(reqs[0].url.contains(&expected_token));
    }

    #[test]
    fn prepare_requests_static_url() {
        let d = MockDownloadable {
            direct_path: Some("/unused".into()),
            static_url: Some("https://static.cdn.example.com/media/abc123".into()),
            media_key: None,
            file_sha256: Some(vec![5; 32]),
            file_enc_sha256: None,
            media_type: MediaType::Image,
        };
        let reqs = DownloadUtils::prepare_download_requests(&d, &mock_media_conn()).unwrap();
        // Static URL bypasses host construction → single request
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://static.cdn.example.com/media/abc123");
        assert!(matches!(
            &reqs[0].decryption,
            MediaDecryption::Plaintext { .. }
        ));
    }

    #[test]
    fn prepare_requests_missing_direct_path_no_static_url() {
        let d = MockDownloadable {
            direct_path: None,
            static_url: None,
            media_key: Some(vec![1; 32]),
            file_sha256: Some(vec![2; 32]),
            file_enc_sha256: Some(vec![3; 32]),
            media_type: MediaType::Image,
        };
        let err = DownloadUtils::prepare_download_requests(&d, &mock_media_conn()).unwrap_err();
        assert!(err.to_string().contains("Missing direct_path"));
    }

    #[test]
    fn validate_plaintext_sha256_ok() {
        use sha2::Digest;
        let data = b"test newsletter media content";
        let hash = Sha256::digest(data);
        assert!(DownloadUtils::validate_plaintext_sha256(data, hash.as_slice()).is_ok());
    }

    #[test]
    fn validate_plaintext_sha256_mismatch() {
        let data = b"test newsletter media content";
        let wrong_hash = vec![0u8; 32];
        let err = DownloadUtils::validate_plaintext_sha256(data, &wrong_hash).unwrap_err();
        assert!(err.to_string().contains("SHA-256 mismatch"));
    }

    #[test]
    fn copy_and_validate_plaintext_ok() {
        use sha2::Digest;
        use std::io::Cursor;
        let data = b"streaming newsletter content";
        let hash = Sha256::digest(data);
        let reader = Cursor::new(data.to_vec());
        let mut writer = Vec::new();
        let bytes = DownloadUtils::copy_and_validate_plaintext_to_writer(
            reader,
            hash.as_slice(),
            &mut writer,
        )
        .unwrap();
        assert_eq!(bytes, data.len() as u64);
        assert_eq!(writer, data);
    }

    #[test]
    fn copy_and_validate_plaintext_mismatch() {
        use std::io::Cursor;
        let data = b"streaming newsletter content";
        let wrong_hash = vec![0u8; 32];
        let reader = Cursor::new(data.to_vec());
        let mut writer = Vec::new();
        let err =
            DownloadUtils::copy_and_validate_plaintext_to_writer(reader, &wrong_hash, &mut writer)
                .unwrap_err();
        assert!(err.to_string().contains("SHA-256 mismatch"));
    }

    // -------------------------------------------------------------------
    // Tests for build_download_urls
    // -------------------------------------------------------------------

    #[test]
    fn build_download_urls_basic() {
        let enc_hash = [0xAA; 32];
        let urls = DownloadUtils::build_download_urls(
            "/v/t1/media.enc",
            &enc_hash,
            MediaType::Image,
            &mock_media_conn(),
        )
        .unwrap();

        assert_eq!(urls.len(), 2);
        let hash_token = BASE64_URL_SAFE.encode(&enc_hash);
        assert!(urls[0].starts_with("https://cdn1.example.com/v/t1/media.enc"));
        assert!(urls[0].contains(&format!("hash={}", hash_token)));
        assert!(urls[0].contains("mms-type=image"));
        assert!(urls[0].ends_with("__wa-mms="));
        assert!(urls[1].starts_with("https://cdn2.example.com"));
    }

    #[test]
    fn build_download_urls_invalid_path() {
        let err = DownloadUtils::build_download_urls(
            "no-slash",
            &[0; 32],
            MediaType::Image,
            &mock_media_conn(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not start with slash"));
    }

    #[test]
    fn build_download_urls_no_hosts() {
        let conn = MediaConnection {
            hosts: vec![],
            auth: "tok".into(),
        };
        let err =
            DownloadUtils::build_download_urls("/path", &[0; 32], MediaType::Image, &conn)
                .unwrap_err();
        assert!(err.to_string().contains("No media hosts"));
    }

    #[test]
    fn build_download_url_from_msg_encrypted() {
        let d = MockDownloadable {
            direct_path: Some("/v/t1/encrypted.enc".into()),
            static_url: None,
            media_key: Some(vec![1; 32]),
            file_sha256: Some(vec![2; 32]),
            file_enc_sha256: Some(vec![3; 32]),
            media_type: MediaType::Video,
        };
        let urls = DownloadUtils::build_download_url_from_msg(&d, &mock_media_conn()).unwrap();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("mms-type=video"));
        // Should use file_enc_sha256 as hash token.
        let expected_token = BASE64_URL_SAFE.encode([3u8; 32]);
        assert!(urls[0].contains(&format!("hash={}", expected_token)));
    }

    #[test]
    fn build_download_url_from_msg_static_url() {
        let d = MockDownloadable {
            direct_path: Some("/unused".into()),
            static_url: Some("https://static.cdn.example.com/vid/xyz".into()),
            media_key: None,
            file_sha256: Some(vec![5; 32]),
            file_enc_sha256: None,
            media_type: MediaType::Image,
        };
        let urls = DownloadUtils::build_download_url_from_msg(&d, &mock_media_conn()).unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://static.cdn.example.com/vid/xyz");
    }

    // -------------------------------------------------------------------
    // Tests for DownloadableThumbnail
    // -------------------------------------------------------------------

    #[test]
    fn thumbnail_trait_extended_text() {
        let etm = wa::message::ExtendedTextMessage {
            thumbnail_direct_path: Some("/thumb/path".into()),
            thumbnail_sha256: Some(vec![0xAA; 32]),
            thumbnail_enc_sha256: Some(vec![0xBB; 32]),
            media_key: Some(vec![0xCC; 32]),
            ..Default::default()
        };
        assert_eq!(etm.thumbnail_direct_path(), Some("/thumb/path"));
        assert_eq!(etm.thumbnail_media_type(), MediaType::LinkThumbnail);
        assert_eq!(etm.thumbnail_enc_sha256().unwrap().len(), 32);

        let urls =
            DownloadUtils::build_thumbnail_download_urls(&etm, &mock_media_conn()).unwrap();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("mms-type=thumbnail-link"));
    }

    #[test]
    fn thumbnail_trait_image_message() {
        let img = wa::message::ImageMessage {
            thumbnail_direct_path: Some("/img/thumb".into()),
            thumbnail_sha256: Some(vec![0x11; 32]),
            thumbnail_enc_sha256: Some(vec![0x22; 32]),
            media_key: Some(vec![0x33; 32]),
            ..Default::default()
        };
        assert_eq!(img.thumbnail_direct_path(), Some("/img/thumb"));
        assert_eq!(img.thumbnail_media_type(), MediaType::Image);
    }

    #[test]
    fn thumbnail_trait_video_message() {
        let vid = wa::message::VideoMessage {
            thumbnail_direct_path: Some("/vid/thumb".into()),
            thumbnail_sha256: Some(vec![0x44; 32]),
            thumbnail_enc_sha256: Some(vec![0x55; 32]),
            media_key: Some(vec![0x66; 32]),
            ..Default::default()
        };
        assert_eq!(vid.thumbnail_direct_path(), Some("/vid/thumb"));
        assert_eq!(vid.thumbnail_media_type(), MediaType::Image);
    }

    #[test]
    fn thumbnail_trait_document_message() {
        let doc = wa::message::DocumentMessage {
            thumbnail_direct_path: Some("/doc/thumb".into()),
            thumbnail_sha256: Some(vec![0x77; 32]),
            thumbnail_enc_sha256: Some(vec![0x88; 32]),
            media_key: Some(vec![0x99; 32]),
            ..Default::default()
        };
        assert_eq!(doc.thumbnail_direct_path(), Some("/doc/thumb"));
        assert_eq!(doc.thumbnail_media_type(), MediaType::Image);
    }

    #[test]
    fn thumbnail_missing_path_errors() {
        let etm = wa::message::ExtendedTextMessage {
            thumbnail_direct_path: None,
            thumbnail_enc_sha256: Some(vec![0xBB; 32]),
            media_key: Some(vec![0xCC; 32]),
            ..Default::default()
        };
        let err =
            DownloadUtils::build_thumbnail_download_urls(&etm, &mock_media_conn()).unwrap_err();
        assert!(err.to_string().contains("thumbnail_direct_path"));
    }

    // -------------------------------------------------------------------
    // Tests for download_any_ref
    // -------------------------------------------------------------------

    #[test]
    fn download_any_image() {
        let msg = wa::Message {
            image_message: Some(Box::new(wa::message::ImageMessage {
                direct_path: Some("/img/path".into()),
                media_key: Some(vec![1; 32]),
                file_sha256: Some(vec![2; 32]),
                file_enc_sha256: Some(vec![3; 32]),
                file_length: Some(4096),
                ..Default::default()
            })),
            ..Default::default()
        };
        let d = download_any_ref(&msg).unwrap();
        assert_eq!(d.app_info(), MediaType::Image);
        assert_eq!(d.direct_path(), Some("/img/path"));
    }

    #[test]
    fn download_any_video() {
        let msg = wa::Message {
            video_message: Some(Box::new(wa::message::VideoMessage {
                direct_path: Some("/vid/path".into()),
                media_key: Some(vec![1; 32]),
                ..Default::default()
            })),
            ..Default::default()
        };
        let d = download_any_ref(&msg).unwrap();
        assert_eq!(d.app_info(), MediaType::Video);
    }

    #[test]
    fn download_any_audio() {
        let msg = wa::Message {
            audio_message: Some(Box::new(wa::message::AudioMessage {
                direct_path: Some("/aud/path".into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let d = download_any_ref(&msg).unwrap();
        assert_eq!(d.app_info(), MediaType::Audio);
    }

    #[test]
    fn download_any_document() {
        let msg = wa::Message {
            document_message: Some(Box::new(wa::message::DocumentMessage {
                direct_path: Some("/doc/path".into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let d = download_any_ref(&msg).unwrap();
        assert_eq!(d.app_info(), MediaType::Document);
    }

    #[test]
    fn download_any_sticker() {
        let msg = wa::Message {
            sticker_message: Some(Box::new(wa::message::StickerMessage {
                direct_path: Some("/stk/path".into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let d = download_any_ref(&msg).unwrap();
        assert_eq!(d.app_info(), MediaType::Sticker);
    }

    #[test]
    fn download_any_empty_message() {
        let msg = wa::Message::default();
        let err = download_any_ref(&msg).unwrap_err();
        assert!(err.to_string().contains("No downloadable media"));
    }

    #[test]
    fn download_any_priority_order() {
        // When multiple media types present, image wins (checked first).
        let msg = wa::Message {
            image_message: Some(Box::new(wa::message::ImageMessage {
                direct_path: Some("/img".into()),
                ..Default::default()
            })),
            video_message: Some(Box::new(wa::message::VideoMessage {
                direct_path: Some("/vid".into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let d = download_any_ref(&msg).unwrap();
        assert_eq!(d.app_info(), MediaType::Image);
    }

    #[test]
    fn media_type_of_returns_correct_type() {
        let msg = wa::Message {
            audio_message: Some(Box::new(wa::message::AudioMessage::default())),
            ..Default::default()
        };
        assert_eq!(media_type_of(&msg), Some(MediaType::Audio));
    }

    #[test]
    fn media_type_of_empty_is_none() {
        let msg = wa::Message::default();
        assert_eq!(media_type_of(&msg), None);
    }

    // -------------------------------------------------------------------
    // Tests for decrypt_stream_to_file
    // -------------------------------------------------------------------

    #[test]
    fn decrypt_stream_to_file_roundtrip() {
        use crate::upload::encrypt_media;

        let plaintext = b"Decrypt to file test payload with some content.";
        let enc = encrypt_media(plaintext, MediaType::Image).unwrap();

        let tmp = std::env::temp_dir().join("wacore_test_decrypt_to_file.bin");
        let reader = std::io::Cursor::new(enc.data_to_upload.clone());
        let bytes = DownloadUtils::decrypt_stream_to_file(
            reader,
            &enc.media_key,
            MediaType::Image,
            &tmp,
        )
        .unwrap();

        assert_eq!(bytes, plaintext.len() as u64);
        let content = std::fs::read(&tmp).unwrap();
        assert_eq!(content, plaintext);

        std::fs::remove_file(&tmp).ok();
    }

    // -------------------------------------------------------------------
    // Tests for Downloadable trait implementations on protobuf types
    // -------------------------------------------------------------------

    #[test]
    fn downloadable_image_message() {
        let img = wa::message::ImageMessage {
            direct_path: Some("/enc/img".into()),
            media_key: Some(vec![1; 32]),
            file_sha256: Some(vec![2; 32]),
            file_enc_sha256: Some(vec![3; 32]),
            file_length: Some(9000),
            static_url: Some("https://cdn.example.com/static".into()),
            ..Default::default()
        };
        assert_eq!(img.direct_path(), Some("/enc/img"));
        assert_eq!(img.media_key().unwrap().len(), 32);
        assert_eq!(img.file_length(), Some(9000));
        assert_eq!(img.app_info(), MediaType::Image);
        assert_eq!(img.static_url(), Some("https://cdn.example.com/static"));
        assert!(img.is_encrypted());
    }

    #[test]
    fn downloadable_sticker_no_static_url() {
        let stk = wa::message::StickerMessage {
            direct_path: Some("/stk".into()),
            media_key: None,
            ..Default::default()
        };
        assert_eq!(stk.static_url(), None);
        assert!(!stk.is_encrypted());
        assert_eq!(stk.app_info(), MediaType::Sticker);
    }

    #[test]
    fn downloadable_external_blob() {
        let blob = ExternalBlobReference {
            direct_path: Some("/blob/path".into()),
            media_key: Some(vec![9; 32]),
            file_sha256: Some(vec![10; 32]),
            file_enc_sha256: Some(vec![11; 32]),
            file_size_bytes: Some(65536),
            ..Default::default()
        };
        assert_eq!(blob.app_info(), MediaType::AppState);
        assert_eq!(blob.file_length(), Some(65536));
    }

    #[test]
    fn downloadable_history_sync() {
        let hsn = HistorySyncNotification {
            direct_path: Some("/hist/path".into()),
            media_key: Some(vec![12; 32]),
            file_sha256: Some(vec![13; 32]),
            file_enc_sha256: Some(vec![14; 32]),
            file_length: Some(1048576),
            ..Default::default()
        };
        assert_eq!(hsn.app_info(), MediaType::History);
        assert_eq!(hsn.file_length(), Some(1048576));
    }
}
