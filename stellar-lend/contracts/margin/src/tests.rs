#![cfg(test)]

use crate::account::{CollateralAssetConfig, MarginAccount, MarginCallLevel, MarginMode};
use crate::cross::{
    borrow_cross_margin, calculate_multi_collateral_summary, deposit_cross_collateral,
    liquidate_cross_margin_multi_collateral, withdraw_cross_collateral,
};
use crate::isolated::{
    borrow_isolated_market, deposit_isolated_market, liquidate_isolated_market_position,
    repay_isolated_market, withdraw_isolated_market, IsolatedMarket, IsolatedUserPosition,
};
use soroban_sdk::{testutils::Address as _, Address, Env, Map, Vec};

#[test]
fn test_multi_collateral_cross_margin_summary_and_borrowing() {
    let env = Env::default();
    let user = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    let btc = Address::generate(&env);

    let mut configs = Map::new(&env);
    // USDC: $1.00, 80% LTV, 85% liq threshold, 7 decimals
    configs.set(
        usdc.clone(),
        CollateralAssetConfig {
            asset: usdc.clone(),
            collateral_factor: 8000,
            liquidation_threshold: 8500,
            price: 10_000_000,
            decimals: 7,
        },
    );
    // XLM: $0.50, 60% LTV, 70% liq threshold, 7 decimals
    configs.set(
        xlm.clone(),
        CollateralAssetConfig {
            asset: xlm.clone(),
            collateral_factor: 6000,
            liquidation_threshold: 7000,
            price: 5_000_000,
            decimals: 7,
        },
    );
    // BTC: $50,000, 70% LTV, 75% liq threshold, 7 decimals
    configs.set(
        btc.clone(),
        CollateralAssetConfig {
            asset: btc.clone(),
            collateral_factor: 7000,
            liquidation_threshold: 7500,
            price: 500_000_000_000,
            decimals: 7,
        },
    );

    let mut account = MarginAccount {
        owner: user.clone(),
        mode: MarginMode::Cross,
        positions: Vec::new(&env),
        total_collateral_value: 0,
        total_debt_value: 0,
    };

    // Deposit 1000 USDC ($1,000)
    assert!(deposit_cross_collateral(
        &env,
        &mut account,
        usdc.clone(),
        1000 * 10_000_000,
        &configs
    )
    .is_ok());
    // Deposit 2000 XLM ($1,000)
    assert!(
        deposit_cross_collateral(&env, &mut account, xlm.clone(), 2000 * 10_000_000, &configs)
            .is_ok()
    );

    // Summary calculation
    let summary = calculate_multi_collateral_summary(&env, &account, &configs);
    // Total collateral: $1,000 + $1,000 = $2,000
    assert_eq!(summary.total_collateral_value, 2000 * 10_000_000);
    // Borrow power: ($1000 * 0.8) + ($1000 * 0.6) = $800 + $600 = $1,400
    assert_eq!(summary.weighted_borrow_power, 1400 * 10_000_000);
    // Liquidation collateral: ($1000 * 0.85) + ($1000 * 0.70) = $850 + $700 = $1,550
    assert_eq!(summary.liquidation_collateral_value, 1550 * 10_000_000);
    assert_eq!(summary.margin_call_level, MarginCallLevel::Safe);
    assert!(!summary.is_liquidatable);

    // Borrow 1200 USDC ($1,200 <= $1,400 max borrow power) -> Succeeds
    assert!(borrow_cross_margin(
        &env,
        &mut account,
        usdc.clone(),
        1200 * 10_000_000,
        &configs
    )
    .is_ok());

    // Borrow additional 300 USDC (total debt $1,500 > $1,400 borrow power) -> Rejected
    assert!(
        borrow_cross_margin(&env, &mut account, usdc.clone(), 300 * 10_000_000, &configs).is_err()
    );

    // Withdrawing XLM while debt is outstanding:
    // Withdrawing 1500 XLM would drop collateral to ($1000 USDC + $250 XLM) = $1250 < $1200 debt
    assert!(withdraw_cross_collateral(
        &env,
        &mut account,
        xlm.clone(),
        1500 * 10_000_000,
        &configs
    )
    .is_err());

    // Small withdrawal of 200 XLM ($100) -> Allowed
    assert!(
        withdraw_cross_collateral(&env, &mut account, xlm.clone(), 200 * 10_000_000, &configs)
            .is_ok()
    );
}

