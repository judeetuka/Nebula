use crate::types::events::{Event, LazyConversation};
use bytes::Bytes;
use std::collections::HashSet;
use std::sync::Arc;
use wacore::history_sync::process_history_sync;
use waproto::whatsapp::message::HistorySyncNotification;

use crate::client::Client;

impl Client {
    pub(crate) async fn handle_history_sync(
        self: &Arc<Self>,
        message_id: String,
        notification: HistorySyncNotification,
    ) {
        if self.skip_history_sync_enabled() {
            log::debug!(
                "Skipping history sync for message {} (Type: {:?})",
                message_id,
                notification.sync_type()
            );
            // Send receipt so the phone considers this chunk delivered and stops
            // retrying. This intentionally diverges from WhatsApp Web's AB prop
            // drop path (which sends no receipt) because bots will never process
            // history, and without the receipt the phone would keep re-uploading
            // blobs that will never be consumed.
            self.send_protocol_receipt(
                message_id,
                crate::types::presence::ReceiptType::HistorySync,
            )
            .await;
            return;
        }

        // Enqueue a MajorSyncTask for the dedicated sync worker to consume.
        let task = crate::sync_task::MajorSyncTask::HistorySync {
            message_id,
            notification: Box::new(notification),
        };
        if let Err(e) = self.major_sync_task_sender.send(task).await {
            log::error!("Failed to enqueue history sync task: {e}");
        }
    }

