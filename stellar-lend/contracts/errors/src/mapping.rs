//! Canonical mappings between per-contract `LendingError` and [`CoreError`].
//!
//! Each contract in the workspace exposes a stable, ABI-facing `LendingError` enum
//! (see `hello-world/src/errors.rs`). This module centralizes the *second* hop of
//! the consolidation: `LendingError → CoreError`, so analytics, logging, and
//! recovery all agree on a single category regardless of which contract produced
//! the error.
//!
//! The mapping is intentionally explicit (no fallback to `CoreError::Internal`) so
//! every numeric code has a known destination. If a new `LendingError` variant is
//! added, this file must be updated and the unit test below enforces coverage.

use crate::{CoreError, IntoError};

/// All possible `LendingError` numeric codes (kept as a `u32` list so the mapping
/// can be defined for any contract variant without forcing a dependency on a
/// specific contract crate).
///
/// The numbers are owned by the deployed contract's ABI; this crate must agree
/// with them but does not own them. Update this constant when codes change.
pub const LENDING_ERROR_UNAUTHORIZED: u32 = 1;
pub const LENDING_ERROR_INVALID_AMOUNT: u32 = 2;
pub const LENDING_ERROR_INVALID_ASSET: u32 = 3;
pub const LENDING_ERROR_INVALID_PARAMETER: u32 = 4;
pub const LENDING_ERROR_INSUFFICIENT_BALANCE: u32 = 5;
pub const LENDING_ERROR_INSUFFICIENT_COLLATERAL: u32 = 6;
pub const LENDING_ERROR_INSUFFICIENT_COLLATERAL_RATIO: u32 = 7;
pub const LENDING_ERROR_OVERFLOW: u32 = 8;
pub const LENDING_ERROR_PROTOCOL_PAUSED: u32 = 9;
pub const LENDING_ERROR_REENTRANCY: u32 = 10;
pub const LENDING_ERROR_NOT_INITIALIZED: u32 = 11;
pub const LENDING_ERROR_ALREADY_INITIALIZED: u32 = 12;
pub const LENDING_ERROR_DATA_NOT_FOUND: u32 = 13;
pub const LENDING_ERROR_DIVISION_BY_ZERO: u32 = 14;
pub const LENDING_ERROR_NO_DEBT: u32 = 15;
pub const LENDING_ERROR_ASSET_NOT_ENABLED: u32 = 16;
pub const LENDING_ERROR_LIMIT_EXCEEDED: u32 = 17;
pub const LENDING_ERROR_INVALID_STATE: u32 = 18;
pub const LENDING_ERROR_PRICE_UNAVAILABLE: u32 = 19;
pub const LENDING_ERROR_INSUFFICIENT_LIQUIDITY: u32 = 20;
pub const LENDING_ERROR_INVALID_CALLBACK: u32 = 21;
pub const LENDING_ERROR_CALLBACK_FAILED: u32 = 22;
pub const LENDING_ERROR_NOT_REPAID: u32 = 23;
pub const LENDING_ERROR_TREASURY_NOT_SET: u32 = 24;
pub const LENDING_ERROR_INSUFFICIENT_RESERVE: u32 = 25;
pub const LENDING_ERROR_INVALID_FEE: u32 = 26;
pub const LENDING_ERROR_GOVERNANCE_REQUIRED: u32 = 27;
pub const LENDING_ERROR_GOVERNANCE_ERROR: u32 = 28;
pub const LENDING_ERROR_COMMIT_REQUIRED: u32 = 29;
pub const LENDING_ERROR_COMMIT_NOT_FOUND: u32 = 30;
pub const LENDING_ERROR_COMMIT_NOT_READY: u32 = 31;
pub const LENDING_ERROR_COMMIT_EXPIRED: u32 = 32;
pub const LENDING_ERROR_FEE_CAP_EXCEEDED: u32 = 33;
pub const LENDING_ERROR_NOT_FOUND: u32 = 34;
pub const LENDING_ERROR_ALREADY_EXISTS: u32 = 35;

