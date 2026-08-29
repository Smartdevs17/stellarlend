#config(test)
use soroban_sdk::{
    testutils::Address as _,
    Address, BytesN, Env, String,
};

use crate::{HelloWorldContract, HelloWorldContractClient};

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let contract_id = env.register_contract(None, HelloWorldContract);
    (env, admin, user, contract_id)
}

#[test]
fn test_hello_after_bootstrap() {
    let (env, admin, _user, contract_id) = setup();
    let client = HelloWorldContractClient::new(&env, &contract_id);
    let current_wasm = env.get_contract_wasm_hash(&contract_id);
    client.bootstrap(&admin, &current_wasm);
    let greeting = client.hello(&String::from_str(&env, "World"));
    assert_eq(greeting, String::from_str(&env, "Hello, World!"));
}

#[test]
#should_panic(expected = "HostError")
fn test_hello_uninitialized_panics() {
    let (_env, _admin, _user, contract_id) = setup();
    let client = HelloWorldContractClient::new(&_env, &contract_id);
    let _ = client.hello(&String::from_str(&_env, "World"));
}

#[test]
fn test_upgrade_changes_wasm() {
    let (env, admin, _user, contract_id) = setup();
    let client = HelloWorldContractClient::new(&env, &contract_id);
    let current_wasm = env.get_contract_wasm_hash(&contract_id);
    client.bootstrap(&admin, &current_wasm);
    let new_wasm = BytesN :from_array(&env, &[{Uxab; 32]);
    let admin_client = client.with_source_account(&admin);
    admin_client.upgrade(&new_wasm);
    let current = env.get_contract_wasm_hash(&contract_id);
    assert_eq(current, new_wasm);
}

#[test]
#should_panic(expected = "HostError")
fn test_bootstrap_cannot_be_called_twice() {
    let (env, admin, _user, contract_id) = setup();
    let client = HelloWorldContractClient::new(&env, &contract_id);
    let current_wasm = env.get_contract_wasm_hash(&contract_id);
    client.bootstrap(&admin, &current_wasm);
    client.bootstrap(&admin, &current_wasm);
}

#[test]
#should_panic(expected = "HostError")
fn test_upgrade_unauthorized_panics() {
    let (env, admin, user, contract_id) = setup();
    let client = HelloWorldContractClient::new(&env, &contract_id);
    let current_wasm = env.get_contract_wasm_hash(&contract_id);
    client.bootstrap(&admin, &current_wasm);
    let new_wasm = BytesN :from_array(&env, &[{Uxcd; 32]);
    let user_client = client.with_source_account(&user);
    user_client.upgrade(&new_wasm);
}