    /// Process history sync with streaming and lazy parsing.
    ///
    /// Memory efficient: raw bytes are wrapped in LazyConversation and only
    /// parsed if the event handler actually accesses the conversation data.
    pub(crate) async fn process_history_sync_task(
        self: &Arc<Self>,
        message_id: String,
        mut notification: HistorySyncNotification,
    ) {
        log::info!(
            "Processing history sync for message {} (Size: {}, Type: {:?})",
            message_id,
            notification.file_length(),
            notification.sync_type()
        );

        self.send_protocol_receipt(
            message_id.clone(),
            crate::types::presence::ReceiptType::HistorySync,
        )
        .await;

        // Use take() to avoid cloning large payloads - moves ownership instead
        let compressed_data = if let Some(inline_payload) =
            notification.initial_hist_bootstrap_inline_payload.take()
        {
            log::info!(
                "Found inline history sync payload ({} bytes). Using directly.",
                inline_payload.len()
            );
            inline_payload
        } else {
            log::info!("Downloading external history sync blob...");
            match self.download(&notification).await {
                Ok(data) => {
                    log::info!("Successfully downloaded history sync blob.");
                    data
                }
                Err(e) => {
                    log::error!("Failed to download history sync blob: {:?}", e);
                    return;
                }
            }
        };

        // Get own user for pushname extraction (moved into blocking task, no clone needed)
        let own_user = {
            let device_snapshot = self.persistence_manager.get_device_snapshot().await;
            device_snapshot.pn.as_ref().map(|j| j.to_non_ad().user)
        };

        // Check if anyone is listening for events
        let has_listeners = self.core.event_bus.has_handlers();

        let mut seen_phones = HashSet::new();
        let parse_result = if has_listeners {
            // Use a bounded channel to stream raw conversation bytes as Bytes (zero-copy)
            let (tx, mut rx) = tokio::sync::mpsc::channel::<Bytes>(16);

            // Run streaming parsing in blocking thread
            // own_user is moved directly, no clone needed
            let parse_handle = tokio::task::spawn_blocking(move || {
                let own_user_ref = own_user.as_deref();

                // Streaming: decompresses and extracts raw bytes incrementally
                // No parsing happens here - just raw byte extraction
                // Uses Bytes for zero-copy reference counting
                process_history_sync(
                    &compressed_data,
                    own_user_ref,
                    Some(|raw_bytes: Bytes| {
                        // Send Bytes through channel (zero-copy clone)
                        let _ = tx.blocking_send(raw_bytes);
                    }),
                )
                // tx dropped here, closing channel
            });

            // Receive and dispatch lazy conversations as they come in
            let mut conv_count = 0usize;
            while let Some(raw_bytes) = rx.recv().await {
                conv_count += 1;
                if conv_count.is_multiple_of(25) {
                    log::info!("History sync progress: {conv_count} conversations processed...");
                }
                // Wrap Bytes in LazyConversation using from_bytes (true zero-copy)
                let lazy_conv = LazyConversation::from_bytes(raw_bytes);

                // Extract LID-PN mapping from conversation fields (pn_jid, lid_jid)
                // Skip if we already stored a mapping for this phone number (dedup)
                if let Some(conv) = lazy_conv.get() {
                    if let (Some(pn_jid), Some(lid_jid)) =
                        (&conv.pn_jid, &conv.lid_jid)
                    {
                        let pn_user = pn_jid.split('@').next().unwrap_or(pn_jid);
                        let lid_user = lid_jid.split('@').next().unwrap_or(lid_jid);
                        if !pn_user.is_empty() && !lid_user.is_empty()
                            && seen_phones.insert(pn_user.to_string())
                        {
                            if let Err(e) = self
                                .add_lid_pn_mapping(
                                    lid_user,
                                    pn_user,
                                    crate::lid_pn_cache::LearningSource::MigrationSyncLatest,
                                )
                                .await
                            {
                                log::warn!(
                                    "Failed to store conversation LID-PN mapping {lid_user} <-> {pn_user}: {e}"
                                );
                            }
                        }
                    }
                }

                self.core.event_bus.dispatch(&Event::JoinedGroup(lazy_conv));
            }

            // Wait for parsing to complete
            parse_handle.await
        } else {
            // No listeners - skip conversation processing entirely
            log::debug!("No event handlers registered, skipping conversation processing");

            // own_user is moved directly, no clone needed
            tokio::task::spawn_blocking(move || {
                let own_user_ref = own_user.as_deref();

                // Pass None for callback - conversations are skipped at protobuf level
                process_history_sync::<fn(Bytes)>(&compressed_data, own_user_ref, None)
            })
            .await
        };

        match parse_result {
            Ok(Ok(sync_result)) => {
                log::info!(
                    "Successfully processed HistorySync (message {message_id}); {} conversations, {} LID-PN mappings",
                    sync_result.conversations_processed,
                    sync_result.lid_pn_mappings.len()
                );

                // Store LID-PN mappings from field 15 (phone_number_to_lid_mappings)
                // Mirrors whatsmeow's storeHistoricalPNLIDMappings()
                // Skip phone numbers already seen from conversation extraction (dedup)
                for (pn_jid, lid_jid) in &sync_result.lid_pn_mappings {
                    let pn_user = pn_jid.split('@').next().unwrap_or(pn_jid);
                    let lid_user = lid_jid.split('@').next().unwrap_or(lid_jid);
                    if !pn_user.is_empty() && !lid_user.is_empty()
                        && seen_phones.insert(pn_user.to_string())
                    {
                        if let Err(e) = self
                            .add_lid_pn_mapping(
                                lid_user,
                                pn_user,
                                crate::lid_pn_cache::LearningSource::MigrationSyncLatest,
                            )
                            .await
                        {
                            log::warn!(
                                "Failed to store historical LID-PN mapping {lid_user} <-> {pn_user}: {e}"
                            );
                        }
                    }
                }

                // Update own push name if found
                if let Some(new_name) = sync_result.own_pushname {
                    log::info!("Updating own push name from history sync to '{new_name}'");
                    self.update_push_name_and_notify(new_name).await;
                }
            }
            Ok(Err(e)) => {
                log::error!("Failed to process HistorySync data: {:?}", e);
            }
            Err(e) => {
                log::error!("History sync blocking task panicked: {:?}", e);
            }
        }
    }
}
