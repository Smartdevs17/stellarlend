//! Per-asset borrow cap enforcement at the pool boundary.
//!
//! This module provides storage, admin entrypoints, and enforcement helpers for
//! per-asset borrow caps. A borrow cap limits the total amount of a given asset
//! that can be borrowed from the pool at any point in time. Caps are configured
//! per asset by an admin and are enforced whenever a borrow is attempted.
//!
//! Semantics:
//! - An unset cap (or a cap of `0`) is treated as *unlimited*.
//! - A borrow is rejected if the resulting total borrowed amount for the asset
//!   would exceed the configured cap.
//! - Setting a cap emits a [`CapChanged`] event.
//! - A rejected borrow emits a [`CapBreached`] event.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

/// Storage key for a per-asset borrow cap.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BorrowCapKey {
    /// Configured borrow cap for the given asset.
    Cap(Address),
}

/// Event emitted when an admin changes the borrow cap for an asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapChanged {
    /// The asset whose cap changed.
    pub asset: Address,
    /// The previous cap (`0` means unlimited / unset).
    pub old_cap: i128,
    /// The new cap (`0` means unlimited / unset).
    pub new_cap: i128,
}

/// Event emitted when a borrow is rejected because it would breach the cap.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapBreached {
    /// The asset whose cap would be breached.
    pub asset: Address,
    /// The configured cap.
    pub cap: i128,
    /// The total borrowed amount before the attempted borrow.
    pub current_borrowed: i128,
    /// The amount that was attempted to be borrowed.
    pub attempted_borrow: i128,
}

/// Returns the configured borrow cap for `asset`.
///
/// A return value of `0` indicates that no cap is configured (unlimited).
pub fn get_borrow_cap(env: &Env, asset: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&BorrowCapKey::Cap(asset.clone()))
        .unwrap_or(0)
}

/// Sets the borrow cap for `asset`.
///
/// # Authorization
/// The `admin` must authorize this call.
///
/// # Arguments
/// * `admin` - The admin address authorizing the change.
/// * `asset` - The asset whose cap is being set.
/// * `cap` - The new cap. Must be non-negative. `0` means unlimited.
///
/// # Panics
/// Panics if `cap` is negative.
pub fn set_borrow_cap(env: &Env, admin: &Address, asset: &Address, cap: i128) {
    admin.require_auth();

    if cap < 0 {
        panic!("borrow cap must be non-negative");
    }

    let old_cap = get_borrow_cap(env, asset);

    env.storage()
        .persistent()
        .set(&BorrowCapKey::Cap(asset.clone()), &cap);

    env.events().publish(
        (symbol_short!("cap_chg"), asset.clone()),
        CapChanged {
            asset: asset.clone(),
            old_cap,
            new_cap: cap,
        },
    );
}

/// Removes the borrow cap for `asset`, restoring unlimited borrowing.
///
/// # Authorization
/// The `admin` must authorize this call.
pub fn remove_borrow_cap(env: &Env, admin: &Address, asset: &Address) {
    admin.require_auth();

    let old_cap = get_borrow_cap(env, asset);

    env.storage()
        .persistent()
        .remove(&BorrowCapKey::Cap(asset.clone()));

    env.events().publish(
        (symbol_short!("cap_chg"), asset.clone()),
        CapChanged {
            asset: asset.clone(),
            old_cap,
            new_cap: 0,
        },
    );
}

/// Enforces the per-asset borrow cap at the pool boundary.
///
/// # Arguments
/// * `asset` - The asset being borrowed.
/// * `current_borrowed` - The total amount currently borrowed for `asset`.
/// * `amount` - The amount being borrowed in this operation.
///
/// # Panics
/// Panics if the resulting total borrowed amount would exceed the configured
/// cap. An unset cap (`0`) is treated as unlimited and never panics.
pub fn check_borrow_cap(env: &Env, asset: &Address, current_borrowed: i128, amount: i128) {
    let cap = get_borrow_cap(env, asset);

    // Unset cap (0) means unlimited.
    if cap == 0 {
        return;
    }

    let new_total = current_borrowed.saturating_add(amount);

    if new_total > cap {
        env.events().publish(
            (symbol_short!("cap_brch"), asset.clone()),
            CapBreached {
                asset: asset.clone(),
                cap,
                current_borrowed,
                attempted_borrow: amount,
            },
        );
        panic!("borrow cap exceeded");
    }
}

