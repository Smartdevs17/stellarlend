// Reusable reentrancy guard implementation.

use crate::error::{GuardBehavior, ReentrancyError};
use crate::keys::ReentrancyKey;
use crate::GuardState;
use soroban_sdk::{Address, Env, IntoVal, Val};

/// Configuration for a reentrancy guard instance.
///
/// Contracts may adjust behaviour without changing call sites.
/// [`ReentrancyGuardConfig::default`] matches the historical default
/// behaviour of the per-contract guards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReentrancyGuardConfig {
    /// Behaviour when a read-only guard is re-entered.
    pub read_only_behavior: GuardBehavior,
}

impl Default for ReentrancyGuardConfig {
    fn default() -> Self {
        ReentrancyGuardConfig {
            read_only_behavior: GuardBehavior::AllowReadOnlyReentry,
        }
    }
}

/// Summary counter produced when a bundle of guards is provisioned through
/// [`ReentrancyGuard::configure`]. Useful for tests and gas bookkeeping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReentrancyConfigured {
    /// Number of function-level guard keys configured.
    pub function_guards: u32,
    /// Whether cross-contract caller-bound guards are enabled.
    pub cross_contract_guards: u32,
    /// Number of read-only guard keys configured.
    pub read_only_guards: u32,
    /// Whether the constructor guard is enabled.
    pub constructor_guards: u32,
}

/// RAII reentrancy guard.
///
/// The guard is armed (writes a temporary-storage marker) on construction
/// and automatically disarmed via [`Drop`] when it leaves scope, even on
/// panic — the checks-effects-interactions pattern central to safe
/// execution.
pub struct ReentrancyGuard<'a> {
    env: &'a Env,
    key: Val,
    cross_contract_key: Option<Val>,
    state_before: GuardState,
    is_read_only: bool,
    _config: ReentrancyGuardConfig,
}

fn lock_key(env: &Env, key: &ReentrancyKey) -> Val {
    key.clone().into_val(env)
}

impl<'a> ReentrancyGuard<'a> {
    /// Create a global reentrancy guard.
    pub fn new(env: &'a Env) -> Result<Self, ReentrancyError> {
        Self::new_with_key(env, ReentrancyKey::GlobalLock, false)
    }

    /// Create a guard bound to a specific key.
    pub fn new_with_key(
        env: &'a Env,
        key: ReentrancyKey,
        is_read_only: bool,
    ) -> Result<Self, ReentrancyError> {
        Self::with_config(env, key, is_read_only, ReentrancyGuardConfig::default(), None)
    }

