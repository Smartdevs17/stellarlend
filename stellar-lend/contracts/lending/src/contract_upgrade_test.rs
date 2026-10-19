use super::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

fn setup(env: &Env) -> (LendingContractClient<'_>, Address) {
    let contract_id = env.register(LendingContract, ());
    let client = LendingContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin, &1_000_000_000, &1000);
    (client, admin)
}

#[test]
fn upgrade_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let stranger = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[7u8; 32]);
    assert_eq!(
        client.try_upgrade(&stranger, &hash),
        Err(Ok(BorrowError::Unauthorized))
    );
}

#[test]
fn upgrade_requires_admin_signature() {
    let env = Env::default();
    let (client, admin) = {
        env.mock_all_auths();
        setup(&env)
    };
    env.set_auths(&[]);
    let hash = BytesN::from_array(&env, &[7u8; 32]);
    // No auth supplied: the host rejects before any code swap.
    assert!(client.try_upgrade(&admin, &hash).is_err());
}

#[test]
fn upgrade_to_unknown_wasm_fails_in_host() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    // Authorised, but the hash was never uploaded: `update_current_contract_wasm` traps,
    // proving the admin path reaches the code swap.
    let hash = BytesN::from_array(&env, &[7u8; 32]);
    assert!(matches!(client.try_upgrade(&admin, &hash), Err(Err(_))));
}
