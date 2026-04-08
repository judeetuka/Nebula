//! Encoding of app state mutations for sending to WhatsApp servers.
//!
//! This module is the inverse of [`crate::decode`]. It takes plaintext
//! `SyncActionValue` payloads, encrypts them, computes the required MACs,
//! and assembles them into `SyncdMutation` / `SyncdPatch` protobufs ready
//! for wire transmission.

use crate::hash::{generate_content_mac, generate_patch_mac, HashState};
use crate::keys::ExpandedAppStateKeys;
use prost::Message;
use rand::TryRngCore;
use wacore_libsignal::crypto::aes_256_cbc_encrypt_into;
use wacore_libsignal::crypto::CryptographicMac;
use waproto::whatsapp as wa;

/// Compute the HMAC-SHA256 of index bytes using the index key.
///
/// This is the same MAC that [`crate::hash::validate_index_mac`] checks on
/// the decode path; here we produce it for outgoing mutations.
pub fn compute_index_mac(index_json: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let mut mac =
        CryptographicMac::new("HmacSha256", key).expect("HmacSha256 is a valid algorithm");
    mac.update(index_json);
    mac.finalize()
}

/// Encode a single app state mutation.
///
/// This is the reverse of [`crate::decode::decode_record`]. It takes a
/// plaintext `SyncActionValue`, encrypts it with AES-256-CBC, computes the
/// content MAC and index MAC, and returns a fully formed `SyncdMutation`.
///
/// # Arguments
/// * `op` - The operation type (`Set` or `Remove`).
/// * `index` - The index components (e.g. `["mute", "jid"]`).
/// * `action_value` - The action payload to encrypt.
/// * `keys` - The expanded app state keys for this collection.
/// * `key_id` - The key ID to embed in the record.
pub fn encode_mutation(
    op: wa::syncd_mutation::SyncdOperation,
    index: &[String],
    action_value: wa::SyncActionValue,
    keys: &ExpandedAppStateKeys,
    key_id: &[u8],
) -> wa::SyncdMutation {
    // 1. Build SyncActionData with the index serialized as JSON bytes.
    let index_json = serde_json::to_vec(index).expect("index serialization cannot fail");

    let action_data = wa::SyncActionData {
        index: Some(index_json.clone()),
        value: Some(action_value),
        padding: Some(vec![]), // Empty padding — matches whatsmeow
        version: Some(2),
    };
    let plaintext = action_data.encode_to_vec();

    // 2. Compute the index MAC (HMAC-SHA256 of the JSON index).
    let index_mac = compute_index_mac(&index_json, &keys.index);

    // 3. Generate a random 16-byte IV.
    let mut iv = [0u8; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut iv)
        .expect("OS RNG should not fail");

    // 4. AES-256-CBC encrypt the protobuf plaintext.
    let mut ciphertext = Vec::new();
    aes_256_cbc_encrypt_into(&plaintext, &keys.value_encryption, &iv, &mut ciphertext)
        .expect("AES-CBC encryption with valid 32-byte key and 16-byte IV");

    // 5. Build the value blob prefix: IV || ciphertext.
    let mut value_with_iv = Vec::with_capacity(iv.len() + ciphertext.len());
    value_with_iv.extend_from_slice(&iv);
    value_with_iv.extend_from_slice(&ciphertext);

    // 6. Compute the content MAC (HMAC-SHA512 truncated to 32 bytes).
    let value_mac = generate_content_mac(op, &value_with_iv, key_id, &keys.value_mac);

    // 7. Final value blob: IV || ciphertext || value_mac.
    let mut value_blob = value_with_iv;
    value_blob.extend_from_slice(&value_mac);

    // 8. Assemble the SyncdMutation protobuf.
    wa::SyncdMutation {
        operation: Some(op as i32),
        record: Some(wa::SyncdRecord {
            index: Some(wa::SyncdIndex {
                blob: Some(index_mac),
            }),
            value: Some(wa::SyncdValue {
                blob: Some(value_blob),
            }),
            key_id: Some(wa::KeyId {
                id: Some(key_id.to_vec()),
            }),
        }),
    }
}

