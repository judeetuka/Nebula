//! Process recreation recovery for Android.
//!
//! Detects if the node was previously Active before Android killed the process,
//! enabling automatic reconnection to the cluster.

use crate::storage::db::StorageManager;
use anyhow::Result;

/// Check if the node was in Active state before the process was killed.
pub fn check_recovery_needed(storage: &StorageManager) -> Result<Option<String>> {
    match storage.get_config("last_active_state")? {
        Some(state) if state == "active" => {
            let cluster_id = storage.get_config("last_cluster_id")?;
            Ok(cluster_id)
        }
        _ => Ok(None),
    }
}

/// Mark the node as active for recovery detection.
pub fn mark_active(storage: &StorageManager, cluster_id: &str) -> Result<()> {
    storage.set_config("last_active_state", "active")?;
    storage.set_config("last_cluster_id", cluster_id)?;
    Ok(())
}

/// Clear the active marker (on graceful shutdown).
pub fn clear_active(storage: &StorageManager) -> Result<()> {
    storage.remove_config("last_active_state")?;
    storage.remove_config("last_cluster_id")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_recovery_on_fresh_start() {
        let s = StorageManager::new_in_memory(b"recovery-test").unwrap();
        assert_eq!(check_recovery_needed(&s).unwrap(), None);
    }

    #[test]
    fn test_recovery_after_mark_active() {
        let s = StorageManager::new_in_memory(b"recovery-test").unwrap();
        mark_active(&s, "cluster-abc").unwrap();
        assert_eq!(
            check_recovery_needed(&s).unwrap(),
            Some("cluster-abc".into())
        );
    }

    #[test]
    fn test_clear_active_removes_recovery() {
        let s = StorageManager::new_in_memory(b"recovery-test").unwrap();
        mark_active(&s, "c1").unwrap();
        clear_active(&s).unwrap();
        assert_eq!(check_recovery_needed(&s).unwrap(), None);
    }
}
