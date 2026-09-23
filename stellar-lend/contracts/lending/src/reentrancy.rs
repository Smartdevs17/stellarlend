// Reentrancy guard for the lending protocol.
//
// Behaviour-preserving wrapper over the shared, reusable reentrancy
// primitive in `stellarlend-security`. All locking logic lives in the
// shared crate; this module keeps the historical contract-local public
// surface (`ReentrancyError`, `ReentrancyKey`, `ReentrancyGuard`) so
// existing call sites are unchanged.

use soroban_sdk::{contracterror, contracttype, Address, Env};
use stellarlend_security::ReentrancyError as SharedError;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReentrancyError {
    ReentrancyDetected = 1,
    CrossContractReentrancy = 2,
    ConstructorReentrancy = 3,
    DelegateCallReentrancy = 4,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
pub enum GuardState {
    NotEntered = 0,
    Entered = 1,
}

#[contracttype]
#[derive(Clone)]
pub enum ReentrancyKey {
    GlobalLock,
    DepositLock,
    WithdrawLock,
    BorrowLock,
    RepayLock,
    LiquidateLock,
    FlashLoanLock,
    DepositCollateralLock,
    CrossContractLock(Address),
    ReadOnlyLock,
    ConstructorLock,
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
            ReentrancyKey::DepositCollateralLock => K::DepositCollateralLock,
            ReentrancyKey::CrossContractLock(a) => K::CrossContractLock(a.clone()),
            ReentrancyKey::ReadOnlyLock => K::ReadOnlyLock,
            ReentrancyKey::ConstructorLock => K::ConstructorLock,
            ReentrancyKey::DelegateCallLock => K::DelegateCallLock,
        }
    }
}

fn map_err(e: SharedError) -> ReentrancyError {
    match e {
        SharedError::ReentrancyDetected => ReentrancyError::ReentrancyDetected,
        SharedError::CrossContractReentrancy => ReentrancyError::CrossContractReentrancy,
        SharedError::ConstructorReentrancy => ReentrancyError::ConstructorReentrancy,
        SharedError::DelegateCallReentrancy => ReentrancyError::DelegateCallReentrancy,
    }
}

pub struct ReentrancyGuard<'a> {
    inner: stellarlend_security::ReentrancyGuard<'a>,
}

impl<'a> ReentrancyGuard<'a> {
    pub fn new(env: &'a Env) -> Result<Self, ReentrancyError> {
        stellarlend_security::ReentrancyGuard::new(env)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    pub fn new_with_key(
        env: &'a Env,
        key: ReentrancyKey,
        is_read_only: bool,
    ) -> Result<Self, ReentrancyError> {
        stellarlend_security::ReentrancyGuard::new_with_key(env, key.to_shared(), is_read_only)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    /// Like [`ReentrancyGuard::new_with_key`], but also arms a cross-contract
    /// lock bound to the given caller address.
    pub fn new_with_caller(
        env: &'a Env,
        key: ReentrancyKey,
        caller: &Address,
        is_read_only: bool,
    ) -> Result<Self, ReentrancyError> {
        stellarlend_security::ReentrancyGuard::new_with_caller(env, key.to_shared(), caller, is_read_only)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    pub fn new_constructor(env: &'a Env) -> Result<Self, ReentrancyError> {
        stellarlend_security::ReentrancyGuard::new_constructor(env)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    pub fn new_read_only(env: &'a Env) -> Result<Self, ReentrancyError> {
        stellarlend_security::ReentrancyGuard::new_read_only(env)
            .map(|inner| Self { inner })
            .map_err(map_err)
    }

    pub fn is_read_only_reentrancy(&self) -> bool {
        self.inner.is_read_only_reentrancy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_state_transitions() {
        assert_eq!(GuardState::NotEntered, GuardState::NotEntered);
        assert_eq!(GuardState::Entered, GuardState::Entered);
        let _ = ReentrancyError::ReentrancyDetected;
    }
}
