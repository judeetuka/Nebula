use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use prost::Message;
use tokio::sync::Mutex;
use wacore::appstate::hash::HashState;
use wacore::appstate::keys::ExpandedAppStateKeys;
use wacore::appstate::patch_decode::{PatchList, WAPatchName, parse_patch_list};
use wacore::appstate::{
    build_patch, collect_key_ids_from_patch_list, encode_mutation, expand_app_state_keys,
    process_patch, process_snapshot,
};
use wacore::store::traits::Backend;
use wacore_binary::node::Node;
use waproto::whatsapp as wa;

// Re-export Mutation from wacore for backwards compatibility
pub use wacore::appstate::Mutation;

// ---------------------------------------------------------------------------
// Index constants (match whatsmeow appstate/keys.go)
// ---------------------------------------------------------------------------

// regular_low indexes
pub const INDEX_PIN: &str = "pin_v1";
pub const INDEX_ARCHIVE: &str = "archive";
pub const INDEX_MARK_CHAT_AS_READ: &str = "markChatAsRead";
pub const INDEX_SETTING_UNARCHIVE_CHATS: &str = "setting_unarchiveChats";

// regular indexes
pub const INDEX_LABEL_ASSOCIATION_MESSAGE: &str = "label_message";
pub const INDEX_LABEL_EDIT: &str = "label_edit";
pub const INDEX_LABEL_ASSOCIATION_CHAT: &str = "label_jid";

// regular_high indexes
pub const INDEX_STAR: &str = "star";
pub const INDEX_MUTE: &str = "mute";
pub const INDEX_DELETE_MESSAGE_FOR_ME: &str = "deleteMessageForMe";
pub const INDEX_CLEAR_CHAT: &str = "clearChat";
pub const INDEX_DELETE_CHAT: &str = "deleteChat";
pub const INDEX_USER_STATUS_MUTE: &str = "userStatusMute";

// critical_unblock_low indexes
pub const INDEX_CONTACT: &str = "contact";

// critical_block indexes
pub const INDEX_SETTING_PUSH_NAME: &str = "setting_pushName";

// ---------------------------------------------------------------------------
// MutationInfo / PatchInfo — mirrors Go appstate.MutationInfo / PatchInfo
// ---------------------------------------------------------------------------

/// Information about a single mutation to the app state.
#[derive(Debug, Clone)]
pub struct MutationInfo {
    /// The index components (e.g. `["mute", "120363@s.whatsapp.net"]`).
    pub index: Vec<String>,
    /// The version for this mutation type (static per index kind).
    pub version: i32,
    /// The action payload.
    pub value: wa::SyncActionValue,
}

/// A patch to be sent to the server, containing one or more mutations in the
/// same app state collection.
#[derive(Debug, Clone)]
pub struct PatchInfo {
    /// Timestamp for the patch. If `None`, the current time is used.
    pub timestamp: Option<SystemTime>,
    /// The app state collection type.
    pub patch_type: WAPatchName,
    /// The individual mutations in this patch.
    pub mutations: Vec<MutationInfo>,
}

// ---------------------------------------------------------------------------
// Mutation Builders — match whatsmeow appstate/encode.go
// ---------------------------------------------------------------------------

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn new_message_range(
    last_message_timestamp: Option<i64>,
    last_message_key: Option<wa::MessageKey>,
) -> wa::sync_action_value::SyncActionMessageRange {
    let ts = last_message_timestamp.unwrap_or_else(now_secs);
    let messages = if let Some(key) = last_message_key {
        vec![wa::sync_action_value::SyncActionMessage {
            key: Some(key),
            timestamp: Some(ts),
        }]
    } else {
        vec![]
    };
    wa::sync_action_value::SyncActionMessageRange {
        last_message_timestamp: Some(ts),
        last_system_message_timestamp: None,
        messages,
    }
}

/// Build an app state patch for muting or unmuting a chat.
///
/// If `mute` is true and `mute_duration` is `None`, the chat is muted forever.
/// If `mute_duration` is `Some(duration)`, the chat is muted until `now + duration`.
pub fn build_mute(target: &str, mute: bool, mute_duration: Option<Duration>) -> PatchInfo {
    let mute_end_timestamp = if mute {
        match mute_duration {
            Some(d) if !d.is_zero() => Some(now_millis() + d.as_millis() as i64),
            _ => Some(-1), // muted forever
        }
    } else {
        None
    };
    build_mute_abs(target, mute, mute_end_timestamp)
}

/// Build a mute patch with an absolute mute-end timestamp (milliseconds).
pub fn build_mute_abs(target: &str, mute: bool, mute_end_timestamp: Option<i64>) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularHigh,
        mutations: vec![MutationInfo {
            index: vec![INDEX_MUTE.to_string(), target.to_string()],
            version: 2,
            value: wa::SyncActionValue {
                mute_action: Some(wa::sync_action_value::MuteAction {
                    muted: Some(mute),
                    mute_end_timestamp,
                    auto_muted: None,
                }),
                ..Default::default()
            },
        }],
    }
}

fn new_pin_mutation(target: &str, pin: bool) -> MutationInfo {
    MutationInfo {
        index: vec![INDEX_PIN.to_string(), target.to_string()],
        version: 5,
        value: wa::SyncActionValue {
            pin_action: Some(wa::sync_action_value::PinAction { pinned: Some(pin) }),
            ..Default::default()
        },
    }
}

/// Build an app state patch for pinning or unpinning a chat.
pub fn build_pin(target: &str, pin: bool) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularLow,
        mutations: vec![new_pin_mutation(target, pin)],
    }
}

/// Build an app state patch for archiving or unarchiving a chat.
///
/// Archiving a chat will also unpin it automatically.
pub fn build_archive(
    target: &str,
    archive: bool,
    last_message_timestamp: Option<i64>,
    last_message_key: Option<wa::MessageKey>,
) -> PatchInfo {
    let archive_mutation = MutationInfo {
        index: vec![INDEX_ARCHIVE.to_string(), target.to_string()],
        version: 3,
        value: wa::SyncActionValue {
            archive_chat_action: Some(wa::sync_action_value::ArchiveChatAction {
                archived: Some(archive),
                message_range: Some(new_message_range(last_message_timestamp, last_message_key)),
            }),
            ..Default::default()
        },
    };

    let mut mutations = vec![archive_mutation];
    if archive {
        // Archiving also unpins
        mutations.push(new_pin_mutation(target, false));
    }

    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularLow,
        mutations,
    }
}

