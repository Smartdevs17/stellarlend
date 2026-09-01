//! Storage migration / schema-version helpers.
//!
//! When a contract's storage layout changes across an upgrade, deployed state must be
//! migrated safely. These helpers centralize the "track a schema version, only migrate
//! forward, never regress" pattern so every contract agrees on the rules.

use core::fmt::Debug;
use soroban_sdk::{contracterror, Env, IntoVal, Val};

/// Errors produced by the versioned migration machinery.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MigrationError {
    /// A migration requested a *decrease* in schema version.
    VersionRegression = 1,
    /// A version was read that is ahead of what this build knows how to migrate.
    UnsupportedVersion = 2,
}

/// Tracks the current storage schema version so migrations can run in order.
///
/// `VersionedStorage` owns the `schema_version` key location and exposes monotonic
/// "bump only" semantics. Concrete controllers supply the key enum and the storage
/// `Env`.
pub struct VersionedStorage {
    env: Env,
}

impl VersionedStorage {
    /// Creates a versioned facade.
    pub fn new(env: &Env) -> Self {
        Self { env: env.clone() }
    }

    /// Reads the current schema version, defaulting to `0` (un-initialised).
    pub fn version<K>(&self, version_key: &K) -> u32
    where
        K: IntoVal<Env, Val>,
    {
        self.env
            .storage()
            .persistent()
            .get::<K, u32>(version_key)
            .unwrap_or(0)
    }

    /// Writes `next` as the current version, refusing to regress (returns
    /// [`MigrationError::VersionRegression`] when `next < current`).
    pub fn bump<K>(&self, version_key: &K, next: u32) -> Result<(), MigrationError>
    where
        K: IntoVal<Env, Val> + Clone,
    {
        let current = self.version(version_key);
        if next < current {
            return Err(MigrationError::VersionRegression);
        }
        self.env.storage().persistent().set(version_key, &next);
        Ok(())
    }
}

/// Shared schema-version downgrade guard usable by any `#[contracterror]` dispatcher.
pub trait SchemaVersion {
    /// The build's highest supported schema version.
    const CURRENT: u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, contracttype};

    #[contracttype]
    #[derive(Clone)]
    enum Key {
        SchemaVersion,
    }

    #[contract]
    struct V;

    #[contractimpl]
    impl V {
        pub fn ping() {}
    }

    #[test]
    fn version_defaults_to_zero() {
        let env = Env::default();
        let contract = env.register(V, ());
        let vs = VersionedStorage::new(&env);
        env.as_contract(&contract, || {
            assert_eq!(vs.version(&Key::SchemaVersion), 0);
        });
    }

    #[test]
    fn bump_is_monotonic() {
        let env = Env::default();
        let contract = env.register(V, ());
        let vs = VersionedStorage::new(&env);
        env.as_contract(&contract, || {
            assert!(vs.bump(&Key::SchemaVersion, 3).is_ok());
            assert_eq!(vs.version(&Key::SchemaVersion), 3);
            // Regression is rejected and leaves the stored version intact.
            assert_eq!(
                vs.bump(&Key::SchemaVersion, 1),
                Err(MigrationError::VersionRegression)
            );
            assert_eq!(vs.version(&Key::SchemaVersion), 3);
        });
    }
}
