//! Human-readable messages and recovery suggestions.
//!
//! Every [`CoreError`] category has a default message and a default
//! [`RecoveryAction`]. Contract error enums registered through
//! [`crate::impl_contract_error!`] may override both per variant; anything they
//! leave out falls back to the category defaults defined here.

use soroban_sdk::{contracttype, Env, String};

use crate::recovery::RecoveryDecision;
use crate::CoreError;

/// What the caller (user, frontend, bot) should do to recover from an error.
///
/// This is deliberately more specific than [`RecoveryDecision`]: the decision
/// says *whether* to retry, the action says *what to change* first.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RecoveryAction {
    /// Transient condition — back off and submit the same call again.
    RetryLater = 1,
    /// Sign with the correct account or obtain the required role.
    CheckAuthorization = 2,
    /// Correct the malformed or out-of-range parameters.
    FixInput = 3,
    /// Use an asset the protocol lists and has enabled.
    UseSupportedAsset = 4,
    /// Lower the requested amount below the applicable cap or limit.
    ReduceAmount = 5,
    /// Top up the balance/liquidity the operation draws on.
    AddFunds = 6,
    /// Add collateral or repay debt to restore position health.
    AddCollateral = 7,
    /// The protocol or feature is paused; wait for governance to resume it.
    WaitForUnpause = 8,
    /// A reentrant call was blocked; let the outer call finish and resubmit
    /// as a separate transaction.
    WaitForCompletion = 9,
    /// Initialize the contract/feature before using it.
    InitializeFirst = 10,
    /// Initialization already happened; treat the call as a no-op.
    SkipInitialization = 11,
    /// Re-read on-chain state; the referenced item or lifecycle step changed.
    RefreshState = 12,
    /// The identifier is taken; choose a different one or reuse the existing entry.
    UseDifferentIdentifier = 13,
    /// Price data is stale or missing; wait for the next oracle update.
    WaitForOracle = 14,
    /// Adjust slippage/deadline tolerances and resubmit.
    AdjustTolerance = 15,
    /// Requires an administrator/governance action to fix configuration.
    ContactAdmin = 16,
    /// Indicates a protocol bug or broken invariant; report it.
    ReportBug = 17,
    /// The referenced item does not exist; double-check the identifier.
    VerifyIdentifier = 18,
}

impl RecoveryAction {
    /// Every action, in numeric order.
    pub const ALL: &'static [RecoveryAction] = &[
        RecoveryAction::RetryLater,
        RecoveryAction::CheckAuthorization,
        RecoveryAction::FixInput,
        RecoveryAction::UseSupportedAsset,
        RecoveryAction::ReduceAmount,
        RecoveryAction::AddFunds,
        RecoveryAction::AddCollateral,
        RecoveryAction::WaitForUnpause,
        RecoveryAction::WaitForCompletion,
        RecoveryAction::InitializeFirst,
        RecoveryAction::SkipInitialization,
        RecoveryAction::RefreshState,
        RecoveryAction::UseDifferentIdentifier,
        RecoveryAction::WaitForOracle,
        RecoveryAction::AdjustTolerance,
        RecoveryAction::ContactAdmin,
        RecoveryAction::ReportBug,
        RecoveryAction::VerifyIdentifier,
    ];

    /// Actionable, user-facing suggestion text.
    pub const fn suggestion(self) -> &'static str {
        match self {
            RecoveryAction::RetryLater => "Wait briefly and submit the transaction again.",
            RecoveryAction::CheckAuthorization => {
                "Sign with the account that owns the position or holds the required role."
            }
            RecoveryAction::FixInput => "Check the parameters and resubmit with valid values.",
            RecoveryAction::UseSupportedAsset => {
                "Use an asset that is listed and enabled for this operation."
            }
            RecoveryAction::ReduceAmount => {
                "Lower the amount so it stays within the configured caps and limits."
            }
            RecoveryAction::AddFunds => {
                "Top up the balance or wait for more liquidity, then try again."
            }
            RecoveryAction::AddCollateral => {
                "Deposit more collateral or repay debt to restore a healthy position."
            }
            RecoveryAction::WaitForUnpause => {
                "This operation is paused; wait for governance to resume it."
            }
            RecoveryAction::WaitForCompletion => {
                "Let the in-flight call finish, then submit this as a separate transaction."
            }
            RecoveryAction::InitializeFirst => {
                "Initialize the contract or feature before calling this function."
            }
            RecoveryAction::SkipInitialization => {
                "Already initialized; no further action is needed."
            }
            RecoveryAction::RefreshState => {
                "Reload the latest on-chain state and retry once the required step has happened."
            }
            RecoveryAction::UseDifferentIdentifier => {
                "That entry already exists; use a different identifier or the existing entry."
            }
            RecoveryAction::WaitForOracle => {
                "Price data is stale or unavailable; retry after the next oracle update."
            }
            RecoveryAction::AdjustTolerance => {
                "Widen the slippage or deadline tolerance, or wait for the market to settle."
            }
            RecoveryAction::ContactAdmin => {
                "Protocol configuration is required; contact the protocol administrators."
            }
            RecoveryAction::ReportBug => {
                "This indicates an unexpected protocol condition; please report it."
            }
            RecoveryAction::VerifyIdentifier => {
                "Nothing exists under that identifier; double-check it and resubmit."
            }
        }
    }

    /// Whether an automated caller may retry after taking this action.
    pub const fn decision(self) -> RecoveryDecision {
        match self {
            RecoveryAction::RetryLater
            | RecoveryAction::RefreshState
            | RecoveryAction::WaitForOracle => RecoveryDecision::Retry,
            RecoveryAction::CheckAuthorization
            | RecoveryAction::FixInput
            | RecoveryAction::UseSupportedAsset
            | RecoveryAction::ReduceAmount
            | RecoveryAction::AddFunds
            | RecoveryAction::AddCollateral
            | RecoveryAction::WaitForUnpause
            | RecoveryAction::WaitForCompletion
            | RecoveryAction::InitializeFirst
            | RecoveryAction::SkipInitialization
            | RecoveryAction::UseDifferentIdentifier
            | RecoveryAction::AdjustTolerance
            | RecoveryAction::ContactAdmin
            | RecoveryAction::ReportBug
            | RecoveryAction::VerifyIdentifier => RecoveryDecision::Terminal,
        }
    }
}

