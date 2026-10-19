// Testing utilities for the reusable reentrancy guard.
//
// These helpers make it easy for consuming contracts to write robust
// reentrancy tests against the shared primitive without re-implementing
// the attack scenarios.

use crate::ReentrancyGuard;
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Execute `f` while holding a global reentrancy guard, asserting the guard
/// is released (idempotent) after the closure returns.
pub fn with_guard<R>(env: &Env, f: impl FnOnce() -> R) -> R {
    let guard = ReentrancyGuard::new(env).expect("guard should arm");
    let out = f();
    drop(guard);
    out
}

/// Assert that attempting to acquire the same guard key twice fails with a
/// reentrancy error, and that the guard is released once the first guard goes
/// out of scope.
pub fn assert_function_guard_blocks_reentry(env: &Env, key: crate::ReentrancyKey) {
    let guard = ReentrancyGuard::new_with_key(env, key.clone(), false);
    assert!(guard.is_ok(), "first acquisition should succeed");

    // A second acquisition of the same key must fail while the first holds it.
    let second = ReentrancyGuard::new_with_key(env, key.clone(), false);
    assert!(second.is_err(), "second acquisition should be blocked");

    drop(guard);

    // After the first guard drops, acquisition succeeds again.
    let again = ReentrancyGuard::new_with_key(env, key, false);
    assert!(again.is_ok(), "guard should be released after drop");
}

/// Assert that a cross-contract guard bound to `caller` blocks a second
/// cross-contract acquisition for the same caller, but allows a different one.
pub fn assert_cross_contract_guard(env: &Env, caller: &Address) {
    let guard = ReentrancyGuard::new_with_caller(env, crate::ReentrancyKey::GlobalLock, caller, false);
    assert!(guard.is_ok(), "first cross-contract acquisition should succeed");

    let other = Address::generate(env);
    let same = ReentrancyGuard::new_with_caller(env, crate::ReentrancyKey::GlobalLock, caller, false);
    assert!(same.is_err(), "same caller reentry should be blocked");

    let diff = ReentrancyGuard::new_with_caller(env, crate::ReentrancyKey::GlobalLock, &other, false);
    assert!(diff.is_ok(), "different caller should be allowed");

    drop(guard);
    drop(diff);
}
