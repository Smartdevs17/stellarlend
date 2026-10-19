//! Reentrancy protection for value-moving AMM entrypoints.
//!
//! The guard arms a lock in temporary storage when an entrypoint starts and
//! clears it when the guard is dropped, so the lock is released on every exit
//! path, including early `?` returns.

use soroban_sdk::{contracttype, Env, IntoVal, Val};

use crate::amm::AmmError;

/// Storage key for the AMM reentrancy lock.
#[contracttype]
#[derive(Clone)]
enum ReentrancyKey {
    Lock,
}

/// RAII guard that rejects nested entry into guarded AMM entrypoints.
pub(crate) struct ReentrancyGuard<'a> {
    env: &'a Env,
    key: Val,
}

impl<'a> ReentrancyGuard<'a> {
    /// Acquire the lock, failing with [`AmmError::ReentrancyDetected`] if it is already held.
    pub(crate) fn enter(env: &'a Env) -> Result<Self, AmmError> {
        let key: Val = ReentrancyKey::Lock.into_val(env);
        if env.storage().temporary().has(&key) {
            return Err(AmmError::ReentrancyDetected);
        }
        env.storage().temporary().set(&key, &true);
        Ok(Self { env, key })
    }
}

impl Drop for ReentrancyGuard<'_> {
    fn drop(&mut self) {
        self.env.storage().temporary().remove(&self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AmmContract;

    #[test]
    fn nested_entry_is_rejected_and_lock_is_released_on_drop() {
        let env = Env::default();
        let contract_id = env.register(AmmContract {}, ());

        env.as_contract(&contract_id, || {
            let outer = ReentrancyGuard::enter(&env).expect("first entry should succeed");
            assert_eq!(
                ReentrancyGuard::enter(&env).err(),
                Some(AmmError::ReentrancyDetected)
            );

            drop(outer);
            assert!(ReentrancyGuard::enter(&env).is_ok());
        });
    }
}
