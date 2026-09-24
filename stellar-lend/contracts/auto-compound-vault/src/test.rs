#![cfg(test)]
use crate::{AutoCompoundVault, AutoCompoundVaultClient, VaultConfig, VaultError};
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup() -> (Env, Address, Address, AutoCompoundVaultClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let share_token = Address::generate(&env);
    let underlying = Address::generate(&env);
    let reward = Address::generate(&env);
    let contract_id = env.register(AutoCompoundVault, ());
    let client = AutoCompoundVaultClient::new(&env, &contract_id);

    let config = VaultConfig {
        performance_fee_bps: 1_000,
        management_fee_bps: 200,
        harvest_interval_secs: 3600,
        slippage_tolerance_bps: 100,
        deposit_paused: false,
        withdraw_paused: false,
        active: true,
    };

    client.initialize(&admin, &share_token, &underlying, &reward, &config);
    (env, admin, share_token, client)
}

fn setup_with_real_tokens() -> (
    Env,
    Address,
    Address,
    Address,
    AutoCompoundVaultClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(AutoCompoundVault, ());
    let share_token = env
        .register_stellar_asset_contract_v2(contract_id.clone())
        .address();
    let underlying_admin = Address::generate(&env);
    let underlying = env
        .register_stellar_asset_contract_v2(underlying_admin)
        .address();
    let client = AutoCompoundVaultClient::new(&env, &contract_id);

    let config = VaultConfig {
        performance_fee_bps: 1_000,
        management_fee_bps: 200,
        harvest_interval_secs: 3600,
        slippage_tolerance_bps: 100,
        deposit_paused: false,
        withdraw_paused: false,
        active: true,
    };
    client.initialize(&admin, &share_token, &underlying, &underlying, &config);

    (env, contract_id, share_token, underlying, client)
}

#[test]
fn test_initialize() {
    let (_env, _admin, _share_token, client) = setup();
    let config = client.get_config();
    assert_eq!(config.performance_fee_bps, 1_000);
    assert_eq!(config.management_fee_bps, 200);
    assert!(!config.deposit_paused);
    assert!(!config.withdraw_paused);
    assert!(config.active);
}

#[test]
fn test_preview_deposit_empty_vault() {
    let (_env, _admin, _share_token, client) = setup();
    let shares = client.preview_deposit(&1000);
    assert_eq!(shares, 1000);
}

#[test]
fn test_preview_withdraw_empty_vault() {
    let (_env, _admin, _share_token, client) = setup();
    let assets = client.preview_withdraw(&1000);
    assert_eq!(assets, 0);
}

#[test]
fn test_get_share_price_initial() {
    let (_env, _admin, _share_token, client) = setup();
    let price = client.get_share_price();
    assert_eq!(price, 1_000_000_000);
}

#[test]
fn test_get_vault_snapshot() {
    let (_env, _admin, _share_token, client) = setup();
    let snapshot = client.get_vault_snapshot();
    assert_eq!(snapshot.total_assets, 0);
    assert_eq!(snapshot.total_shares, 0);
    assert_eq!(snapshot.share_price, 1_000_000_000);
    assert_eq!(snapshot.accrued_management_fees, 0);
    assert_eq!(snapshot.accrued_performance_fees, 0);
}

#[test]
fn test_deposit_paused() {
    let (_env, admin, _share_token, client) = setup();
    let user = Address::generate(&_env);

    let paused_config = VaultConfig {
        deposit_paused: true,
        ..client.get_config()
    };
    client.set_config(&admin, &paused_config);

    let result = client.try_deposit(&user, &1000, &0);
    assert_eq!(result, Err(Ok(VaultError::DepositPaused)));
}

#[test]
fn test_withdraw_paused() {
    let (_env, admin, _share_token, client) = setup();
    let user = Address::generate(&_env);

    let paused_config = VaultConfig {
        withdraw_paused: true,
        ..client.get_config()
    };
    client.set_config(&admin, &paused_config);

    let result = client.try_withdraw(&user, &100, &0);
    assert_eq!(result, Err(Ok(VaultError::WithdrawPaused)));
}

#[test]
fn test_harvest_interval_not_met() {
    let (_env, _contract_id, _share_token, _underlying, client) = setup_with_real_tokens();
    let caller = Address::generate(&_env);

    let result = client.try_harvest(&caller, &0);
    assert_eq!(result, Err(Ok(VaultError::NoRewardsToHarvest)));
}

#[test]
fn deposit_and_withdraw_move_underlying_tokens() {
    let (env, contract_id, _share_token, underlying, client) = setup_with_real_tokens();
    let user = Address::generate(&env);
    let underlying_client = token::StellarAssetClient::new(&env, &underlying);
    underlying_client.mint(&user, &1_000);

    let shares = client.deposit(&user, &1_000, &1_000);
    assert_eq!(shares, 1_000);
    assert_eq!(token::Client::new(&env, &underlying).balance(&user), 0);
    assert_eq!(
        token::Client::new(&env, &underlying).balance(&contract_id),
        1_000
    );

    let assets = client.withdraw(&user, &400, &400);
    assert_eq!(assets, 400);
    assert_eq!(token::Client::new(&env, &underlying).balance(&user), 400);
    assert_eq!(
        token::Client::new(&env, &underlying).balance(&contract_id),
        600
    );
}

