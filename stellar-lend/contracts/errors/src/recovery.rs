//! Error recovery patterns.
//!
//! Provides a small, `no_std` vocabulary for deciding whether a failed operation is
//! retryable, idempotent, or terminal so controllers (off-chain or on-chain) handle
//! failures consistently instead of each caller reimplementing the same heuristics.

use crate::CoreError;

/// Whether and how a caller should respond to an error.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RecoveryDecision {
    /// The operation may be retried after backoff (transient/rate/pricing).
    Retry,
    /// The operation is safe to retry because it is idempotent (safe to re-submit).
    RetryIdempotent,
    /// The operation should not be retried without changing inputs.
    Terminal,
}

/// Classifies a [`CoreError`] into a [`RecoveryDecision`] using conservative defaults.
///
/// The mapping assumes the protocol's retryable failures are: rate limits, price
/// unavailability, and transient state; everything else is terminal.
pub fn recover(error: CoreError) -> RecoveryDecision {
    match error {
        CoreError::LimitExceeded | CoreError::PriceUnavailable | CoreError::InvalidState => {
            RecoveryDecision::Retry
        }
        CoreError::Unauthorized
        | CoreError::InvalidInput
        | CoreError::InvalidAsset
        | CoreError::Insufficient
        | CoreError::GuaranteeViolated
        | CoreError::Overflow
        | CoreError::Paused
        | CoreError::Reentrancy
        | CoreError::NotInitialized
        | CoreError::AlreadyInitialized
        | CoreError::NotFound
        | CoreError::AlreadyExists
        | CoreError::DivisionByZero
        | CoreError::Internal => RecoveryDecision::Terminal,
    }
}

/// Returns a human-readable retry hint for a decision (for dashboards/logs).
pub fn hint(decision: RecoveryDecision) -> &'static str {
    match decision {
        RecoveryDecision::Retry => "retry-with-backoff",
        RecoveryDecision::RetryIdempotent => "retry-idempotent",
        RecoveryDecision::Terminal => "do-not-retry",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_and_price_are_retryable() {
        assert_eq!(recover(CoreError::LimitExceeded), RecoveryDecision::Retry);
        assert_eq!(
            recover(CoreError::PriceUnavailable),
            RecoveryDecision::Retry
        );
    }

    #[test]
    fn auth_and_input_are_terminal() {
        assert_eq!(recover(CoreError::Unauthorized), RecoveryDecision::Terminal);
        assert_eq!(recover(CoreError::InvalidInput), RecoveryDecision::Terminal);
    }
}
