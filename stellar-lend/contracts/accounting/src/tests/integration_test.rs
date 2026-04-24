#![cfg(test)]

use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

use crate::{AccountingContract, AccountingContractClient};

#[test]
fn test_full_subscription_workflow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountingContract);
    let client = AccountingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    // 1. Initialize contract
    client.initialize(&admin);

    // 2. Create subscription
    let total_amount = 12_000_000i128; // 12M stroops
    let start_time = env.ledger().timestamp();
    
    let subscription_id = client.create_subscription(&merchant, &total_amount, &start_time);
    assert_eq!(subscription_id, 1);

    // 3. Check initial deferred revenue
    let deferred_result = client.try_get_deferred_revenue(&merchant);
    assert!(deferred_result.is_ok());
    let deferred = deferred_result.unwrap().unwrap();
    assert_eq!(deferred, total_amount);

    // 4. Configure recognition rule (straight-line, 1 year)
    let method = 0u32; // Straight-line
    let recognition_period = 365 * 24 * 60 * 60u64;
    
    client.configure_recognition_rule(&merchant, &subscription_id, &method, &recognition_period);

    // 5. Get revenue schedule
    let schedule_result = client.try_get_revenue_schedule(&subscription_id);
    assert!(schedule_result.is_ok());
    let schedule = schedule_result.unwrap().unwrap();
    assert_eq!(schedule.subscription_id, subscription_id);
    assert_eq!(schedule.total_amount, total_amount);
    assert!(schedule.entries.len() > 0);

    // 6. Advance time by 1 month
    env.ledger().with_mut(|li| {
        li.timestamp = start_time + (30 * 24 * 60 * 60);
    });

    // 7. Recognize revenue
    let recognition_result = client.try_recognize_revenue(&merchant, &subscription_id);
    assert!(recognition_result.is_ok());
    let recognition = recognition_result.unwrap().unwrap();
    assert!(recognition.recognized_amount > 0);
    assert_eq!(recognition.subscription_id, subscription_id);

    // 8. Check deferred revenue decreased
    let deferred_after = client.try_get_deferred_revenue(&merchant).unwrap().unwrap();
    assert!(deferred_after < total_amount);

    // 9. Get analytics
    let end_time = start_time + (365 * 24 * 60 * 60);
    let analytics_result = client.try_get_revenue_analytics(&merchant, &start_time, &end_time);
    assert!(analytics_result.is_ok());
    let analytics = analytics_result.unwrap().unwrap();
    assert_eq!(analytics.subscription_count, 1);
    assert!(analytics.recognized_revenue > 0);
}

#[test]
fn test_contract_modification_workflow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountingContract);
    let client = AccountingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    client.initialize(&admin);

    // Create subscription
    let total_amount = 10_000_000i128;
    let start_time = env.ledger().timestamp();
    let subscription_id = client.create_subscription(&merchant, &total_amount, &start_time);

    // Configure recognition
    client.configure_recognition_rule(&merchant, &subscription_id, &0u32, &(365 * 24 * 60 * 60));

    // Modify contract
    let new_amount = 15_000_000i128;
    client.handle_contract_modification(&merchant, &subscription_id, &new_amount);

    // Check deferred revenue updated
    let deferred = client.try_get_deferred_revenue(&merchant).unwrap().unwrap();
    assert_eq!(deferred, new_amount);
}

#[test]
fn test_cancellation_workflow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountingContract);
    let client = AccountingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    client.initialize(&admin);

    // Create subscription
    let total_amount = 12_000_000i128;
    let start_time = env.ledger().timestamp();
    let subscription_id = client.create_subscription(&merchant, &total_amount, &start_time);

    // Configure recognition (1 year)
    let recognition_period = 365 * 24 * 60 * 60u64;
    client.configure_recognition_rule(&merchant, &subscription_id, &0u32, &recognition_period);

    // Advance time by 3 months
    env.ledger().with_mut(|li| {
        li.timestamp = start_time + (90 * 24 * 60 * 60);
    });

    // Recognize revenue for 3 months
    client.try_recognize_revenue(&merchant, &subscription_id).unwrap().unwrap();

    // Cancel subscription
    let refund_result = client.try_handle_cancellation(&merchant, &subscription_id);
    assert!(refund_result.is_ok());
    let refund = refund_result.unwrap().unwrap();
    
    // Refund should be approximately 75% (9 months remaining out of 12)
    let expected_refund = (total_amount * 9) / 12;
    let tolerance = total_amount / 100; // 1% tolerance
    assert!((refund - expected_refund).abs() < tolerance);
}

#[test]
fn test_multiple_subscriptions() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountingContract);
    let client = AccountingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    client.initialize(&admin);

    let start_time = env.ledger().timestamp();

    // Create multiple subscriptions
    let sub1 = client.create_subscription(&merchant, &10_000_000, &start_time);
    let sub2 = client.create_subscription(&merchant, &20_000_000, &start_time);
    let sub3 = client.create_subscription(&merchant, &30_000_000, &start_time);

    assert_eq!(sub1, 1);
    assert_eq!(sub2, 2);
    assert_eq!(sub3, 3);

    // Check total deferred revenue
    let deferred = client.try_get_deferred_revenue(&merchant).unwrap().unwrap();
    assert_eq!(deferred, 60_000_000);

    // Get analytics
    let end_time = start_time + (365 * 24 * 60 * 60);
    let analytics_result = client.try_get_revenue_analytics(&merchant, &start_time, &end_time);
    assert!(analytics_result.is_ok());
    let analytics = analytics_result.unwrap().unwrap();
    assert_eq!(analytics.subscription_count, 3);
    assert_eq!(analytics.total_revenue, 60_000_000);
}
