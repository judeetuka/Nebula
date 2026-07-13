//! Pre-key management for Signal Protocol.
//!
//! Protocol types are defined in `wacore::iq::prekeys`.
//!
//! Rate limiting mirrors whatsmeow: a 10-minute cooldown between upload calls
//! prevents race-condition spam when multiple "prekeys low" notifications arrive
//! near-simultaneously. On first registration (db count = 0, server count = 0),
//! 812 keys are uploaded in a single batch.

use crate::client::Client;
use anyhow;
use log;
use rand::TryRngCore;
use std::time::{Duration, Instant};
use wacore::iq::prekeys::{PreKeyCountSpec, PreKeyFetchSpec, PreKeyUploadSpec};
use wacore::libsignal::protocol::{KeyPair, PreKeyBundle, PublicKey};
use wacore::libsignal::store::record_helpers::new_pre_key_record;
use wacore_binary::jid::Jid;

pub use wacore::prekeys::PreKeyUtils;

/// Number of pre-keys to upload per batch under normal operation.
const WANTED_PRE_KEY_COUNT: usize = 50;

/// Threshold below which we trigger a prekey upload.
/// Matches whatsmeow's `MinPreKeyCount`.
const MIN_PRE_KEY_COUNT: usize = 30;

/// Number of pre-keys to upload on first registration when the server has zero keys
/// and the local database has zero keys. Matches whatsmeow's initial upload count.
const INITIAL_PRE_KEY_COUNT: usize = 812;

/// Minimum interval between consecutive prekey upload operations.
/// Matches whatsmeow's 10-minute cooldown.
const PREKEY_UPLOAD_COOLDOWN: Duration = Duration::from_secs(10 * 60);

impl Client {
    pub(crate) async fn fetch_pre_keys(
        &self,
        jids: &[Jid],
        reason: Option<&str>,
    ) -> Result<std::collections::HashMap<Jid, PreKeyBundle>, anyhow::Error> {
        let spec = match reason {
            Some(r) => PreKeyFetchSpec::with_reason(jids.to_vec(), r),
            None => PreKeyFetchSpec::new(jids.to_vec()),
        };

        let bundles = self.execute(spec).await?;

        for jid in bundles.keys() {
            log::debug!("Successfully parsed pre-key bundle for {jid}");
        }

        Ok(bundles)
    }

    /// Query the WhatsApp server for how many pre-keys it currently has for this device.
    pub(crate) async fn get_server_pre_key_count(&self) -> Result<usize, crate::request::IqError> {
        let response = self.execute(PreKeyCountSpec::new()).await?;
        Ok(response.count)
    }

    /// Check whether the prekey upload cooldown has elapsed.
    ///
    /// Returns `true` if an upload should proceed (enough time has passed),
    /// `false` if the cooldown is still active.
    async fn should_upload_prekeys(&self) -> bool {
        let guard = self.last_prekey_upload.lock().await;
        match *guard {
            Some(last) => last.elapsed() >= PREKEY_UPLOAD_COOLDOWN,
            None => true, // Never uploaded before — always proceed
        }
    }

    /// Record that a prekey upload just completed successfully.
    async fn mark_prekey_upload_time(&self) {
        let mut guard = self.last_prekey_upload.lock().await;
        *guard = Some(Instant::now());
    }