/// Build an app state patch for starring or unstarring a message.
///
/// `target` is the chat JID, `sender` is the sender JID (or same as target for
/// 1:1 chats), `message_id` is the message ID, `from_me` indicates direction.
pub fn build_star(
    target: &str,
    sender: &str,
    message_id: &str,
    from_me: bool,
    starred: bool,
) -> PatchInfo {
    let is_from_me = if from_me { "1" } else { "0" };
    // If target and sender have the same user part, use "0" for sender
    let sender_str = if target == sender {
        "0".to_string()
    } else {
        sender.to_string()
    };

    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularHigh,
        mutations: vec![MutationInfo {
            index: vec![
                INDEX_STAR.to_string(),
                target.to_string(),
                message_id.to_string(),
                is_from_me.to_string(),
                sender_str,
            ],
            version: 2,
            value: wa::SyncActionValue {
                star_action: Some(wa::sync_action_value::StarAction {
                    starred: Some(starred),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for editing a label.
pub fn build_label_edit(
    label_id: &str,
    label_name: &str,
    label_color: i32,
    deleted: bool,
) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::Regular,
        mutations: vec![MutationInfo {
            index: vec![INDEX_LABEL_EDIT.to_string(), label_id.to_string()],
            version: 3,
            value: wa::SyncActionValue {
                label_edit_action: Some(wa::sync_action_value::LabelEditAction {
                    name: Some(label_name.to_string()),
                    color: Some(label_color),
                    deleted: Some(deleted),
                    ..Default::default()
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for labeling or unlabeling a chat.
pub fn build_label_chat(target: &str, label_id: &str, labeled: bool) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::Regular,
        mutations: vec![MutationInfo {
            index: vec![
                INDEX_LABEL_ASSOCIATION_CHAT.to_string(),
                label_id.to_string(),
                target.to_string(),
            ],
            version: 3,
            value: wa::SyncActionValue {
                label_association_action: Some(wa::sync_action_value::LabelAssociationAction {
                    labeled: Some(labeled),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for labeling or unlabeling a message.
pub fn build_label_message(
    target: &str,
    label_id: &str,
    message_id: &str,
    labeled: bool,
) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::Regular,
        mutations: vec![MutationInfo {
            index: vec![
                INDEX_LABEL_ASSOCIATION_MESSAGE.to_string(),
                label_id.to_string(),
                target.to_string(),
                message_id.to_string(),
                "0".to_string(),
                "0".to_string(),
            ],
            version: 3,
            value: wa::SyncActionValue {
                label_association_action: Some(wa::sync_action_value::LabelAssociationAction {
                    labeled: Some(labeled),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for marking a chat as read or unread.
pub fn build_mark_chat_as_read(
    target: &str,
    read: bool,
    last_message_timestamp: Option<i64>,
    last_message_key: Option<wa::MessageKey>,
) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularLow,
        mutations: vec![MutationInfo {
            index: vec![INDEX_MARK_CHAT_AS_READ.to_string(), target.to_string()],
            version: 3,
            value: wa::SyncActionValue {
                mark_chat_as_read_action: Some(wa::sync_action_value::MarkChatAsReadAction {
                    read: Some(read),
                    message_range: Some(new_message_range(
                        last_message_timestamp,
                        last_message_key,
                    )),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for deleting a chat.
pub fn build_delete_chat(
    target: &str,
    last_message_timestamp: Option<i64>,
    last_message_key: Option<wa::MessageKey>,
    delete_media: bool,
) -> PatchInfo {
    let delete_media_str = if delete_media { "1" } else { "0" };
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularHigh,
        mutations: vec![MutationInfo {
            index: vec![
                INDEX_DELETE_CHAT.to_string(),
                target.to_string(),
                delete_media_str.to_string(),
            ],
            version: 6,
            value: wa::SyncActionValue {
                delete_chat_action: Some(wa::sync_action_value::DeleteChatAction {
                    message_range: Some(new_message_range(
                        last_message_timestamp,
                        last_message_key,
                    )),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for setting the push name.
pub fn build_setting_push_name(push_name: &str) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::CriticalBlock,
        mutations: vec![MutationInfo {
            index: vec![INDEX_SETTING_PUSH_NAME.to_string()],
            version: 1,
            value: wa::SyncActionValue {
                push_name_setting: Some(wa::sync_action_value::PushNameSetting {
                    name: Some(push_name.to_string()),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for deleting a message locally (delete for me).
///
/// `target` is the chat JID, `sender` is the message sender JID (or same as
/// target for 1:1 chats), `message_id` is the message ID, `from_me`
/// indicates direction, and `delete_media` controls whether media is deleted.
pub fn build_delete_message_for_me(
    target: &str,
    sender: &str,
    message_id: &str,
    from_me: bool,
    delete_media: bool,
    message_timestamp: Option<i64>,
) -> PatchInfo {
    let is_from_me = if from_me { "1" } else { "0" };
    let sender_str = if target == sender {
        "0".to_string()
    } else {
        sender.to_string()
    };

    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularHigh,
        mutations: vec![MutationInfo {
            index: vec![
                INDEX_DELETE_MESSAGE_FOR_ME.to_string(),
                target.to_string(),
                message_id.to_string(),
                is_from_me.to_string(),
                sender_str,
            ],
            version: 3,
            value: wa::SyncActionValue {
                delete_message_for_me_action: Some(
                    wa::sync_action_value::DeleteMessageForMeAction {
                        delete_media: Some(delete_media),
                        message_timestamp,
                    },
                ),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for clearing a chat.
///
/// The optional `last_message_timestamp` and `last_message_key` help the
/// server determine which messages to clear. If `delete_media` is true,
/// media files are also removed.
pub fn build_clear_chat(
    target: &str,
    last_message_timestamp: Option<i64>,
    last_message_key: Option<wa::MessageKey>,
    delete_media: bool,
) -> PatchInfo {
    let delete_media_str = if delete_media { "1" } else { "0" };
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularHigh,
        mutations: vec![MutationInfo {
            index: vec![
                INDEX_CLEAR_CHAT.to_string(),
                target.to_string(),
                delete_media_str.to_string(),
            ],
            version: 7,
            value: wa::SyncActionValue {
                clear_chat_action: Some(wa::sync_action_value::ClearChatAction {
                    message_range: Some(new_message_range(
                        last_message_timestamp,
                        last_message_key,
                    )),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for updating a contact name.
pub fn build_contact(target: &str, full_name: &str, first_name: &str) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::CriticalUnblockLow,
        mutations: vec![MutationInfo {
            index: vec![INDEX_CONTACT.to_string(), target.to_string()],
            version: 2,
            value: wa::SyncActionValue {
                contact_action: Some(wa::sync_action_value::ContactAction {
                    full_name: Some(full_name.to_string()),
                    first_name: Some(first_name.to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for the "unarchive chats" setting.
///
/// When enabled, receiving a new message in an archived chat will
/// automatically unarchive it.
pub fn build_setting_unarchive_chats(unarchive: bool) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularLow,
        mutations: vec![MutationInfo {
            index: vec![INDEX_SETTING_UNARCHIVE_CHATS.to_string()],
            version: 1,
            value: wa::SyncActionValue {
                unarchive_chats_setting: Some(wa::sync_action_value::UnarchiveChatsSetting {
                    unarchive_chats: Some(unarchive),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build an app state patch for muting or unmuting a contact's status updates.
pub fn build_user_status_mute(target: &str, muted: bool) -> PatchInfo {
    PatchInfo {
        timestamp: None,
        patch_type: WAPatchName::RegularHigh,
        mutations: vec![MutationInfo {
            index: vec![INDEX_USER_STATUS_MUTE.to_string(), target.to_string()],
            version: 2,
            value: wa::SyncActionValue {
                user_status_mute_action: Some(wa::sync_action_value::UserStatusMuteAction {
                    muted: Some(muted),
                }),
                ..Default::default()
            },
        }],
    }
}

/// Build a `ProtocolMessage` requesting app state sync keys from the primary
/// device. The caller should send this via `send_peer_message`.
pub fn build_app_state_key_request(key_ids: &[Vec<u8>]) -> wa::Message {
    let proto_key_ids: Vec<wa::message::AppStateSyncKeyId> = key_ids
        .iter()
        .map(|id| wa::message::AppStateSyncKeyId {
            key_id: Some(id.clone()),
        })
        .collect();

    wa::Message {
        protocol_message: Some(Box::new(wa::message::ProtocolMessage {
            r#type: Some(
                wa::message::protocol_message::Type::AppStateSyncKeyRequest as i32,
            ),
            app_state_sync_key_request: Some(wa::message::AppStateSyncKeyRequest {
                key_ids: proto_key_ids,
            }),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Build a `ProtocolMessage` to notify the primary device of a fatal app state
/// exception. Sending this will cause all linked devices to be logged out.
pub fn build_fatal_app_state_exception_notification(
    collections: &[WAPatchName],
) -> wa::Message {
    let names: Vec<String> = collections.iter().map(|c| c.as_str().to_string()).collect();
    wa::Message {
        protocol_message: Some(Box::new(wa::message::ProtocolMessage {
            r#type: Some(
                wa::message::protocol_message::Type::AppStateFatalExceptionNotification as i32,
            ),
            app_state_fatal_exception_notification: Some(
                wa::message::AppStateFatalExceptionNotification {
                    collection_names: names,
                    timestamp: Some(now_millis()),
                },
            ),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Build a `ProtocolMessage` requesting the primary device to send an
/// unencrypted copy of an app state collection for recovery.
pub fn build_app_state_recovery_request(collection: WAPatchName) -> wa::Message {
    wa::Message {
        protocol_message: Some(Box::new(wa::message::ProtocolMessage {
            r#type: Some(
                wa::message::protocol_message::Type::PeerDataOperationRequestMessage as i32,
            ),
            peer_data_operation_request_message: Some(
                wa::message::PeerDataOperationRequestMessage {
                    peer_data_operation_request_type: Some(
                        wa::message::PeerDataOperationRequestType::CompanionSyncdSnapshotFatalRecovery as i32,
                    ),
                    syncd_collection_fatal_recovery_request: Some(
                        wa::message::peer_data_operation_request_message::SyncDCollectionFatalRecoveryRequest {
                            collection_name: Some(collection.as_str().to_string()),
                            timestamp: Some(now_secs()),
                        },
                    ),
                    ..Default::default()
                },
            ),
            ..Default::default()
        })),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// AppStateProcessor
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppStateProcessor {
    backend: Arc<dyn Backend>,
    key_cache: Arc<Mutex<HashMap<String, ExpandedAppStateKeys>>>,
}

impl AppStateProcessor {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self {
            backend,
            key_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn get_app_state_key(&self, key_id: &[u8]) -> Result<ExpandedAppStateKeys> {
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD_NO_PAD;
        let id_b64 = STANDARD_NO_PAD.encode(key_id);
        if let Some(cached) = self.key_cache.lock().await.get(&id_b64).cloned() {
            return Ok(cached);
        }
        let key_opt = self.backend.get_sync_key(key_id).await?;
        let key = key_opt.ok_or_else(|| anyhow!("app state key not found"))?;
        let expanded: ExpandedAppStateKeys = expand_app_state_keys(&key.key_data);
        self.key_cache.lock().await.insert(id_b64, expanded.clone());
        Ok(expanded)
    }

    /// Pre-fetch and cache all keys needed for a patch list.
    async fn prefetch_keys(&self, pl: &PatchList) -> Result<()> {
        let key_ids = collect_key_ids_from_patch_list(pl.snapshot.as_ref(), &pl.patches);
        for key_id in key_ids {
            // This will fetch and cache if not already cached
            let _ = self.get_app_state_key(&key_id).await;
        }
        Ok(())
    }

    pub async fn decode_patch_list<FDownload>(
        &self,
        stanza_root: &Node,
        download: FDownload,
        validate_macs: bool,
    ) -> Result<(Vec<Mutation>, HashState, PatchList)>
    where
        FDownload: Fn(&wa::ExternalBlobReference) -> Result<Vec<u8>> + Send + Sync,
    {
        let mut pl = parse_patch_list(stanza_root)?;

        // Download external snapshot if present (matches WhatsApp Web behavior)
        if pl.snapshot.is_none()
            && let Some(ext) = &pl.snapshot_ref
            && let Ok(data) = download(ext)
            && let Ok(snapshot) = wa::SyncdSnapshot::decode(data.as_slice())
        {
            pl.snapshot = Some(snapshot);
        }

        // Download external mutations for each patch (matches WhatsApp Web behavior)
        // WhatsApp Web: if (r.externalMutations) { n = yield downloadExternalPatch(e, r) }
        for patch in &mut pl.patches {
            if let Some(ext) = &patch.external_mutations {
                let patch_version = patch.version.as_ref().and_then(|v| v.version).unwrap_or(0);
                match download(ext) {
                    Ok(data) => match wa::SyncdMutations::decode(data.as_slice()) {
                        Ok(ext_mutations) => {
                            log::trace!(
                                target: "AppState",
                                "Downloaded external mutations for patch v{}: {} mutations (inline had {})",
                                patch_version,
                                ext_mutations.mutations.len(),
                                patch.mutations.len()
                            );
                            patch.mutations = ext_mutations.mutations;
                        }
                        Err(e) => {
                            log::warn!(
                                target: "AppState",
                                "Failed to decode external mutations for patch v{}: {}",
                                patch_version,
                                e
                            );
                        }
                    },
                    Err(e) => {
                        log::warn!(
                            target: "AppState",
                            "Failed to download external mutations for patch v{}: {}",
                            patch_version,
                            e
                        );
                    }
                }
            }
        }

        self.process_patch_list(pl, validate_macs).await
    }

    pub async fn process_patch_list(
        &self,
        pl: PatchList,
        validate_macs: bool,
    ) -> Result<(Vec<Mutation>, HashState, PatchList)> {
        // Pre-fetch all keys we'll need
        self.prefetch_keys(&pl).await?;

        let mut state = self.backend.get_version(pl.name.as_str()).await?;
        let mut new_mutations: Vec<Mutation> = Vec::new();
        let collection_name = pl.name.as_str();

        // Process snapshot if present
        if let Some(snapshot) = &pl.snapshot {
            let keys_map = self.key_cache.lock().await.clone();
            let snapshot_clone = snapshot.clone();
            let collection_name_owned = collection_name.to_string();

            // Offload CPU-intensive snapshot processing to a blocking thread
            let result = tokio::task::spawn_blocking(move || {
                let get_keys = |key_id: &[u8]| -> Result<
                    ExpandedAppStateKeys,
                    wacore::appstate::AppStateError,
                > {
                    use base64::Engine;
                    use base64::engine::general_purpose::STANDARD_NO_PAD;
                    let id_b64 = STANDARD_NO_PAD.encode(key_id);
                    keys_map
                        .get(&id_b64)
                        .cloned()
                        .ok_or(wacore::appstate::AppStateError::KeyNotFound)
                };

                let mut snapshot_state = HashState::default();
                let result = process_snapshot(
                    &snapshot_clone,
                    &mut snapshot_state,
                    get_keys,
                    validate_macs,
                    &collection_name_owned,
                )?;
                Ok::<_, wacore::appstate::AppStateError>((result, snapshot_state))
            })
            .await
            .map_err(|e| anyhow!("Blocking task failed: {}", e))?
            .map_err(|e| anyhow!("{}", e))?;

            let (snapshot_result, snapshot_state) = result;
            state = snapshot_state;

            new_mutations.extend(snapshot_result.mutations);

            // Persist state and MACs
            self.backend
                .set_version(collection_name, state.clone())
                .await?;
            if !snapshot_result.mutation_macs.is_empty() {
                self.backend
                    .put_mutation_macs(
                        collection_name,
                        state.version,
                        &snapshot_result.mutation_macs,
                    )
                    .await?;
            }
        }

        // Snapshot the key cache once for all patches (prefetch_keys already populated it)
        let keys_map = self.key_cache.lock().await.clone();
        let collection_name_owned = collection_name.to_string();

        // Process patches
        for patch in &pl.patches {
            // Collect index MACs we need to look up (pre-allocate with upper bound)
            let mut need_db_lookup: Vec<Vec<u8>> = Vec::with_capacity(patch.mutations.len());
            for m in &patch.mutations {
                if let Some(rec) = &m.record
                    && let Some(ind) = &rec.index
                    && let Some(index_mac) = &ind.blob
                    && !need_db_lookup.iter().any(|v| v == index_mac)
                {
                    need_db_lookup.push(index_mac.clone());
                }
            }

            // Batch fetch previous value MACs from database
            let mut db_prev: HashMap<Vec<u8>, Vec<u8>> =
                HashMap::with_capacity(need_db_lookup.len());
            for index_mac in need_db_lookup {
                if let Some(mac) = self
                    .backend
                    .get_mutation_mac(collection_name, &index_mac)
                    .await?
                {
                    db_prev.insert(index_mac, mac);
                }
            }

            // Clone data for blocking task
            let patch_clone = patch.clone();
            let state_clone = state.clone();
            let keys = keys_map.clone();
            let coll = collection_name_owned.clone();

            // Offload CPU-intensive patch processing to a blocking thread
            let result = tokio::task::spawn_blocking(move || {
                let get_keys = |key_id: &[u8]| -> Result<
                    ExpandedAppStateKeys,
                    wacore::appstate::AppStateError,
                > {
                    use base64::Engine;
                    use base64::engine::general_purpose::STANDARD_NO_PAD;
                    let id_b64 = STANDARD_NO_PAD.encode(key_id);
                    keys.get(&id_b64)
                        .cloned()
                        .ok_or(wacore::appstate::AppStateError::KeyNotFound)
                };

                let get_prev_value_mac = |index_mac: &[u8]| -> Result<
                    Option<Vec<u8>>,
                    wacore::appstate::AppStateError,
                > { Ok(db_prev.get(index_mac).cloned()) };

                let mut state = state_clone;
                process_patch(
                    &patch_clone,
                    &mut state,
                    get_keys,
                    get_prev_value_mac,
                    validate_macs,
                    &coll,
                )
            })
            .await
            .map_err(|e| anyhow!("Blocking task failed: {}", e))?
            .map_err(|e| anyhow!("{}", e))?;

            // Update local state with the result from the blocking task
            state = result.state;

            new_mutations.extend(result.mutations);

            // Persist state and MACs
            self.backend
                .set_version(collection_name, state.clone())
                .await?;
            if !result.removed_index_macs.is_empty() {
                self.backend
                    .delete_mutation_macs(collection_name, &result.removed_index_macs)
                    .await?;
            }
            if !result.added_macs.is_empty() {
                self.backend
                    .put_mutation_macs(collection_name, state.version, &result.added_macs)
                    .await?;
            }
        }

        // Handle case where we only have a snapshot and no patches
        if pl.patches.is_empty() && pl.snapshot.is_some() {
            self.backend
                .set_version(collection_name, state.clone())
                .await?;
        }

        Ok((new_mutations, state, pl))
    }

    pub async fn get_missing_key_ids(&self, pl: &PatchList) -> Result<Vec<Vec<u8>>> {
        let key_ids = collect_key_ids_from_patch_list(pl.snapshot.as_ref(), &pl.patches);
        let mut missing = Vec::new();
        for id in key_ids {
            if self.backend.get_sync_key(&id).await?.is_none() {
                missing.push(id);
            }
        }
        Ok(missing)
    }

    /// Encode a `PatchInfo` into a serialized `SyncdPatch` protobuf ready for
    /// wire transmission.
    ///
    /// This is the Rust equivalent of whatsmeow's `Processor.EncodePatch`. It:
    ///
    /// 1. Retrieves the latest app state key from the store.
    /// 2. Sets the timestamp on each mutation's `SyncActionValue`.
    /// 3. Encrypts each mutation with `encode_mutation`.
    /// 4. Updates the LTHash state and computes snapshot/patch MACs via `build_patch`.
    /// 5. Serializes the resulting `SyncdPatch` to bytes.
    ///
    /// The caller is responsible for sending the resulting bytes inside an IQ
    /// stanza and handling conflict/retry logic.
    pub async fn encode_patch(&self, patch_info: &PatchInfo) -> Result<(Vec<u8>, u64)> {
        // 1. Get the latest key
        let (key_id, sync_key) = self
            .backend
            .get_latest_sync_key()
            .await?
            .ok_or_else(|| anyhow!("no app state keys found, creating keys is not yet supported"))?;
        let keys = expand_app_state_keys(&sync_key.key_data);

        // 2. Get current state
        let collection_name = patch_info.patch_type.as_str();
        let mut state = self.backend.get_version(collection_name).await?;
        let current_version = state.version;

        // 3. Determine timestamp
        let timestamp_millis = patch_info
            .timestamp
            .unwrap_or_else(SystemTime::now)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        // 4. Encode each mutation
        let mut encoded_mutations = Vec::with_capacity(patch_info.mutations.len());
        for mutation_info in &patch_info.mutations {
            let mut value = mutation_info.value.clone();
            value.timestamp = Some(timestamp_millis);

            let encoded = encode_mutation(
                wa::syncd_mutation::SyncdOperation::Set,
                &mutation_info.index,
                value,
                &keys,
                &key_id,
            );
            encoded_mutations.push(encoded);
        }

        // 5. Build the patch (updates hash state, computes MACs)
        let new_version = current_version + 1;
        let syncd_patch = build_patch(
            encoded_mutations,
            collection_name,
            new_version,
            &keys,
            &key_id,
            &mut state,
        );

        // 6. Persist the updated state
        self.backend
            .set_version(collection_name, state)
            .await?;

        // 7. Serialize to bytes
        let encoded_bytes = syncd_patch.encode_to_vec();
        Ok((encoded_bytes, current_version))
    }

    pub async fn sync_collection<D, FDownload>(
        &self,
        driver: &D,
        name: WAPatchName,
        validate_macs: bool,
        download: FDownload,
    ) -> Result<Vec<Mutation>>
    where
        D: AppStateSyncDriver + Sync,
        FDownload: Fn(&wa::ExternalBlobReference) -> Result<Vec<u8>> + Send + Sync,
    {
        let mut all = Vec::new();
        loop {
            let state = self.backend.get_version(name.as_str()).await?;
            let node = driver.fetch_collection(name, state.version).await?;
            let (mut muts, _new_state, list) = self
                .decode_patch_list(&node, &download, validate_macs)
                .await?;
            all.append(&mut muts);
            if !list.has_more_patches {
                break;
            }
        }
        Ok(all)
    }
}

#[async_trait]
pub trait AppStateSyncDriver {
    async fn fetch_collection(&self, name: WAPatchName, after_version: u64) -> Result<Node>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use std::collections::HashMap;
    use wacore::appstate::WAPATCH_INTEGRITY;
    use wacore::appstate::hash::HashState;
    use wacore::appstate::hash::generate_content_mac;
    use wacore::appstate::keys::expand_app_state_keys;
    use wacore::appstate::processor::AppStateMutationMAC;
    use wacore::libsignal::crypto::aes_256_cbc_encrypt_into;
    use wacore::store::error::Result as StoreResult;
    use wacore::store::traits::{
        AppStateSyncKey, AppSyncStore, ChatSettings, ChatSettingsStore, ContactEntry,
        DeviceListRecord, DeviceStore, LidPnMappingEntry, MessageSecretInsert, MsgSecretStore,
        ProtocolStore, SignalStore,
    };
    use wacore_binary::jid::Jid;

    type MockMacMap = Arc<Mutex<HashMap<(String, Vec<u8>), Vec<u8>>>>;

    #[derive(Default, Clone)]
    struct MockBackend {
        versions: Arc<Mutex<HashMap<String, HashState>>>,
        macs: MockMacMap,
        keys: Arc<Mutex<HashMap<Vec<u8>, AppStateSyncKey>>>,
        latest_key: Arc<Mutex<Option<(Vec<u8>, AppStateSyncKey)>>>,
    }

    // Implement SignalStore - Signal protocol cryptographic operations
    #[async_trait]
    impl SignalStore for MockBackend {
        async fn put_identity(&self, _: &str, _: [u8; 32]) -> StoreResult<()> {
            Ok(())
        }
        async fn load_identity(&self, _: &str) -> StoreResult<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn delete_identity(&self, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn get_session(&self, _: &str) -> StoreResult<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn put_session(&self, _: &str, _: &[u8]) -> StoreResult<()> {
            Ok(())
        }
        async fn delete_session(&self, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn store_prekey(&self, _: u32, _: &[u8], _: bool) -> StoreResult<()> {
            Ok(())
        }
        async fn load_prekey(&self, _: u32) -> StoreResult<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn remove_prekey(&self, _: u32) -> StoreResult<()> {
            Ok(())
        }
        async fn store_signed_prekey(&self, _: u32, _: &[u8]) -> StoreResult<()> {
            Ok(())
        }
        async fn load_signed_prekey(&self, _: u32) -> StoreResult<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn load_all_signed_prekeys(&self) -> StoreResult<Vec<(u32, Vec<u8>)>> {
            Ok(vec![])
        }
        async fn remove_signed_prekey(&self, _: u32) -> StoreResult<()> {
            Ok(())
        }
        async fn put_sender_key(&self, _: &str, _: &[u8]) -> StoreResult<()> {
            Ok(())
        }
        async fn get_sender_key(&self, _: &str) -> StoreResult<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn delete_sender_key(&self, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn migrate_pn_to_lid(&self, _: &str, _: &str) -> StoreResult<()> {
            Ok(())
        }
    }

    // Implement AppSyncStore - WhatsApp app state synchronization
    #[async_trait]
    impl AppSyncStore for MockBackend {
        async fn get_sync_key(&self, key_id: &[u8]) -> StoreResult<Option<AppStateSyncKey>> {
            Ok(self.keys.lock().await.get(key_id).cloned())
        }
        async fn set_sync_key(&self, key_id: &[u8], key: AppStateSyncKey) -> StoreResult<()> {
            self.keys.lock().await.insert(key_id.to_vec(), key);
            Ok(())
        }
        async fn get_latest_sync_key(&self) -> StoreResult<Option<(Vec<u8>, AppStateSyncKey)>> {
            Ok(self.latest_key.lock().await.clone())
        }
        async fn get_version(&self, name: &str) -> StoreResult<HashState> {
            Ok(self
                .versions
                .lock()
                .await
                .get(name)
                .cloned()
                .unwrap_or_default())
        }
        async fn set_version(&self, name: &str, state: HashState) -> StoreResult<()> {
            self.versions.lock().await.insert(name.to_string(), state);
            Ok(())
        }
        async fn put_mutation_macs(
            &self,
            name: &str,
            _version: u64,
            mutations: &[AppStateMutationMAC],
        ) -> StoreResult<()> {
            let mut macs = self.macs.lock().await;
            for m in mutations {
                macs.insert((name.to_string(), m.index_mac.clone()), m.value_mac.clone());
            }
            Ok(())
        }
        async fn get_mutation_mac(
            &self,
            name: &str,
            index_mac: &[u8],
        ) -> StoreResult<Option<Vec<u8>>> {
            Ok(self
                .macs
                .lock()
                .await
                .get(&(name.to_string(), index_mac.to_vec()))
                .cloned())
        }
        async fn delete_mutation_macs(&self, _: &str, _: &[Vec<u8>]) -> StoreResult<()> {
            Ok(())
        }
    }

    // Implement ProtocolStore - WhatsApp Web protocol alignment
    #[async_trait]
    impl ProtocolStore for MockBackend {
        async fn get_skdm_recipients(&self, _: &str) -> StoreResult<Vec<Jid>> {
            Ok(vec![])
        }
        async fn add_skdm_recipients(&self, _: &str, _: &[Jid]) -> StoreResult<()> {
            Ok(())
        }
        async fn clear_skdm_recipients(&self, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn get_lid_mapping(&self, _: &str) -> StoreResult<Option<LidPnMappingEntry>> {
            Ok(None)
        }
        async fn get_pn_mapping(&self, _: &str) -> StoreResult<Option<LidPnMappingEntry>> {
            Ok(None)
        }
        async fn put_lid_mapping(&self, _: &LidPnMappingEntry) -> StoreResult<()> {
            Ok(())
        }
        async fn get_all_lid_mappings(&self) -> StoreResult<Vec<LidPnMappingEntry>> {
            Ok(vec![])
        }
        async fn save_base_key(&self, _: &str, _: &str, _: &[u8]) -> StoreResult<()> {
            Ok(())
        }
        async fn has_same_base_key(&self, _: &str, _: &str, _: &[u8]) -> StoreResult<bool> {
            Ok(false)
        }
        async fn delete_base_key(&self, _: &str, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn update_device_list(&self, _: DeviceListRecord) -> StoreResult<()> {
            Ok(())
        }
        async fn get_devices(&self, _: &str) -> StoreResult<Option<DeviceListRecord>> {
            Ok(None)
        }
        async fn mark_forget_sender_key(&self, _: &str, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn consume_forget_marks(&self, _: &str) -> StoreResult<Vec<String>> {
            Ok(vec![])
        }
        async fn get_tc_token(
            &self,
            _: &str,
        ) -> StoreResult<Option<wacore::store::traits::TcTokenEntry>> {
            Ok(None)
        }
        async fn put_tc_token(
            &self,
            _: &str,
            _: &wacore::store::traits::TcTokenEntry,
        ) -> StoreResult<()> {
            Ok(())
        }
        async fn delete_tc_token(&self, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn get_all_tc_token_jids(&self) -> StoreResult<Vec<String>> {
            Ok(vec![])
        }
        async fn delete_expired_tc_tokens(&self, _: i64) -> StoreResult<u32> {
            Ok(0)
        }
        async fn put_contact(&self, _: &str, _: &str, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn put_contact_push_name(&self, _: &str, _: &str) -> StoreResult<(bool, String)> {
            Ok((false, String::new()))
        }
        async fn get_saved_contacts(&self) -> StoreResult<Vec<ContactEntry>> {
            Ok(vec![])
        }
        async fn get_all_contacts(&self) -> StoreResult<Vec<ContactEntry>> {
            Ok(vec![])
        }
        async fn put_business_name(&self, _: &str, _: &str) -> StoreResult<(bool, String)> {
            Ok((false, String::new()))
        }
    }

    // Implement DeviceStore - Device persistence
    #[async_trait]
    impl DeviceStore for MockBackend {
        async fn save(&self, _: &wacore::store::Device) -> StoreResult<()> {
            Ok(())
        }
        async fn load(&self) -> StoreResult<Option<wacore::store::Device>> {
            Ok(Some(wacore::store::Device::new()))
        }
        async fn exists(&self) -> StoreResult<bool> {
            Ok(true)
        }
        async fn create(&self) -> StoreResult<i32> {
            Ok(1)
        }
    }

    // Implement MsgSecretStore - Per-message encryption secrets
    #[async_trait]
    impl MsgSecretStore for MockBackend {
        async fn put_message_secret(&self, _: &str, _: &str, _: &str, _: &[u8]) -> StoreResult<()> {
            Ok(())
        }
        async fn put_message_secrets(&self, _: &[MessageSecretInsert]) -> StoreResult<()> {
            Ok(())
        }
        async fn get_message_secret(
            &self,
            _: &str,
            _: &str,
            _: &str,
        ) -> StoreResult<Option<(Vec<u8>, String)>> {
            Ok(None)
        }
    }

    // Implement ChatSettingsStore - Local chat settings
    #[async_trait]
    impl ChatSettingsStore for MockBackend {
        async fn put_muted_until(&self, _: &str, _: i64) -> StoreResult<()> {
            Ok(())
        }
        async fn put_pinned(&self, _: &str, _: bool) -> StoreResult<()> {
            Ok(())
        }
        async fn put_archived(&self, _: &str, _: bool) -> StoreResult<()> {
            Ok(())
        }
        async fn get_chat_settings(&self, _: &str) -> StoreResult<ChatSettings> {
            Ok(ChatSettings::default())
        }
    }

    fn create_encrypted_mutation(
        op: wa::syncd_mutation::SyncdOperation,
        index_mac: &[u8],
        plaintext: &[u8],
        keys: &wacore::appstate::keys::ExpandedAppStateKeys,
        key_id_bytes: &[u8],
    ) -> wa::SyncdMutation {
        let iv = vec![0u8; 16];

        let mut ciphertext = Vec::new();
        aes_256_cbc_encrypt_into(plaintext, &keys.value_encryption, &iv, &mut ciphertext)
            .expect("AES-CBC encryption should succeed with valid inputs");
        let mut value_with_iv = iv;
        value_with_iv.extend_from_slice(&ciphertext);
        let value_mac = generate_content_mac(op, &value_with_iv, key_id_bytes, &keys.value_mac);
        let mut value_blob = value_with_iv;
        value_blob.extend_from_slice(&value_mac);

        wa::SyncdMutation {
            operation: Some(op as i32),
            record: Some(wa::SyncdRecord {
                index: Some(wa::SyncdIndex {
                    blob: Some(index_mac.to_vec()),
                }),
                value: Some(wa::SyncdValue {
                    blob: Some(value_blob),
                }),
                key_id: Some(wa::KeyId {
                    id: Some(key_id_bytes.to_vec()),
                }),
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Existing decode tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_process_patch_list_handles_set_overwrite_correctly() {
        let backend = Arc::new(MockBackend::default());
        let processor = AppStateProcessor::new(backend.clone());
        let collection_name = WAPatchName::Regular;
        let index_mac = vec![1; 32];
        let key_id_bytes = b"test_key_id".to_vec();
        let master_key = [7u8; 32];
        let keys = expand_app_state_keys(&master_key);

        let sync_key = AppStateSyncKey {
            key_data: master_key.to_vec(),
            ..Default::default()
        };
        backend
            .set_sync_key(&key_id_bytes, sync_key)
            .await
            .expect("test backend should accept sync key");

        let original_plaintext = wa::SyncActionData {
            value: Some(wa::SyncActionValue {
                timestamp: Some(1000),
                ..Default::default()
            }),
            ..Default::default()
        }
        .encode_to_vec();
        let original_mutation = create_encrypted_mutation(
            wa::syncd_mutation::SyncdOperation::Set,
            &index_mac,
            &original_plaintext,
            &keys,
            &key_id_bytes,
        );

        let mut initial_state = HashState {
            version: 1,
            ..Default::default()
        };
        let (hash_result, res) =
            initial_state.update_hash(std::slice::from_ref(&original_mutation), |_, _| Ok(None));
        assert!(res.is_ok() && !hash_result.has_missing_remove);
        backend
            .set_version(collection_name.as_str(), initial_state.clone())
            .await
            .expect("test backend should accept app state version");

        let original_value_blob = original_mutation
            .record
            .expect("mutation should have record")
            .value
            .expect("record should have value")
            .blob
            .expect("value should have blob");
        let original_value_mac = original_value_blob[original_value_blob.len() - 32..].to_vec();
        backend
            .put_mutation_macs(
                collection_name.as_str(),
                1,
                &[AppStateMutationMAC {
                    index_mac: index_mac.clone(),
                    value_mac: original_value_mac.clone(),
                }],
            )
            .await
            .expect("test backend should accept mutation MACs");

        let new_plaintext = wa::SyncActionData {
            value: Some(wa::SyncActionValue {
                timestamp: Some(2000),
                ..Default::default()
            }),
            ..Default::default()
        }
        .encode_to_vec();
        let overwrite_mutation = create_encrypted_mutation(
            wa::syncd_mutation::SyncdOperation::Set,
            &index_mac,
            &new_plaintext,
            &keys,
            &key_id_bytes,
        );

        let patch_list = PatchList {
            name: collection_name,
            has_more_patches: false,
            patches: vec![wa::SyncdPatch {
                mutations: vec![overwrite_mutation.clone()],
                version: Some(wa::SyncdVersion { version: Some(2) }),
                key_id: Some(wa::KeyId {
                    id: Some(key_id_bytes),
                }),
                ..Default::default()
            }],
            snapshot: None,
            snapshot_ref: None,
        };

        let result = processor.process_patch_list(patch_list, false).await;

        assert!(
            result.is_ok(),
            "Processing the patch should succeed, but it failed: {:?}",
            result.err()
        );
        let (_, final_state, _) = result.expect("process_patch_list should succeed");

        let mut expected_state = initial_state.clone();
        let new_value_blob = overwrite_mutation
            .record
            .expect("mutation should have record")
            .value
            .expect("record should have value")
            .blob
            .expect("value should have blob");
        let new_value_mac = new_value_blob[new_value_blob.len() - 32..].to_vec();

        WAPATCH_INTEGRITY.subtract_then_add_in_place(
            &mut expected_state.hash,
            &[original_value_mac],
            &[new_value_mac],
        );

        assert_eq!(
            final_state.hash, expected_state.hash,
            "The final LTHash is incorrect, meaning the overwrite was not handled properly."
        );
        assert_eq!(
            final_state.version, 2,
            "The version should be updated to that of the patch."
        );
    }

    // -----------------------------------------------------------------------
    // Mutation builder tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_mute_forever() {
        let patch = build_mute("120363@s.whatsapp.net", true, None);
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);
        assert_eq!(patch.mutations.len(), 1);

        let m = &patch.mutations[0];
        assert_eq!(m.index, vec!["mute", "120363@s.whatsapp.net"]);
        assert_eq!(m.version, 2);

        let mute_action = m.value.mute_action.as_ref().expect("mute_action should be set");
        assert_eq!(mute_action.muted, Some(true));
        assert_eq!(mute_action.mute_end_timestamp, Some(-1));
    }

    #[test]
    fn test_build_mute_with_duration() {
        let before = now_millis();
        let patch = build_mute("120363@s.whatsapp.net", true, Some(Duration::from_secs(3600)));
        let after = now_millis();

        let mute_action = patch.mutations[0]
            .value
            .mute_action
            .as_ref()
            .expect("mute_action should be set");
        assert_eq!(mute_action.muted, Some(true));

        let end_ts = mute_action.mute_end_timestamp.expect("should have end timestamp");
        // end timestamp should be roughly now + 1 hour
        assert!(end_ts >= before + 3_600_000);
        assert!(end_ts <= after + 3_600_000);
    }

    #[test]
    fn test_build_unmute() {
        let patch = build_mute("120363@s.whatsapp.net", false, None);
        let mute_action = patch.mutations[0]
            .value
            .mute_action
            .as_ref()
            .expect("mute_action should be set");
        assert_eq!(mute_action.muted, Some(false));
        assert_eq!(mute_action.mute_end_timestamp, None);
    }

    #[test]
    fn test_build_pin() {
        let patch = build_pin("120363@s.whatsapp.net", true);
        assert_eq!(patch.patch_type, WAPatchName::RegularLow);
        assert_eq!(patch.mutations.len(), 1);

        let m = &patch.mutations[0];
        assert_eq!(m.index, vec!["pin_v1", "120363@s.whatsapp.net"]);
        assert_eq!(m.version, 5);

        let pin_action = m.value.pin_action.as_ref().expect("pin_action should be set");
        assert_eq!(pin_action.pinned, Some(true));
    }

    #[test]
    fn test_build_archive_also_unpins() {
        let patch = build_archive("120363@s.whatsapp.net", true, None, None);
        assert_eq!(patch.patch_type, WAPatchName::RegularLow);
        assert_eq!(
            patch.mutations.len(),
            2,
            "archive=true should produce archive + unpin mutations"
        );

        // First mutation: archive
        let archive_action = patch.mutations[0]
            .value
            .archive_chat_action
            .as_ref()
            .expect("archive_chat_action should be set");
        assert_eq!(archive_action.archived, Some(true));

        // Second mutation: unpin
        let pin_action = patch.mutations[1]
            .value
            .pin_action
            .as_ref()
            .expect("pin_action should be set for unpin");
        assert_eq!(pin_action.pinned, Some(false));
    }

    #[test]
    fn test_build_unarchive_no_unpin() {
        let patch = build_archive("120363@s.whatsapp.net", false, None, None);
        assert_eq!(
            patch.mutations.len(),
            1,
            "archive=false should produce only the archive mutation"
        );
    }

    #[test]
    fn test_build_star() {
        let patch = build_star(
            "120363@s.whatsapp.net",
            "556699@s.whatsapp.net",
            "3EB0ABCDEF",
            false,
            true,
        );
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);

        let m = &patch.mutations[0];
        assert_eq!(
            m.index,
            vec![
                "star",
                "120363@s.whatsapp.net",
                "3EB0ABCDEF",
                "0",
                "556699@s.whatsapp.net",
            ]
        );
        assert_eq!(m.version, 2);

        let star_action = m.value.star_action.as_ref().expect("star_action should be set");
        assert_eq!(star_action.starred, Some(true));
    }

    #[test]
    fn test_build_star_same_sender_uses_zero() {
        let patch = build_star(
            "120363@s.whatsapp.net",
            "120363@s.whatsapp.net",
            "msg1",
            true,
            true,
        );
        let m = &patch.mutations[0];
        assert_eq!(m.index[4], "0", "sender should be '0' when same as target");
        assert_eq!(m.index[3], "1", "from_me=true should be '1'");
    }

    #[test]
    fn test_build_label_edit() {
        let patch = build_label_edit("label_1", "Important", 4, false);
        assert_eq!(patch.patch_type, WAPatchName::Regular);
        assert_eq!(patch.mutations.len(), 1);

        let m = &patch.mutations[0];
        assert_eq!(m.index, vec!["label_edit", "label_1"]);
        assert_eq!(m.version, 3);

        let label_action = m
            .value
            .label_edit_action
            .as_ref()
            .expect("label_edit_action should be set");
        assert_eq!(label_action.name.as_deref(), Some("Important"));
        assert_eq!(label_action.color, Some(4));
        assert_eq!(label_action.deleted, Some(false));
    }

    #[test]
    fn test_build_label_chat() {
        let patch = build_label_chat("120363@s.whatsapp.net", "label_1", true);
        assert_eq!(patch.patch_type, WAPatchName::Regular);

        let m = &patch.mutations[0];
        assert_eq!(
            m.index,
            vec!["label_jid", "label_1", "120363@s.whatsapp.net"]
        );
        assert_eq!(m.version, 3);

        let action = m
            .value
            .label_association_action
            .as_ref()
            .expect("label_association_action should be set");
        assert_eq!(action.labeled, Some(true));
    }

    #[test]
    fn test_build_label_message() {
        let patch = build_label_message("120363@s.whatsapp.net", "label_1", "msg_42", true);
        assert_eq!(patch.patch_type, WAPatchName::Regular);

        let m = &patch.mutations[0];
        assert_eq!(
            m.index,
            vec![
                "label_message",
                "label_1",
                "120363@s.whatsapp.net",
                "msg_42",
                "0",
                "0",
            ]
        );
    }

    #[test]
    fn test_build_mark_chat_as_read() {
        let patch = build_mark_chat_as_read("120363@s.whatsapp.net", true, None, None);
        assert_eq!(patch.patch_type, WAPatchName::RegularLow);

        let m = &patch.mutations[0];
        assert_eq!(m.index, vec!["markChatAsRead", "120363@s.whatsapp.net"]);
        assert_eq!(m.version, 3);

        let action = m
            .value
            .mark_chat_as_read_action
            .as_ref()
            .expect("mark_chat_as_read_action should be set");
        assert_eq!(action.read, Some(true));
        assert!(action.message_range.is_some());
    }

    #[test]
    fn test_build_delete_chat() {
        let patch = build_delete_chat("120363@s.whatsapp.net", None, None, true);
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);

        let m = &patch.mutations[0];
        assert_eq!(
            m.index,
            vec!["deleteChat", "120363@s.whatsapp.net", "1"]
        );
        assert_eq!(m.version, 6);
        assert!(m.value.delete_chat_action.is_some());
    }

    #[test]
    fn test_build_setting_push_name() {
        let patch = build_setting_push_name("MyName");
        assert_eq!(patch.patch_type, WAPatchName::CriticalBlock);

        let m = &patch.mutations[0];
        assert_eq!(m.index, vec!["setting_pushName"]);
        assert_eq!(m.version, 1);

        let action = m
            .value
            .push_name_setting
            .as_ref()
            .expect("push_name_setting should be set");
        assert_eq!(action.name.as_deref(), Some("MyName"));
    }

    // -----------------------------------------------------------------------
    // Protocol message builder tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_app_state_key_request() {
        let key_ids = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let msg = build_app_state_key_request(&key_ids);

        let proto = msg.protocol_message.expect("should have protocol_message");
        assert_eq!(
            proto.r#type,
            Some(wa::message::protocol_message::Type::AppStateSyncKeyRequest as i32)
        );

        let req = proto
            .app_state_sync_key_request
            .expect("should have key request");
        assert_eq!(req.key_ids.len(), 2);
        assert_eq!(req.key_ids[0].key_id, Some(vec![1, 2, 3]));
        assert_eq!(req.key_ids[1].key_id, Some(vec![4, 5, 6]));
    }

    #[test]
    fn test_build_fatal_exception_notification() {
        let msg = build_fatal_app_state_exception_notification(&[
            WAPatchName::Regular,
            WAPatchName::RegularHigh,
        ]);

        let proto = msg.protocol_message.expect("should have protocol_message");
        assert_eq!(
            proto.r#type,
            Some(
                wa::message::protocol_message::Type::AppStateFatalExceptionNotification as i32,
            )
        );

        let notif = proto
            .app_state_fatal_exception_notification
            .expect("should have fatal exception");
        assert_eq!(notif.collection_names, vec!["regular", "regular_high"]);
        assert!(notif.timestamp.is_some());
    }

    #[test]
    fn test_build_recovery_request() {
        let msg = build_app_state_recovery_request(WAPatchName::CriticalUnblockLow);

        let proto = msg.protocol_message.expect("should have protocol_message");
        assert_eq!(
            proto.r#type,
            Some(
                wa::message::protocol_message::Type::PeerDataOperationRequestMessage as i32,
            )
        );

        let pdo = proto
            .peer_data_operation_request_message
            .expect("should have PDO request");
        assert_eq!(
            pdo.peer_data_operation_request_type,
            Some(
                wa::message::PeerDataOperationRequestType::CompanionSyncdSnapshotFatalRecovery
                    as i32,
            )
        );

        let recovery = pdo
            .syncd_collection_fatal_recovery_request
            .expect("should have recovery request");
        assert_eq!(
            recovery.collection_name.as_deref(),
            Some("critical_unblock_low")
        );
        assert!(recovery.timestamp.is_some());
    }

    // -----------------------------------------------------------------------
    // Encode patch integration test
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_encode_patch_produces_valid_syncd_patch() {
        let backend = Arc::new(MockBackend::default());
        let processor = AppStateProcessor::new(backend.clone());

        let master_key = [42u8; 32];
        let key_id = b"encode_test_key".to_vec();
        let sync_key = AppStateSyncKey {
            key_data: master_key.to_vec(),
            ..Default::default()
        };

        // Store the key so get_sync_key works
        backend
            .set_sync_key(&key_id, sync_key.clone())
            .await
            .expect("should store sync key");

        // Set it as the latest key so encode_patch can find it
        *backend.latest_key.lock().await = Some((key_id.clone(), sync_key));

        // Set initial version to 0
        backend
            .set_version("regular_high", HashState::default())
            .await
            .expect("should set version");

        let patch_info = build_mute("120363@s.whatsapp.net", true, None);
        let result = processor.encode_patch(&patch_info).await;
        assert!(result.is_ok(), "encode_patch failed: {:?}", result.err());

        let (encoded_bytes, version) = result.unwrap();
        assert_eq!(version, 0, "should return the version before increment");
        assert!(!encoded_bytes.is_empty(), "encoded bytes should not be empty");

        // Decode the serialized patch to verify structure
        let decoded_patch = wa::SyncdPatch::decode(encoded_bytes.as_slice())
            .expect("should decode back to SyncdPatch");

        assert_eq!(decoded_patch.mutations.len(), 1);
        assert!(decoded_patch.snapshot_mac.is_some());
        assert!(decoded_patch.patch_mac.is_some());
        assert!(decoded_patch.key_id.is_some());
        assert_eq!(
            decoded_patch.key_id.as_ref().and_then(|k| k.id.as_ref()),
            Some(&key_id)
        );

        // Verify the stored version was incremented
        let stored_state = backend
            .get_version("regular_high")
            .await
            .expect("should get version");
        assert_eq!(stored_state.version, 1, "version should be incremented to 1");
        assert_ne!(
            stored_state.hash,
            [0u8; 128],
            "hash should be updated from zero"
        );
    }

    #[tokio::test]
    async fn test_encode_patch_fails_without_key() {
        let backend = Arc::new(MockBackend::default());
        let processor = AppStateProcessor::new(backend.clone());

        let patch_info = build_pin("120363@s.whatsapp.net", true);
        let result = processor.encode_patch(&patch_info).await;
        assert!(
            result.is_err(),
            "encode_patch should fail when no key is available"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("no app state keys found"),
            "error should mention missing keys: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_encode_patch_roundtrip_decode() {
        // Encode a patch, then verify the mutations can be decoded back by the
        // decode path (process_patch_list).
        let backend = Arc::new(MockBackend::default());
        let processor = AppStateProcessor::new(backend.clone());

        let master_key = [99u8; 32];
        let key_id = b"roundtrip_key".to_vec();
        let sync_key = AppStateSyncKey {
            key_data: master_key.to_vec(),
            ..Default::default()
        };

        backend
            .set_sync_key(&key_id, sync_key.clone())
            .await
            .unwrap();
        *backend.latest_key.lock().await = Some((key_id.clone(), sync_key));
        backend
            .set_version("regular_low", HashState::default())
            .await
            .unwrap();

        let patch_info = build_pin("120363@s.whatsapp.net", true);
        let (encoded_bytes, _version) = processor.encode_patch(&patch_info).await.unwrap();

        // Decode the SyncdPatch and wrap it into a PatchList for process_patch_list
        let decoded_patch = wa::SyncdPatch::decode(encoded_bytes.as_slice()).unwrap();

        // Reset version so process_patch_list starts fresh
        backend
            .set_version("regular_low", HashState::default())
            .await
            .unwrap();

        let pl = PatchList {
            name: WAPatchName::RegularLow,
            has_more_patches: false,
            patches: vec![decoded_patch],
            snapshot: None,
            snapshot_ref: None,
        };

        // process_patch_list should successfully decode the patch we just encoded.
        // We skip MAC validation because process_patch_list validates the
        // snapshot MAC against the *new* state, and our single-patch scenario
        // matches exactly what we encoded.
        let result = processor.process_patch_list(pl, false).await;
        assert!(
            result.is_ok(),
            "roundtrip decode should succeed: {:?}",
            result.err()
        );

        let (mutations, _final_state, _) = result.unwrap();
        assert_eq!(mutations.len(), 1, "should decode exactly 1 mutation");
        assert_eq!(mutations[0].index, vec!["pin_v1", "120363@s.whatsapp.net"]);
        assert!(
            mutations[0]
                .action_value
                .as_ref()
                .and_then(|v| v.pin_action.as_ref())
                .and_then(|p| p.pinned)
                .unwrap_or(false),
            "decoded mutation should have pinned=true"
        );
    }

    // -----------------------------------------------------------------------
    // Mutation builder tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_mute_forever() {
        let patch = build_mute("120363@s.whatsapp.net", true, None);
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec!["mute", "120363@s.whatsapp.net"]
        );
        assert_eq!(patch.mutations[0].version, 2);
        let mute_action = patch.mutations[0]
            .value
            .mute_action
            .as_ref()
            .expect("mute_action should be set");
        assert_eq!(mute_action.muted, Some(true));
        assert_eq!(mute_action.mute_end_timestamp, Some(-1));
    }

    #[test]
    fn test_build_mute_with_duration() {
        let patch = build_mute(
            "120363@s.whatsapp.net",
            true,
            Some(Duration::from_secs(3600)),
        );
        let mute_action = patch.mutations[0]
            .value
            .mute_action
            .as_ref()
            .expect("mute_action should be set");
        assert_eq!(mute_action.muted, Some(true));
        // Should be a positive timestamp (now + 1 hour)
        assert!(mute_action.mute_end_timestamp.unwrap() > 0);
    }

    #[test]
    fn test_build_mute_unmute() {
        let patch = build_mute("120363@s.whatsapp.net", false, None);
        let mute_action = patch.mutations[0]
            .value
            .mute_action
            .as_ref()
            .expect("mute_action should be set");
        assert_eq!(mute_action.muted, Some(false));
        assert_eq!(mute_action.mute_end_timestamp, None);
    }

    #[test]
    fn test_build_pin() {
        let patch = build_pin("120363@s.whatsapp.net", true);
        assert_eq!(patch.patch_type, WAPatchName::RegularLow);
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec!["pin_v1", "120363@s.whatsapp.net"]
        );
        assert_eq!(patch.mutations[0].version, 5);
        let pin_action = patch.mutations[0]
            .value
            .pin_action
            .as_ref()
            .expect("pin_action should be set");
        assert_eq!(pin_action.pinned, Some(true));
    }

    #[test]
    fn test_build_archive_includes_unpin() {
        let patch = build_archive("120363@s.whatsapp.net", true, None, None);
        assert_eq!(patch.patch_type, WAPatchName::RegularLow);
        // Archive + unpin = 2 mutations
        assert_eq!(patch.mutations.len(), 2);
        assert_eq!(
            patch.mutations[0].index,
            vec!["archive", "120363@s.whatsapp.net"]
        );
        // Second mutation should be an unpin
        assert_eq!(
            patch.mutations[1].index,
            vec!["pin_v1", "120363@s.whatsapp.net"]
        );
        let pin_action = patch.mutations[1]
            .value
            .pin_action
            .as_ref()
            .expect("pin_action should be set");
        assert_eq!(pin_action.pinned, Some(false));
    }

    #[test]
    fn test_build_archive_unarchive_no_unpin() {
        let patch = build_archive("120363@s.whatsapp.net", false, None, None);
        // Unarchive does NOT unpin
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec!["archive", "120363@s.whatsapp.net"]
        );
    }

    #[test]
    fn test_build_star() {
        let patch = build_star(
            "120363@s.whatsapp.net",
            "120363@s.whatsapp.net",
            "ABCDEF123",
            true,
            true,
        );
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);
        assert_eq!(patch.mutations.len(), 1);
        // Same target and sender -> sender should be "0"
        assert_eq!(
            patch.mutations[0].index,
            vec!["star", "120363@s.whatsapp.net", "ABCDEF123", "1", "0"]
        );
        let star_action = patch.mutations[0]
            .value
            .star_action
            .as_ref()
            .expect("star_action should be set");
        assert_eq!(star_action.starred, Some(true));
    }

    #[test]
    fn test_build_star_group_different_sender() {
        let patch = build_star(
            "120363999@g.us",
            "559999@s.whatsapp.net",
            "MSG123",
            false,
            false,
        );
        // Different target/sender -> sender JID is preserved
        assert_eq!(
            patch.mutations[0].index,
            vec![
                "star",
                "120363999@g.us",
                "MSG123",
                "0",
                "559999@s.whatsapp.net"
            ]
        );
    }

    #[test]
    fn test_build_label_edit() {
        let patch = build_label_edit("label_1", "Important", 4, false);
        assert_eq!(patch.patch_type, WAPatchName::Regular);
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec!["label_edit", "label_1"]
        );
        assert_eq!(patch.mutations[0].version, 3);
        let label_action = patch.mutations[0]
            .value
            .label_edit_action
            .as_ref()
            .expect("label_edit_action should be set");
        assert_eq!(label_action.name, Some("Important".to_string()));
        assert_eq!(label_action.color, Some(4));
        assert_eq!(label_action.deleted, Some(false));
    }

    #[test]
    fn test_build_label_chat() {
        let patch = build_label_chat("120363@s.whatsapp.net", "label_2", true);
        assert_eq!(patch.patch_type, WAPatchName::Regular);
        assert_eq!(
            patch.mutations[0].index,
            vec!["label_jid", "label_2", "120363@s.whatsapp.net"]
        );
    }

    #[test]
    fn test_build_label_message() {
        let patch = build_label_message(
            "120363@s.whatsapp.net",
            "label_3",
            "MSG789",
            true,
        );
        assert_eq!(patch.patch_type, WAPatchName::Regular);
        assert_eq!(
            patch.mutations[0].index,
            vec![
                "label_message",
                "label_3",
                "120363@s.whatsapp.net",
                "MSG789",
                "0",
                "0"
            ]
        );
    }

    #[test]
    fn test_build_mark_chat_as_read() {
        let patch = build_mark_chat_as_read("120363@s.whatsapp.net", true, None, None);
        assert_eq!(patch.patch_type, WAPatchName::RegularLow);
        assert_eq!(
            patch.mutations[0].index,
            vec!["markChatAsRead", "120363@s.whatsapp.net"]
        );
        let action = patch.mutations[0]
            .value
            .mark_chat_as_read_action
            .as_ref()
            .expect("mark_chat_as_read_action should be set");
        assert_eq!(action.read, Some(true));
    }

    #[test]
    fn test_build_delete_chat() {
        let patch = build_delete_chat("120363@s.whatsapp.net", None, None, true);
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);
        assert_eq!(
            patch.mutations[0].index,
            vec!["deleteChat", "120363@s.whatsapp.net", "1"]
        );
        assert_eq!(patch.mutations[0].version, 6);
    }

    #[test]
    fn test_build_setting_push_name() {
        let patch = build_setting_push_name("Alice");
        assert_eq!(patch.patch_type, WAPatchName::CriticalBlock);
        assert_eq!(
            patch.mutations[0].index,
            vec!["setting_pushName"]
        );
        assert_eq!(patch.mutations[0].version, 1);
        let action = patch.mutations[0]
            .value
            .push_name_setting
            .as_ref()
            .expect("push_name_setting should be set");
        assert_eq!(action.name, Some("Alice".to_string()));
    }

    #[test]
    fn test_build_delete_message_for_me() {
        let patch = build_delete_message_for_me(
            "120363@s.whatsapp.net",
            "559999@s.whatsapp.net",
            "MSG456",
            false,
            true,
            Some(1_700_000_000),
        );
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec![
                "deleteMessageForMe",
                "120363@s.whatsapp.net",
                "MSG456",
                "0",
                "559999@s.whatsapp.net"
            ]
        );
        assert_eq!(patch.mutations[0].version, 3);
        let action = patch.mutations[0]
            .value
            .delete_message_for_me_action
            .as_ref()
            .expect("delete_message_for_me_action should be set");
        assert_eq!(action.delete_media, Some(true));
        assert_eq!(action.message_timestamp, Some(1_700_000_000));
    }

    #[test]
    fn test_build_delete_message_for_me_same_sender() {
        let patch = build_delete_message_for_me(
            "120363@s.whatsapp.net",
            "120363@s.whatsapp.net",
            "MSG789",
            true,
            false,
            None,
        );
        // Same target and sender -> sender should be "0"
        assert_eq!(
            patch.mutations[0].index,
            vec!["deleteMessageForMe", "120363@s.whatsapp.net", "MSG789", "1", "0"]
        );
    }

    #[test]
    fn test_build_clear_chat() {
        let patch = build_clear_chat("120363@s.whatsapp.net", None, None, true);
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec!["clearChat", "120363@s.whatsapp.net", "1"]
        );
        assert_eq!(patch.mutations[0].version, 7);
        let action = patch.mutations[0]
            .value
            .clear_chat_action
            .as_ref()
            .expect("clear_chat_action should be set");
        assert!(action.message_range.is_some());
    }

    #[test]
    fn test_build_clear_chat_no_media() {
        let patch = build_clear_chat("120363@s.whatsapp.net", None, None, false);
        assert_eq!(
            patch.mutations[0].index,
            vec!["clearChat", "120363@s.whatsapp.net", "0"]
        );
    }

    #[test]
    fn test_build_contact() {
        let patch = build_contact("120363@s.whatsapp.net", "John Doe", "John");
        assert_eq!(patch.patch_type, WAPatchName::CriticalUnblockLow);
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec!["contact", "120363@s.whatsapp.net"]
        );
        assert_eq!(patch.mutations[0].version, 2);
        let action = patch.mutations[0]
            .value
            .contact_action
            .as_ref()
            .expect("contact_action should be set");
        assert_eq!(action.full_name, Some("John Doe".to_string()));
        assert_eq!(action.first_name, Some("John".to_string()));
    }

    #[test]
    fn test_build_setting_unarchive_chats() {
        let patch = build_setting_unarchive_chats(true);
        assert_eq!(patch.patch_type, WAPatchName::RegularLow);
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec!["setting_unarchiveChats"]
        );
        assert_eq!(patch.mutations[0].version, 1);
        let action = patch.mutations[0]
            .value
            .unarchive_chats_setting
            .as_ref()
            .expect("unarchive_chats_setting should be set");
        assert_eq!(action.unarchive_chats, Some(true));
    }

    #[test]
    fn test_build_user_status_mute() {
        let patch = build_user_status_mute("559999@s.whatsapp.net", true);
        assert_eq!(patch.patch_type, WAPatchName::RegularHigh);
        assert_eq!(patch.mutations.len(), 1);
        assert_eq!(
            patch.mutations[0].index,
            vec!["userStatusMute", "559999@s.whatsapp.net"]
        );
        assert_eq!(patch.mutations[0].version, 2);
        let action = patch.mutations[0]
            .value
            .user_status_mute_action
            .as_ref()
            .expect("user_status_mute_action should be set");
        assert_eq!(action.muted, Some(true));
    }

    #[test]
    fn test_build_app_state_key_request() {
        let key_ids = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let msg = build_app_state_key_request(&key_ids);
        let proto_msg = msg
            .protocol_message
            .as_ref()
            .expect("protocol_message should be set");
        assert_eq!(
            proto_msg.r#type,
            Some(wa::message::protocol_message::Type::AppStateSyncKeyRequest as i32)
        );
        let request = proto_msg
            .app_state_sync_key_request
            .as_ref()
            .expect("app_state_sync_key_request should be set");
        assert_eq!(request.key_ids.len(), 2);
    }

    #[test]
    fn test_build_fatal_app_state_exception_notification() {
        let msg = build_fatal_app_state_exception_notification(&[
            WAPatchName::Regular,
            WAPatchName::RegularHigh,
        ]);
        let proto_msg = msg
            .protocol_message
            .as_ref()
            .expect("protocol_message should be set");
        assert_eq!(
            proto_msg.r#type,
            Some(
                wa::message::protocol_message::Type::AppStateFatalExceptionNotification
                    as i32
            )
        );
        let notification = proto_msg
            .app_state_fatal_exception_notification
            .as_ref()
            .expect("notification should be set");
        assert_eq!(notification.collection_names.len(), 2);
        assert_eq!(notification.collection_names[0], "regular");
        assert_eq!(notification.collection_names[1], "regular_high");
    }

    #[test]
    fn test_build_app_state_recovery_request() {
        let msg = build_app_state_recovery_request(WAPatchName::RegularHigh);
        let proto_msg = msg
            .protocol_message
            .as_ref()
            .expect("protocol_message should be set");
        assert_eq!(
            proto_msg.r#type,
            Some(
                wa::message::protocol_message::Type::PeerDataOperationRequestMessage
                    as i32
            )
        );
        let pdo = proto_msg
            .peer_data_operation_request_message
            .as_ref()
            .expect("pdo should be set");
        assert_eq!(
            pdo.peer_data_operation_request_type,
            Some(
                wa::message::PeerDataOperationRequestType::CompanionSyncdSnapshotFatalRecovery
                    as i32
            )
        );
        let recovery = pdo
            .syncd_collection_fatal_recovery_request
            .as_ref()
            .expect("recovery request should be set");
        assert_eq!(
            recovery.collection_name,
            Some("regular_high".to_string())
        );
    }

    /// Verify that all mutation index formats match the whatsmeow Go constants
    /// exactly (regression guard).
    #[test]
    fn test_mutation_index_constants_match_whatsmeow() {
        assert_eq!(INDEX_PIN, "pin_v1");
        assert_eq!(INDEX_ARCHIVE, "archive");
        assert_eq!(INDEX_MARK_CHAT_AS_READ, "markChatAsRead");
        assert_eq!(INDEX_SETTING_UNARCHIVE_CHATS, "setting_unarchiveChats");
        assert_eq!(INDEX_LABEL_ASSOCIATION_MESSAGE, "label_message");
        assert_eq!(INDEX_LABEL_EDIT, "label_edit");
        assert_eq!(INDEX_LABEL_ASSOCIATION_CHAT, "label_jid");
        assert_eq!(INDEX_STAR, "star");
        assert_eq!(INDEX_MUTE, "mute");
        assert_eq!(INDEX_DELETE_MESSAGE_FOR_ME, "deleteMessageForMe");
        assert_eq!(INDEX_CLEAR_CHAT, "clearChat");
        assert_eq!(INDEX_DELETE_CHAT, "deleteChat");
        assert_eq!(INDEX_USER_STATUS_MUTE, "userStatusMute");
        assert_eq!(INDEX_CONTACT, "contact");
        assert_eq!(INDEX_SETTING_PUSH_NAME, "setting_pushName");
    }
}