#[test]
fn test_multi_collateral_cross_margin_liquidation() {
    let env = Env::default();
    let user = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);

    let mut configs = Map::new(&env);
    configs.set(
        usdc.clone(),
        CollateralAssetConfig {
            asset: usdc.clone(),
            collateral_factor: 8000,
            liquidation_threshold: 8500,
            price: 10_000_000,
            decimals: 7,
        },
    );
    // XLM starts at $1.00
    configs.set(
        xlm.clone(),
        CollateralAssetConfig {
            asset: xlm.clone(),
            collateral_factor: 6000,
            liquidation_threshold: 7000,
            price: 10_000_000,
            decimals: 7,
        },
    );

    let mut account = MarginAccount {
        owner: user.clone(),
        mode: MarginMode::Cross,
        positions: Vec::new(&env),
        total_collateral_value: 0,
        total_debt_value: 0,
    };

    deposit_cross_collateral(&env, &mut account, xlm.clone(), 1000 * 10_000_000, &configs).unwrap();
    borrow_cross_margin(&env, &mut account, usdc.clone(), 500 * 10_000_000, &configs).unwrap();

    // Now XLM price crashes from $1.00 to $0.40
    configs.set(
        xlm.clone(),
        CollateralAssetConfig {
            asset: xlm.clone(),
            collateral_factor: 6000,
            liquidation_threshold: 7000,
            price: 4_000_000,
            decimals: 7,
        },
    );

    // Collateral value: $400, Liq collateral value: $400 * 0.70 = $280. Debt = $500.
    // Health factor = $280 / $500 = 0.56x (< 0.90x -> ForcedClose, is_liquidatable = true)
    let summary = calculate_multi_collateral_summary(&env, &account, &configs);
    assert!(summary.is_liquidatable);
    assert_eq!(summary.margin_call_level, MarginCallLevel::ForcedClose);

    // Liquidator repays 200 USDC debt, seizing XLM collateral with 5% bonus
    let seize_res = liquidate_cross_margin_multi_collateral(
        &env,
        &mut account,
        usdc.clone(),
        200 * 10_000_000,
        xlm.clone(),
        &configs,
    );
    assert!(seize_res.is_ok());
    let seized = seize_res.unwrap();
    // 200 USDC value * 1.05 = $210 value in XLM at $0.40 = 525 XLM
    assert_eq!(seized, 525 * 10_000_000);
}

