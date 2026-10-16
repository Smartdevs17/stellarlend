//! The [`ContractError`] trait and the [`impl_contract_error!`] registration macro.
//!
//! Registering a contract's `#[contracterror]` enum gives every variant:
//!
//! * a protocol-wide unique **global code** (see [`crate::domain`]),
//! * a normalized [`CoreError`] **category**,
//! * a human-readable **message**, and
//! * a **recovery action** with suggestion text and a retry decision,
//!
//! without changing the enum or its on-chain numeric codes.
//!
//! ```rust
//! use soroban_sdk::{contracterror, Env};
//! use stellarlend_errors::{
//!     domains, impl_contract_error, ContractError, CoreError, IntoError, RecoveryAction,
//! };
//!
//! #[contracterror]
//! #[derive(Copy, Clone, Debug, Eq, PartialEq)]
//! #[repr(u32)]
//! pub enum PoolError {
//!     Unauthorized = 1,
//!     InsufficientLiquidity = 2,
//! }
//!
//! impl_contract_error! {
//!     PoolError => domains::AMM;
//!     Unauthorized => Unauthorized, "Caller is not the pool admin";
//!     InsufficientLiquidity => Insufficient, "Pool reserves cannot cover the trade", ReduceAmount;
//! }
//!
//! let e = PoolError::InsufficientLiquidity;
//! assert_eq!(e.global_code(), 3_000_002);
//! assert_eq!(e.into_core(), CoreError::Insufficient);
//! assert_eq!(e.recovery_action(), RecoveryAction::ReduceAmount);
//! assert_eq!(PoolError::from_local_code(1), Some(PoolError::Unauthorized));
//!
//! let env = Env::default();
//! let info = stellarlend_errors::explain::<PoolError>(&env, 2).unwrap();
//! assert!(!info.retryable);
//! ```

use soroban_sdk::Env;

use crate::catalog::{ErrorDescriptor, ErrorInfo, RecoveryAction, Severity};
use crate::domain::ErrorDomain;
use crate::recovery::RecoveryDecision;
use crate::CoreError;

/// A contract error enum registered with the unified error framework.
///
/// Implement it with [`impl_contract_error!`] rather than by hand: the macro
/// forces every variant to be listed (the generated `match` is exhaustive) and
/// checks at compile time that all codes fit the global code layout.
pub trait ContractError: Copy + Sized + 'static {
    /// The namespace this enum's codes live in.
    const DOMAIN: ErrorDomain;

    /// Every variant of the enum, in declaration order.
    fn variants() -> &'static [Self];

    /// The numeric code emitted on-chain.
    fn local_code(self) -> u32;

    /// Full static description of this variant.
    fn describe(self) -> ErrorDescriptor;

    /// Protocol-wide unique code.
    fn global_code(self) -> u32 {
        Self::DOMAIN.global_code(self.local_code())
    }

    fn category(self) -> CoreError {
        self.describe().category
    }

    fn severity(self) -> Severity {
        self.describe().severity
    }

    fn message(self) -> &'static str {
        self.describe().message
    }

    fn recovery_action(self) -> RecoveryAction {
        self.describe().action
    }

    fn suggestion(self) -> &'static str {
        self.describe().suggestion()
    }

    fn decision(self) -> RecoveryDecision {
        self.describe().decision()
    }

    fn is_retryable(self) -> bool {
        self.describe().is_retryable()
    }

    /// Soroban-serializable description, suitable for returning from a view function.
    fn info(self, env: &Env) -> ErrorInfo {
        self.describe().to_info(env)
    }

    /// Looks a variant up by its on-chain code.
    fn from_local_code(code: u32) -> Option<Self> {
        Self::variants()
            .iter()
            .copied()
            .find(|v| v.local_code() == code)
    }
}

/// Explains an on-chain error code of enum `E`, or `None` if `E` has no such code.
///
/// Contracts can expose this directly:
///
/// ```ignore
/// pub fn explain_error(env: Env, code: u32) -> Option<ErrorInfo> {
///     stellarlend_errors::explain::<MyError>(&env, code)
/// }
/// ```
pub fn explain<E: ContractError>(env: &Env, local_code: u32) -> Option<ErrorInfo> {
    E::from_local_code(local_code).map(|e| e.info(env))
}

/// Builds an [`ErrorDescriptor`]; used by [`impl_contract_error!`].
#[doc(hidden)]
pub const fn __descriptor(
    domain: ErrorDomain,
    name: &'static str,
    local_code: u32,
    category: CoreError,
    message: &'static str,
    action: RecoveryAction,
) -> ErrorDescriptor {
    ErrorDescriptor {
        name,
        contract: domain.contract,
        module: domain.module,
        local_code,
        global_code: domain.global_code(local_code),
        category,
        severity: category.severity(),
        message,
        action,
    }
}

