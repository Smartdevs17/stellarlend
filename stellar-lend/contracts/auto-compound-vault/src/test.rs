#![cfg(test)]
use crate::{AutoCompoundVault, AutoCompoundVaultClient, VaultConfig, VaultError};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn setup() -> (Env, Address, Address, AutoCompoundVaultClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(AutoCompoundVault, ());
    let share_token = env
        .register_stellar_asset_contract_v2(contract_id.clone())
        .address();
    let underlying = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let reward = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
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

    client.initialize(&admin, &share_token, &underlying, &reward, &config);
    (env, admin, share_token, client)
}

fn setup_with_assets() -> (
    Env,
    Address,
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
    let underlying = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let reward = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
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
    client.initialize(&admin, &share_token, &underlying, &reward, &config);
    (env, admin, contract_id, share_token, underlying, client)
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
    let (_env, _admin, _share_token, client) = setup();
    let caller = Address::generate(&_env);

    let result = client.try_harvest(&caller, &0);
    assert!(result.is_ok());
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
fn test_deposit_and_withdraw_move_underlying_assets() {
    let (env, _admin, contract_id, share_token, underlying, client) = setup_with_assets();
    let user = Address::generate(&env);
    let amount = 1_000i128;
    let underlying_asset = token::StellarAssetClient::new(&env, &underlying);
    let underlying_client = token::Client::new(&env, &underlying);
    let share_client = token::Client::new(&env, &share_token);

    underlying_asset.mint(&user, &amount);
    assert_eq!(underlying_client.balance(&user), amount);

    let shares = client.deposit(&user, &amount, &amount);
    assert_eq!(shares, amount);
    assert_eq!(underlying_client.balance(&user), 0);
    assert_eq!(underlying_client.balance(&contract_id), amount);
    assert_eq!(share_client.balance(&user), shares);

    let assets = client.withdraw(&user, &shares, &amount);
    assert_eq!(assets, amount);
    assert_eq!(underlying_client.balance(&contract_id), 0);
    assert_eq!(underlying_client.balance(&user), amount);
    assert_eq!(share_client.balance(&user), 0);
}

#[test]
fn test_deposit_requires_underlying_balance() {
    let (env, _admin, _contract_id, share_token, _underlying, client) = setup_with_assets();
    let user = Address::generate(&env);
    let share_client = token::Client::new(&env, &share_token);

    assert!(client.try_deposit(&user, &1_000, &0).is_err());
    assert_eq!(client.get_vault_snapshot().total_assets, 0);
    assert_eq!(share_client.balance(&user), 0);
}

#[test]
fn test_harvest_only_accounts_real_underlying() {
    let (env, admin, contract_id, _share_token, underlying, client) = setup_with_assets();
    let user = Address::generate(&env);
    let deposit_amount = 1_000i128;
    let reward_amount = 100i128;
    let underlying_asset = token::StellarAssetClient::new(&env, &underlying);
    let underlying_client = token::Client::new(&env, &underlying);

    underlying_asset.mint(&user, &deposit_amount);
    client.deposit(&user, &deposit_amount, &deposit_amount);

    env.ledger().set_timestamp(1);
    assert_eq!(client.harvest(&admin, &0), 0);
    assert_eq!(client.get_vault_snapshot().total_assets, deposit_amount);

    underlying_asset.mint(&admin, &reward_amount);
    underlying_client.transfer(&admin, &contract_id, &reward_amount);
    env.ledger().set_timestamp(3_601);

    let reinvested = client.harvest(&admin, &reward_amount);
    assert_eq!(reinvested, 90);
    let snapshot = client.get_vault_snapshot();
    assert_eq!(snapshot.total_assets, deposit_amount + reinvested);
    assert_eq!(snapshot.accrued_performance_fees, 10);
    assert_eq!(
        underlying_client.balance(&contract_id),
        deposit_amount + reward_amount
    );
}
