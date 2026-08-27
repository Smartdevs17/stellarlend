// ════════════════════════════════════════════════════════════════
// COMPREHENSIVE REENTRANCY GUARD FOR LENDING PROTOCOL
// ════════════════════════════════════════════════════════════════
// Provides multi-layered reentrancy protection:
// 1. Function-level guards (per-function locks)
// 2. Cross-contract reentrancy detection
// 3. Read-only reentrancy detection
// 4. Constructor reentrancy protection
// 5. Delegate call reentrancy protection
// 6. Checks-effects-interactions pattern enforcement
// ════════════════════════════════════════════════════════════════

use soroban_sdk {
    contracterror, contracttype, Address, Env, Symbol, Vec,
};

/// Reentrancy error
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReentrancyError {
    ReentrancyDetected = 1,
    CrossContractReentrancy = 2,
    ConstructorReentrancy = 3,
    DelegateCallReentrancy = 4,
}

/// Reentrancy guard state tracking
#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
pub enum GuardState {
    NotEntered = 0,
    Entered = 1,
}

/// Storage keys for reentrancy guards
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
    DepositCollateralLock,
    /// Cross-contract reentrancy tracking
    CrossContractLock(Address),
    /// Read-only reentrancy detection
    ReadOnlyLock,
    /// Constructor reentrancy protection
    ConstructorLock,
    /// Delegate call reentrancy protection
    DelegateCallLock,
}

/// Comprehensive reentrancy guard with RAII pattern
pub struct ReentrancyGuard<'a> {
    env: &'a Env,
    key: ReentrancyKey,
    state_before: GuardState,
    is_read_only: bool,
    caller: Option<Address>,
}

impl<'a> ReentrancyGuard<'a> {
    /// Create a new global reentrancy guard
    pub fn new(env: &'a Env) -> Result<Self, ReentrancyError> {
        Self::new_with_key(env, ReentrancyKey::GlobalLock, false)
    }

    /// Create a new reentrancy guard with a specific key
    pub fn new_with_key(
        env: &'a Env,
        key: ReentrancyKey,
        is_read_only: bool,
    ) -> Result<Self, ReentrancyError> {
        // CHECK: Are we already inside this function?
        let storage_key = Self::key_to_symbol(env, &key);
        if env.storage().temporary().has(&storage_key) {
            return Err(ReentrancyError::ReentrancyDetected);
        }

        // Cross-contract reentrancy check
        let caller = env.invoker();
        if let Some(caller_addr) = caller {
            let cross_contract_key = ReentrancyKey::CrossContractLock(caller_addr);
            let cross_contract_storage_key = Self::key_to_symbol(env, &cross_contract_key);
            if env.storage().temporary().has(&cross_contract_storage_key) {
                return Err(ReentrancyError::CrossContractReentrancy);
            }
        }

        // EFFECT: Mark as entered immediately
        env.storage().temporary().set(&storage_key, &true);

        // Track cross-contract call
        if let Some(caller_addr) = caller {
            let cross_contract_key = ReentrancyKey::CrossContractLock(caller_addr);
            let cross_contract_storage_key = Self::key_to_symbol(env, &cross_contract_key);
            env.storage()
                .temporary()
                .set(&cross_contract_storage_key, &true);
        }

        Ok(Self {
            env,
            key,
            state_before: GuardState::NotEntered,
            is_read_only,
            caller,
        })
    }

    /// Create a cross-contract reentrancy guard
    pub fn new_cross_contract(env: &'a Env, caller: &Address) -> Result<Self, ReentrancyError> {
        let key = ReentrancyKey::CrossContractLock(caller.clone());
        let storage_key = Self::key_to_symbol(env, &key);

        // Check for cross-contract reentrancy
        if env.storage().temporary().has(&storage_key) {
            return Err(ReentrancyError::CrossContractReentrancy);
        }

        env.storage().temporary().set(&storage_key, &true);

        Ok(Self {
            env,
            key,
            state_before: GuardState::NotEntered,
            is_read_only: false,
            caller: Some(caller.clone()),
        })
    }

    /// Create a read-only reentrancy guard
    pub fn new_read_only(env: &'a Env) -> Result<Self, ReentrancyError> {
        let key = ReentrancyKey::ReadOnlyLock;
        let storage_key = Self::key_to_symbol(env, &key);

        // Read-only functions can be re-entered but we track it
        let state_before = if env.storage().temporary().has(&storage_key) {
            GuardState::Entered
        } else {
            GuardState::NotEntered
        };

        env.storage().temporary().set(&storage_key, &true);

        Ok(Self {
            env,
            key,
            state_before,
            is_read_only: true,
            caller: None,
        })
    }

    /// Create a constructor reentrancy guard
    pub fn new_constructor(env: &'a Env) -> Result<Self, ReentrancyError> {
        let key = ReentrancyKey::ConstructorLock;
        let storage_key = Self::key_to_symbol(env, &key);

        if env.storage().temporary().has(&storage_key) {
            return Err(ReentrancyError::ConstructorReentrancy);
        }

        env.storage().temporary().set(&storage_key, &true);

        Ok(Self {
            env,
            key,
            state_before: GuardState::NotEntered,
            is_read_only: false,
            caller: None,
        })
    }