/// Build a `SyncdPatch` from a set of mutations.
///
/// This updates the caller's [`HashState`] in-place (LTHash, version, and
/// index-value map) and computes the snapshot MAC and patch MAC required by
/// the WhatsApp protocol.
///
/// # Arguments
/// * `mutations` - The encoded mutations to include.
/// * `collection_name` - The app state collection (e.g. `"regular"`, `"regular_high"`).
/// * `new_version` - The version number for this patch.
/// * `keys` - The expanded app state keys.
/// * `key_id` - The key ID to embed in the patch.
/// * `hash_state` - The current hash state; will be mutated.
pub fn build_patch(
    mutations: Vec<wa::SyncdMutation>,
    collection_name: &str,
    new_version: u64,
    keys: &ExpandedAppStateKeys,
    key_id: &[u8],
    hash_state: &mut HashState,
) -> wa::SyncdPatch {
    // 1. Snapshot the current index_value_map so the update_hash closure can
    //    look up previous value MACs without conflicting with the &mut self
    //    borrow on HashState.
    let prev_index_values = hash_state.index_value_map.clone();

    // 2. Update the LTHash with the new mutations.
    let (_, _) = hash_state.update_hash(&mutations, |index_mac, _| {
        let hex_key = hex::encode(index_mac);
        Ok(prev_index_values.get(&hex_key).cloned())
    });

    // 3. Update the index_value_map with the value MACs from the new mutations.
    for mutation in &mutations {
        if let Some(record) = &mutation.record {
            let index_mac_hex = record
                .index
                .as_ref()
                .and_then(|i| i.blob.as_ref())
                .map(|b| hex::encode(b))
                .unwrap_or_default();
            let value_mac = record
                .value
                .as_ref()
                .and_then(|v| v.blob.as_ref())
                .filter(|b| b.len() >= 32)
                .map(|b| b[b.len() - 32..].to_vec());
            if let Some(vm) = value_mac {
                hash_state.index_value_map.insert(index_mac_hex, vm);
            }
        }
    }

    // 4. Advance the version.
    hash_state.version = new_version;

    // 5. Compute the snapshot MAC from the updated hash state.
    let snapshot_mac = hash_state.generate_snapshot_mac(collection_name, &keys.snapshot_mac);

    // 6. Assemble the patch — whatsmeow does NOT set PatchVersion or DeviceIndex
    //    on outgoing patches; only SnapshotMAC, KeyID, and Mutations.
    let mut patch = wa::SyncdPatch {
        mutations,
        snapshot_mac: Some(snapshot_mac),
        key_id: Some(wa::KeyId {
            id: Some(key_id.to_vec()),
        }),
        ..Default::default()
    };

    // 7. Compute the patch MAC (HMAC-SHA256 over snapshot_mac + value MACs + version + name).
    let patch_mac = generate_patch_mac(&patch, collection_name, &keys.patch_mac, new_version);
    patch.patch_mac = Some(patch_mac);

    patch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode_record;
    use crate::hash::validate_index_mac;
    use crate::keys::expand_app_state_keys;
    use crate::lthash::WAPATCH_INTEGRITY;

    fn test_keys() -> ExpandedAppStateKeys {
        expand_app_state_keys(&[7u8; 32])
    }

    #[test]
    fn roundtrip_encode_decode() {
        let keys = test_keys();
        let key_id = b"roundtrip_key";
        let index = vec!["mute".to_string(), "120363@s.whatsapp.net".to_string()];
        let action = wa::SyncActionValue {
            timestamp: Some(1_700_000_000),
            ..Default::default()
        };

        let mutation = encode_mutation(
            wa::syncd_mutation::SyncdOperation::Set,
            &index,
            action.clone(),
            &keys,
            key_id,
        );

        // Decode it back and verify.
        let record = mutation.record.as_ref().expect("record must be present");
        let decoded = decode_record(
            wa::syncd_mutation::SyncdOperation::Set,
            record,
            &keys,
            key_id,
            true, // validate MACs
        )
        .expect("round-trip decode should succeed with valid MACs");

        assert_eq!(decoded.index, index);
        assert_eq!(decoded.action_value.as_ref().and_then(|v| v.timestamp), Some(1_700_000_000));
        assert_eq!(decoded.operation, wa::syncd_mutation::SyncdOperation::Set);
    }

    #[test]
    fn index_mac_matches_validate_index_mac() {
        let keys = test_keys();
        let index_json = serde_json::to_vec(&["star", "some_jid"]).unwrap();
        let mac = compute_index_mac(&index_json, &keys.index);
        validate_index_mac(&index_json, &mac, &keys.index)
            .expect("computed index MAC should pass validation");
    }

    #[test]
    fn value_blob_layout_iv_ciphertext_mac() {
        let keys = test_keys();
        let key_id = b"layout_test";
        let action = wa::SyncActionValue {
            timestamp: Some(42),
            ..Default::default()
        };

        let mutation = encode_mutation(
            wa::syncd_mutation::SyncdOperation::Set,
            &["test".to_string()],
            action,
            &keys,
            key_id,
        );

        let blob = mutation
            .record
            .as_ref()
            .and_then(|r| r.value.as_ref())
            .and_then(|v| v.blob.as_ref())
            .expect("value blob must be present");

        // Blob must be at least 16 (IV) + 16 (min ciphertext block) + 32 (MAC).
        assert!(blob.len() >= 64, "blob length {} is too short", blob.len());

        // Last 32 bytes are the content MAC; verify it.
        let (iv_plus_ct, value_mac) = blob.split_at(blob.len() - 32);
        let expected_mac = generate_content_mac(
            wa::syncd_mutation::SyncdOperation::Set,
            iv_plus_ct,
            key_id,
            &keys.value_mac,
        );
        assert_eq!(value_mac, expected_mac.as_slice());
    }

    #[test]
    fn build_patch_produces_valid_macs() {
        let keys = test_keys();
        let key_id = b"patch_key";
        let mut hash_state = HashState::default();

        let mutation = encode_mutation(
            wa::syncd_mutation::SyncdOperation::Set,
            &["pin".to_string(), "jid@s.whatsapp.net".to_string()],
            wa::SyncActionValue {
                timestamp: Some(999),
                ..Default::default()
            },
            &keys,
            key_id,
        );

        let patch = build_patch(
            vec![mutation],
            "regular",
            1,
            &keys,
            key_id,
            &mut hash_state,
        );

        // Version must be set.
        assert_eq!(
            patch.version.as_ref().and_then(|v| v.version),
            Some(1)
        );

        // Snapshot MAC must be present.
        assert!(patch.snapshot_mac.is_some());

        // Patch MAC must be present and match recomputation.
        let recomputed_patch_mac =
            generate_patch_mac(&patch, "regular", &keys.patch_mac, 1);
        assert_eq!(
            patch.patch_mac.as_ref().expect("patch_mac must be set"),
            &recomputed_patch_mac
        );

        // Hash state version must be updated.
        assert_eq!(hash_state.version, 1);

        // Hash state must no longer be all zeros (LTHash was updated).
        assert_ne!(hash_state.hash, [0u8; 128]);
    }

    #[test]
    fn build_patch_updates_index_value_map() {
        let keys = test_keys();
        let key_id = b"ivm_key";
        let mut hash_state = HashState::default();

        let index = vec!["archive".to_string(), "jid".to_string()];
        let index_json = serde_json::to_vec(&index).unwrap();
        let expected_index_mac_hex = hex::encode(compute_index_mac(&index_json, &keys.index));

        let mutation = encode_mutation(
            wa::syncd_mutation::SyncdOperation::Set,
            &index,
            wa::SyncActionValue {
                timestamp: Some(1),
                ..Default::default()
            },
            &keys,
            key_id,
        );

        let _ = build_patch(
            vec![mutation],
            "regular",
            1,
            &keys,
            key_id,
            &mut hash_state,
        );

        assert!(
            hash_state.index_value_map.contains_key(&expected_index_mac_hex),
            "index_value_map should contain the new mutation's index MAC"
        );
    }

    #[test]
    fn build_patch_overwrite_subtracts_previous_value() {
        let keys = test_keys();
        let key_id = b"overwrite_key";
        let mut hash_state = HashState::default();
        let index = vec!["mute".to_string(), "jid".to_string()];

        // First patch: initial SET.
        let m1 = encode_mutation(
            wa::syncd_mutation::SyncdOperation::Set,
            &index,
            wa::SyncActionValue {
                timestamp: Some(100),
                ..Default::default()
            },
            &keys,
            key_id,
        );
        let _ = build_patch(vec![m1], "regular", 1, &keys, key_id, &mut hash_state);
        let hash_after_first = hash_state.hash;

        // Second patch: overwrite the same index.
        let m2 = encode_mutation(
            wa::syncd_mutation::SyncdOperation::Set,
            &index,
            wa::SyncActionValue {
                timestamp: Some(200),
                ..Default::default()
            },
            &keys,
            key_id,
        );

        // Extract the new value MAC for manual verification.
        let new_value_mac = m2
            .record
            .as_ref()
            .and_then(|r| r.value.as_ref())
            .and_then(|v| v.blob.as_ref())
            .map(|b| b[b.len() - 32..].to_vec())
            .unwrap();

        // Grab the old value MAC from the index_value_map.
        let index_json = serde_json::to_vec(&index).unwrap();
        let index_mac_hex = hex::encode(compute_index_mac(&index_json, &keys.index));
        let old_value_mac = hash_state.index_value_map.get(&index_mac_hex).cloned().unwrap();

        let _ = build_patch(vec![m2], "regular", 2, &keys, key_id, &mut hash_state);

        // The hash should equal: subtract old, add new.
        let expected =
            WAPATCH_INTEGRITY.subtract_then_add(&hash_after_first, &[old_value_mac], &[new_value_mac]);
        assert_eq!(hash_state.hash.as_slice(), expected.as_slice());
    }
}
