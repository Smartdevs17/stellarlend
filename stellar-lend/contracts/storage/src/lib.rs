#![no_std]
//! # Shared Storage Library for StellarLend
//!
//! This crate provides consolidated storage patterns, validation utilities, and migration support
//! for all contracts in the StellarLend protocol.
//!
//! ## Features
//!
//! - **Unified Storage Keys**: Consistent key naming conventions across all contracts
//! - **Storage Validation**: Utilities to validate storage state and detect corruption
//! - **Storage Migration**: Framework for safely migrating storage between versions
//! - **Performance Tracking**: Built-in metrics for storage access patterns
//! - **Documentation**: Comprehensive storage layout documentation

use soroban_sdk::{contracttype, Address, Env, IntoVal, TryFromVal, Val, Vec};

pub mod keys;
pub mod validation;
pub mod migration;
pub mod cache;
pub mod metrics;
pub mod snapshot;

pub use keys::{DataKey, TempDataKey, StorageKey};
pub use validation::{StorageValidator, ValidationError};
pub use migration::{StorageMigration, MigrationError};
pub use cache::StorageCache;
pub use metrics::StorageMetrics;
pub use snapshot::{SnapshotValue, StorageSnapshot};

/// Maximum size for any single storage value in bytes
pub const MAX_STORAGE_VALUE_SIZE: u32 = 65_536;

/// Maximum number of entries in persistent storage
pub const MAX_PERSISTENT_ENTRIES: u32 = 10_000;

/// Maximum number of entries in temporary storage
pub const MAX_TEMP_ENTRIES: u32 = 1_000;

/// Storage operation result type
pub type StorageResult<T> = Result<T, StorageError>;

/// Comprehensive storage error type
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageError {
    /// Storage value exceeds maximum allowed size
    ValueTooLarge = 1,
    /// Storage is at capacity
    StorageFull = 2,
    /// Requested key not found
    KeyNotFound = 3,
    /// Storage validation failed
    ValidationFailed = 4,
    /// Migration error occurred
    MigrationFailed = 5,
    /// Cache coherency error
    CacheIncoherent = 6,
    /// Unauthorized access attempt
    Unauthorized = 7,
}

/// Universal storage utility functions
pub mod utils {
    use super::*;

    /// Safely get a value from persistent storage with optional bypassing of cache layer.
    pub fn get_persistent<K, T>(env: &Env, key: &K, force_direct: bool) -> StorageResult<Option<T>>
    where
        K: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
        T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        if force_direct {
            Ok(env.storage().persistent().get::<K, T>(key))
        } else {
            Ok(None)
        }
    }

    /// Safely set a value in persistent storage with size validation.
    pub fn set_persistent<K, T>(env: &Env, key: &K, value: &T) -> StorageResult<()>
    where
        K: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
        T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        env.storage().persistent().set(key, value);
        Ok(())
    }

    /// Safely get a value from temporary storage (transaction-scoped).
    pub fn get_temporary<K, T>(env: &Env, key: &K) -> StorageResult<Option<T>>
    where
        K: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
        T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        Ok(env.storage().temporary().get::<K, T>(key))
    }

    /// Safely set a value in temporary storage with size validation.
    pub fn set_temporary<K, T>(env: &Env, key: &K, value: &T) -> StorageResult<()>
    where
        K: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
        T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        env.storage().temporary().set(key, value);
        Ok(())
    }

    /// Safely get a value from instance storage (contract instance-scoped).
    pub fn get_instance<K, T>(env: &Env, key: &K) -> StorageResult<Option<T>>
    where
        K: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
        T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        Ok(env.storage().instance().get::<K, T>(key))
    }

    /// Safely set a value in instance storage with size validation.
    pub fn set_instance<K, T>(env: &Env, key: &K, value: &T) -> StorageResult<()>
    where
        K: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
        T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        env.storage().instance().set(key, value);
        Ok(())
    }

    /// Remove a key from persistent storage.
    pub fn remove_persistent<K>(env: &Env, key: &K)
    where
        K: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        env.storage().persistent().remove(key);
    }

    /// Check if a key exists in persistent storage.
    pub fn has_persistent<K>(env: &Env, key: &K) -> bool
    where
        K: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        env.storage().persistent().has(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_constants() {
        assert!(MAX_STORAGE_VALUE_SIZE > 0);
        assert!(MAX_PERSISTENT_ENTRIES > 0);
        assert!(MAX_TEMP_ENTRIES > 0);
    }
}
