mod common;

use soroban_sdk::Env;
use stellarlend_lending::LendingContractClient;
use test_utils::snapshotless_env;

/// Deployed with no auths mocked, so every admin-gated call must fail.
fn setup() -> (Env, LendingContractClient<'static>) {
    let env = snapshotless_env();
    let (client, _admin) = common::deploy(&env);
    (env, client)
}

#[test]
#[should_panic(expected = "HostError")]
fn initialize_deposit_settings_requires_admin_auth() {
    let (_env, client) = setup();
    client.initialize_deposit_settings(&1_000_000_000, &100);
}

#[test]
#[should_panic(expected = "HostError")]
fn set_deposit_paused_requires_admin_auth() {
    let (_env, client) = setup();
    client.set_deposit_paused(&true);
}

#[test]
#[should_panic(expected = "HostError")]
fn set_emergency_withdraw_limit_requires_admin_auth() {
    let (_env, client) = setup();
    client.set_emergency_withdraw_limit(&1_000);
}

#[test]
#[should_panic(expected = "HostError")]
fn initialize_withdraw_settings_requires_admin_auth() {
    let (_env, client) = setup();
    client.initialize_withdraw_settings(&100);
}

#[test]
#[should_panic(expected = "HostError")]
fn set_withdraw_paused_requires_admin_auth() {
    let (_env, client) = setup();
    client.set_withdraw_paused(&true);
}

#[test]
#[should_panic(expected = "HostError")]
fn initialize_borrow_settings_requires_admin_auth() {
    let (_env, client) = setup();
    client.initialize_borrow_settings(&1_000_000_000, &1_000);
}
