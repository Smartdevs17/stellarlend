//! # StellarLend Unified Error Framework
//!
//! Provides one consistent error-handling foundation for every StellarLend contract
//! (issue #708). Different contract modules (`hello-world`, `lending`, ...) previously
//! defined bespoke error enums with duplicated overlapping variants; this crate
//! centralizes the shared hierarchy and the conversion, logging, recovery and
//! analytics helpers so modules stay consistent while keeping their per-contract
//! numeric codes stable.
//!
//! ## Design
//!
//! * [`CoreError`] is the canonical, namespace-agnostic error type. Each operational
//!   outcome a module can express gets a single well-documented variant.
//! * [`ErrorCode`] mirrors the contract's own `#[contracterror]` enums without forcing
//!   modules to re-declare them here. Modules convert their `u32` codes via
//!   [`FromCode`] / [`TryFromCode`] and map to [`CoreError`] through [`IntoError`].
//! * Numeric ABI is intentionally **kept in each contract crate**. This library does
//!   not redefine codes owned by deployed contracts (that would break the on-chain
//!   interface); it provides the shared *framework* around them.
//!
//! This crate is `#![no_std]` so it can be compiled into Soroban WASM contracts.

#![no_std]

use soroban_sdk::{contracterror, Env, Symbol};

pub mod analytics;
pub mod logging;
pub mod recovery;
pub mod testing;

pub use analytics::ErrorAnalytics;
pub use logging::{log_error, log_error_with_tag};
pub use recovery::{recover, RecoveryDecision};

/// Canonical, namespace-agnostic error categories shared across all contracts.
///
/// Modules keep their own numeric `#[contracterror]` codes (ABI-stable), but every
/// module error maps onto exactly one [`CoreError`] so tooling, analytics and logs
/// agree on what happened regardless of which contract produced it.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CoreError {
    /// Caller lacked permission.
    Unauthorized = 1,
    /// Input was malformed or out of range.
    InvalidInput = 2,
    /// A referenced asset/token was missing or unsupported.
    InvalidAsset = 3,
    /// Resource/balance too low for the operation.
    Insufficient = 4,
    /// Guarantee (e.g. collateral ratio) not satisfied.
    GuaranteeViolated = 5,
    /// Arithmetic overflow or underflow.
    Overflow = 6,
    /// Operation blocked by a protocol/operation pause.
    Paused = 7,
    /// Reentrant call detected.
    Reentrancy = 8,
    /// Required setup has not happened.
    NotInitialized = 9,
    /// Initialization attempted more than once.
    AlreadyInitialized = 10,
    /// Requested state does not exist.
    NotFound = 11,
    /// A requested entity already exists.
    AlreadyExists = 12,
    /// Division by zero.
    DivisionByZero = 13,
    /// A protocol-enforced bound was exceeded.
    LimitExceeded = 14,
    /// Action invalid for current state.
    InvalidState = 15,
    /// Pricing/oracle data unavailable.
    PriceUnavailable = 16,
    /// Generic/internal failure not otherwise classified.
    Internal = 17,
}

impl CoreError {
    /// A short, stable machine-readable `Symbol` tag for the category. Useful for
    /// analytics aggregation (see [`analytics::ErrorAnalytics`]).
    pub fn tag(self, env: &Env) -> Symbol {
        // Stable short tags (< 32 chars) safe for Soroban `Symbol`.
        let s = match self {
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
        };
        Symbol::new(env, s)
    }
}

/// A contract-local error that can be converted to a [`CoreError`].
///
/// Implement this for each module's `#[contracterror]` enum so `?`-style mapping and
/// the analytics stack can normalize any module error into the shared hierarchy.
pub trait IntoError {
    fn into_core(self) -> CoreError;
}

/// Converts a raw contract error `u32` code into a [`CoreError`].
///
/// Rather than forcing every contract crate to expose its enum here, modules that
/// want analytical normalization can supply a closure mapping their code numbers to a
/// [`CoreError`]; [`TryFromCode::from_code`] centralizes "translate a numeric ABI code
/// into a category".
pub trait TryFromCode: Sized {
    /// Given an `env` (unused today but reserved for future structured-log wiring)
    /// and a contract error code, produce a `CoreError`.
    fn from_code(env: &Env, code: u32) -> Option<CoreError>;
}

/// Trivial, dependency-free implementation of [`IntoError`] for [`CoreError`] itself.
impl IntoError for CoreError {
    #[inline]
    fn into_core(self) -> CoreError {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_error_tags_are_stable() {
        let env = Env::default();
        assert_eq!(
            CoreError::Unauthorized.tag(&env),
            Symbol::new(&env, "unauthorized")
        );
        assert_eq!(
            CoreError::AlreadyInitialized.tag(&env),
            Symbol::new(&env, "already_initialized")
        );
    }

    #[test]
    fn core_error_into_core_is_identity() {
        assert_eq!(CoreError::Overflow.into_core(), CoreError::Overflow);
    }

    #[test]
    fn core_error_codes_are_unique_and_in_range() {
        let variants = [
            CoreError::Unauthorized,
            CoreError::InvalidInput,
            CoreError::InvalidAsset,
            CoreError::Insufficient,
            CoreError::GuaranteeViolated,
            CoreError::Overflow,
            CoreError::Paused,
            CoreError::Reentrancy,
            CoreError::NotInitialized,
            CoreError::AlreadyInitialized,
            CoreError::NotFound,
            CoreError::AlreadyExists,
            CoreError::DivisionByZero,
            CoreError::LimitExceeded,
            CoreError::InvalidState,
            CoreError::PriceUnavailable,
            CoreError::Internal,
        ];
        for (i, v) in variants.iter().enumerate() {
            for w in variants.iter().skip(i + 1) {
                assert_ne!(*v as u32, *w as u32, "duplicate code {}", *v as u32);
            }
        }
    }
}