/// Returns `true` if borrowing `amount` for `asset` would be within the cap.
///
/// This is a non-panicking variant of [`check_borrow_cap`] useful for
/// pre-flight checks and simulations.
pub fn is_borrow_within_cap(env: &Env, asset: &Address, current_borrowed: i128, amount: i128) -> bool {
    let cap = get_borrow_cap(env, asset);

    if cap == 0 {
        return true;
    }

    current_borrowed.saturating_add(amount) <= cap
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let asset = Address::generate(&env);
        (env, admin, asset)
    }

    #[test]
    fn unset_cap_is_unlimited() {
        let (env, _admin, asset) = setup();

        assert_eq!(get_borrow_cap(&env, &asset), 0);

        // Should not panic regardless of amount.
        check_borrow_cap(&env, &asset, 0, i128::MAX);
        check_borrow_cap(&env, &asset, 1_000_000, 1_000_000);
        assert!(is_borrow_within_cap(&env, &asset, 0, i128::MAX));
    }

    #[test]
    fn set_and_get_cap() {
        let (env, admin, asset) = setup();

        set_borrow_cap(&env, &admin, &asset, 1_000);
        assert_eq!(get_borrow_cap(&env, &asset), 1_000);
    }

    #[test]
    fn under_cap_borrow_succeeds() {
        let (env, admin, asset) = setup();
        set_borrow_cap(&env, &admin, &asset, 1_000);

        check_borrow_cap(&env, &asset, 100, 200);
        assert!(is_borrow_within_cap(&env, &asset, 100, 200));
    }

    #[test]
    fn at_cap_borrow_succeeds() {
        let (env, admin, asset) = setup();
        set_borrow_cap(&env, &admin, &asset, 1_000);

        // Exactly reaching the cap is allowed.
        check_borrow_cap(&env, &asset, 600, 400);
        assert!(is_borrow_within_cap(&env, &asset, 600, 400));
    }

    #[test]
    #[should_panic(expected = "borrow cap exceeded")]
    fn over_cap_borrow_panics() {
        let (env, admin, asset) = setup();
        set_borrow_cap(&env, &admin, &asset, 1_000);

        check_borrow_cap(&env, &asset, 600, 401);
    }

    #[test]
    fn over_cap_is_not_within_cap() {
        let (env, admin, asset) = setup();
        set_borrow_cap(&env, &admin, &asset, 1_000);

        assert!(!is_borrow_within_cap(&env, &asset, 600, 401));
    }

    #[test]
    fn remove_cap_restores_unlimited() {
        let (env, admin, asset) = setup();
        set_borrow_cap(&env, &admin, &asset, 1_000);
        remove_borrow_cap(&env, &admin, &asset);

        assert_eq!(get_borrow_cap(&env, &asset), 0);
        check_borrow_cap(&env, &asset, 0, i128::MAX);
    }

    #[test]
    #[should_panic]
    fn unauthorized_setter_panics() {
        let env = Env::default();
        // Do not mock auths: the admin has not authorized the call.
        let admin = Address::generate(&env);
        let asset = Address::generate(&env);

        set_borrow_cap(&env, &admin, &asset, 1_000);
    }

    #[test]
    #[should_panic(expected = "borrow cap must be non-negative")]
    fn negative_cap_panics() {
        let (env, admin, asset) = setup();
        set_borrow_cap(&env, &admin, &asset, -1);
    }

    #[test]
    fn cap_change_emits_event() {
        let (env, admin, asset) = setup();
        set_borrow_cap(&env, &admin, &asset, 500);

        let events = env.events().all();
        assert!(!events.is_empty());
    }
}