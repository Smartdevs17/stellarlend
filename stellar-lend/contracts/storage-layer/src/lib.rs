//! # StellarLend Storage Abstraction Layer (issue #707)
//!
//! Contracts historically reached for `env.storage()` with slightly different
//! conventions — some used a "cache-aware" `get_snapshot(force_direct)` accessor
//! (`hello-world`), others used plain `persistent().get()`/`set()` everywhere
//! (`lending/data_store`). This crate consolidates the *access pattern* so every
//! module reads and writes storage identically, while remaining generic over concrete
//! key/value types so no contract has to adopt a whole new data model.
//!
//! ## Features
//!
//! * [`Storage`] — a thin, tier-aware read/write facade over instance, persistent and
//!   temporary storage.
//! * [`cache`] — a "snapshot with cache bypass" helper generalizing the
//!   `get_snapshot(key, force_direct)` pattern: callers can serve from cache normally
//!   and force a direct read when invalidation requires it.
//! * [`migration`] — schema-version-tracking helpers so storage can be migrated
//!   safely across contract upgrades.
//! * [`init`] — upgradeable-initialization safety helpers (idempotent bootstrap guard,
//!   admin-guarded versioned upgrades), the shared backing for issue #710.
//!
//! This crate is `#![no_std]` so it compiles into Soroban WASM. It is intentionally
//! independent of [`stellarlend-common`'s `storage`](../../common/src/storage.rs),
//! which provides the get-or-default pattern; this crate adds the cache/migration
//! layers on top.

#![no_std]

use core::fmt::Debug;
use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

pub mod cache;
pub mod init;
pub mod migration;

pub use cache::{get_snapshot, CacheableStorage};
pub use init::{BootstrapGuard, InitError, VersionedBootstrapper};
pub use migration::{SchemaVersion, VersionedStorage};

/// Tier of Soroban storage a value lives in.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StorageTier {
    /// Per-contract instance storage (bumps automatically, limited size).
    Instance,
    /// Persistent storage (caller manages TTL).
    Persistent,
    /// Temporary transaction-scoped storage.
    Temporary,
}

/// A small, tier-aware facade over the three Soroban storage tiers.
///
/// All accessors are generic over `K`/`V` so any contract key/value type works. The
/// facade centralizes *where* values go so modules stop hand-rolling reads/writes per
/// tier.
#[derive(Clone)]
pub struct Storage {
    env: Env,
}

impl Storage {
    /// Creates the facade bound to `env`.
    pub fn new(env: &Env) -> Self {
        Self { env: env.clone() }
    }

    /// Reads `key` from `tier`, returning `None` when absent.
    pub fn get<K, V>(&self, tier: StorageTier, key: &K) -> Option<V>
    where
        K: IntoVal<Env, Val>,
        V: TryFromVal<Env, Val>,
        V::Error: Debug,
    {
        match tier {
            StorageTier::Instance => self.env.storage().instance().get(key),
            StorageTier::Persistent => self.env.storage().persistent().get(key),
            StorageTier::Temporary => self.env.storage().temporary().get(key),
        }
    }

    /// Reads `key`, falling back to `default` when absent.
    pub fn get_or<K, V>(&self, tier: StorageTier, key: &K, default: V) -> V
    where
        K: IntoVal<Env, Val>,
        V: TryFromVal<Env, Val>,
        V::Error: Debug,
    {
        self.get::<K, V>(tier, key).unwrap_or(default)
    }

    /// Writes `value` to `key` in `tier`.
    pub fn set<K, V>(&self, tier: StorageTier, key: &K, value: &V)
    where
        K: IntoVal<Env, Val>,
        V: IntoVal<Env, Val>,
    {
        match tier {
            StorageTier::Instance => self.env.storage().instance().set(key, value),
            StorageTier::Persistent => self.env.storage().persistent().set(key, value),
            StorageTier::Temporary => self.env.storage().temporary().set(key, value),
        }
    }

    /// Returns `true` when `key` is present in `tier`.
    pub fn has<K>(&self, tier: StorageTier, key: &K) -> bool
    where
        K: IntoVal<Env, Val>,
    {
        match tier {
            StorageTier::Instance => self.env.storage().instance().has(key),
            StorageTier::Persistent => self.env.storage().persistent().has(key),
            StorageTier::Temporary => self.env.storage().temporary().has(key),
        }
    }

    /// Removes `key` from `tier` (persistent/instance only; temporary is auto-scoped).
    pub fn remove<K>(&self, tier: StorageTier, key: &K)
    where
        K: IntoVal<Env, Val>,
    {
        match tier {
            StorageTier::Instance => self.env.storage().instance().remove(key),
            StorageTier::Persistent => self.env.storage().persistent().remove(key),
            StorageTier::Temporary => self.env.storage().temporary().remove(key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Symbol};

    #[contracttype]
    #[derive(Clone)]
    enum TestKey {
        Counter,
        Text,
    }

    #[contract]
    struct StorageTestContract;

    #[contractimpl]
    impl StorageTestContract {
        pub fn ping() {}
    }

    fn setup() -> (Env, Address, Storage) {
        let env = Env::default();
        let contract = env.register(StorageTestContract, ());
        let storage = Storage::new(&env);
        (env, contract, storage)
    }

    #[test]
    fn persistent_get_or_and_set_round_trip() {
        let (env, contract, storage) = setup();
        env.as_contract(&contract, || {
            assert_eq!(
                storage.get_or(StorageTier::Persistent, &TestKey::Counter, 42i128),
                42i128
            );
            storage.set(StorageTier::Persistent, &TestKey::Counter, &7i128);
            assert_eq!(
                storage.get(StorageTier::Persistent, &TestKey::Counter),
                Some(7i128)
            );
            assert!(storage.has(StorageTier::Persistent, &TestKey::Counter));
            storage.remove(StorageTier::Persistent, &TestKey::Counter);
            assert!(!storage.has(StorageTier::Persistent, &TestKey::Counter));
        });
    }

    #[test]
    fn instance_store_works() {
        let (env, contract, storage) = setup();
        env.as_contract(&contract, || {
            storage.set(
                StorageTier::Instance,
                &TestKey::Text,
                &Symbol::new(&env, "hello"),
            );
            assert!(storage.has(StorageTier::Instance, &TestKey::Text));
        });
    }
}