/// How serious an error is, for alerting and UI treatment.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Severity {
    /// Expected user-level rejection (bad input, insufficient funds).
    User = 1,
    /// Temporary protocol condition (paused, stale price, rate limited).
    Transient = 2,
    /// Configuration or lifecycle problem needing operator attention.
    Operational = 3,
    /// Broken invariant or security-relevant event; alert immediately.
    Critical = 4,
}

impl CoreError {
    /// Every category, in numeric order.
    pub const ALL: &'static [CoreError] = &[
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

    /// Default human-readable message for the category.
    pub const fn message(self) -> &'static str {
        match self {
            CoreError::Unauthorized => "Caller is not authorized to perform this action",
            CoreError::InvalidInput => "Input is malformed or out of range",
            CoreError::InvalidAsset => "Asset is unknown or not supported",
            CoreError::Insufficient => "Insufficient balance or liquidity",
            CoreError::GuaranteeViolated => "Operation would violate a collateral or safety guarantee",
            CoreError::Overflow => "Arithmetic overflow or underflow",
            CoreError::Paused => "Operation is paused",
            CoreError::Reentrancy => "Reentrant call blocked",
            CoreError::NotInitialized => "Contract or feature is not initialized",
            CoreError::AlreadyInitialized => "Contract or feature is already initialized",
            CoreError::NotFound => "Requested item was not found",
            CoreError::AlreadyExists => "Item already exists",
            CoreError::DivisionByZero => "Division by zero",
            CoreError::LimitExceeded => "A protocol limit was exceeded",
            CoreError::InvalidState => "Operation is not valid in the current state",
            CoreError::PriceUnavailable => "Price data is unavailable or stale",
            CoreError::Internal => "Internal contract error",
        }
    }

    /// Default recovery action for the category.
    pub const fn default_action(self) -> RecoveryAction {
        match self {
            CoreError::Unauthorized => RecoveryAction::CheckAuthorization,
            CoreError::InvalidInput => RecoveryAction::FixInput,
            CoreError::InvalidAsset => RecoveryAction::UseSupportedAsset,
            CoreError::Insufficient => RecoveryAction::AddFunds,
            CoreError::GuaranteeViolated => RecoveryAction::AddCollateral,
            CoreError::Overflow => RecoveryAction::ReduceAmount,
            CoreError::Paused => RecoveryAction::WaitForUnpause,
            CoreError::Reentrancy => RecoveryAction::WaitForCompletion,
            CoreError::NotInitialized => RecoveryAction::InitializeFirst,
            CoreError::AlreadyInitialized => RecoveryAction::SkipInitialization,
            CoreError::NotFound => RecoveryAction::VerifyIdentifier,
            CoreError::AlreadyExists => RecoveryAction::UseDifferentIdentifier,
            CoreError::DivisionByZero => RecoveryAction::FixInput,
            CoreError::LimitExceeded => RecoveryAction::RetryLater,
            CoreError::InvalidState => RecoveryAction::RefreshState,
            CoreError::PriceUnavailable => RecoveryAction::WaitForOracle,
            CoreError::Internal => RecoveryAction::ContactAdmin,
        }
    }

    /// Default severity for the category.
    pub const fn severity(self) -> Severity {
        match self {
            CoreError::InvalidInput
            | CoreError::InvalidAsset
            | CoreError::Insufficient
            | CoreError::GuaranteeViolated
            | CoreError::NotFound
            | CoreError::AlreadyExists
            | CoreError::AlreadyInitialized
            | CoreError::LimitExceeded
            | CoreError::Unauthorized => Severity::User,
            CoreError::Paused | CoreError::PriceUnavailable | CoreError::InvalidState => {
                Severity::Transient
            }
            CoreError::NotInitialized | CoreError::Internal => Severity::Operational,
            CoreError::Overflow | CoreError::DivisionByZero | CoreError::Reentrancy => {
                Severity::Critical
            }
        }
    }
}

