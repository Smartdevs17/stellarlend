//! Storage snapshots for versioning and rollback support

use soroban_sdk::{contracttype, Env, Val, IntoVal, TryFromVal, Vec};

/// Snapshot of a storage value with timestamp
#[contracttype]
#[derive(Clone, Debug)]
pub struct SnapshotValue {
    /// The actual stored value
    pub value: Val,
    /// Timestamp when this snapshot was taken
    pub timestamp: u64,
    /// Version number for this snapshot
    pub version: u32,
}

impl SnapshotValue {
    /// Create a new snapshot value
    pub fn new(value: Val, timestamp: u64, version: u32) -> Self {
        SnapshotValue {
            value,
            timestamp,
            version,
        }
    }
}

/// Storage snapshot metadata
#[contracttype]
#[derive(Clone, Debug)]
pub struct StorageSnapshot {
    /// Unique snapshot ID
    pub snapshot_id: u64,
    /// Timestamp when snapshot was created
    pub created_at: u64,
    /// Number of entries in this snapshot
    pub entry_count: u32,
    /// Hash of snapshot contents for verification
    pub content_hash: u64,
    /// Description or label for this snapshot
    pub label: Option<u64>, // Use u64 instead of String to keep it soroban-compatible
}

impl StorageSnapshot {
    /// Create a new storage snapshot
    pub fn new(snapshot_id: u64, created_at: u64, entry_count: u32, content_hash: u64) -> Self {
        StorageSnapshot {
            snapshot_id,
            created_at,
            entry_count,
            content_hash,
            label: None,
        }
    }

    /// Set a label for this snapshot
    pub fn with_label(mut self, label: u64) -> Self {
        self.label = Some(label);
        self
    }
}

/// Snapshot history metadata
#[contracttype]
#[derive(Clone, Debug)]
pub struct SnapshotHistory {
    /// Number of snapshot IDs in chronological order
    pub snapshot_count: u32,
    /// Current active snapshot
    pub current_snapshot_id: u64,
    /// Oldest snapshot ID for rollback capability
    pub oldest_snapshot_id: u64,
}

impl SnapshotHistory {
    /// Create a new snapshot history
    pub fn new(current_id: u64) -> Self {
        SnapshotHistory {
            snapshot_count: 1,
            current_snapshot_id: current_id,
            oldest_snapshot_id: current_id,
        }
    }

    /// Get the number of snapshots
    pub fn len(&self) -> u32 {
        self.snapshot_count
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.snapshot_count == 0
    }
}

/// Snapshot comparison result
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotComparison {
    /// Whether snapshots are identical
    pub are_equal: bool,
    /// Number of differing entries
    pub differing_entries: u32,
}

/// Snapshot management utilities
pub struct SnapshotManager;

impl SnapshotManager {
    /// Create a new snapshot
    pub fn create_snapshot(
        env: &Env,
        snapshot_id: u64,
        entry_count: u32,
        content_hash: u64,
    ) -> StorageSnapshot {
        StorageSnapshot::new(
            snapshot_id,
            env.ledger().timestamp(),
            entry_count,
            content_hash,
        )
    }

    /// Compare two snapshots
    pub fn compare_snapshots(
        snapshot1: &StorageSnapshot,
        snapshot2: &StorageSnapshot,
    ) -> SnapshotComparison {
        let are_equal = snapshot1.content_hash == snapshot2.content_hash;
        let differing_entries = if are_equal { 0 } else { 1 };

        SnapshotComparison {
            are_equal,
            differing_entries,
        }
    }

    /// Calculate time elapsed since snapshot
    pub fn time_since_snapshot(snapshot: &StorageSnapshot, current_time: u64) -> u64 {
        current_time.saturating_sub(snapshot.created_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_creation() {
        let snapshot = StorageSnapshot::new(1, 1000, 10, 12345);
        assert_eq!(snapshot.snapshot_id, 1);
        assert_eq!(snapshot.entry_count, 10);
        assert_eq!(snapshot.content_hash, 12345);
        assert!(snapshot.label.is_none());
    }

    #[test]
    fn test_snapshot_with_label() {
        let snapshot = StorageSnapshot::new(1, 1000, 10, 12345).with_label(999);
        assert_eq!(snapshot.label, Some(999));
    }

    #[test]
    fn test_snapshot_comparison() {
        let snap1 = StorageSnapshot::new(1, 1000, 10, 12345);
        let snap2 = StorageSnapshot::new(2, 2000, 10, 12345);

        let comparison = SnapshotManager::compare_snapshots(&snap1, &snap2);
        assert!(comparison.are_equal);
    }

    #[test]
    fn test_snapshot_comparison_different() {
        let snap1 = StorageSnapshot::new(1, 1000, 10, 12345);
        let snap2 = StorageSnapshot::new(2, 2000, 10, 54321);

        let comparison = SnapshotManager::compare_snapshots(&snap1, &snap2);
        assert!(!comparison.are_equal);
    }

    #[test]
    fn test_time_since_snapshot() {
        let snapshot = StorageSnapshot::new(1, 1000, 10, 12345);
        let elapsed = SnapshotManager::time_since_snapshot(&snapshot, 2000);
        assert_eq!(elapsed, 1000);
    }
}
