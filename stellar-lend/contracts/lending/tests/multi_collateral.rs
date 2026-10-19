//! Multi-collateral borrow positions and their liquidation (issue #1195).
//!
//! A position can hold several collateral assets; its value and health factor
//! sum every asset at its own oracle price, and a liquidation can seize any
//! one of them.

mod common;

use common::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Address;
use stellarlend_lending::{BorrowError, PauseType, MAX_COLLATERAL_ASSETS};

/// Prices are cached per ledger, so a test moves to a new ledger after
/// changing one.
fn set_price(f: &Fixture, asset: &Address, price: i128) {
    f.oracle_client().set_price(asset, &price);
    f.env.ledger().with_mut(|l| l.timestamp += 1);
}

/// A borrower with 1,000 debt backed by 1,500 of asset A (the borrow
/// collateral) and `extra` of asset B. Returns (borrower, asset B).
fn two_asset_position(f: &Fixture, extra: i128) -> (Address, Address) {
    let borrower = Address::generate(&f.env);
    let asset_b = Address::generate(&f.env);
    f.client.borrow(
        &borrower,
        &f.debt_asset,
        &1_000,
        &f.collateral_asset,
        &1_500,
    );
    f.client.deposit_collateral(&borrower, &asset_b, &extra);
    (borrower, asset_b)
}

#[test]
fn position_holds_several_collateral_assets() {
    let f = setup();
    let (borrower, asset_b) = two_asset_position(&f, 500);

    let balances = f.client.get_user_collaterals(&borrower);
    assert_eq!(balances.len(), 2);
    assert_eq!(balances.get(f.collateral_asset.clone()), Some(1_500));
    assert_eq!(balances.get(asset_b.clone()), Some(500));

    // Adding more of an existing asset tops it up rather than adding an entry.
    f.client.deposit_collateral(&borrower, &asset_b, &250);
    let balances = f.client.get_user_collaterals(&borrower);
    assert_eq!(balances.len(), 2);
    assert_eq!(balances.get(asset_b), Some(750));

    // The legacy single-asset view reports the largest holding.
    let primary = f.client.get_user_collateral(&borrower);
    assert_eq!(primary.asset, f.collateral_asset);
    assert_eq!(primary.amount, 1_500);
}

#[test]
fn collateral_value_and_health_factor_sum_every_asset() {
    let f = setup();
    let (borrower, asset_b) = two_asset_position(&f, 500);
    set_price(&f, &asset_b, 2 * PRICE_SCALE);

    // 1,500 * 1.0 + 500 * 2.0
    assert_eq!(f.client.get_collateral_value(&borrower), 2_500);
    // 2,500 * 80% / 1,000 debt = 2.0
    assert_eq!(f.client.get_health_factor(&borrower), 20_000);

    let summary = f.client.get_user_position(&borrower);
    assert_eq!(summary.collateral_value, 2_500);
    assert_eq!(summary.health_factor, 20_000);
}

#[test]
fn a_second_asset_keeps_a_position_healthy_when_the_first_falls() {
    let f = setup();
    let single = Address::generate(&f.env);
    f.client
        .borrow(&single, &f.debt_asset, &1_000, &f.collateral_asset, &1_500);
    let (diversified, _) = two_asset_position(&f, 500);

    set_price(&f, &f.collateral_asset, PRICE_SCALE / 2);

    // 750 * 80% / 1,000 = 0.6 with one asset; (750 + 500) * 80% / 1,000 = 1.0 with two.
    assert_eq!(f.client.get_health_factor(&single), 6_000);
    assert_eq!(f.client.get_health_factor(&diversified), 10_000);
    assert!(f
        .client
        .try_liquidate(
            &Address::generate(&f.env),
            &diversified,
            &f.debt_asset,
            &f.collateral_asset,
            &100,
        )
        .is_err());
}

#[test]
fn number_of_collateral_assets_is_capped() {
    let f = setup();
    let user = Address::generate(&f.env);
    for _ in 0..MAX_COLLATERAL_ASSETS {
        f.client
            .deposit_collateral(&user, &Address::generate(&f.env), &1_000);
    }
    assert_eq!(
        f.client
            .try_deposit_collateral(&user, &Address::generate(&f.env), &1_000),
        Err(Ok(BorrowError::TooManyCollateralAssets))
    );

    // Topping up an asset already held is still allowed at the cap.
    let held = f.client.get_user_collaterals(&user).keys().get(0).unwrap();
    f.client.deposit_collateral(&user, &held, &1_000);
}

#[test]
fn healthy_position_cannot_be_liquidated() {
    let f = setup();
    let (borrower, asset_b) = two_asset_position(&f, 500);
    assert_eq!(
        f.client.try_liquidate(
            &Address::generate(&f.env),
            &borrower,
            &f.debt_asset,
            &asset_b,
            &100,
        ),
        Err(Ok(BorrowError::PositionHealthy))
    );
}