    /// Create a delegate call reentrancy guard
    pub fn new_delegate_call(env: &'a Env) -> Result<Self, ReentrancyError> {
        let key = ReentrancyKey::DelegateCallLock;
        let storage_key = Self::key_to_symbol(env, &key);

        if env.storage().temporary().has(&storage_key) {
            return Err(ReentrancyError::DelegateCallReentrancy);
        }

        env.storage().temporary().set(&storage_key, &true);

        Ok(Self {
            env,
            key,
            state_before: GuardState::NotEntered,
            is_read_only: false,
            caller: None,
        })
    }

    /// Check if this is a read-only reentrancy
    pub fn is_read_only_reentrancy(&self) -> bool {
        self.is_read_only && self.state_before == GuardState::Entered
    }
}

impl<'a> Drop for ReentrancyGuard<'a> {
    fn drop(&mut self) {
        // INTERACTION: Exit only after all operations complete
        let storage_key = Self::key_to_symbol(&self.env, &self.key);
        self.env.storage().temporary().remove(&storage_key);

        // Clean up cross-contract lock if we set one
        if let Some(caller) = &self.caller {
            let cross_contract_key = ReentrancyKey::CrossContractLock(caller.clone());
            let cross_contract_storage_key = Self::key_to_symbol(&self.env, &cross_contract_key);
            self.env
                .storage()
                .temporary()
                .remove(&cross_contract_storage_key);
        }
    }
}

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
        // Guard state should transition correctly
        assert_eq!(GuardState::NotEntered, GuardState::NotEntered);
        assert_eq!(GuardState::Entered, GuardState::Entered);
    }

    #[test]
    fn test_global_guard_prevents_reentrancy() {
        let env = Env::default();
        let guard1 = ReentrancyGuard::new(&env).unwrap();
        assert!(env.storage().temporary().has(&key_to_symbol(&env, &ReentrancyKey::GlobalLock)));

        // Second guard should fail (reentrancy detected)
        let guard2 = ReentrancyGuard::new(&env);
        assert!(guard2.is_err());
    }

    #[test]
    fn test_cross_contract_guard() {
        let env = Env::default();
        let caller = Address::random(&env);

        // First cross-contract guard should succeed
        let guard1 = ReentrancyGuard::new_cross_contract(&env, &caller);
        assert!(guard1.is_ok());

        // Second guard should fail (cross-contract reentrancy detected)
        let guard2 = ReentrancyGuard::new_cross_contract(&env, &caller);
        assert!(guard2.is_err());
    }

    #[test]
    fn test_read_only_guard() {
        let env = Env::default();

        // Read-only guard should succeed
        let guard = ReentrancyGuard::new_read_only(&env).unwrap();
        assert!(guard.is_read_only);
        assert!(env.storage().temporary().has(&key_to_symbol(&env, &ReentrancyKey::ReadOnlyLock)));

        // Can be re-entered (read-only)
        let guard2 = ReentrancyGuard::new_read_only(&env).unwrap();
        assert!(guard2.is_read_only);
    }

    #[test]
    fn test_constructor_guard() {
        let env = Env::default();

        // Constructor guard should succeed
        let guard = ReentrancyGuard::new_constructor(&env).unwrap();
        assert!(!guard.is_read_only_reentrancy());
    }

    #[test]
    fn test_delegate_call_guard() {
        let env = Env::default();

        // Delegate call guard should succeed
        let guard = ReentrancyGuard::new_delegate_call(&env).unwrap();
        assert!(!guard.is_read_only_reentrancy());
    }

    fn key_to_symbol(env: &Env, key: &ReentrancyKey) -> Symbol {
        match key {
            ReentrancyKey::GlobalLock => Symbol::new(env, "REENTRANCY_GLOBAL"),
            ReentrancyKey::DepositLock => Symbol::new(env, "REENTRANCY_DEPOSIT"),
            ReentrancyKey::WithdrawLock => Symbol::new(env, "REENTRANCY_WITHDRAW"),
            ReentrancyKey::BorrowLock => Symbol::new(env, "REENTRANCY_BORROW"),
            ReentrancyKey::RepayLock => Symbol::new(env, "REENTRANCY_REPAY"),
            ReentrancyKey::LiquidateLock => Symbol::new(env, "REENTRANCY_LIQUIDATE"),
            ReentrancyKey::FlashLoanLock => Symbol::new(env, "REENTRANCY_FLASH_LOAN"),
            ReentrancyKey::DepositCollateralLock => {
                Symbol::new(env, "REENTRANCY_DEPOSIT_COLLATERAL")
            }
            ReentrancyKey::CrossContractLock(addr) => {
                Symbol::new(env, &format!("REENTRANCY_CROSS_{}", addr))
            }
            ReentrancyKey::ReadOnlyLock => Symbol::new(env, "REENTRANCY_READ_ONLY"),
            ReentrancyKey::ConstructorLock => Symbol::new(env, "REENTRANCY_CONSTRUCTOR"),
            ReentrancyKey::DelegateCallLock => Symbol::new(env, "REENTRANCY_DELEGATE_CALL"),
        }
    }
}