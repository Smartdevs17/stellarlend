use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
#[should_panic(expected = "HostError")]
fn initialize_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(LendingContract, ());
    let client = LendingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin, &1_000_000_000, &1_000);
}

