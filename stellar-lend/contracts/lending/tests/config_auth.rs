use soroban_sdk::{testutils::Address as _, Address, Env};
use stellarlend_lending::{LendingContract, LendingContractClient};

fn setup() -> (Env, LendingContractClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(LendingContract, ());
    let client = LendingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &1_000_000_000, &1_000);
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
