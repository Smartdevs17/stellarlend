mod common;

use common::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    vec, Address,
};
use stellarlend_lending::{DepositError, DepositRequest};
use test_utils::snapshotless_env;

#[test]
fn default_asset_supply_is_uncapped_and_tracks_total_supply() {
    let f = setup();
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    let asset = &f.collateral_asset;

    assert_eq!(f.client.get_asset_supply_cap(asset), None);
    assert_eq!(f.client.get_asset_total_supply(asset), 0);

    f.client.deposit(&alice, asset, &12_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 12_000);

    f.client.deposit(&bob, asset, &8_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 20_000);
}

#[test]
fn admin_can_configure_update_and_clear_per_asset_supply_cap() {
    let f = setup();
    let user = Address::generate(&f.env);
    let asset = &f.collateral_asset;

    f.client
        .set_asset_supply_cap(&f.admin, asset, &Some(15_000));
    assert!(!f.env.events().all().events().is_empty());
    assert_eq!(f.client.get_asset_supply_cap(asset), Some(15_000));

    // Deposit above 15_000 is rejected while capped.
    assert_eq!(
        f.client.try_deposit(&user, asset, &16_000),
        Err(Ok(DepositError::ExceedsDepositCap))
    );

    // Raising the cap allows the deposit.
    f.client
        .set_asset_supply_cap(&f.admin, asset, &Some(20_000));
    assert_eq!(f.client.get_asset_supply_cap(asset), Some(20_000));
    assert_eq!(f.client.deposit(&user, asset, &16_000), 16_000);

    // Clearing the cap removes the per-asset limit.
    f.client.set_asset_supply_cap(&f.admin, asset, &None);
    assert_eq!(f.client.get_asset_supply_cap(asset), None);
    assert_eq!(f.client.deposit(&user, asset, &10_000), 26_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 26_000);
}

#[test]
fn set_asset_supply_cap_rejects_unauthorized_and_non_positive_values() {
    let f = setup();
    let attacker = Address::generate(&f.env);
    let asset = &f.collateral_asset;

    assert_eq!(
        f.client
            .try_set_asset_supply_cap(&attacker, asset, &Some(10_000)),
        Err(Ok(DepositError::Unauthorized))
    );
    assert_eq!(
        f.client.try_set_asset_supply_cap(&f.admin, asset, &Some(0)),
        Err(Ok(DepositError::InvalidAmount))
    );
    assert_eq!(
        f.client
            .try_set_asset_supply_cap(&f.admin, asset, &Some(-500)),
        Err(Ok(DepositError::InvalidAmount))
    );
    assert_eq!(f.client.get_asset_supply_cap(asset), None);

    // Also verify real signature check without mocked auths.
    let env = snapshotless_env();
    env.mock_all_auths();
    let (client, admin) = deploy(&env);
    env.set_auths(&[]);
    let unauthed_asset = Address::generate(&env);
    assert!(client
        .try_set_asset_supply_cap(&admin, &unauthed_asset, &Some(10_000))
        .is_err());
}

#[test]
fn single_and_multi_user_deposits_enforce_per_asset_supply_cap() {
    let f = setup();
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    let asset = &f.collateral_asset;

    f.client
        .set_asset_supply_cap(&f.admin, asset, &Some(25_000));

    assert_eq!(f.client.deposit(&alice, asset, &15_000), 15_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 15_000);

    // Exact cap boundary succeeds across users.
    assert_eq!(f.client.deposit(&bob, asset, &10_000), 10_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 25_000);

    // Any additional deposit for this asset is rejected and state is preserved.
    assert_eq!(
        f.client.try_deposit(&bob, asset, &100),
        Err(Ok(DepositError::ExceedsDepositCap))
    );
    assert_eq!(f.client.get_asset_total_supply(asset), 25_000);
    assert_eq!(
        f.client.get_user_collateral_deposit(&bob, asset).amount,
        10_000
    );
}

