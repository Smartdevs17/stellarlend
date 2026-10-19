#![cfg(test)]
use crate::{RouterError, YieldRouter, YieldRouterClient};
use pool_interfaces::{RiskProfile, RouterConfig};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, Address, YieldRouterClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(YieldRouter, ());
    let client = YieldRouterClient::new(&env, &contract_id);

    let config = RouterConfig {
        min_apy_differential_bps: 100,
        max_pools: 5,
        rebalance_cooldown_secs: 3600,
        slippage_tolerance_bps: 100,
        default_risk_profile: RiskProfile::Moderate,
    };

    client.initialize(&admin, &config);
    (env, admin, client)
}

#[test]
fn test_initialize() {
    let (_env, admin, client) = setup();
    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, Some(admin));
}

#[test]
fn test_register_pool() {
    let (_env, admin, client) = setup();
    let pool = Address::generate(&_env);
    let asset = Address::generate(&_env);
    client.register_pool(&admin, &pool, &asset);
    assert!(client.is_pool_registered(&pool));
}

#[test]
fn test_register_multiple_pools() {
    let (_env, admin, client) = setup();
    let asset = Address::generate(&_env);

    for _ in 0..5 {
        let pool = Address::generate(&_env);
        client.register_pool(&admin, &pool, &asset);
    }

    let pools = client.get_registered_pools();
    assert_eq!(pools.len(), 5);
}

#[test]
fn test_register_pool_max_limit() {
    let (_env, admin, client) = setup();
    let asset = Address::generate(&_env);

    for _ in 0..5 {
        let pool = Address::generate(&_env);
        client.register_pool(&admin, &pool, &asset);
    }

    let extra_pool = Address::generate(&_env);
    let result = client.try_register_pool(&admin, &extra_pool, &asset);
    assert_eq!(result, Err(Ok(RouterError::MaxPoolsExceeded)));
}

#[test]
fn test_deposit_no_pools() {
    let (_env, _admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);
    let result = client.try_deposit(&user, &asset, &1000, &RiskProfile::Conservative, &0);
    assert_eq!(result, Err(Ok(RouterError::NoPoolsConfigured)));
}

#[test]
fn test_deposit_with_pools() {
    let (_env, admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);
    let pool = Address::generate(&_env);

    client.register_pool(&admin, &pool, &asset);
    let result = client.deposit(&user, &asset, &1000, &RiskProfile::Moderate, &0);
    assert!(result > 0);
}

#[test]
fn test_withdraw_no_position() {
    let (_env, _admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);
    let result = client.try_withdraw(&user, &asset, &100, &0);
    assert_eq!(result, Err(Ok(RouterError::UserPositionNotFound)));
}

#[test]
fn test_get_user_position_no_position() {
    let (_env, _admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);
    let position = client.get_user_position(&user, &asset);
    assert!(position.is_none());
}

#[test]
fn test_rebalance_no_position() {
    let (_env, _admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);
    let result = client.try_rebalance(&user, &asset, &Some(RiskProfile::Conservative), &0);
    assert_eq!(result, Err(Ok(RouterError::UserPositionNotFound)));
}

#[test]
fn test_set_paused() {
    let (_env, admin, client) = setup();
    client.set_paused(&admin, &true);
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);

    let result = client.try_deposit(&user, &asset, &1000, &RiskProfile::Conservative, &0);
    assert_eq!(result, Err(Ok(RouterError::DepositPaused)));
}

#[test]
fn test_unauthorized_set_config() {
    let (env, _admin, client) = setup();
    let bad_admin = Address::generate(&env);
    let config = client.get_config();
    let result = client.try_set_config(&bad_admin, &config);
    assert_eq!(result, Err(Ok(RouterError::Unauthorized)));
}

#[test]
fn test_get_allocations_empty() {
    let (_env, _admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);
    let allocs = client.get_allocations(&user, &asset);
    assert_eq!(allocs.len(), 0);
}

#[test]
fn test_config_update() {
    let (_env, admin, client) = setup();
    let new_config = RouterConfig {
        min_apy_differential_bps: 200,
        max_pools: 3,
        rebalance_cooldown_secs: 7200,
        slippage_tolerance_bps: 50,
        default_risk_profile: RiskProfile::Aggressive,
    };
    client.set_config(&admin, &new_config);
    let stored = client.get_config();
    assert_eq!(stored.max_pools, 3);
    assert_eq!(stored.min_apy_differential_bps, 200);
}

