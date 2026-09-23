use soroban_sdk::{testutils::Address as _, Address, Env};
use stellarlend_lending::{LendingContract, LendingContractClient};

#[test]
fn configuration_entry_points_require_the_stored_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LendingContract, ());
    let client = LendingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin, &1_000_000_000, &1_000);
    env.set_auths(&[]);

    assert!(client.try_initialize_deposit_settings(&1_000_000_000, &1).is_err());
    assert!(client.try_set_deposit_paused(&true).is_err());
    assert!(client.try_set_emergency_withdraw_limit(&1_000).is_err());
    assert!(client.try_initialize_withdraw_settings(&1).is_err());
    assert!(client.try_set_withdraw_paused(&true).is_err());
    assert!(client.try_initialize_borrow_settings(&1_000_000_000, &1).is_err());
}