/// Static, allocation-free description of one error variant.
///
/// Produced by [`crate::ContractError::describe`]; convert to the on-chain
/// [`ErrorInfo`] with [`ErrorDescriptor::to_info`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ErrorDescriptor {
    /// Variant identifier, e.g. `"InsufficientCollateral"`.
    pub name: &'static str,
    pub contract: crate::ContractId,
    pub module: u32,
    /// Code as emitted on-chain by the contract.
    pub local_code: u32,
    /// Protocol-wide unique code (see [`crate::domain`]).
    pub global_code: u32,
    pub category: CoreError,
    pub severity: Severity,
    pub message: &'static str,
    pub action: RecoveryAction,
}

impl ErrorDescriptor {
    pub const fn suggestion(&self) -> &'static str {
        self.action.suggestion()
    }

    pub const fn decision(&self) -> RecoveryDecision {
        self.action.decision()
    }

    pub const fn is_retryable(&self) -> bool {
        !matches!(self.action.decision(), RecoveryDecision::Terminal)
    }

    /// Converts to the Soroban-serializable [`ErrorInfo`] for view functions and events.
    pub fn to_info(&self, env: &Env) -> ErrorInfo {
        ErrorInfo {
            global_code: self.global_code,
            local_code: self.local_code,
            contract: self.contract as u32,
            module: self.module,
            category: self.category as u32,
            action: self.action,
            retryable: self.is_retryable(),
            name: String::from_str(env, self.name),
            message: String::from_str(env, self.message),
            suggestion: String::from_str(env, self.action.suggestion()),
        }
    }
}

/// Soroban-serializable error description that contracts may return from
/// view functions (e.g. `explain_error(code)`) so clients can render the
/// message and recovery suggestion without an off-chain lookup table.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorInfo {
    pub global_code: u32,
    pub local_code: u32,
    pub contract: u32,
    pub module: u32,
    /// [`CoreError`] discriminant.
    pub category: u32,
    pub action: RecoveryAction,
    pub retryable: bool,
    pub name: String,
    pub message: String,
    pub suggestion: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recover;

    #[test]
    fn all_lists_are_in_numeric_order() {
        for (i, c) in CoreError::ALL.iter().enumerate() {
            assert_eq!(*c as u32, i as u32 + 1);
        }
        for (i, a) in RecoveryAction::ALL.iter().enumerate() {
            assert_eq!(*a as u32, i as u32 + 1);
        }
    }

    #[test]
    fn category_default_action_agrees_with_recover() {
        for c in CoreError::ALL {
            assert_eq!(
                c.default_action().decision(),
                recover(*c),
                "{:?} default action disagrees with recover()",
                c
            );
        }
    }

    #[test]
    fn every_category_and_action_has_text() {
        for c in CoreError::ALL {
            assert!(!c.message().is_empty());
        }
        for a in RecoveryAction::ALL {
            assert!(a.suggestion().ends_with('.'));
        }
    }
}
