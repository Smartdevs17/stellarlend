// Storage keys for the reusable reentrancy guard.

use soroban_sdk::{contracttype, Address};

/// Reentrancy storage keys.
///
/// Contracts can either use the built-in variants or supply their own
/// through [`crate::ReentrancyKey::Custom`], keeping compatibility with
/// legacy per-contract key sets that named their locks (e.g. deposit,
/// withdraw, borrow, repay, liquidate, flash-loan).
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReentrancyKey {
    /// Global reentrancy lock.
    GlobalLock,
    /// Function-specific locks.
    DepositLock,
    WithdrawLock,
    BorrowLock,
    RepayLock,
    LiquidateLock,
    FlashLoanLock,
    DepositCollateralLock,
    /// Cross-contract reentrancy tracking, bound to a caller address.
    CrossContractLock(Address),
    /// Read-only reentrancy detection.
    ReadOnlyLock,
    /// Constructor/initializer reentrancy protection.
    ConstructorLock,
    /// Delegate call reentrancy protection.
    DelegateCallLock,
    /// Arbitrary contract-specific key supplied by a caller.
    Custom(u32),
}
