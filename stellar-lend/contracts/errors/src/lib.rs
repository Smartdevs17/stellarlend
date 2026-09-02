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
//! * Each contract exposes its own `#[contracterror]` enum (e.g. `LendingError` in
//!   `hello-world`). The numeric ABI codes are owned by the contract crate so the
//!   on-chain interface does not change. This crate only re-declares the
//!   *category* mapping in [`mapping::lending_code_to_core`].
//! * Implement [`IntoError`] (or the lighter-weight [`LendingCode`] trait) on every
//!   per-module error enum so the framework can normalize failures with a single
//!   `into_core()` call. A blanket impl for `T: LendingCode` means most contracts
//!   only need to write `impl LendingCode for MyError { fn code(&self) -> u32 { *self as u32 } }`.
//! * Three stable helpers build on top of the normalized category:
//!   * [`analytics::ErrorAnalytics`] — per-category counters for dashboards.
//!   * [`logging::log_error`] — uniform Soroban event emission.
//!   * [`recovery::recover`] — decide `Retry` vs `Terminal`.
//!
//! This crate is `#![no_std]` so it can be compiled into Soroban WASM contracts.
//!
//! ## Quick start
//!
//! ```rust
//! use soroban_sdk::{contracterror, Env};
//! use stellarlend_errors::{
//!     lending_code_to_core, CoreError, ErrorAnalytics, IntoError,
//!     LendingCode, RecoveryDecision,
//! };
//! use stellarlend_errors::recovery::recover;
//!
//! // 1. Declare your contract's public error enum with stable numeric codes.
//! #[contracterror]
//! #[derive(Copy, Clone, Debug)]
//! #[repr(u32)]
//! pub enum MyError {
//!     Unauthorized = 1,
//!     Insufficient = 5,
//! }
//!
//! // 2. Connect it to the unified framework.
//! impl LendingCode for MyError {
//!     fn code(&self) -> u32 { *self as u32 }
//! }
//! // `IntoError` is now provided by the blanket impl — no manual impl needed.
//!
//! // 3. Use it.
//! let env = Env::default();
//! let mut tally = ErrorAnalytics::new(&env);
//! tally.record(MyError::Unauthorized);
//!
//! let core = MyError::Insufficient.into_core();
//! assert_eq!(core, CoreError::Insufficient);
//! assert_eq!(recover(core), RecoveryDecision::Terminal);
//! assert_eq!(lending_code_to_core(1), Some(CoreError::Unauthorized));
//! ```

#![no_std]

use soroban_sdk::{contracterror, Env, Symbol};

pub mod analytics;
pub mod benchmark;
pub mod logging;
pub mod mapping;
pub mod recovery;
pub mod testing;

pub use analytics::ErrorAnalytics;
pub use logging::{log_error, log_error_with_tag};
pub use mapping::{lending_code_to_core, lending_code_to_core_or_internal, LendingCode};
pub use recovery::{recover, hint, RecoveryDecision};

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
///
/// In most cases you don't need to write a manual `IntoError` impl — instead
/// implement [`LendingCode`] (a one-method trait that returns the error's
/// numeric code) and the blanket `impl<T: LendingCode> IntoError for T` does the
/// rest. Only use a manual `IntoError` impl when the error category depends on
/// the variant (e.g. to collapse several variants into one category, or to
/// enrich with context before mapping).
pub trait IntoError {
    fn into_core(self) -> CoreError;
}

/// Trivial, dependency-free implementation of [`IntoError`] for [`CoreError`] itself.
impl IntoError for CoreError {
    #[inline]
    fn into_core(self) -> CoreError {
        self
    }
}

#[cfg(test)]
mod integration_test;

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