#[test]
fn test_isolated_market_creation_and_risk_containment() {
    let env = Env::default();
    let market_id = Address::generate(&env);
    let volatile_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let attacker_token = Address::generate(&env);

    // Create isolated market with 50,000 USDC debt ceiling
    let mut market = IsolatedMarket::new(
        market_id.clone(),
        volatile_token.clone(),
        usdc.clone(),
        50_000 * 10_000_000, // debt ceiling
        6000,                // 60% LTV
        7000,                // 70% liq threshold
        500,                 // 5% bonus
    )
    .unwrap();

    let user = Address::generate(&env);
    let mut position = IsolatedUserPosition {
        owner: user.clone(),
        market_id: market_id.clone(),
        collateral_amount: 0,
        debt_amount: 0,
    };

    // 1. Deposit 100,000 volatile tokens ($1.00 each)
    assert!(deposit_isolated_market(
        &mut market,
        &mut position,
        volatile_token.clone(),
        100_000 * 10_000_000
    )
    .is_ok());
    assert_eq!(market.total_collateral, 100_000 * 10_000_000);

    // 2. Reject unapproved collateral asset
    assert!(
        deposit_isolated_market(&mut market, &mut position, attacker_token.clone(), 1000).is_err()
    );

    // 3. Reject borrowing unapproved asset (cannot borrow attacker token)
    assert!(borrow_isolated_market(
        &mut market,
        &mut position,
        attacker_token.clone(),
        1000 * 10_000_000,
        10_000_000,
        10_000_000,
        7
    )
    .is_err());

    // 4. Borrow 30,000 USDC (within 50,000 debt ceiling and 60,000 collateral capacity)
    assert!(borrow_isolated_market(
        &mut market,
        &mut position,
        usdc.clone(),
        30_000 * 10_000_000,
        10_000_000, // $1.00 volatile token
        10_000_000, // $1.00 usdc
        7
    )
    .is_ok());
    assert_eq!(market.current_debt, 30_000 * 10_000_000);

    // 5. Strict Risk Containment: Exceeding market debt ceiling
    // Trying to borrow 25,000 USDC more would make total debt 55,000 > 50,000 ceiling -> Must fail!
    assert!(borrow_isolated_market(
        &mut market,
        &mut position,
        usdc.clone(),
        25_000 * 10_000_000,
        10_000_000,
        10_000_000,
        7
    )
    .is_err());

    // 6. Partial repayment restores debt ceiling headroom
    let repaid = repay_isolated_market(
        &mut market,
        &mut position,
        usdc.clone(),
        10_000 * 10_000_000,
    )
    .unwrap();
    assert_eq!(repaid, 10_000 * 10_000_000);
    assert_eq!(market.current_debt, 20_000 * 10_000_000);
    assert_eq!(position.debt_amount, 20_000 * 10_000_000);

    // 7. Withdrawal constraint: cannot withdraw collateral below required margin
    assert!(withdraw_isolated_market(
        &mut market,
        &mut position,
        volatile_token.clone(),
        80_000 * 10_000_000, // remaining 20,000 collateral only supports 12,000 debt, but debt is 20,000
        10_000_000,
        10_000_000,
        7
    )
    .is_err());
}

#[test]
fn test_isolated_market_bad_debt_containment() {
    let env = Env::default();
    let market_id = Address::generate(&env);
    let volatile_token = Address::generate(&env);
    let usdc = Address::generate(&env);

    let mut market = IsolatedMarket::new(
        market_id.clone(),
        volatile_token.clone(),
        usdc.clone(),
        100_000 * 10_000_000,
        6000, // 60%
        7000, // 70%
        1000, // 10%
    )
    .unwrap();

    let user = Address::generate(&env);
    let mut position = IsolatedUserPosition {
        owner: user.clone(),
        market_id: market_id.clone(),
        collateral_amount: 100 * 10_000_000, // 100 tokens
        debt_amount: 50 * 10_000_000,        // 50 USDC debt
    };
    market.total_collateral = 100 * 10_000_000;
    market.current_debt = 50 * 10_000_000;

    // Volatile token price crashes from $1.00 to $0.10!
    // Collateral value: 100 * $0.10 = $10.00. Liq threshold: $7.00. Debt = $50.00.
    // Liquidator attempts to liquidate position by repaying 50 USDC
    let (repaid, seized) = liquidate_isolated_market_position(
        &mut market,
        &mut position,
        50 * 10_000_000,
        1_000_000,  // $0.10
        10_000_000, // $1.00
        7,
    )
    .unwrap();

    // Liquidator repays covered portion ($9.0909090) to seize all remaining collateral
    assert_eq!(repaid, 90_909_090);
    // All 100 collateral tokens were seized
    assert_eq!(seized, 100 * 10_000_000);
    assert_eq!(position.collateral_amount, 0);

    // CRITICAL RISK CONTAINMENT CHECK:
    // Bad debt ($40.9090910) is contained strictly in market.bad_debt_contained!
    // Remaining debt was wiped from position and isolated in the market pool.
    assert_eq!(market.bad_debt_contained, 409_090_910);
    assert_eq!(position.debt_amount, 0);
}
