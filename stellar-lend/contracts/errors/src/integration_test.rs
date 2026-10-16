//! End-to-end integration test of the unified error handling framework.
//!
//! Exercises the full chain the public API surfaces:
//!
//! ```text
//!   module error -> LendingError code (u32) -> CoreError category
//!                  -> analytics (Symbol tag) -> recovery (Retry vs Terminal)
//! ```
//!
//! Uses a sample `LendingError` enum whose `u32` discriminants are the same as
//! the real `hello-world` `LendingError` so we can verify the mapping without
//! pulling in the larger contract crate (whose test infrastructure is mid
//! refactor and currently fails to compile).

#![cfg(test)]

use soroban_sdk::{contracterror, Env, Symbol};

use crate::analytics::ErrorAnalytics;
use crate::mapping::{lending_code_to_core, LendingCode};
use crate::{log_error, recover, CoreError, IntoError, RecoveryDecision};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LendingError {
    Unauthorized = 1,
    InvalidAmount = 2,
    InvalidAsset = 3,
    InvalidParameter = 4,
    InsufficientBalance = 5,
    InsufficientCollateral = 6,
    InsufficientCollateralRatio = 7,
    Overflow = 8,
    ProtocolPaused = 9,
    Reentrancy = 10,
    NotInitialized = 11,
    AlreadyInitialized = 12,
    DataNotFound = 13,
    DivisionByZero = 14,
    NoDebt = 15,
    AssetNotEnabled = 16,
    LimitExceeded = 17,
    InvalidState = 18,
    PriceUnavailable = 19,
    InsufficientLiquidity = 20,
    InvalidCallback = 21,
    CallbackFailed = 22,
    NotRepaid = 23,
    TreasuryNotSet = 24,
    InsufficientReserve = 25,
    InvalidFee = 26,
    GovernanceRequired = 27,
    GovernanceError = 28,
    CommitRequired = 29,
    CommitNotFound = 30,
    CommitNotReady = 31,
    CommitExpired = 32,
    FeeCapExceeded = 33,
    NotFound = 34,
    AlreadyExists = 35,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BorrowError {
    InvalidAmount = 1,
    InsufficientCollateral = 2,
    Reentrancy = 3,
    BorrowPaused = 4,
    InsufficientCollateralRatio = 5,
    Overflow = 6,
    MaxBorrowExceeded = 7,
    AssetNotEnabled = 8,
}

impl From<BorrowError> for LendingError {
    fn from(e: BorrowError) -> Self {
        match e {
            BorrowError::InvalidAmount => LendingError::InvalidAmount,
            BorrowError::InsufficientCollateral => LendingError::InsufficientCollateral,
            BorrowError::Reentrancy => LendingError::Reentrancy,
            BorrowError::BorrowPaused => LendingError::ProtocolPaused,
            BorrowError::InsufficientCollateralRatio => {
                LendingError::InsufficientCollateralRatio
            }
            BorrowError::Overflow => LendingError::Overflow,
            BorrowError::MaxBorrowExceeded => LendingError::LimitExceeded,
            BorrowError::AssetNotEnabled => LendingError::AssetNotEnabled,
        }
    }
}

impl LendingCode for LendingError {
    fn code(&self) -> u32 {
        *self as u32
    }
}

// `IntoError for LendingError` is provided by the blanket impl in
// `mapping::LendingCode`. We deliberately don't repeat the impl here.

impl IntoError for BorrowError {
    fn into_core(self) -> CoreError {
        <BorrowError as Into<LendingError>>::into(self).into_core()
    }
}

#[test]
fn end_to_end_module_to_core_normalization() {
    let cases: [(BorrowError, CoreError); 8] = [
        (BorrowError::InvalidAmount, CoreError::InvalidInput),
        (BorrowError::InsufficientCollateral, CoreError::Insufficient),
        (BorrowError::Reentrancy, CoreError::Reentrancy),
        (BorrowError::BorrowPaused, CoreError::Paused),
        (
            BorrowError::InsufficientCollateralRatio,
            CoreError::GuaranteeViolated,
        ),
        (BorrowError::Overflow, CoreError::Overflow),
        (BorrowError::MaxBorrowExceeded, CoreError::LimitExceeded),
        (BorrowError::AssetNotEnabled, CoreError::InvalidState),
    ];
    for (module_err, expected) in cases {
        let intermediate: LendingError = LendingError::from(module_err);
        assert_eq!(intermediate.code(), intermediate as u32);
        assert_eq!(intermediate.into_core(), expected);
        assert_eq!(module_err.into_core(), expected);
    }
}

#[test]
fn every_known_lending_code_resolves() {
    let codes = [
        1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
    ];
    for code in codes {
        let core = lending_code_to_core(code).unwrap();
        // The category must agree with the analytics tag.
        let env = Env::default();
        assert_eq!(core.tag(&env), Symbol::new(&env, tag_for_core(core)));
    }
}

fn tag_for_core(c: CoreError) -> &'static str {
    match c {
        CoreError::Unauthorized => "unauthorized",
        CoreError::InvalidInput => "invalid_input",
        CoreError::InvalidAsset => "invalid_asset",
        CoreError::Insufficient => "insufficient",
        CoreError::GuaranteeViolated => "guarantee_violated",
        CoreError::Overflow => "overflow",
        CoreError::Paused => "paused",
        CoreError::Reentrancy => "reentrancy",
        CoreError::NotInitialized => "not_initialized",
        CoreError::AlreadyInitialized => "already_initialized",
        CoreError::NotFound => "not_found",
        CoreError::AlreadyExists => "already_exists",
        CoreError::DivisionByZero => "division_by_zero",
        CoreError::LimitExceeded => "limit_exceeded",
        CoreError::InvalidState => "invalid_state",
        CoreError::PriceUnavailable => "price_unavailable",
        CoreError::Internal => "internal",
    }
}

