//! Per-asset liquidation incentive (penalty) configuration (issue #1193).
//!
//! Each collateral asset can override the global incentive, both within
//! `MIN_LIQUIDATION_INCENTIVE_BPS..=MAX_LIQUIDATION_INCENTIVE_BPS`, and a
//! liquidation applies the incentive of the asset it seizes.

mod common;

use common::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Address;
use stellarlend_lending::{
    BorrowError, DEFAULT_LIQUIDATION_INCENTIVE_BPS, MAX_LIQUIDATION_INCENTIVE_BPS,
    MIN_LIQUIDATION_INCENTIVE_BPS,
};

#[test]
fn assets_default_to_the_global_incentive() {
    let f = setup();
    assert_eq!(
        f.client.get_liquidation_incentive_bps(),
        DEFAULT_LIQUIDATION_INCENTIVE_BPS
    );
    assert_eq!(
        f.client.get_asset_liq_incentive_bps(&f.collateral_asset),
        DEFAULT_LIQUIDATION_INCENTIVE_BPS
    );

    f.client.set_liquidation_incentive_bps(&f.admin, &700);
    assert_eq!(
        f.client.get_asset_liq_incentive_bps(&f.collateral_asset),
        700
    );
}

#[test]
fn per_asset_override_applies_only_to_that_asset() {
    let f = setup();
    let other = Address::generate(&f.env);

    f.client
        .set_asset_liq_incentive_bps(&f.admin, &f.collateral_asset, &Some(500));
    assert_eq!(
        f.client.get_asset_liq_incentive_bps(&f.collateral_asset),
        500
    );
    assert_eq!(
        f.client.get_asset_liq_incentive_bps(&other),
        DEFAULT_LIQUIDATION_INCENTIVE_BPS
    );

    // A global change leaves the override alone.
    f.client.set_liquidation_incentive_bps(&f.admin, &1_500);
    assert_eq!(
        f.client.get_asset_liq_incentive_bps(&f.collateral_asset),
        500
    );
    assert_eq!(f.client.get_asset_liq_incentive_bps(&other), 1_500);

    // Clearing the override falls back to the global incentive.
    f.client
        .set_asset_liq_incentive_bps(&f.admin, &f.collateral_asset, &None);
    assert_eq!(
        f.client.get_asset_liq_incentive_bps(&f.collateral_asset),
        1_500
    );
}

#[test]
fn bounds_are_enforced_for_global_and_per_asset_incentives() {
    let f = setup();
    let asset = &f.collateral_asset;

    for bps in [
        i128::MIN,
        -1,
        0,
        MIN_LIQUIDATION_INCENTIVE_BPS - 1,
        MAX_LIQUIDATION_INCENTIVE_BPS + 1,
        10_000,
    ] {
        assert_eq!(
            f.client.try_set_liquidation_incentive_bps(&f.admin, &bps),
            Err(Ok(BorrowError::IncentiveOutOfBounds)),
            "global {bps}"
        );
        assert_eq!(
            f.client
                .try_set_asset_liq_incentive_bps(&f.admin, asset, &Some(bps)),
            Err(Ok(BorrowError::IncentiveOutOfBounds)),
            "per-asset {bps}"
        );
    }
    // Rejected values leave the configuration untouched.
    assert_eq!(
        f.client.get_liquidation_incentive_bps(),
        DEFAULT_LIQUIDATION_INCENTIVE_BPS
    );
    assert_eq!(
        f.client.get_asset_liq_incentive_bps(asset),
        DEFAULT_LIQUIDATION_INCENTIVE_BPS
    );

    // Both ends of the range are accepted.
    for bps in [MIN_LIQUIDATION_INCENTIVE_BPS, MAX_LIQUIDATION_INCENTIVE_BPS] {
        f.client.set_liquidation_incentive_bps(&f.admin, &bps);
        assert_eq!(f.client.get_liquidation_incentive_bps(), bps);
        f.client
            .set_asset_liq_incentive_bps(&f.admin, asset, &Some(bps));
        assert_eq!(f.client.get_asset_liq_incentive_bps(asset), bps);
    }
}

#[test]
fn only_the_admin_can_change_incentives() {
    let f = setup();
    let stranger = Address::generate(&f.env);

    assert_eq!(
        f.client.try_set_liquidation_incentive_bps(&stranger, &500),
        Err(Ok(BorrowError::Unauthorized))
    );
    assert_eq!(
        f.client
            .try_set_asset_liq_incentive_bps(&stranger, &f.collateral_asset, &Some(500)),
        Err(Ok(BorrowError::Unauthorized))
    );

    // The admin must also sign.
    f.env.set_auths(&[]);
    assert!(f
        .client
        .try_set_liquidation_incentive_bps(&f.admin, &500)
        .is_err());
    assert!(f
        .client
        .try_set_asset_liq_incentive_bps(&f.admin, &f.collateral_asset, &Some(500))
        .is_err());
}

#[test]
fn liquidation_applies_the_seized_assets_incentive() {
    let f = setup();
    let cheap = Address::generate(&f.env);
    let generous = Address::generate(&f.env);
    f.client
        .set_asset_liq_incentive_bps(&f.admin, &cheap, &Some(500));
    f.client
        .set_asset_liq_incentive_bps(&f.admin, &generous, &Some(2_000));

    let borrower = Address::generate(&f.env);
    f.client.borrow(
        &borrower,
        &f.debt_asset,
        &1_000,
        &f.collateral_asset,
        &1_500,
    );
    f.client.deposit_collateral(&borrower, &cheap, &500);
    f.client.deposit_collateral(&borrower, &generous, &500);
    // (1,500 * 0.1 + 500 + 500) * 80% / 1,000 = 0.92
    f.oracle_client()
        .set_price(&f.collateral_asset, &(PRICE_SCALE / 10));
    f.env.ledger().with_mut(|l| l.timestamp += 1);

    let liquidator = Address::generate(&f.env);
    let a = f
        .client
        .liquidate(&liquidator, &borrower, &f.debt_asset, &cheap, &100);
    let b = f
        .client
        .liquidate(&liquidator, &borrower, &f.debt_asset, &generous, &100);
    let c = f.client.liquidate(
        &liquidator,
        &borrower,
        &f.debt_asset,
        &f.collateral_asset,
        &100,
    );

    assert_eq!((a.incentive_bps, a.collateral_seized), (500, 105));
    assert_eq!((b.incentive_bps, b.collateral_seized), (2_000, 120));
    // No override: global 10%, at a price of 0.1 -> 100 * 1.1 / 0.1.
    assert_eq!((c.incentive_bps, c.collateral_seized), (1_000, 1_100));
}
