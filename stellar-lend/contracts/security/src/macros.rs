// Convenience macros for the reusable reentrancy guard.

/// Create a function-level reentrancy guard.
///
/// ```
/// use stellarlend_security::{ReentrancyKey, reentrancy_guard};
/// # #[allow(unused)]
/// fn guarded() { /* ... */ }
/// ```
#[macro_export]
macro_rules! reentrancy_guard {
    ($env:expr, $key:expr) => {
        $crate::ReentrancyGuard::new_with_key($env, $key, false)
    };
}

/// Create a cross-contract reentrancy guard bound to a caller address.
#[macro_export]
macro_rules! cross_contract_guard {
    ($env:expr, $caller:expr) => {
        $crate::ReentrancyGuard::new_with_caller($env, $crate::ReentrancyKey::GlobalLock, $caller, false)
    };
}

/// Create a read-only reentrancy guard.
#[macro_export]
macro_rules! read_only_guard {
    ($env:expr) => {
        $crate::ReentrancyGuard::new_read_only($env)
    };
}
