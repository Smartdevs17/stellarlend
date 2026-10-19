//! Regression tests for past lending-contract incidents (#1440).
//!
//! One test per incident, named `incident_<issue>_<what_must_not_recur>`.
//! Every test here must have a row in `docs/INCIDENT_REGRESSIONS.md`, which
//! records the fix commit and root cause; `index_lists_every_incident` keeps
//! the two in sync.

mod common;

use soroban_sdk::{testutils::Address as _, Address};
use stellarlend_lending::{BorrowError, LendingContract, LendingContractClient};
use test_utils::snapshotless_env;

const INDEX: &str = include_str!("../../../../docs/INCIDENT_REGRESSIONS.md");

const INCIDENT_TESTS: &[&str] = &[
    "incident_915_initialize_requires_admin_auth",
    "incident_1105_initialize_cannot_take_over_live_contract",
    "incident_923_config_setters_reject_non_admin_caller",
    "incident_935_config_setters_require_stored_admin_auth",
    "incident_1128_collateral_deposit_requires_user_auth",
];

/// #915: anyone could front-run `initialize` and install themselves as admin
/// because the supplied admin never had to authorize. Fixed in 739ed3cf.
#[test]
fn incident_915_initialize_requires_admin_auth() {
    let env = snapshotless_env();
    let contract_id = env.register(LendingContract, ());
    let client = LendingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    assert!(client
        .try_initialize(&admin, &common::DEBT_CEILING, &common::MIN_BORROW)
        .is_err());
    assert_eq!(client.get_admin(), None);
}

/// #1105: calling `initialize` again on a live contract swapped the admin and
/// reset accounting. Fixed in f3246f67; the lending contract must keep its
/// already-initialized guard.
#[test]
fn incident_1105_initialize_cannot_take_over_live_contract() {
    let env = snapshotless_env();
    env.mock_all_auths();
    let (client, admin) = common::deploy(&env);
    let attacker = Address::generate(&env);

    assert_eq!(
        client.try_initialize(&attacker, &1, &1),
        Err(Ok(BorrowError::Unauthorized))
    );
    assert_eq!(client.get_admin(), Some(admin));
}

/// #923: configuration setters that take an `admin` argument must compare it
/// with the stored admin, not just check that the caller signed.
#[test]
fn incident_923_config_setters_reject_non_admin_caller() {
    let env = snapshotless_env();
    env.mock_all_auths();
    let (client, _admin) = common::deploy(&env);
    let attacker = Address::generate(&env);
    let fake_oracle = Address::generate(&env);

    assert_eq!(
        client.try_set_oracle(&attacker, &fake_oracle),
        Err(Ok(BorrowError::Unauthorized))
    );
    assert_eq!(
        client.try_set_liquidation_threshold_bps(&attacker, &10_000),
        Err(Ok(BorrowError::Unauthorized))
    );
}

/// #935: parameterless setters (deposit/withdraw/borrow settings, pauses,
/// emergency limit) changed protocol configuration without any auth.
/// Fixed in 0b3b306f by requiring the stored admin's authorization.
#[test]
fn incident_935_config_setters_require_stored_admin_auth() {
    let env = snapshotless_env();
    env.mock_all_auths();
    let (client, _admin) = common::deploy(&env);
    env.set_auths(&[]);

    assert!(client
        .try_initialize_deposit_settings(&common::DEPOSIT_CAP, &1)
        .is_err());
    assert!(client.try_set_deposit_paused(&true).is_err());
    assert!(client.try_set_emergency_withdraw_limit(&1).is_err());
    assert!(client.try_initialize_withdraw_settings(&1).is_err());
    assert!(client.try_set_withdraw_paused(&true).is_err());
    assert!(client
        .try_initialize_borrow_settings(&common::DEBT_CEILING, &1)
        .is_err());
}

/// #1128: `deposit_collateral` credited collateral for a user without that
/// user's authorization, so an attacker could fabricate positions for any
/// address. Fixed in b6d5d96f; the lending contract must keep the same guard.
#[test]
fn incident_1128_collateral_deposit_requires_user_auth() {
    let env = snapshotless_env();
    env.mock_all_auths();
    let (client, _admin) = common::deploy(&env);
    env.set_auths(&[]);
    let victim = Address::generate(&env);
    let asset = Address::generate(&env);

    assert!(client
        .try_deposit_collateral(&victim, &asset, &1_000_000)
        .is_err());
    assert_eq!(client.get_user_position(&victim).collateral_balance, 0);
}

#[test]
fn index_lists_every_incident() {
    for name in INCIDENT_TESTS {
        assert!(
            INDEX.contains(name),
            "{name} is missing from docs/INCIDENT_REGRESSIONS.md"
        );
    }
}
