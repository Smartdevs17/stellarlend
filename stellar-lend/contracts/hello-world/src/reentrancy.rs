// ════════════════════════════════════════════════════════════════
// REENTRANCY GUARD (hello-world)
// ════════════════════════════════════════════════════════════════
// Thin, behaviour-preserving wrapper over the shared reusable
// reentrancy primitive in `stellarlend-security`. All locking logic
// lives in the shared crate; this module exposes the historical
// hello-world API (bare `u32` error codes) so existing call sites are
// unchanged.
// ════════════════════════════════════════════════════════════════

use soroban_sdk::{contracttype, Address, Env, Val};
use stellarlend_security::ReentrancyError as SharedError;

/// Reentrancy guard state tracking
#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
pub enum GuardState {
    NotEntered = 0,
    Entered = 1,
}

/// Storage keys for reentrancy guards.
///
/// Mirrors the historical hello-world key set; each variant maps onto the
/// shared [`stellarlend_security::ReentrancyKey`].
#[contracttype]
#[derive(Clone)]
pub enum ReentrancyKey {
    /// Global reentrancy lock
    GlobalLock,
    /// Function-specific locks
    DepositLock,
    WithdrawLock,
    BorrowLock,
    RepayLock,
    LiquidateLock,
    FlashLoanLock,
    /// Cross-contract reentrancy tracking
    CrossContractLock(Address),
    /// Read-only reentrancy detection
    ReadOnlyLock,
    /// Constructor reentrancy protection
    ConstructorLock,
    /// Delegate call reentrancy protection
    DelegateCallLock,
}

impl ReentrancyKey {
    fn to_shared(&self) -> stellarlend_security::ReentrancyKey {
        use stellarlend_security::ReentrancyKey as K;
        match self {
            ReentrancyKey::GlobalLock => K::GlobalLock,
            ReentrancyKey::DepositLock => K::DepositLock,
            ReentrancyKey::WithdrawLock => K::WithdrawLock,
            ReentrancyKey::BorrowLock => K::BorrowLock,
            ReentrancyKey::RepayLock => K::RepayLock,
            ReentrancyKey::LiquidateLock => K::LiquidateLock,
            ReentrancyKey::FlashLoanLock => K::FlashLoanLock,
            ReentrancyKey::CrossContractLock(a) => K::CrossContractLock(a.clone()),
            ReentrancyKey::ReadOnlyLock => K::ReadOnlyLock,
            ReentrancyKey::ConstructorLock => K::ConstructorLock,
            ReentrancyKey::DelegateCallLock => K::DelegateCallLock,
        }
    }
}

/// Legacy error code mapping to the historical hello-world `u32` errors.
fn map_err(e: SharedError) -> u32 {
    match e {
        SharedError::ReentrancyDetected => 7,
        SharedError::CrossContractReentrancy => 8,
        SharedError::ConstructorReentrancy => 9,
        SharedError::DelegateCallReentrancy => 10,
    }
}

/// Comprehensive reentrancy guard with RAII pattern.
pub struct ReentrancyGuard<'a> {
    inner: stellarlend_security::ReentrancyGuard<'a>,
}

impl<'a> ReentrancyGuard<'a> {
    /// Create a new global reentrancy guard
    pub fn new(env: &'a Env) -> Result<Self, u32> {
        stellarlend_security::ReentrancyGuard::new(env)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    /// Create a new reentrancy guard with a specific key
    pub fn new_with_key(env: &'a Env, key: ReentrancyKey, is_read_only: bool) -> Result<Self, u32> {
        stellarlend_security::ReentrancyGuard::new_with_key(env, key.to_shared(), is_read_only)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    /// Create a cross-contract reentrancy guard
    pub fn new_cross_contract(env: &'a Env, caller: &Address) -> Result<Self, u32> {
        stellarlend_security::ReentrancyGuard::new_with_caller(
            env,
            stellarlend_security::ReentrancyKey::GlobalLock,
            caller,
            false,
        )
        .map(|inner| Self { inner })
        .map_err(map_err)
    }

    /// Create a read-only reentrancy guard
    pub fn new_read_only(env: &'a Env) -> Result<Self, u32> {
        stellarlend_security::ReentrancyGuard::new_read_only(env)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    /// Create a constructor reentrancy guard
    pub fn new_constructor(env: &'a Env) -> Result<Self, u32> {
        stellarlend_security::ReentrancyGuard::new_constructor(env)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    /// Create a delegate call reentrancy guard
    pub fn new_delegate_call(env: &'a Env) -> Result<Self, u32> {
        stellarlend_security::ReentrancyGuard::new_delegate_call(env)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    /// Check if this is a read-only reentrancy
    pub fn is_read_only_reentrancy(&self) -> bool {
        self.inner.is_read_only_reentrancy()
    }
}

// Re-export a type alias so downstream callers that refer to the guard by path
// (e.g. `crate::reentrancy::ReentrancyGuard`) keep working.
pub use self::ReentrancyGuard as Guard;

/// Helper macro for function-level reentrancy guards
#[macro_export]
macro_rules! reentrancy_guard {
    ($env:expr, $key:expr) => {
        $crate::reentrancy::ReentrancyGuard::new_with_key($env, $key, false)
    };
}

/// Helper macro for cross-contract reentrancy guards
#[macro_export]
macro_rules! cross_contract_guard {
    ($env:expr, $caller:expr) => {
        $crate::reentrancy::ReentrancyGuard::new_cross_contract($env, $caller)
    };
}

/// Helper macro for read-only reentrancy guards
#[macro_export]
macro_rules! read_only_guard {
    ($env:expr) => {
        $crate::reentrancy::ReentrancyGuard::new_read_only($env)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_state_transitions() {
        assert_eq!(GuardState::NotEntered, GuardState::NotEntered);
        assert_eq!(GuardState::Entered, GuardState::Entered);
    }
}
