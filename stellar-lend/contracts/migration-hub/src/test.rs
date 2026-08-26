#![cfg(test)]

use super::*;
use crate::types::{MigrationStatus, ProtocolType};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    vec, Address, BytesN, Env, String,
};

#[test]
fn test_migration_stellar_other() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lending = Address::generate(&env);
    let bridge = Address::generate(&env);
    let asset = env.register_stellar_asset_contract(admin.clone());

    let contract_id = env.register_contract(None, MigrationHub);
    let client = MigrationHubClient::new(&env, &contract_id);

    client.initialize(&admin, &lending, &bridge, &100, &2_000_000);

    // Mock tokens for user
    let token = soroban_sdk::token::Client::new(&env, &asset);
    token.mint(&user, &1000);

    let migration_id = client.migrate(
        &user,
        &ProtocolType::StellarOther,
        &Address::generate(&env), // source protocol address
        &asset,
        &500,
    );

    let record = client.get_migration(&migration_id).unwrap();
    assert_eq!(record.status, MigrationStatus::Completed);
    assert_eq!(record.amount, 500);
    assert_eq!(token.balance(&contract_id), 500); // Funds moved to Hub

    let analytics = client.get_analytics();
    assert_eq!(analytics.successful_migrations, 1);
    assert_eq!(analytics.total_migrated_value, 500);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #8)")]
fn test_migration_deadline_exceeded() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lending = Address::generate(&env);
    let bridge = Address::generate(&env);
    let asset = Address::generate(&env);

    let contract_id = env.register_contract(None, MigrationHub);
    let client = MigrationHubClient::new(&env, &contract_id);

    client.initialize(&admin, &lending, &bridge, &100, &1000); // Deadline at 1000

    env.ledger().set_timestamp(2000);

    client.migrate(
        &user,
        &ProtocolType::StellarOther,
        &Address::generate(&env), // source protocol address
        &asset,
        &500,
    );
}

#[test]
fn test_upgrade_propose_and_status() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let lending = Address::generate(&env);
    let bridge = Address::generate(&env);

    let contract_id = env.register_contract(None, MigrationHub);
    let client = MigrationHubClient::new(&env, &contract_id);

    client.initialize(&admin, &lending, &bridge, &100, &2_000_000);
    let current_hash = client.current_wasm_hash();
    let new_hash = BytesN::from_array(&env, &[1u8; 32]);

    let proposal_id = client.upgrade_propose(&admin, &new_hash, &1);
    let status = client.upgrade_status(proposal_id);
    assert_eq!(status.target_version, 1);
    assert_eq!(status.stage, stellarlend_common::upgrade::UpgradeStage::Approved);
}

#[test]
fn test_upgrade_execute_and_rollback() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let lending = Address::generate(&env);
    let bridge = Address::generate(&env);

    let contract_id = env.register_contract(None, MigrationHub);
    let client = MigrationHubClient::new(&env, &contract_id);

    client.initialize(&admin, &lending, &bridge, &100, &2_000_000);
    let new_hash = BytesN::from_array(&env, &[2u8; 32]);

    let proposal_id = client.upgrade_propose(&admin, &new_hash, &1);
    client.upgrade_queue_timelock(&admin, &proposal_id);
    env.ledger().set_timestamp(env.ledger().timestamp() + 172_801);
    client.upgrade_execute(&admin, &proposal_id);

    assert_eq!(client.current_version(), 1);
    assert_eq!(client.current_wasm_hash(), new_hash);

    client.upgrade_rollback(&admin, &proposal_id);
    assert_eq!(client.current_version(), 0);
}
