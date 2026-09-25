#![cfg(test)]

use super::*;
use crate::types::{MigrationStatus, ProtocolType};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, BytesN, Env,
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

    let token_admin = StellarAssetClient::new(&env, &asset);
    token_admin.mint(&user, &1000);

    let token = soroban_sdk::token::Client::new(&env, &asset);

    let migration_id = client.migrate(
        &user,
        &ProtocolType::StellarOther,
        &Address::generate(&env),
        &asset,
        &500,
    );

    let record = client.get_migration(&migration_id).unwrap();
    assert_eq!(record.status, MigrationStatus::Completed);
    assert_eq!(record.amount, 500);
    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&lending), 500);
    assert_eq!(client.verify_migration(&migration_id), true);

    let analytics = client.get_analytics();
    assert_eq!(analytics.successful_migrations, 1);
    assert_eq!(analytics.total_migrated_value, 500);
}

#[test]
fn test_migration_aave_mock() {
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

    let token_admin = StellarAssetClient::new(&env, &asset);
    token_admin.mint(&user, &1500);

    let token = soroban_sdk::token::Client::new(&env, &asset);

    let migration_id = client.migrate(
        &user,
        &ProtocolType::AaveMock,
        &Address::generate(&env),
        &asset,
        &750,
    );

    let record = client.get_migration(&migration_id).unwrap();
    assert_eq!(record.status, MigrationStatus::Completed);
    assert_eq!(record.amount, 750);
    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&lending), 750);
    assert_eq!(client.verify_migration(&migration_id), true);
}

#[test]
fn test_invariant_no_stranded_funds_multi_user() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let lending = Address::generate(&env);
    let bridge = Address::generate(&env);
    let asset = env.register_stellar_asset_contract(admin.clone());

    let contract_id = env.register_contract(None, MigrationHub);
    let client = MigrationHubClient::new(&env, &contract_id);

    client.initialize(&admin, &lending, &bridge, &100, &2_000_000);

    let token_admin = StellarAssetClient::new(&env, &asset);
    token_admin.mint(&user1, &1000);
    token_admin.mint(&user2, &2000);

    let token = soroban_sdk::token::Client::new(&env, &asset);

    let id1 = client.migrate(
        &user1,
        &ProtocolType::StellarOther,
        &Address::generate(&env),
        &asset,
        &400,
    );
    let id2 = client.migrate(
        &user2,
        &ProtocolType::AaveMock,
        &Address::generate(&env),
        &asset,
        &1200,
    );

    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&lending), 1600);
    assert_eq!(client.verify_migration(&id1), true);
    assert_eq!(client.verify_migration(&id2), true);
}

#[test]
fn test_refund_stranded_funds_when_failed() {
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

    let token_admin = StellarAssetClient::new(&env, &asset);
    token_admin.mint(&user, &1000);
    token_admin.mint(&contract_id, &500);

    let token = soroban_sdk::token::Client::new(&env, &asset);

    let record = MigrationRecord {
        user: user.clone(),
        protocol: ProtocolType::StellarOther,
        asset: asset.clone(),
        amount: 500,
        status: MigrationStatus::Failed,
        timestamp: env.ledger().timestamp(),
        source_pool: Address::generate(&env),
        destination_pool: lending.clone(),
        interest_at_migration: 0,
        is_partial: false,
        source_position_id: None,
    };
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Migration(1), &record);
    });

    client.refund_migration(&user, &1);
    assert_eq!(token.balance(&user), 1500);
    assert_eq!(token.balance(&contract_id), 0);

    let updated = client.get_migration(&1).unwrap();
    assert_eq!(updated.status, MigrationStatus::Refunded);
}

#[test]
fn test_refund_completed_migration_rejected() {
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

    let token_admin = StellarAssetClient::new(&env, &asset);
    token_admin.mint(&user, &1000);

    let migration_id = client.migrate(
        &user,
        &ProtocolType::StellarOther,
        &Address::generate(&env),
        &asset,
        &500,
    );

    let res = client.try_refund_migration(&user, &migration_id);
    assert!(res.is_err());
}

#[test]
fn test_refund_unauthorized_user_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let attacker = Address::generate(&env);
    let lending = Address::generate(&env);
    let bridge = Address::generate(&env);
    let asset = env.register_stellar_asset_contract(admin.clone());

    let contract_id = env.register_contract(None, MigrationHub);
    let client = MigrationHubClient::new(&env, &contract_id);

    client.initialize(&admin, &lending, &bridge, &100, &2_000_000);

    let token_admin = StellarAssetClient::new(&env, &asset);
    token_admin.mint(&contract_id, &500);

    let record = MigrationRecord {
        user: user.clone(),
        protocol: ProtocolType::StellarOther,
        asset: asset.clone(),
        amount: 500,
        status: MigrationStatus::Failed,
        timestamp: env.ledger().timestamp(),
        source_pool: Address::generate(&env),
        destination_pool: lending.clone(),
        interest_at_migration: 0,
        is_partial: false,
        source_position_id: None,
    };
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Migration(1), &record);
    });

    let res = client.try_refund_migration(&attacker, &1);
    assert!(res.is_err());
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

    client.initialize(&admin, &lending, &bridge, &100, &1000);

    env.ledger().set_timestamp(2000);

    client.migrate(
        &user,
        &ProtocolType::StellarOther,
        &Address::generate(&env),
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
    let initial_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.upgrade_init(&admin, &initial_hash, &1);
    let current_hash = client.current_wasm_hash();
    assert_eq!(current_hash, initial_hash);
    let new_hash = BytesN::from_array(&env, &[1u8; 32]);

    let proposal_id = client.upgrade_propose(&admin, &new_hash, &1);
    let status = client.upgrade_status(&proposal_id);
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
    let initial_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.upgrade_init(&admin, &initial_hash, &1);
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
