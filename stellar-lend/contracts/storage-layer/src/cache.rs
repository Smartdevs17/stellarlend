//! Cache-aware storage helpers.
//!
//! Generalizes the `get_snapshot(key, force_direct)` pattern found in
//! `hello-world/src/storage.rs`: normally callers serve a value from an in-memory /
//! ephemeral cache, but when invalidation requires it they force a direct read from
//! persistent storage. This module provides that decision uniformly.

use core::fmt::Debug;
use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

/// Cache-aware storage facade.
///
/// `get_snapshot` returns the cached value when `force_direct` is false (the caller
/// owns the cache) and the authoritative persistent value when `force_direct` is true
/// or when no cache is present. This matches the `hello-world` contract's intent
/// without each module re-implementing the branch.
#[derive(Clone)]
pub struct CacheableStorage {
    env: Env,
}

impl CacheableStorage {
    /// Creates the facade bound to `env`.
    pub fn new(env: &Env) -> Self {
        Self { env: env.clone() }
    }

    /// Reads a snapshot: direct persistent read when `force_direct`, otherwise `None`
    /// (the caller is expected to serve from its cache).
    pub fn snapshot<K, T>(&self, key: &K, force_direct: bool) -> Option<T>
    where
        K: IntoVal<Env, Val>,
        T: TryFromVal<Env, Val>,
        T::Error: Debug,
    {
        if force_direct {
            self.env.storage().persistent().get(key)
        } else {
            None
        }
    }
}

/// Free-function form of [`CacheableStorage::snapshot`], mirroring the historical
/// `get_snapshot(env, key, force_direct)` signature used by `hello-world`.
pub fn get_snapshot<K, T>(env: &Env, key: &K, force_direct: bool) -> Option<T>
where
    K: IntoVal<Env, Val>,
    T: TryFromVal<Env, Val>,
    T::Error: Debug,
{
    CacheableStorage::new(env).snapshot(key, force_direct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, contracttype};

    #[contracttype]
    #[derive(Clone)]
    enum Key {
        V,
    }

    #[contract]
    struct C;

    #[contractimpl]
    impl C {
        pub fn ping() {}
    }

    #[test]
    fn snapshot_returns_direct_when_forced_and_none_otherwise() {
        let env = Env::default();
        let contract = env.register(C, ());
        let storage = CacheableStorage::new(&env);

        env.as_contract(&contract, || {
            env.storage().persistent().set(&Key::V, &123i128);
            // Without force, the caller serves from cache -> None.
            assert_eq!(storage.snapshot(&Key::V, false), None::<i128>);
            // With force, we read straight from persistent storage.
            assert_eq!(storage.snapshot(&Key::V, true), Some(123i128));
        });
    }
}
