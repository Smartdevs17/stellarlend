//! Unified storage key definitions for all contracts

use soroban_sdk::{contracttype, Address};

/// Universal data key for persistent storage across contracts
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DataKey {
    // Admin and authorization
    Admin,
    Owner,

    // Configuration
    Config,
    Parameters,

    // User data
    User(Address),
    UserBalance(Address),
    UserPosition(Address),

    // Asset data
    Asset(Address),
    AssetConfig(Address),
    AssetPrice(Address),

    // Counters and indices
    NextId,
    Count,
    Index,

    // Governance
    Governance,
    Proposal(u64),
    Vote(u64, Address),

    // Custom key for contracts to extend
    Custom(u32),
}

/// Temporary (transaction-scoped) storage keys
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum TempDataKey {
    // Cache entries
    TokenBalance(Address, Address),
    AssetPrice(Address),
    InterestIndex,
    LendingIndex,

    // Transaction state
    FlashLoanAmount,
    ReentrancyGuard,

    // Accumulated values
    AccumulatedValue,
    AccumulatedFee,

    // Custom key for contracts to extend
    Custom(u32),
}

/// Snapshot keys for versioning
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SnapshotKey {
    Version,
    Timestamp,
    Hash,
}

/// Generic storage key structure for extensibility
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    Persistent(DataKey),
    Temporary(TempDataKey),
    Instance(DataKey),
}

impl StorageKey {
    /// Create a persistent storage key
    pub fn persistent(key: DataKey) -> Self {
        StorageKey::Persistent(key)
    }

    /// Create a temporary storage key
    pub fn temporary(key: TempDataKey) -> Self {
        StorageKey::Temporary(key)
    }

    /// Create an instance storage key
    pub fn instance(key: DataKey) -> Self {
        StorageKey::Instance(key)
    }
}

/// Namespace prefix for contracts to avoid key collisions
#[derive(Clone, Debug, PartialEq)]
pub struct StorageNamespace {
    pub prefix: u32,
}

impl StorageNamespace {
    pub fn new(prefix: u32) -> Self {
        StorageNamespace { prefix }
    }

    /// Create a namespaced data key
    pub fn data_key(&self, key: DataKey) -> (u32, DataKey) {
        (self.prefix, key)
    }

    /// Create a namespaced temp key
    pub fn temp_key(&self, key: TempDataKey) -> (u32, TempDataKey) {
        (self.prefix, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_namespace() {
        let ns = StorageNamespace::new(1);
        let key = DataKey::Admin;
        let (prefix, namespaced_key) = ns.data_key(key.clone());
        assert_eq!(prefix, 1);
        assert_eq!(namespaced_key, key);
    }
}
