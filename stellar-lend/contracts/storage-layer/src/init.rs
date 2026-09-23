//! Upgradeable-initialization safety helpers (issue #710).
//!
//! Contracts historically initialized themselves exactly once via a constructor-style
//! flow with no upgrade path. Following the bootstrap pattern adopted in PR #892 for
//! `hello-world` / `migration-hub`, this module provides the shared, storage-agnostic
//! primitives every upgradeable contract needs:
//!
//! * an idempotent [`BootstrapGuard`] — a persisted `Initialized` flag so a contract
//!   can only be bootstrapped once (re-initialization attack protection);
//! * an admin-guarded [`VersionedBootstrapper`] — tracks who may perform an upgrade and
//!   a monotonically increasing version, so upgrades are deterministic and versioned.
//!
//! It deliberately does **not** own any concrete key type — callers provide their key,
//! exactly like PR #892's `bootstrap.rs` / `upgrade.rs` but reusably.

use crate::{Storage, StorageTier};
use core::fmt::Debug;
use soroban_sdk::{contracterror, Address, Env, IntoVal, Symbol, Val};

/// Errors produced by the bootstrap/version machinery.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InitError {
    /// `bootstrap` was called again after the contract was already initialized.
    AlreadyInitialized = 1,
    /// An action that requires a prior bootstrap ran before initialization.
    NotInitialized = 2,
    /// The caller is not the designated upgrade authority.
    NotAuthorized = 3,
    /// An upgrade attempted to move the version backwards.
    VersionRegression = 4,
}

/// Tracks the one-time bootstrap flag.
///
/// `BootstrapGuard` is key-agnostic: callers pass their own key and the storage tier
/// (persistent by default) so it composes with the [`Storage`] facade.
pub struct BootstrapGuard {
    env: Env,
}

impl BootstrapGuard {
    /// Creates a guard bound to `env`.
    pub fn new(env: &Env) -> Self {
        Self { env: env.clone() }
    }

    /// Reads the initialized flag under `init_key`.
    pub fn is_initialized<K>(&self, init_key: &K, tier: StorageTier) -> bool
    where
        K: IntoVal<Env, Val>,
    {
        Storage::new(&self.env).has(tier, init_key)
    }

    /// Marks the contract as bootstrapped under `init_key`. Returns
    /// `AlreadyInitialized` if it was already set (idempotency guard).
    pub fn try_bootstrap<K>(&self, init_key: &K, tier: StorageTier) -> Result<(), InitError>
    where
        K: IntoVal<Env, Val>,
    {
        if self.is_initialized(init_key, tier) {
            return Err(InitError::AlreadyInitialized);
        }
        Storage::new(&self.env).set(tier, init_key, &true);
        Ok(())
    }
}

/// Combines the bootstrap flag with version tracking and an upgrade authority,
/// mirroring PR #892's `bootstrap` + `upgrade` flow as a reusable component. Uses
/// persistent storage and caller-supplied `Symbol` keys to avoid collisions.
pub struct VersionedBootstrapper {
    env: Env,
    init_key: Symbol,
    version_key: Symbol,
    authority_key: Symbol,
}

impl VersionedBootstrapper {
    /// Creates a bootstrapper using fixed `Symbol` keys. Callers choose distinct
    /// symbol names to avoid collisions with their own storage.
    pub fn new(env: &Env, init_key: Symbol, version_key: Symbol, authority_key: Symbol) -> Self {
        Self {
            env: env.clone(),
            init_key,
            version_key,
            authority_key,
        }
    }

    /// One-time bootstrap: records the `authority` and sets version to `version`.
    pub fn bootstrap(&self, authority: &Address, version: u32) -> Result<(), InitError> {
        let storage = Storage::new(&self.env);
        let tier = StorageTier::Persistent;
        if storage.has(tier, &self.init_key) {
            return Err(InitError::AlreadyInitialized);
        }
        storage.set(tier, &self.init_key, &true);
        storage.set(tier, &self.authority_key, authority);
        storage.set(tier, &self.version_key, &version);
        Ok(())
    }

    /// Returns the current version (`0` when not bootstrapped).
    pub fn version(&self) -> u32 {
        Storage::new(&self.env).get_or(StorageTier::Persistent, &self.version_key, 0u32)
    }

    /// Returns the upgrade authority if set.
    pub fn authority(&self) -> Result<Address, InitError> {
        Storage::new(&self.env)
            .get(StorageTier::Persistent, &self.authority_key)
            .ok_or(InitError::NotInitialized)
    }

    /// Admin-guarded upgrade: only `authority` may bump the version, and it must be
    /// strictly forward.
    pub fn upgrade(&self, caller: &Address, next_version: u32) -> Result<(), InitError> {
        let authority = self.authority()?;
        if caller != &authority {
            return Err(InitError::NotAuthorized);
        }
        let current = self.version();
        if next_version <= current {
            return Err(InitError::VersionRegression);
        }
        Storage::new(&self.env).set(StorageTier::Persistent, &self.version_key, &next_version);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    struct C;

    #[contractimpl]
    impl C {
        pub fn ping() {}
    }

    fn class(env: &Env) -> VersionedBootstrapper {
        VersionedBootstrapper::new(
            env,
            Symbol::new(env, "init"),
            Symbol::new(env, "ver"),
            Symbol::new(env, "aut"),
        )
    }

    #[test]
    fn bootstrap_is_idempotent() {
        let env = Env::default();
        let contract = env.register(C, ());
        env.as_contract(&contract, || {
            let b = class(&env);
            let admin = Address::generate(&env);
            assert!(b.bootstrap(&admin, 1).is_ok());
            assert_eq!(b.version(), 1);
            // Second bootstrap rejected.
            assert_eq!(b.bootstrap(&admin, 2), Err(InitError::AlreadyInitialized));
        });
    }

    #[test]
    fn upgrade_requires_authority_and_moves_forward() {
        let env = Env::default();
        let contract = env.register(C, ());
        env.as_contract(&contract, || {
            let b = class(&env);
            let admin = Address::generate(&env);
            let other = Address::generate(&env);
            b.bootstrap(&admin, 1).unwrap();

            // Non-authority rejected.
            assert_eq!(b.upgrade(&other, 2), Err(InitError::NotAuthorized));
            // Authored forward bump ok.
            assert!(b.upgrade(&admin, 2).is_ok());
            assert_eq!(b.version(), 2);
            // Regression rejected.
            assert_eq!(b.upgrade(&admin, 1), Err(InitError::VersionRegression));
        });
    }
}
