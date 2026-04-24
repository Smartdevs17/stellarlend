#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{AccountingContract, AccountingContractClient};

fn setup_test_env<'a>(env: &'a Env) -> (AccountingContractClient<'a>, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountingContract);
    let client = AccountingContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let merchant = Address::generate(env);

    client.initialize(&admin);

    (client, admin, merchant)
}

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountingContract);
    let client = AccountingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let result = client.try_initialize(&admin);

    assert!(result.is_ok());
}

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountingContract);
    let client = AccountingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let result = client.try_initialize(&admin);
    assert!(result.is_err());
}

#[test]
fn test_get_deferred_revenue_initial() {
    let env = Env::default();
    let (client, _admin, merchant) = setup_test_env(&env);

    let result = client.try_get_deferred_revenue(&merchant);
    assert!(result.is_ok());
    let deferred = result.unwrap();
    assert!(deferred.is_ok());
    assert_eq!(deferred.unwrap(), 0);
}

#[test]
fn test_configure_recognition_rule() {
    let env = Env::default();
    let (client, _admin, merchant) = setup_test_env(&env);

    let subscription_id = 1u64;
    let method = 0u32; // Straight-line
    let recognition_period = 365 * 24 * 60 * 60u64; // 1 year

    // This will fail because subscription doesn't exist yet
    let result = client.try_configure_recognition_rule(&merchant, &subscription_id, &method, &recognition_period);
    
    // Expected to fail with SubscriptionNotFound
    assert!(result.is_err());
}

#[test]
fn test_get_revenue_schedule_not_found() {
    let env = Env::default();
    let (client, _admin, _merchant) = setup_test_env(&env);

    let subscription_id = 999u64;
    let result = client.try_get_revenue_schedule(&subscription_id);
    
    // Should fail with SubscriptionNotFound
    assert!(result.is_err());
}

#[test]
fn test_get_revenue_analytics() {
    let env = Env::default();
    let (client, _admin, merchant) = setup_test_env(&env);

    let start_time = env.ledger().timestamp();
    let end_time = start_time + (365 * 24 * 60 * 60);

    let result = client.try_get_revenue_analytics(&merchant, &start_time, &end_time);
    
    // Should succeed with empty analytics
    assert!(result.is_ok());
}

#[test]
fn test_handle_cancellation_not_found() {
    let env = Env::default();
    let (client, _admin, merchant) = setup_test_env(&env);

    let subscription_id = 999u64;
    let result = client.try_handle_cancellation(&merchant, &subscription_id);
    
    // Should fail with SubscriptionNotFound
    assert!(result.is_err());
}

#[test]
fn test_handle_contract_modification_not_found() {
    let env = Env::default();
    let (client, _admin, merchant) = setup_test_env(&env);

    let subscription_id = 999u64;
    let new_amount = 15_000_000i128;
    let result = client.try_handle_contract_modification(&merchant, &subscription_id, &new_amount);
    
    // Should fail with SubscriptionNotFound
    assert!(result.is_err());
}

#[test]
fn test_recognize_revenue_not_found() {
    let env = Env::default();
    let (client, _admin, merchant) = setup_test_env(&env);

    let subscription_id = 999u64;
    let result = client.try_recognize_revenue(&merchant, &subscription_id);
    
    // Should fail with SubscriptionNotFound
    assert!(result.is_err());
}

// Note: Full integration tests would require implementing the create_subscription
// helper function as a contract entrypoint, or testing through a complete workflow.
// The tests above verify the contract interface and basic error handling.