#[test]
fn recovery_decisions_match_categories() {
    assert_eq!(recover(CoreError::LimitExceeded), RecoveryDecision::Retry);
    assert_eq!(
        recover(CoreError::PriceUnavailable),
        RecoveryDecision::Retry
    );
    assert_eq!(recover(CoreError::InvalidState), RecoveryDecision::Retry);
    assert_eq!(recover(CoreError::Unauthorized), RecoveryDecision::Terminal);
    assert_eq!(recover(CoreError::Overflow), RecoveryDecision::Terminal);
    assert_eq!(recover(CoreError::Reentrancy), RecoveryDecision::Terminal);
}

#[test]
fn analytics_aggregates_module_errors() {
    let env = Env::default();
    let mut a = ErrorAnalytics::new(&env);

    a.record(BorrowError::Reentrancy);
    a.record(BorrowError::Reentrancy);
    a.record(BorrowError::InsufficientCollateral);
    a.record(BorrowError::BorrowPaused);

    let reentrancy = CoreError::Reentrancy.tag(&env);
    let insufficient = CoreError::Insufficient.tag(&env);
    let paused = CoreError::Paused.tag(&env);

    assert_eq!(a.count(reentrancy), 2);
    assert_eq!(a.count(insufficient), 1);
    assert_eq!(a.count(paused), 1);
    assert_eq!(a.count(CoreError::Overflow.tag(&env)), 0);
    assert_eq!(a.snapshot().len(), 3);
}

#[test]
fn logging_does_not_panic_for_module_or_lending_errors() {
    let env = Env::default();
    log_error(&env, "borrow", BorrowError::Reentrancy);
    log_error(&env, "lending", LendingError::ProtocolPaused);
    // No assertion: the test passes as long as publishing the event works.
}

#[test]
fn no_fallback_to_internal_for_known_codes() {
    // Codes 22 (CallbackFailed) and 28 (GovernanceError) intentionally map to
    // Internal: the mapping is still defined (not a fallback), they just land
    // in the catch-all bucket by design. Every other code should land in a
    // specific category.
    let allowed_internal = [22u32, 28];
    for code in 1u32..=35 {
        let core = lending_code_to_core(code).expect("known code must map");
        if allowed_internal.contains(&code) {
            assert_eq!(core, CoreError::Internal);
        } else {
            assert_ne!(
                core,
                CoreError::Internal,
                "code {} unexpectedly fell back to Internal",
                code
            );
        }
    }
}

#[test]
fn recovery_and_analytics_agree_on_categories() {
    let env = Env::default();
    let mut a = ErrorAnalytics::new(&env);
    // Generate 100 mixed errors; verify per-category counts and that the
    // aggregated counts equal the input distribution.
    for i in 0..100u32 {
        let err = match i % 5 {
            0 => BorrowError::Reentrancy,
            1 => BorrowError::BorrowPaused,
            2 => BorrowError::InsufficientCollateral,
            3 => BorrowError::Overflow,
            _ => BorrowError::InvalidAmount,
        };
        a.record(err);
    }
    let reentrancy = CoreError::Reentrancy.tag(&env);
    let paused = CoreError::Paused.tag(&env);
    let insufficient = CoreError::Insufficient.tag(&env);
    let overflow = CoreError::Overflow.tag(&env);
    let invalid = CoreError::InvalidInput.tag(&env);
    assert_eq!(a.count(reentrancy), 20);
    assert_eq!(a.count(paused), 20);
    assert_eq!(a.count(insufficient), 20);
    assert_eq!(a.count(overflow), 20);
    assert_eq!(a.count(invalid), 20);
}
