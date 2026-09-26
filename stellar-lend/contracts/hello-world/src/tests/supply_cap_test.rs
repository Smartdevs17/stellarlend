#![cfg(test)]

use crate::cross_asset::AssetConfig;
use crate::{HelloContract, HelloContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn create_test_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_config(env: &Env, asset: Option<Address>, max_supply: i128) -> AssetConfig {
    AssetConfig {
        asset: asset.clone(),
        collateral_factor: 8000,
        liquidation_threshold: 8500,
        reserve_factor: 500,
        max_supply,
        max_borrow: 0,
        can_collateralize: true,
        can_borrow: false,
        price: 1_0000000,
        price_updated_at: env.ledger().timestamp(),
        is_isolated: false,
        is_frozen: false,
    }
}

fn setup_protocol<'a>(
    env: &'a Env,
    admin: &'a Address,
    asset: Option<Address>,
    max_supply: i128,
) -> HelloContractClient<'a> {
    let contract_id = env.register(HelloContract, ());
    let client = HelloContractClient::new(env, &contract_id);
    client.initialize(admin);
    client.initialize_ca(admin);
    client.initialize_asset(&asset, &make_config(env, asset.clone(), max_supply));
    client
}

#[test]
fn test_supply_cap_blocks_deposit_over_limit() {
    let env = create_test_env();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let dai = Address::generate(&env);

    let client = setup_protocol(&env, &admin, Some(dai.clone()), 1000);

    // Deposit 700 succeeds (under cap)
    client.cross_asset_deposit(&user1, &Some(dai.clone()), &700);

    // Deposit 400 would push total to 1100 > 1000 — must fail
    let result = client.try_cross_asset_deposit(&user2, &Some(dai.clone()), &400);
    assert!(result.is_err(), "deposit exceeding supply cap should fail");
}

#[test]
fn test_supply_cap_at_exact_boundary() {
    let env = create_test_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let dai = Address::generate(&env);

    let client = setup_protocol(&env, &admin, Some(dai.clone()), 500);

    // Deposit exactly at cap must succeed
    let result = client.try_cross_asset_deposit(&user, &Some(dai.clone()), &500);
    assert!(result.is_ok(), "deposit at cap should succeed");
}

#[test]
fn test_supply_cap_update_allows_more_deposits() {
    let env = create_test_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let dai = Address::generate(&env);

    let client = setup_protocol(&env, &admin, Some(dai.clone()), 300);

    // 400 fails (over cap)
    assert!(
        client
            .try_cross_asset_deposit(&user, &Some(dai.clone()), &400)
            .is_err(),
        "deposit over cap should fail"
    );

    // Admin raises cap to 1000
    client.update_ca_config(
        &Some(dai.clone()),
        &None,
        &None,
        &Some(1000), // new max_supply
        &None,
        &None,
        &None,
    );

    // Now 400 succeeds
    assert!(
        client
            .try_cross_asset_deposit(&user, &Some(dai.clone()), &400)
            .is_ok(),
        "deposit within raised cap should succeed"
    );
}

#[test]
fn test_supply_headroom_analytics() {
    let env = create_test_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let dai = Address::generate(&env);

    let client = setup_protocol(&env, &admin, Some(dai.clone()), 1000);

    // Before any deposit: headroom = full cap
    let (avail, cap, current) = client.get_supply_headroom(&Some(dai.clone()));
    assert_eq!(cap, 1000);
    assert_eq!(current, 0);
    assert_eq!(avail, 1000);

    // Deposit 300
    client.cross_asset_deposit(&user, &Some(dai.clone()), &300);

    let (avail2, cap2, current2) = client.get_supply_headroom(&Some(dai.clone()));
    assert_eq!(cap2, 1000);
    assert_eq!(current2, 300);
    assert_eq!(avail2, 700);
}

#[test]
fn test_cross_asset_deposit_graceful_partial_fill_and_status() {
    let env = create_test_env();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let dai = Address::generate(&env);

    let _client = setup_protocol(&env, &admin, Some(dai.clone()), 1000);

    // Initial status: Normal
    let (status, current, cap, headroom) =
        crate::cross_asset::get_supply_cap_status(&env, Some(dai.clone())).unwrap();
    assert_eq!(status, crate::cross_asset::SupplyCapStatus::Normal);
    assert_eq!(current, 0);
    assert_eq!(cap, 1000);
    assert_eq!(headroom, 1000);

    // 1. User1 deposits 850 under graceful deposit -> Accepted in full (85% = Elevated)
    let (pos1, accepted1, is_degraded1) = crate::cross_asset::cross_asset_deposit_graceful(
        &env,
        user1.clone(),
        Some(dai.clone()),
        850,
    )
    .unwrap();
    assert_eq!(accepted1, 850);
    assert!(!is_degraded1);
    assert_eq!(pos1.collateral, 850);

    let (status_elevated, current_elevated, _, _) =
        crate::cross_asset::get_supply_cap_status(&env, Some(dai.clone())).unwrap();
    assert_eq!(
        status_elevated,
        crate::cross_asset::SupplyCapStatus::Elevated
    );
    assert_eq!(current_elevated, 850);

    // 2. User2 attempts to deposit 500 when only 150 headroom remains
    // Instead of reverting, graceful degradation accepts the remaining 150!
    let (pos2, accepted2, is_degraded2) = crate::cross_asset::cross_asset_deposit_graceful(
        &env,
        user2.clone(),
        Some(dai.clone()),
        500,
    )
    .unwrap();
    assert_eq!(accepted2, 150);
    assert!(is_degraded2);
    assert_eq!(pos2.collateral, 150);

    // 3. Pool is now at 100% cap -> Status is Capped
    let (status_capped, current_capped, _, headroom_capped) =
        crate::cross_asset::get_supply_cap_status(&env, Some(dai.clone())).unwrap();
    assert_eq!(status_capped, crate::cross_asset::SupplyCapStatus::Capped);
    assert_eq!(current_capped, 1000);
    assert_eq!(headroom_capped, 0);

    // 4. Further deposits when fully capped return SupplyCapExceeded
    let over_res = crate::cross_asset::cross_asset_deposit_graceful(
        &env,
        user1.clone(),
        Some(dai.clone()),
        50,
    );
    assert_eq!(
        over_res,
        Err(crate::cross_asset::CrossAssetError::SupplyCapExceeded)
    );
}
