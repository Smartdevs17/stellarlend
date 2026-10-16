// Reentrancy guard error types and configuration.

use soroban_sdk::contracterror;

/// Errors returned by the reusable reentrancy guard.
///
/// Consuming contracts map these onto their own domain error enums at
/// the call site, so the shared primitive does not leak a foreign error
/// type into public interfaces.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReentrancyError {
    /// A re-entrant call into the same guard key was blocked.
    ReentrancyDetected = 1,
    /// A cross-contract re-entrant call was detected for the armed caller.
    CrossContractReentrancy = 2,
    /// A re-entrant call into a constructor/initializer was blocked.
    ConstructorReentrancy = 3,
    /// A delegate-call re-entrancy was blocked.
    DelegateCallReentrancy = 4,
}

/// How the guard behaves in read-only contexts.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GuardBehavior {
    /// Read-only functions may re-enter, but the re-entrancy is tracked and
    /// surfaced through [`ReentrancyGuard::is_read_only_reentrancy`].
    AllowReadOnlyReentry,
    /// Read-only re-entrancy is rejected outright.
    RejectReadOnlyReentry,
}

impl Default for GuardBehavior {
    fn default() -> Self {
        GuardBehavior::AllowReadOnlyReentry
    }
}

// Unified error registry: protocol-wide global codes, messages and recovery
// suggestions for every variant (see `stellarlend_errors` and docs/ERROR_HANDLING.md).
stellarlend_errors::impl_contract_error! {
    ReentrancyError => stellarlend_errors::domains::SECURITY_REENTRANCY;
    ReentrancyDetected => Reentrancy, "A re-entrant call into the same guard key was blocked";
    CrossContractReentrancy => Reentrancy, "A cross-contract re-entrant call was detected for the armed caller";
    ConstructorReentrancy => Reentrancy, "A re-entrant call into a constructor/initializer was blocked";
    DelegateCallReentrancy => Reentrancy, "A delegate-call re-entrancy was blocked";
}