#[test]
fn supply_caps_are_isolated_per_asset() {
    let f = setup();
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    let asset_a = Address::generate(&f.env);
    let asset_b = Address::generate(&f.env);
    let asset_c = Address::generate(&f.env);

    f.client
        .set_asset_supply_cap(&f.admin, &asset_a, &Some(10_000));
    f.client
        .set_asset_supply_cap(&f.admin, &asset_b, &Some(30_000));

    assert_eq!(f.client.deposit(&alice, &asset_a, &10_000), 10_000);
    assert_eq!(
        f.client.try_deposit(&bob, &asset_a, &100),
        Err(Ok(DepositError::ExceedsDepositCap))
    );

    // Asset B and uncapped Asset C remain unaffected by Asset A reaching its cap.
    assert_eq!(f.client.deposit(&bob, &asset_b, &30_000), 30_000);
    assert_eq!(
        f.client.try_deposit(&bob, &asset_b, &100),
        Err(Ok(DepositError::ExceedsDepositCap))
    );

    assert_eq!(f.client.deposit(&alice, &asset_c, &50_000), 60_000);
    assert_eq!(f.client.get_asset_total_supply(&asset_a), 10_000);
    assert_eq!(f.client.get_asset_total_supply(&asset_b), 30_000);
    assert_eq!(f.client.get_asset_total_supply(&asset_c), 50_000);
}

#[test]
fn withdrawals_and_emergency_withdrawals_free_per_asset_supply_capacity() {
    let f = setup();
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    let asset = &f.collateral_asset;

    f.client
        .set_asset_supply_cap(&f.admin, asset, &Some(20_000));

    f.client.deposit(&alice, asset, &20_000);
    assert_eq!(
        f.client.try_deposit(&bob, asset, &1_000),
        Err(Ok(DepositError::ExceedsDepositCap))
    );

    // Standard withdrawal frees capacity under the asset cap.
    f.client.withdraw(&alice, asset, &8_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 12_000);

    assert_eq!(f.client.deposit(&bob, asset, &8_000), 8_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 20_000);

    // Emergency withdrawal also decrements per-asset total supply.
    f.client.emergency_withdraw(&alice, asset, &5_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 15_000);

    assert_eq!(f.client.deposit(&bob, asset, &5_000), 13_000);
    assert_eq!(f.client.get_asset_total_supply(asset), 20_000);
}

#[test]
fn deposit_batch_enforces_per_asset_supply_cap_atomically() {
    let f = setup();
    let user = Address::generate(&f.env);
    let asset_a = Address::generate(&f.env);
    let asset_b = Address::generate(&f.env);

    f.client
        .set_asset_supply_cap(&f.admin, &asset_a, &Some(15_000));
    f.client
        .set_asset_supply_cap(&f.admin, &asset_b, &Some(25_000));

    // Cumulative entries for asset_a (9_000 + 7_000 = 16_000 > 15_000) must abort the entire batch.
    let over_cap_batch = vec![
        &f.env,
        DepositRequest {
            asset: asset_b.clone(),
            amount: 10_000,
        },
        DepositRequest {
            asset: asset_a.clone(),
            amount: 9_000,
        },
        DepositRequest {
            asset: asset_a.clone(),
            amount: 7_000,
        },
    ];
    assert_eq!(
        f.client.try_deposit_batch(&user, &over_cap_batch),
        Err(Ok(DepositError::ExceedsDepositCap))
    );
    assert_eq!(f.client.get_asset_total_supply(&asset_a), 0);
    assert_eq!(f.client.get_asset_total_supply(&asset_b), 0);
    assert_eq!(
        f.client.get_user_collateral_deposit(&user, &asset_a).amount,
        0
    );

    // Valid batch within both per-asset caps succeeds and updates per-asset totals.
    let valid_batch = vec![
        &f.env,
        DepositRequest {
            asset: asset_b.clone(),
            amount: 10_000,
        },
        DepositRequest {
            asset: asset_a.clone(),
            amount: 9_000,
        },
        DepositRequest {
            asset: asset_a.clone(),
            amount: 6_000,
        },
    ];
    let res = f.client.deposit_batch(&user, &valid_batch);
    assert_eq!(res.count, 3);
    assert_eq!(res.total_amount, 25_000);
    assert_eq!(f.client.get_asset_total_supply(&asset_a), 15_000);
    assert_eq!(f.client.get_asset_total_supply(&asset_b), 10_000);
}