    /// Create a guard with explicit configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn with_config(
        env: &'a Env,
        key: ReentrancyKey,
        is_read_only: bool,
        config: ReentrancyGuardConfig,
        caller: Option<&Address>,
    ) -> Result<Self, ReentrancyError> {
        // Cross-contract lock (caller-bound) is armed first so a re-entering
        // contract is rejected regardless of which function key it uses.
        let cross_contract_key = if let Some(caller) = caller {
            let cc_key = lock_key(env, &ReentrancyKey::CrossContractLock(caller.clone()));
            if env.storage().temporary().has(&cc_key) {
                return Err(ReentrancyError::CrossContractReentrancy);
            }
            env.storage().temporary().set(&cc_key, &true);
            Some(cc_key)
        } else {
            None
        };

        let storage_key = lock_key(env, &key);

        // Read-only reentry is tracked (or rejected) per configuration.
        let was_entered = env.storage().temporary().has(&storage_key);

        if is_read_only
            && was_entered
            && config.read_only_behavior == GuardBehavior::RejectReadOnlyReentry
        {
            return Err(ReentrancyError::ReentrancyDetected);
        }

        // Non-reentrant (write) guards reject a second entry outright.
        if !is_read_only && was_entered {
            return Err(ReentrancyError::ReentrancyDetected);
        }
        env.storage().temporary().set(&storage_key, &true);

        let state_before = if is_read_only && was_entered {
            GuardState::Entered
        } else {
            GuardState::NotEntered
        };

        Ok(Self {
            env,
            key: storage_key,
            cross_contract_key,
            state_before,
            is_read_only,
            _config: config,
        })
    }

    /// Provision a bundle of guards, returning a configuration summary.
    /// This is primarily a documentation/testing utility demonstrating how a
    /// contract wires multiple guard keys in one place.
    pub fn configure(keys: &[(&str, bool)]) -> ReentrancyConfigured {
        let mut out = ReentrancyConfigured::default();
        for (name, is_read_only) in keys {
            if *is_read_only {
                out.read_only_guards += 1;
            } else {
                out.function_guards += 1;
            }
            if *name == "cross_contract" {
                out.cross_contract_guards = 1;
            }
        }
        out
    }

    /// Like [`ReentrancyGuard::new_with_key`], but also arms a cross-contract
    /// lock bound to the given caller address.
    pub fn new_with_caller(
        env: &'a Env,
        key: ReentrancyKey,
        caller: &Address,
        is_read_only: bool,
    ) -> Result<Self, ReentrancyError> {
        Self::with_config(
            env,
            key,
            is_read_only,
            ReentrancyGuardConfig::default(),
            Some(caller),
        )
    }

    /// Create a constructor/initializer reentrancy guard.
    pub fn new_constructor(env: &'a Env) -> Result<Self, ReentrancyError> {
        let storage_key = lock_key(env, &ReentrancyKey::ConstructorLock);
        if env.storage().temporary().has(&storage_key) {
            return Err(ReentrancyError::ConstructorReentrancy);
        }
        env.storage().temporary().set(&storage_key, &true);
        Ok(Self {
            env,
            key: storage_key,
            cross_contract_key: None,
            state_before: GuardState::NotEntered,
            is_read_only: false,
            _config: ReentrancyGuardConfig::default(),
        })
    }

    /// Create a delegate-call reentrancy guard.
    pub fn new_delegate_call(env: &'a Env) -> Result<Self, ReentrancyError> {
        let storage_key = lock_key(env, &ReentrancyKey::DelegateCallLock);
        if env.storage().temporary().has(&storage_key) {
            return Err(ReentrancyError::DelegateCallReentrancy);
        }
        env.storage().temporary().set(&storage_key, &true);
        Ok(Self {
            env,
            key: storage_key,
            cross_contract_key: None,
            state_before: GuardState::NotEntered,
            is_read_only: false,
            _config: ReentrancyGuardConfig::default(),
        })
    }

    /// Create a read-only reentrancy guard. Read-only functions may be
    /// re-entered, but the re-entrancy is tracked and surfaced via
    /// [`ReentrancyGuard::is_read_only_reentrancy`].
    pub fn new_read_only(env: &'a Env) -> Result<Self, ReentrancyError> {
        let storage_key = lock_key(env, &ReentrancyKey::ReadOnlyLock);
        let state_before = if env.storage().temporary().has(&storage_key) {
            GuardState::Entered
        } else {
            GuardState::NotEntered
        };
        env.storage().temporary().set(&storage_key, &true);
        Ok(Self {
            env,
            key: storage_key,
            cross_contract_key: None,
            state_before,
            is_read_only: true,
            _config: ReentrancyGuardConfig::default(),
        })
    }

    /// Whether this is a read-only reentrancy (i.e. a read-only guard was
    /// entered while already in a read-only context).
    pub fn is_read_only_reentrancy(&self) -> bool {
        self.is_read_only && self.state_before == GuardState::Entered
    }
}

impl<'a> Drop for ReentrancyGuard<'a> {
    fn drop(&mut self) {
        self.env.storage().temporary().remove(&self.key);
        if let Some(cc_key) = self.cross_contract_key.take() {
            self.env.storage().temporary().remove(&cc_key);
        }
    }
}
