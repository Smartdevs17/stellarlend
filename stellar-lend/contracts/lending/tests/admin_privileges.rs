//! Least-privilege and parameter-bound tests for the admin surface (#1216).
//!
//! Every capability listed in `docs/ADMIN_PRIVILEGES.md` that is gated or
//! bounded by this change has a test here: the guardian role can pause but
//! never unpause, the insurance pool is bound to the protocol admin, and the
//! borrow / deposit limit setters reject values that would brick the pool.

mod common;

use soroban_sdk::{testutils::Address as _, Address, Env};
use stellarlend_lending::{LendingContract, LendingContractClient, PauseType};
use test_utils::snapshotless_env;

// Contract error codes (the error enums are not re-exported, so assert on codes).
const BORROW_INVALID_AMOUNT: u32 = 4;
const BORROW_PROTOCOL_PAUSED: u32 = 3;
const BORROW_UNAUTHORIZED: u32 = 6;
const DEPOSIT_INVALID_AMOUNT: u32 = 1;
const INSURANCE_UNAUTHORIZED: u32 = 1;

/// Extract the contract error code from a `try_*` call result.
fn err_code<T, E, I>(result: Result<T, Result<E, I>>) -> u32
where
    E: Into<soroban_sdk::Error>,
{
    match result {
        Err(Ok(e)) => e.into().get_code(),
        Err(Err(_)) => panic!("expected a contract error, got a host/invoke error"),
        Ok(_) => panic!("expected a contract error, call succeeded"),
    }
}

fn mocked<'a>() -> (Env, LendingContractClient<'a>, Address) {
    let env = snapshotless_env();
    env.mock_all_auths();
    let (client, admin) = common::deploy(&env);
    (env, client, admin)
}

fn fresh<'a>(env: &Env) -> LendingContractClient<'a> {
    let id = env.register(LendingContract, ());
    LendingContractClient::new(env, &id)
}

// ─── Guardian: pause-only role ───────────────────────────────────────────────

#[test]
fn admin_can_set_guardian() {
    let (env, client, admin) = mocked();
    let guardian = Address::generate(&env);
    assert_eq!(client.get_guardian(), None);
    client.set_guardian(&admin, &guardian);
    assert_eq!(client.get_guardian(), Some(guardian));
}

#[test]
fn non_admin_cannot_set_guardian() {
    let (env, client, _admin) = mocked();
    let attacker = Address::generate(&env);
    assert_eq!(
        err_code(client.try_set_guardian(&attacker, &attacker)),
        BORROW_UNAUTHORIZED
    );
}

#[test]
fn guardian_can_pause_and_paused_operation_is_blocked() {
    let (env, client, admin) = mocked();
    let guardian = Address::generate(&env);
    client.set_guardian(&admin, &guardian);

    client.guardian_pause(&guardian, &PauseType::Borrow);

    let user = Address::generate(&env);
    let asset = Address::generate(&env);
    let collateral = Address::generate(&env);
    assert_eq!(
        err_code(client.try_borrow(&user, &asset, &1_000, &collateral, &2_000)),
        BORROW_PROTOCOL_PAUSED
    );
}

#[test]
fn guardian_cannot_unpause() {
    let (env, client, admin) = mocked();
    let guardian = Address::generate(&env);
    client.set_guardian(&admin, &guardian);
    client.guardian_pause(&guardian, &PauseType::All);

    // The only unpause path is `set_pause`, which is bound to the admin.
    assert_eq!(
        err_code(client.try_set_pause(&guardian, &PauseType::All, &false)),
        BORROW_UNAUTHORIZED
    );

    // The admin can still recover.
    client.set_pause(&admin, &PauseType::All, &false);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);
    let collateral = Address::generate(&env);
    client.borrow(&user, &asset, &1_000, &collateral, &2_000);
}

#[test]
fn guardian_pause_rejects_non_guardian() {
    let (env, client, admin) = mocked();
    let attacker = Address::generate(&env);

    // No guardian configured yet.
    assert_eq!(
        err_code(client.try_guardian_pause(&attacker, &PauseType::All)),
        BORROW_UNAUTHORIZED
    );

    client.set_guardian(&admin, &Address::generate(&env));
    assert_eq!(
        err_code(client.try_guardian_pause(&attacker, &PauseType::All)),
        BORROW_UNAUTHORIZED
    );
    // The admin is not implicitly a guardian either.
    assert_eq!(
        err_code(client.try_guardian_pause(&admin, &PauseType::All)),
        BORROW_UNAUTHORIZED
    );
}

#[test]
#[should_panic(expected = "HostError")]
fn guardian_pause_requires_guardian_signature() {
    let (env, client, admin) = mocked();
    let guardian = Address::generate(&env);
    client.set_guardian(&admin, &guardian);
    env.set_auths(&[]);
    client.guardian_pause(&guardian, &PauseType::All);
}

// ─── Insurance pool bound to the protocol admin ──────────────────────────────