#[test]
fn deposit_requires_underlying_balance() {
    let (env, _contract_id, share_token, _underlying, client) = setup_with_real_tokens();
    let user = Address::generate(&env);

    assert!(client.try_deposit(&user, &1_000, &0).is_err());
    assert_eq!(client.get_vault_snapshot().total_assets, 0);
    assert_eq!(token::Client::new(&env, &share_token).balance(&user), 0);
}

#[test]
fn harvest_only_counts_actual_underlying_surplus() {
    let (env, contract_id, _share_token, underlying, client) = setup_with_real_tokens();
    let user = Address::generate(&env);
    let caller = Address::generate(&env);
    let underlying_client = token::StellarAssetClient::new(&env, &underlying);
    underlying_client.mint(&user, &1_000);
    client.deposit(&user, &1_000, &1_000);

    underlying_client.mint(&contract_id, &100);
    let reinvested = client.harvest(&caller, &0);

    assert_eq!(reinvested, 90);
    assert_eq!(client.get_vault_snapshot().total_assets, 1_090);
}

#[test]
fn harvest_does_not_recount_accrued_performance_fees() {
    let (env, contract_id, _share_token, underlying, client) = setup_with_real_tokens();
    let user = Address::generate(&env);
    let caller = Address::generate(&env);
    let underlying_client = token::StellarAssetClient::new(&env, &underlying);
    underlying_client.mint(&user, &1_000);
    client.deposit(&user, &1_000, &1_000);

    underlying_client.mint(&contract_id, &100);
    assert_eq!(client.harvest(&caller, &0), 90);
    assert_eq!(client.get_vault_snapshot().accrued_performance_fees, 10);

    underlying_client.mint(&contract_id, &100);
    assert_eq!(client.harvest(&caller, &0), 90);
    let snapshot = client.get_vault_snapshot();
    assert_eq!(snapshot.total_assets, 1_180);
    assert_eq!(snapshot.accrued_performance_fees, 20);
}

#[test]
fn test_preview_deposit_invalid() {
    let (_env, _admin, _share_token, client) = setup();
    let result = client.try_preview_deposit(&0);
    assert_eq!(result, Err(Ok(VaultError::InvalidAmount)));
}

#[test]
fn test_preview_withdraw_invalid() {
    let (_env, _admin, _share_token, client) = setup();
    let result = client.try_preview_withdraw(&0);
    assert_eq!(result, Err(Ok(VaultError::InvalidAmount)));
}

#[test]
fn test_invalid_config_fees() {
    let (_env, admin, _share_token, client) = setup();
    let bad_config = VaultConfig {
        performance_fee_bps: 5_000,
        ..client.get_config()
    };
    let result = client.try_set_config(&admin, &bad_config);
    assert_eq!(result, Err(Ok(VaultError::PerformanceFeeExceedsMax)));
}

#[test]
fn test_no_double_transfer_and_harvest_drain_prevention() {
    let (env, contract_id, _share_token, underlying, client) = setup_with_real_tokens();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let underlying_client = token::StellarAssetClient::new(&env, &underlying);

    underlying_client.mint(&alice, &100);
    underlying_client.mint(&bob, &100);

    // Alice deposits 100
    let alice_shares = client.deposit(&alice, &100, &100);
    assert_eq!(alice_shares, 100);
    assert_eq!(token::Client::new(&env, &underlying).balance(&alice), 0);
    assert_eq!(token::Client::new(&env, &underlying).balance(&contract_id), 100);

    // Bob deposits 100
    let bob_shares = client.deposit(&bob, &100, &100);
    assert_eq!(bob_shares, 100);
    assert_eq!(token::Client::new(&env, &underlying).balance(&bob), 0);
    assert_eq!(token::Client::new(&env, &underlying).balance(&contract_id), 200);

    // Total assets must equal 200, matching real vault balance
    let snapshot = client.get_vault_snapshot();
    assert_eq!(snapshot.total_assets, 200);
    assert_eq!(snapshot.total_shares, 200);

    // Harvest should report NoRewardsToHarvest because vault balance matches recognized assets
    let caller = Address::generate(&env);
    let harvest_res = client.try_harvest(&caller, &0);
    assert_eq!(harvest_res, Err(Ok(VaultError::NoRewardsToHarvest)));

    // Alice withdraws all 100 shares
    let alice_assets = client.withdraw(&alice, &100, &100);
    assert_eq!(alice_assets, 100);
    assert_eq!(token::Client::new(&env, &underlying).balance(&alice), 100);
    // Vault balance must still retain Bob's 100 tokens
    assert_eq!(token::Client::new(&env, &underlying).balance(&contract_id), 100);

    // Bob can withdraw his remaining 100 tokens
    let bob_assets = client.withdraw(&bob, &100, &100);
    assert_eq!(bob_assets, 100);
    assert_eq!(token::Client::new(&env, &underlying).balance(&bob), 100);
    assert_eq!(token::Client::new(&env, &underlying).balance(&contract_id), 0);
}