/// Registers a `#[contracterror]` enum with the unified error framework.
///
/// ```text
/// impl_contract_error! {
///     EnumName => <ErrorDomain expr>;
///     Variant => CoreErrorCategory, "message";
///     Variant => CoreErrorCategory, "message", RecoveryActionOverride;
///     ...
/// }
/// ```
///
/// Every variant must be listed — an omitted variant is a compile error. When
/// no recovery action is given, the category's
/// [`CoreError::default_action`] is used. The macro implements
/// [`ContractError`] and [`crate::IntoError`] for the enum.
#[macro_export]
macro_rules! impl_contract_error {
    (
        $ty:ident => $domain:expr;
        $( $variant:ident => $category:ident, $message:literal $(, $action:ident)? );+ $(;)?
    ) => {
        impl $crate::ContractError for $ty {
            const DOMAIN: $crate::ErrorDomain = $domain;

            fn variants() -> &'static [Self] {
                &[$( $ty::$variant ),+]
            }

            #[inline]
            fn local_code(self) -> u32 {
                self as u32
            }

            fn describe(self) -> $crate::ErrorDescriptor {
                match self {
                    $(
                        $ty::$variant => $crate::contract_error::__descriptor(
                            <Self as $crate::ContractError>::DOMAIN,
                            stringify!($variant),
                            $ty::$variant as u32,
                            $crate::CoreError::$category,
                            $message,
                            $crate::impl_contract_error!(@action $category $(, $action)?),
                        ),
                    )+
                }
            }
        }

        impl $crate::IntoError for $ty {
            #[inline]
            fn into_core(self) -> $crate::CoreError {
                <$ty as $crate::ContractError>::category(self)
            }
        }

        // Compile-time guard: every code must fit the global code layout.
        const _: () = {
            $(
                assert!(
                    ($ty::$variant as u32) <= $crate::domain::MAX_LOCAL_CODE,
                    concat!(stringify!($ty), "::", stringify!($variant), " code exceeds MAX_LOCAL_CODE"),
                );
            )+
        };
    };
    (@action $category:ident) => {
        $crate::CoreError::$category.default_action()
    };
    (@action $category:ident, $action:ident) => {
        $crate::RecoveryAction::$action
    };
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{contracterror, Env, String};

    use crate::testing::assert_registry;
    use crate::{
        domains, explain, log_contract_error, ContractError, CoreError, IntoError,
        RecoveryAction, RecoveryDecision, Severity,
    };

    #[contracterror]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(u32)]
    pub enum SampleError {
        Unauthorized = 1,
        InsufficientCollateral = 2,
        StalePrice = 7,
    }

    crate::impl_contract_error! {
        SampleError => domains::LENDING_BORROW;
        Unauthorized => Unauthorized, "Caller is not the position owner";
        InsufficientCollateral => Insufficient, "Collateral is too low", AddCollateral;
        StalePrice => PriceUnavailable, "Oracle price is stale";
    }

    #[test]
    fn registry_is_consistent() {
        assert_registry::<SampleError>();
        assert_eq!(SampleError::variants().len(), 3);
    }

    #[test]
    fn codes_and_categories() {
        let e = SampleError::StalePrice;
        assert_eq!(e.local_code(), 7);
        assert_eq!(e.global_code(), 2_010_007);
        assert_eq!(e.into_core(), CoreError::PriceUnavailable);
        assert_eq!(e.severity(), Severity::Transient);
    }

    #[test]
    fn default_and_overridden_actions() {
        assert_eq!(
            SampleError::Unauthorized.recovery_action(),
            RecoveryAction::CheckAuthorization
        );
        assert_eq!(
            SampleError::InsufficientCollateral.recovery_action(),
            RecoveryAction::AddCollateral
        );
        assert_eq!(SampleError::StalePrice.decision(), RecoveryDecision::Retry);
        assert!(!SampleError::Unauthorized.is_retryable());
    }

    #[test]
    fn lookup_by_local_code() {
        assert_eq!(SampleError::from_local_code(2), Some(SampleError::InsufficientCollateral));
        assert_eq!(SampleError::from_local_code(3), None);
    }

    #[test]
    fn explain_builds_info() {
        let env = Env::default();
        let info = explain::<SampleError>(&env, 2).unwrap();
        assert_eq!(info.global_code, 2_010_002);
        assert_eq!(info.category, CoreError::Insufficient as u32);
        assert_eq!(info.action, RecoveryAction::AddCollateral);
        assert_eq!(info.name, String::from_str(&env, "InsufficientCollateral"));
        assert_eq!(info.message, String::from_str(&env, "Collateral is too low"));
        assert_eq!(
            info.suggestion,
            String::from_str(&env, RecoveryAction::AddCollateral.suggestion())
        );
        assert!(explain::<SampleError>(&env, 99).is_none());
    }

    #[test]
    fn log_contract_error_does_not_panic() {
        let env = Env::default();
        log_contract_error(&env, "borrow", SampleError::StalePrice);
    }
}