#[test]
fn insurance_initialize_rejects_non_admin() {
    let (env, client, admin) = mocked();
    let attacker = Address::generate(&env);
    assert_eq!(
        err_code(client.try_insurance_initialize(&attacker)),
        INSURANCE_UNAUTHORIZED
    );
    client.insurance_initialize(&admin);
}

#[test]
fn insurance_initialize_rejects_before_protocol_initialize() {
    let env = snapshotless_env();
    env.mock_all_auths();
    let client = fresh(&env);
    let someone = Address::generate(&env);
    assert_eq!(
        err_code(client.try_insurance_initialize(&someone)),
        INSURANCE_UNAUTHORIZED
    );
}

#[test]
#[should_panic(expected = "HostError")]
fn insurance_fund_pool_requires_admin_signature() {
    let (env, client, admin) = mocked();
    client.insurance_initialize(&admin);
    env.set_auths(&[]);
    client.insurance_fund_pool(&1_000_000);
}

#[test]
fn insurance_fund_pool_succeeds_for_admin() {
    let (_env, client, admin) = mocked();
    client.insurance_initialize(&admin);
    client.insurance_fund_pool(&1_000_000);
    assert_eq!(client.insurance_get_analytics().pool_balance, 1_000_000);
}

// ─── Parameter bounds ────────────────────────────────────────────────────────

#[test]
fn initialize_rejects_unusable_borrow_limits() {
    let env = snapshotless_env();
    env.mock_all_auths();
    let admin = Address::generate(&env);

    for (ceiling, min) in [(0, 100), (-1, 100), (1_000, 0), (1_000, -5), (1_000, 1_001)] {
        let client = fresh(&env);
        assert_eq!(
            err_code(client.try_initialize(&admin, &ceiling, &min)),
            BORROW_INVALID_AMOUNT,
            "ceiling={ceiling} min={min}"
        );
        // A rejected initialize must not leave an admin behind.
        assert_eq!(client.get_admin(), None);
    }

    // Boundary: min == ceiling is allowed.
    fresh(&env).initialize(&admin, &1_000, &1_000);
}

#[test]
fn initialize_borrow_settings_rejects_unusable_limits() {
    let (_env, client, _admin) = mocked();
    for (ceiling, min) in [(0, 100), (1_000, 0), (1_000, 1_001)] {
        assert_eq!(
            err_code(client.try_initialize_borrow_settings(&ceiling, &min)),
            BORROW_INVALID_AMOUNT,
            "ceiling={ceiling} min={min}"
        );
    }
    client.initialize_borrow_settings(&5_000_000, &500);
}

#[test]
fn initialize_deposit_settings_rejects_min_above_cap() {
    let (_env, client, _admin) = mocked();
    assert_eq!(
        err_code(client.try_initialize_deposit_settings(&1_000, &1_001)),
        DEPOSIT_INVALID_AMOUNT
    );
    client.initialize_deposit_settings(&1_000, &1_000);
}

#[test]
fn rate_and_threshold_setters_are_bounded() {
    let (_env, client, admin) = mocked();

    assert_eq!(
        err_code(client.try_set_variable_borrow_rate_bps(&admin, &10_001)),
        BORROW_INVALID_AMOUNT
    );
    assert_eq!(
        err_code(client.try_set_variable_borrow_rate_bps(&admin, &-1)),
        BORROW_INVALID_AMOUNT
    );
    client.set_variable_borrow_rate_bps(&admin, &10_000);

    assert_eq!(
        err_code(client.try_set_liquidation_threshold_bps(&admin, &0)),
        BORROW_INVALID_AMOUNT
    );
    assert_eq!(
        err_code(client.try_set_liquidation_threshold_bps(&admin, &10_001)),
        BORROW_INVALID_AMOUNT
    );
    client.set_liquidation_threshold_bps(&admin, &8_500);
}

#[test]
fn admin_setters_reject_non_admin_caller() {
    let (env, client, _admin) = mocked();
    let attacker = Address::generate(&env);

    assert_eq!(
        err_code(client.try_set_oracle(&attacker, &attacker)),
        BORROW_UNAUTHORIZED
    );
    assert_eq!(
        err_code(client.try_set_liquidation_threshold_bps(&attacker, &8_000)),
        BORROW_UNAUTHORIZED
    );
    assert_eq!(
        err_code(client.try_set_variable_borrow_rate_bps(&attacker, &500)),
        BORROW_UNAUTHORIZED
    );
    assert_eq!(
        err_code(client.try_set_pause(&attacker, &PauseType::All, &true)),
        BORROW_UNAUTHORIZED
    );
    assert_eq!(
        err_code(client.try_migrate_pool_state(&attacker)),
        BORROW_UNAUTHORIZED
    );
}

#[test]
fn initialize_cannot_be_called_twice() {
    let (env, client, _admin) = mocked();
    let attacker = Address::generate(&env);
    assert_eq!(
        err_code(client.try_initialize(&attacker, &1_000_000, &100)),
        BORROW_UNAUTHORIZED
    );
}