#[test]
fn liquidation_can_seize_any_collateral_asset() {
    let f = setup();
    let liquidator = Address::generate(&f.env);
    let (borrower, asset_b) = two_asset_position(&f, 500);
    // (1,500 * 0.4 + 500) * 80% / 1,000 = 0.88
    set_price(&f, &f.collateral_asset, PRICE_SCALE * 4 / 10);
    assert_eq!(f.client.get_health_factor(&borrower), 8_800);

    // Repay 400 of debt, seize asset B (priced 1.0) with the default 10% incentive.
    let result = f
        .client
        .liquidate(&liquidator, &borrower, &f.debt_asset, &asset_b, &400);
    assert_eq!(result.debt_repaid, 400);
    assert_eq!(result.collateral_seized, 440);
    assert_eq!(result.incentive_bps, 1_000);

    assert_eq!(f.client.get_debt_balance(&borrower), 600);
    let borrower_collateral = f.client.get_user_collaterals(&borrower);
    assert_eq!(borrower_collateral.get(asset_b.clone()), Some(60));
    assert_eq!(
        borrower_collateral.get(f.collateral_asset.clone()),
        Some(1_500)
    );
    assert_eq!(
        f.client.get_user_collaterals(&liquidator).get(asset_b),
        Some(440)
    );
}

#[test]
fn seized_amount_follows_the_collateral_price() {
    let f = setup();
    let (borrower, _) = two_asset_position(&f, 500);
    set_price(&f, &f.collateral_asset, PRICE_SCALE * 4 / 10);

    // Seizing asset A (priced 0.4): 100 * 1.0 * 1.1 / 0.4 = 275.
    let result = f.client.liquidate(
        &Address::generate(&f.env),
        &borrower,
        &f.debt_asset,
        &f.collateral_asset,
        &100,
    );
    assert_eq!(result.collateral_seized, 275);
    assert_eq!(
        f.client
            .get_user_collaterals(&borrower)
            .get(f.collateral_asset.clone()),
        Some(1_225)
    );
}

#[test]
fn repayment_is_capped_by_the_close_factor() {
    let f = setup();
    let (borrower, _) = two_asset_position(&f, 500);
    set_price(&f, &f.collateral_asset, PRICE_SCALE * 4 / 10);

    // Default close factor 50% of 1,000 debt.
    let result = f.client.liquidate(
        &Address::generate(&f.env),
        &borrower,
        &f.debt_asset,
        &f.collateral_asset,
        &900,
    );
    assert_eq!(result.debt_repaid, 500);
    assert_eq!(f.client.get_debt_balance(&borrower), 500);
}

#[test]
fn repayment_shrinks_when_the_seized_asset_runs_out() {
    let f = setup();
    let (borrower, asset_b) = two_asset_position(&f, 300);
    set_price(&f, &f.collateral_asset, PRICE_SCALE * 4 / 10);

    // Repaying 500 would need 550 of B; only 300 is held. Repay what 300 B
    // covers at a 10% incentive: floor(300 / 1.1) = 272, seizing floor(272 * 1.1) = 299.
    let result = f.client.liquidate(
        &Address::generate(&f.env),
        &borrower,
        &f.debt_asset,
        &asset_b,
        &500,
    );
    assert_eq!(result.debt_repaid, 272);
    assert_eq!(result.collateral_seized, 299);
    assert_eq!(f.client.get_debt_balance(&borrower), 728);
    assert_eq!(
        f.client.get_user_collaterals(&borrower).get(asset_b),
        Some(1)
    );
}

#[test]
fn liquidation_fails_closed_when_any_collateral_price_is_missing() {
    let f = setup();
    let (borrower, asset_b) = two_asset_position(&f, 500);
    set_price(&f, &f.collateral_asset, PRICE_SCALE * 4 / 10);
    // Asset B has no usable price. Counting it as worthless would understate
    // the position, so liquidation refuses instead.
    set_price(&f, &asset_b, 0);

    assert_eq!(
        f.client.try_liquidate(
            &Address::generate(&f.env),
            &borrower,
            &f.debt_asset,
            &f.collateral_asset,
            &100,
        ),
        Err(Ok(BorrowError::PriceUnavailable))
    );
}

#[test]
fn liquidation_rejects_invalid_requests() {
    let f = setup();
    let (borrower, _) = two_asset_position(&f, 500);
    set_price(&f, &f.collateral_asset, PRICE_SCALE * 4 / 10);
    let liquidator = Address::generate(&f.env);

    assert_eq!(
        f.client.try_liquidate(
            &borrower,
            &borrower,
            &f.debt_asset,
            &f.collateral_asset,
            &100
        ),
        Err(Ok(BorrowError::SelfLiquidation))
    );
    assert_eq!(
        f.client.try_liquidate(
            &liquidator,
            &borrower,
            &f.debt_asset,
            &f.collateral_asset,
            &0
        ),
        Err(Ok(BorrowError::InvalidAmount))
    );
    // Collateral the borrower does not hold.
    assert_eq!(
        f.client.try_liquidate(
            &liquidator,
            &borrower,
            &f.debt_asset,
            &Address::generate(&f.env),
            &100,
        ),
        Err(Ok(BorrowError::InsufficientCollateral))
    );
    // Debt in an asset the borrower did not borrow.
    assert_eq!(
        f.client.try_liquidate(
            &liquidator,
            &borrower,
            &Address::generate(&f.env),
            &f.collateral_asset,
            &100,
        ),
        Err(Ok(BorrowError::AssetNotSupported))
    );

    f.client.set_pause(&f.admin, &PauseType::Liquidation, &true);
    assert_eq!(
        f.client.try_liquidate(
            &liquidator,
            &borrower,
            &f.debt_asset,
            &f.collateral_asset,
            &100
        ),
        Err(Ok(BorrowError::ProtocolPaused))
    );
}