    /// Ensure the server has at least `MIN_PRE_KEY_COUNT` pre-keys, uploading a
    /// fresh batch when necessary.
    ///
    /// Rate limiting (matches whatsmeow):
    /// - Serialised via `upload_prekeys_lock` to prevent concurrent uploads.
    /// - 10-minute cooldown: if less than 10 minutes have elapsed since the last
    ///   successful upload *and* the server already has `>= WANTED_PRE_KEY_COUNT`
    ///   keys, the request is skipped (likely a race condition).
    /// - On first registration (no local keys, no server keys) an initial batch
    ///   of 812 keys is uploaded instead of the normal 50.
    pub(crate) async fn upload_pre_keys(&self) -> Result<(), anyhow::Error> {
        // Serialise uploads — only one in-flight at a time.
        let _lock = self.upload_prekeys_lock.lock().await;

        // Rate-limit check: if within cooldown, verify the server really is low.
        if !self.should_upload_prekeys().await {
            let server_count = match self.get_server_pre_key_count().await {
                Ok(c) => c,
                Err(e) => return Err(anyhow::anyhow!(e)),
            };
            if server_count >= WANTED_PRE_KEY_COUNT {
                log::debug!(
                    "Canceling prekey upload: cooldown active and server has {} keys (>= {})",
                    server_count,
                    WANTED_PRE_KEY_COUNT,
                );
                return Ok(());
            }
        }

        let server_count = match self.get_server_pre_key_count().await {
            Ok(c) => c,
            Err(e) => return Err(anyhow::anyhow!(e)),
        };

        if server_count >= MIN_PRE_KEY_COUNT {
            log::debug!("Server has {} pre-keys, no upload needed (threshold = {}).", server_count, MIN_PRE_KEY_COUNT);
            return Ok(());
        }

        log::debug!("Server has {} pre-keys (below {}), uploading more.", server_count, MIN_PRE_KEY_COUNT);

        let device_snapshot = self.persistence_manager.get_device_snapshot().await;
        let device_store = self.persistence_manager.get_device_arc().await;

        // Clone the backend Arc and drop the guard early to reduce lock contention.
        // This allows other tasks to access the device while we perform potentially
        // long-running backend operations (loops with many iterations).
        let backend = {
            let device_guard = device_store.read().await;
            device_guard.backend.clone()
        };

        // Determine local DB prekey count to detect first-registration scenario.
        let db_count = self.count_local_prekeys(&*backend).await;

        // First registration: no keys locally and server reports zero.
        let is_initial = db_count == 0 && server_count == 0;
        let wanted = if is_initial {
            log::info!("First registration detected — uploading {} initial pre-keys", INITIAL_PRE_KEY_COUNT);
            INITIAL_PRE_KEY_COUNT
        } else {
            WANTED_PRE_KEY_COUNT
        };

        // Step 1: Try to get existing unuploaded keys from storage
        let mut keys_to_upload = Vec::with_capacity(wanted);
        let mut key_pairs_to_upload = Vec::with_capacity(wanted);

        // Check if we have existing unuploaded keys by trying IDs sequentially
        // We'll check a reasonable range to find existing keys
        let found_count = 0;
        for id in 1..=1000u32 {
            if found_count >= wanted {
                break;
            }

            if let Ok(Some(_record)) = backend.load_prekey(id).await {
                // Check if this key was already uploaded by seeing if it exists on server
                // For simplicity, assume unuploaded keys have a specific pattern or we track separately
                // For now, we'll use existing keys if available but generate new ones with sequential IDs
                break; // We'll generate new ones with better tracking
            }
        }

        // Step 2: Generate new keys with sequential IDs to avoid collisions
        let mut highest_existing_id = 0u32;

        // Find the highest existing pre-key ID to start from
        for id in 1..=16777215u32 {
            match backend.load_prekey(id).await {
                Ok(Some(_)) => {
                    highest_existing_id = id;
                }
                Ok(None) => {
                    break; // Found first gap
                }
                Err(e) => {
                    // Don't silently ignore DB errors - could lead to ID collisions
                    return Err(anyhow::anyhow!("Failed to load prekey {}: {}", id, e));
                }
            }
        }

        let start_id = highest_existing_id + 1;

        for i in 0..wanted {
            let pre_key_id = start_id + i as u32;

            // Ensure we don't exceed the valid range (1 to 0xFFFFFF)
            if pre_key_id > 16777215 {
                log::warn!(
                    "Pre-key ID {} exceeds maximum range, wrapping around",
                    pre_key_id
                );
                break;
            }

            let key_pair = KeyPair::generate(&mut rand::rngs::OsRng.unwrap_err());
            let pre_key_record = new_pre_key_record(pre_key_id, &key_pair);

            keys_to_upload.push((pre_key_id, pre_key_record));
            key_pairs_to_upload.push((pre_key_id, key_pair));
        }

        if keys_to_upload.is_empty() {
            log::warn!("No pre-keys available to upload");
            return Ok(());
        }

        // Step 3: Build upload request using type-safe IqSpec
        let pre_key_pairs: Vec<(u32, PublicKey)> = key_pairs_to_upload
            .iter()
            .map(|(id, key_pair)| (*id, key_pair.public_key))
            .collect();

        let spec = PreKeyUploadSpec::new(
            device_snapshot.registration_id,
            device_snapshot.identity_key.public_key,
            device_snapshot.signed_pre_key_id,
            device_snapshot.signed_pre_key.public_key,
            device_snapshot.signed_pre_key_signature.to_vec(),
            pre_key_pairs,
        );

        // Step 4: Send IQ to upload pre-keys
        self.execute(spec).await?;

        // Step 5: Store the new pre-keys using existing backend interface
        for (id, record) in keys_to_upload {
            // Mark as uploaded since the IQ was successful
            use prost::Message;
            let record_bytes = record.encode_to_vec();
            if let Err(e) = backend.store_prekey(id, &record_bytes, true).await {
                log::warn!("Failed to store prekey id {}: {:?}", id, e);
            }
        }

        // Record successful upload timestamp for rate limiting.
        self.mark_prekey_upload_time().await;

        log::debug!(
            "Successfully uploaded {} new pre-keys with sequential IDs starting from {}.",
            key_pairs_to_upload.len(),
            start_id
        );

        Ok(())
    }