#[test]
fn test_subsequent_deposits_accumulate_instead_of_overwriting() {
    let (_env, admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);
    let pool = Address::generate(&_env);

    client.register_pool(&admin, &pool, &asset);

    // First deposit: 1000
    let dep1 = client.deposit(&user, &asset, &1000, &RiskProfile::Moderate, &0);
    assert_eq!(dep1, 1000);
    let pos1 = client.get_user_position(&user, &asset).unwrap();
    assert_eq!(pos1.total_deposited, 1000);

    // Second deposit: 500
    let dep2 = client.deposit(&user, &asset, &500, &RiskProfile::Moderate, &0);
    assert_eq!(dep2, 500);
    let pos2 = client.get_user_position(&user, &asset).unwrap();
    assert_eq!(pos2.total_deposited, 1500);

    // Withdrawal of 1200 now succeeds (previously would fail with InsufficientBalance)
    let withdrawn = client.withdraw(&user, &asset, &1200, &0);
    assert_eq!(withdrawn, 1200);

    let pos3 = client.get_user_position(&user, &asset).unwrap();
    assert_eq!(pos3.total_deposited, 300);
}

#[test]
fn test_cross_asset_isolation_in_routing() {
    let (_env, admin, client) = setup();
    let user = Address::generate(&_env);
    let usdc = Address::generate(&_env);
    let xlm = Address::generate(&_env);

    let usdc_pool = Address::generate(&_env);
    let xlm_pool = Address::generate(&_env);

    client.register_pool(&admin, &usdc_pool, &usdc);
    client.register_pool(&admin, &xlm_pool, &xlm);

    // Deposit USDC: must only route to USDC pool
    client.deposit(&user, &usdc, &1000, &RiskProfile::Moderate, &0);
    let usdc_allocs = client.get_allocations(&user, &usdc);
    assert_eq!(usdc_allocs.len(), 1);
    assert_eq!(usdc_allocs.get(0).unwrap().pool, usdc_pool);
    assert_eq!(usdc_allocs.get(0).unwrap().asset, usdc);

    // Deposit XLM: must only route to XLM pool
    client.deposit(&user, &xlm, &2000, &RiskProfile::Moderate, &0);
    let xlm_allocs = client.get_allocations(&user, &xlm);
    assert_eq!(xlm_allocs.len(), 1);
    assert_eq!(xlm_allocs.get(0).unwrap().pool, xlm_pool);
    assert_eq!(xlm_allocs.get(0).unwrap().asset, xlm);
}

#[test]
fn test_risk_profile_concentration_caps_enforced() {
    let (_env, admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);

    let pool_a = Address::generate(&_env);
    let pool_b = Address::generate(&_env);

    client.register_pool(&admin, &pool_a, &asset);
    client.register_pool(&admin, &pool_b, &asset);

    // Conservative requires <= 30% per pool. With only 2 pools, 2 * 30% = 60% < 100%,
    // so concentration caps cannot be satisfied without exceeding 30%. Must return RiskProfileMismatch.
    let err = client.try_deposit(&user, &asset, &1000, &RiskProfile::Conservative, &0);
    assert_eq!(err, Err(Ok(RouterError::RiskProfileMismatch)));

    // Moderate requires <= 50% per pool. 2 pools can satisfy 50% each.
    let ok = client.deposit(&user, &asset, &1000, &RiskProfile::Moderate, &0);
    assert_eq!(ok, 1000);

    let allocs = client.get_allocations(&user, &asset);
    assert_eq!(allocs.len(), 2);
    assert_eq!(allocs.get(0).unwrap().weight_bps, 5000);
    assert_eq!(allocs.get(1).unwrap().weight_bps, 5000);
}

#[test]
fn test_full_withdrawal_cleans_up_position() {
    let (_env, admin, client) = setup();
    let user = Address::generate(&_env);
    let asset = Address::generate(&_env);
    let pool = Address::generate(&_env);

    client.register_pool(&admin, &pool, &asset);
    client.deposit(&user, &asset, &1000, &RiskProfile::Moderate, &0);

    let withdrawn = client.withdraw(&user, &asset, &1000, &0);
    assert_eq!(withdrawn, 1000);

    let pos = client.get_user_position(&user, &asset);
    assert!(pos.is_none());

    let allocs = client.get_allocations(&user, &asset);
    assert_eq!(allocs.len(), 0);
}