/// Translates a raw `LendingError` numeric code into a [`CoreError`].
///
/// Returns `None` for codes that are not part of the published ABI (e.g. an
/// out-of-range value); callers may treat that as `CoreError::Internal` if they
/// prefer but the explicit `None` is safer for analytics.
///
/// # Example
/// ```
/// use stellarlend_errors::mapping::lending_code_to_core;
/// use stellarlend_errors::CoreError;
///
/// let core = lending_code_to_core(1).unwrap();
/// assert_eq!(core, CoreError::Unauthorized);
/// ```
pub fn lending_code_to_core(code: u32) -> Option<CoreError> {
    let core = match code {
        LENDING_ERROR_UNAUTHORIZED => CoreError::Unauthorized,
        LENDING_ERROR_INVALID_AMOUNT => CoreError::InvalidInput,
        LENDING_ERROR_INVALID_ASSET => CoreError::InvalidAsset,
        LENDING_ERROR_INVALID_PARAMETER => CoreError::InvalidInput,
        LENDING_ERROR_INSUFFICIENT_BALANCE => CoreError::Insufficient,
        LENDING_ERROR_INSUFFICIENT_COLLATERAL => CoreError::Insufficient,
        LENDING_ERROR_INSUFFICIENT_COLLATERAL_RATIO => CoreError::GuaranteeViolated,
        LENDING_ERROR_OVERFLOW => CoreError::Overflow,
        LENDING_ERROR_PROTOCOL_PAUSED => CoreError::Paused,
        LENDING_ERROR_REENTRANCY => CoreError::Reentrancy,
        LENDING_ERROR_NOT_INITIALIZED => CoreError::NotInitialized,
        LENDING_ERROR_ALREADY_INITIALIZED => CoreError::AlreadyInitialized,
        LENDING_ERROR_DATA_NOT_FOUND => CoreError::NotFound,
        LENDING_ERROR_DIVISION_BY_ZERO => CoreError::DivisionByZero,
        LENDING_ERROR_NO_DEBT => CoreError::InvalidState,
        LENDING_ERROR_ASSET_NOT_ENABLED => CoreError::InvalidState,
        LENDING_ERROR_LIMIT_EXCEEDED => CoreError::LimitExceeded,
        LENDING_ERROR_INVALID_STATE => CoreError::InvalidState,
        LENDING_ERROR_PRICE_UNAVAILABLE => CoreError::PriceUnavailable,
        LENDING_ERROR_INSUFFICIENT_LIQUIDITY => CoreError::Insufficient,
        LENDING_ERROR_INVALID_CALLBACK => CoreError::InvalidInput,
        LENDING_ERROR_CALLBACK_FAILED => CoreError::Internal,
        LENDING_ERROR_NOT_REPAID => CoreError::InvalidState,
        LENDING_ERROR_TREASURY_NOT_SET => CoreError::NotInitialized,
        LENDING_ERROR_INSUFFICIENT_RESERVE => CoreError::Insufficient,
        LENDING_ERROR_INVALID_FEE => CoreError::InvalidInput,
        LENDING_ERROR_GOVERNANCE_REQUIRED => CoreError::InvalidState,
        LENDING_ERROR_GOVERNANCE_ERROR => CoreError::Internal,
        LENDING_ERROR_COMMIT_REQUIRED => CoreError::InvalidState,
        LENDING_ERROR_COMMIT_NOT_FOUND => CoreError::NotFound,
        LENDING_ERROR_COMMIT_NOT_READY => CoreError::InvalidState,
        LENDING_ERROR_COMMIT_EXPIRED => CoreError::InvalidState,
        LENDING_ERROR_FEE_CAP_EXCEEDED => CoreError::LimitExceeded,
        LENDING_ERROR_NOT_FOUND => CoreError::NotFound,
        LENDING_ERROR_ALREADY_EXISTS => CoreError::AlreadyExists,
        _ => return None,
    };
    Some(core)
}

/// Convenience: `lending_code_to_core(code).unwrap_or(CoreError::Internal)`.
///
/// Use this only when the contract ABI is known to be correct; otherwise prefer
/// the explicit `Option` form so unknown codes surface as bugs rather than being
/// silently downgraded.
#[inline]
pub fn lending_code_to_core_or_internal(code: u32) -> CoreError {
    lending_code_to_core(code).unwrap_or(CoreError::Internal)
}