    /// Count how many pre-keys exist in local storage.
    ///
    /// Scans IDs 1..=1000 (reasonable upper bound for stored keys) and counts
    /// non-None results. Used to detect first-registration (db_count == 0).
    async fn count_local_prekeys(
        &self,
        backend: &dyn wacore::store::traits::SignalStore,
    ) -> usize {
        let mut count = 0;
        for id in 1..=1000u32 {
            match backend.load_prekey(id).await {
                Ok(Some(_)) => count += 1,
                Ok(None) => break, // Sequential IDs, first gap means no more keys
                Err(_) => break,
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_match_whatsmeow() {
        // whatsmeow: WantedPreKeyCount = 50
        assert_eq!(WANTED_PRE_KEY_COUNT, 50);
        // whatsmeow: MinPreKeyCount = 30 (updated from 5 to match Go source)
        assert_eq!(MIN_PRE_KEY_COUNT, 30);
        // whatsmeow: initialUpload = 812
        assert_eq!(INITIAL_PRE_KEY_COUNT, 812);
        // whatsmeow: 10 * time.Minute
        assert_eq!(PREKEY_UPLOAD_COOLDOWN, Duration::from_secs(600));
    }

    #[test]
    fn test_cooldown_duration_is_ten_minutes() {
        assert_eq!(PREKEY_UPLOAD_COOLDOWN.as_secs(), 600);
        assert_eq!(PREKEY_UPLOAD_COOLDOWN.as_secs() / 60, 10);
    }

    #[tokio::test]
    async fn test_should_upload_prekeys_when_never_uploaded() {
        let client = crate::test_utils::create_test_client().await;
        // No upload has occurred yet — should return true.
        assert!(client.should_upload_prekeys().await);
    }

    #[tokio::test]
    async fn test_should_not_upload_prekeys_within_cooldown() {
        let client = crate::test_utils::create_test_client().await;
        // Simulate a recent upload.
        {
            let mut guard = client.last_prekey_upload.lock().await;
            *guard = Some(Instant::now());
        }
        // Within cooldown — should return false.
        assert!(!client.should_upload_prekeys().await);
    }

    #[tokio::test]
    async fn test_should_upload_prekeys_after_cooldown_expires() {
        let client = crate::test_utils::create_test_client().await;
        // Simulate an upload that happened well beyond the cooldown.
        {
            let mut guard = client.last_prekey_upload.lock().await;
            *guard = Some(Instant::now() - PREKEY_UPLOAD_COOLDOWN - Duration::from_secs(1));
        }
        // Cooldown expired — should return true.
        assert!(client.should_upload_prekeys().await);
    }

    #[tokio::test]
    async fn test_mark_prekey_upload_time() {
        let client = crate::test_utils::create_test_client().await;
        // Initially None.
        {
            let guard = client.last_prekey_upload.lock().await;
            assert!(guard.is_none());
        }
        // After marking, should be Some and recent.
        client.mark_prekey_upload_time().await;
        {
            let guard = client.last_prekey_upload.lock().await;
            let last = guard.expect("should be Some after marking");
            assert!(last.elapsed() < Duration::from_secs(1));
        }
    }

    #[tokio::test]
    async fn test_upload_prekeys_lock_serialises() {
        let client = crate::test_utils::create_test_client().await;
        // Acquire the lock manually to prove serialisation.
        let _guard = client.upload_prekeys_lock.lock().await;
        // try_lock should fail while we hold it.
        assert!(client.upload_prekeys_lock.try_lock().is_err());
    }
}
