// ════════════════════════════════════════════════════════════════
// REUSABLE REENTRANCY GUARD — Security Primitives Crate
// ════════════════════════════════════════════════════════════════
// A single, reusable reentrancy guard shared across all StellarLend
// contracts. Provides:
//
// 1. Function-level guards (per-function locks)
// 2. Cross-contract reentrancy detection (caller-bound)
// 3. Read-only reentrancy detection
// 4. Constructor reentrancy protection
// 5. Delegate call reentrancy protection
// 6. Guard configuration
// 7. Guard testing utilities
//
// Contracts depend on this crate instead of maintaining their own
// contract-specific copies. Public behaviour is preserved: each
// consuming contract maps [`ReentrancyError`] back to its own error
// enum via `map_err`.
// ════════════════════════════════════════════════════════════════

#![no_std]

mod error;
mod guard;
mod keys;
mod macros;

pub use error::{GuardBehavior, ReentrancyError};
pub use guard::{ReentrancyConfigured, ReentrancyGuard, ReentrancyGuardConfig};
pub use keys::ReentrancyKey;
#[cfg(feature = "testutils")]
pub mod testing;

/// Reentrancy guard state tracking.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GuardState {
    NotEntered = 0,
    Entered = 1,
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

    #[contract]
    struct TestContract;

    #[contractimpl]
    impl TestContract {
        pub fn ping() {}
    }

    /// Run `f` within the storage context of a registered contract.
    fn in_contract<R>(env: &Env, f: impl FnOnce() -> R) -> R {
        let id = env.register(TestContract, ());
        env.as_contract(&id, f)
    }

    #[test]
    fn global_guard_prevents_reentry() {
        let env = Env::default();
        in_contract(&env, || {
            let _g1 = ReentrancyGuard::new(&env).unwrap();
            assert!(ReentrancyGuard::new(&env).is_err());
        });
    }

    #[test]
    fn function_guard_blocks_then_releases() {
        let env = Env::default();
        in_contract(&env, || {
            let _g = ReentrancyGuard::new_with_key(&env, ReentrancyKey::DepositLock, false)
                .unwrap();
            assert!(
                ReentrancyGuard::new_with_key(&env, ReentrancyKey::DepositLock, false).is_err()
            );
            drop(_g);
            assert!(
                ReentrancyGuard::new_with_key(&env, ReentrancyKey::DepositLock, false).is_ok()
            );
        });
    }

    #[test]
    fn constructor_guard_blocks() {
        let env = Env::default();
        in_contract(&env, || {
            let _g = ReentrancyGuard::new_constructor(&env).unwrap();
            assert!(ReentrancyGuard::new_constructor(&env).is_err());
        });
    }

    #[test]
    fn delegate_call_guard_blocks() {
        let env = Env::default();
        in_contract(&env, || {
            let _g = ReentrancyGuard::new_delegate_call(&env).unwrap();
            assert!(ReentrancyGuard::new_delegate_call(&env).is_err());
        });
    }

    #[test]
    fn cross_contract_guard_binds_caller() {
        let env = Env::default();
        in_contract(&env, || {
            let caller = Address::generate(&env);
            let _g =
                ReentrancyGuard::new_with_caller(&env, ReentrancyKey::GlobalLock, &caller, false)
                    .unwrap();
            // Same caller re-entry is rejected while `_g` is held (GlobalLock+cross-contract armed).
            assert!(
                ReentrancyGuard::new_with_caller(&env, ReentrancyKey::GlobalLock, &caller, false)
                    .is_err()
            );
            drop(_g);
            // After release, a different caller acquires a fresh guard.
            let other = Address::generate(&env);
            assert!(
                ReentrancyGuard::new_with_caller(&env, ReentrancyKey::GlobalLock, &other, false)
                    .is_ok()
            );
        });
    }

    #[test]
    fn read_only_reentry_is_tracked() {
        let env = Env::default();
        in_contract(&env, || {
            let _g = ReentrancyGuard::new_read_only(&env).unwrap();
            let inner = ReentrancyGuard::new_read_only(&env).unwrap();
            assert!(inner.is_read_only_reentrancy());
        });
    }

    #[test]
    fn configure_reports_bundle() {
        let cfg = ReentrancyGuard::configure(&[("deposit", false), ("withdraw", false), ("query", true)]);
        assert_eq!(cfg.function_guards, 2);
        assert_eq!(cfg.read_only_guards, 1);
        assert_eq!(cfg.cross_contract_guards, 0);
    }

    #[test]
    fn config_default_allows_read_only_reentry() {
        let cfg = ReentrancyGuardConfig::default();
        assert_eq!(cfg.read_only_behavior, GuardBehavior::AllowReadOnlyReentry);
    }

    #[test]
    fn guard_releases_on_drop() {
        let env = Env::default();
        in_contract(&env, || {
            let _g = ReentrancyGuard::new(&env).unwrap();
            assert!(ReentrancyGuard::new(&env).is_err());
            drop(_g);
            assert!(ReentrancyGuard::new(&env).is_ok());
        });
    }
}
