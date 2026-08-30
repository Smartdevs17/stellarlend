// Comprehensive reentrancy guard for the lending protocol.

use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Val};

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

pub struct ReentrancyGuard<'a> {
    env: &'a Env,
    key: Val,
    cross_contract_key: Option<Val>,
    state_before: GuardState,
    is_read_only: bool,
}

impl<'a> ReentrancyGuard<'a> {
    pub fn new(env: &'a Env) -> Result<Self, ReentrancyError> {
        Self::new_with_key(env, ReentrancyKey::GlobalLock, false)
    }

    pub fn new_with_key(
        env: &'a Env,
        key: ReentrancyKey,
        is_read_only: bool,
    ) -> Result<Self, ReentrancyError> {
        let storage_key = key.clone().into_val(env);
        if env.storage().temporary().has(&storage_key) {
            return Err(ReentrancyError::ReentrancyDetected);
        }

        env.storage().temporary().set(&storage_key, &true);

        Ok(Self {
            env,
            key: storage_key,
            cross_contract_key: None,
            state_before: GuardState::NotEntered,
            is_read_only,
        })
    }

    /// Like [`ReentrancyGuard::new_with_key`], but also arms a cross-contract lock bound to the
    /// given caller address. Callers that know their invoker (extracted via `require_auth`) use
    /// this variant so a re-entering contract is detected even when the same underlying
    /// `ReentrancyKey` differs.
    pub fn new_with_caller(
        env: &'a Env,
        key: ReentrancyKey,
        caller: &Address,
        is_read_only: bool,
    ) -> Result<Self, ReentrancyError> {
        let cc_key = ReentrancyKey::CrossContractLock(caller.clone()).into_val(env);
        if env.storage().temporary().has(&cc_key) {
            return Err(ReentrancyError::CrossContractReentrancy);
        }
        let storage_key = key.clone().into_val(env);
        if env.storage().temporary().has(&storage_key) {
            return Err(ReentrancyError::ReentrancyDetected);
        }
        env.storage().temporary().set(&cc_key, &true);
        env.storage().temporary().set(&storage_key, &true);

        Ok(Self {
            env,
            key: storage_key,
            cross_contract_key: Some(cc_key),
            state_before: GuardState::NotEntered,
            is_read_only,
        })
    }

    pub fn new_constructor(env: &'a Env) -> Result<Self, ReentrancyError> {
        let storage_key = ReentrancyKey::ConstructorLock.into_val(env);
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
        })
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_guard_prevents_reentrancy() {
        let env = Env::default();
        let _guard1 = ReentrancyGuard::new(&env).unwrap();
        assert!(ReentrancyGuard::new(&env).is_err());
    }

    #[test]
    fn test_constructor_guard() {
        let env = Env::default();
        let _guard = ReentrancyGuard::new_constructor(&env).unwrap();
        assert!(ReentrancyGuard::new_constructor(&env).is_err());
    }
}