/// All known `LendingError` codes. Useful for the test that asserts every variant
/// has a mapping.
pub const ALL_LENDING_CODES: &[u32] = &[
    LENDING_ERROR_UNAUTHORIZED,
    LENDING_ERROR_INVALID_AMOUNT,
    LENDING_ERROR_INVALID_ASSET,
    LENDING_ERROR_INVALID_PARAMETER,
    LENDING_ERROR_INSUFFICIENT_BALANCE,
    LENDING_ERROR_INSUFFICIENT_COLLATERAL,
    LENDING_ERROR_INSUFFICIENT_COLLATERAL_RATIO,
    LENDING_ERROR_OVERFLOW,
    LENDING_ERROR_PROTOCOL_PAUSED,
    LENDING_ERROR_REENTRANCY,
    LENDING_ERROR_NOT_INITIALIZED,
    LENDING_ERROR_ALREADY_INITIALIZED,
    LENDING_ERROR_DATA_NOT_FOUND,
    LENDING_ERROR_DIVISION_BY_ZERO,
    LENDING_ERROR_NO_DEBT,
    LENDING_ERROR_ASSET_NOT_ENABLED,
    LENDING_ERROR_LIMIT_EXCEEDED,
    LENDING_ERROR_INVALID_STATE,
    LENDING_ERROR_PRICE_UNAVAILABLE,
    LENDING_ERROR_INSUFFICIENT_LIQUIDITY,
    LENDING_ERROR_INVALID_CALLBACK,
    LENDING_ERROR_CALLBACK_FAILED,
    LENDING_ERROR_NOT_REPAID,
    LENDING_ERROR_TREASURY_NOT_SET,
    LENDING_ERROR_INSUFFICIENT_RESERVE,
    LENDING_ERROR_INVALID_FEE,
    LENDING_ERROR_GOVERNANCE_REQUIRED,
    LENDING_ERROR_GOVERNANCE_ERROR,
    LENDING_ERROR_COMMIT_REQUIRED,
    LENDING_ERROR_COMMIT_NOT_FOUND,
    LENDING_ERROR_COMMIT_NOT_READY,
    LENDING_ERROR_COMMIT_EXPIRED,
    LENDING_ERROR_FEE_CAP_EXCEEDED,
    LENDING_ERROR_NOT_FOUND,
    LENDING_ERROR_ALREADY_EXISTS,
];

/// Blanket [`IntoError`] adapter for any type that exposes its numeric
/// `LendingError` representation. Contract crates that cannot depend on this
/// crate's specific types can implement `LendingCode::code() -> u32` and gain
/// normalization to `CoreError` for free.
pub trait LendingCode {
    fn code(&self) -> u32;
}

impl<T: LendingCode> IntoError for T {
    fn into_core(self) -> CoreError {
        lending_code_to_core_or_internal(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreError;

    #[test]
    fn every_lending_code_maps_to_a_core_error() {
        for &code in ALL_LENDING_CODES {
            assert!(
                lending_code_to_core(code).is_some(),
                "lending code {} is unmapped",
                code
            );
        }
    }

    #[test]
    fn known_codes_have_expected_categories() {
        assert_eq!(
            lending_code_to_core(LENDING_ERROR_UNAUTHORIZED),
            Some(CoreError::Unauthorized)
        );
        assert_eq!(
            lending_code_to_core(LENDING_ERROR_OVERFLOW),
            Some(CoreError::Overflow)
        );
        assert_eq!(
            lending_code_to_core(LENDING_ERROR_PRICE_UNAVAILABLE),
            Some(CoreError::PriceUnavailable)
        );
        assert_eq!(
            lending_code_to_core(LENDING_ERROR_INSUFFICIENT_COLLATERAL_RATIO),
            Some(CoreError::GuaranteeViolated)
        );
        assert_eq!(
            lending_code_to_core(LENDING_ERROR_REENTRANCY),
            Some(CoreError::Reentrancy)
        );
        assert_eq!(
            lending_code_to_core(LENDING_ERROR_DIVISION_BY_ZERO),
            Some(CoreError::DivisionByZero)
        );
        assert_eq!(
            lending_code_to_core(LENDING_ERROR_PROTOCOL_PAUSED),
            Some(CoreError::Paused)
        );
        assert_eq!(
            lending_code_to_core(LENDING_ERROR_NOT_INITIALIZED),
            Some(CoreError::NotInitialized)
        );
    }

    #[test]
    fn unknown_codes_return_none() {
        assert!(lending_code_to_core(99_999).is_none());
        assert!(lending_code_to_core(0).is_none());
    }

    #[test]
    fn unknown_codes_fall_back_to_internal() {
        assert_eq!(
            lending_code_to_core_or_internal(99_999),
            CoreError::Internal
        );
    }

    struct Wrap(u32);
    impl LendingCode for Wrap {
        fn code(&self) -> u32 {
            self.0
        }
    }

    #[test]
    fn lending_code_into_core_works_for_any_type() {
        let w = Wrap(LENDING_ERROR_UNAUTHORIZED);
        assert_eq!(w.into_core(), CoreError::Unauthorized);
        let w = Wrap(99_999);
        assert_eq!(w.into_core(), CoreError::Internal);
    }
}
