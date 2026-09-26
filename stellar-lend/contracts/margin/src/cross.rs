use crate::account::{
    CollateralAssetConfig, CrossMarginSummary, MarginAccount, MarginCallLevel, Position,
};
use soroban_sdk::{Address, Env, Map, Vec};

pub const BPS_SCALE: i128 = 10_000;

/// Base calculation for scaling asset amounts by price and decimals.
pub fn calculate_asset_value(amount: i128, price: i128, decimals: u32) -> i128 {
    if amount <= 0 || price <= 0 {
        return 0;
    }
    let divisor = 10i128.pow(decimals);
    (amount.saturating_mul(price)) / divisor.max(1)
}

/// Calculate cross-margin health and valuation dynamically across multiple collateral types.
pub fn calculate_multi_collateral_summary(
    _env: &Env,
    account: &MarginAccount,
    configs: &Map<Address, CollateralAssetConfig>,
) -> CrossMarginSummary {
    let mut total_collateral_value: i128 = 0;
    let mut weighted_borrow_power: i128 = 0;
    let mut liquidation_collateral_value: i128 = 0;
    let mut total_debt_value: i128 = 0;

    for i in 0..account.positions.len() {
        let pos = account.positions.get(i).unwrap();
        if let Some(config) = configs.get(pos.asset.clone()) {
            if pos.amount > 0 {
                let val = calculate_asset_value(pos.amount, config.price, config.decimals);
                total_collateral_value = total_collateral_value.saturating_add(val);

                let borrow_contrib = (val.saturating_mul(config.collateral_factor)) / BPS_SCALE;
                weighted_borrow_power = weighted_borrow_power.saturating_add(borrow_contrib);

                let liq_contrib = (val.saturating_mul(config.liquidation_threshold)) / BPS_SCALE;
                liquidation_collateral_value =
                    liquidation_collateral_value.saturating_add(liq_contrib);
            }

            if pos.debt > 0 {
                let debt_val = calculate_asset_value(pos.debt, config.price, config.decimals);
                total_debt_value = total_debt_value.saturating_add(debt_val);
            }
        } else {
            // Fallback using entry price and default 7 decimals if unconfigured
            if pos.amount > 0 {
                let val = calculate_asset_value(pos.amount, pos.entry_price, 7);
                total_collateral_value = total_collateral_value.saturating_add(val);
                // Safe default 70% LTV, 75% liquidation threshold
                weighted_borrow_power =
                    weighted_borrow_power.saturating_add((val * 7000) / BPS_SCALE);
                liquidation_collateral_value =
                    liquidation_collateral_value.saturating_add((val * 7500) / BPS_SCALE);
            }
            if pos.debt > 0 {
                let debt_val = calculate_asset_value(pos.debt, pos.entry_price, 7);
                total_debt_value = total_debt_value.saturating_add(debt_val);
            }
        }
    }

    let (health_factor_bps, margin_call_level, is_liquidatable) = if total_debt_value == 0 {
        (i128::MAX, MarginCallLevel::Safe, false)
    } else if liquidation_collateral_value <= 0 {
        (0, MarginCallLevel::ForcedClose, true)
    } else {
        let hf = (liquidation_collateral_value.saturating_mul(BPS_SCALE)) / total_debt_value;
        if hf < 9_000 {
            // Less than 0.90x health factor -> Forced Close
            (hf, MarginCallLevel::ForcedClose, true)
        } else if hf < 10_000 {
            // Between 0.90x and 1.00x health factor -> Liquidatable
            (hf, MarginCallLevel::Liquidation, true)
        } else if hf < 11_500 {
            // Between 1.00x and 1.15x health factor -> Warning
            (hf, MarginCallLevel::Warning, false)
        } else {
            (hf, MarginCallLevel::Safe, false)
        }
    };

    CrossMarginSummary {
        total_collateral_value,
        weighted_borrow_power,
        liquidation_collateral_value,
        total_debt_value,
        health_factor_bps,
        margin_call_level,
        is_liquidatable,
    }
}

/// Traditional check for cross margin health using cached account values.
pub fn check_cross_margin_health(_env: &Env, account: &MarginAccount) -> MarginCallLevel {
    if account.total_debt_value == 0 {
        return MarginCallLevel::Safe;
    }

    let debt_ratio = (account.total_debt_value * 100) / account.total_collateral_value.max(1);

    if debt_ratio >= 90 {
        MarginCallLevel::ForcedClose
    } else if debt_ratio >= 80 {
        MarginCallLevel::Liquidation
    } else if debt_ratio >= 70 {
        MarginCallLevel::Warning
    } else {
        MarginCallLevel::Safe
    }
}

/// Deposit collateral of a specific asset into a cross-margin account.
pub fn deposit_cross_collateral(
    env: &Env,
    account: &mut MarginAccount,
    asset: Address,
    amount: i128,
    configs: &Map<Address, CollateralAssetConfig>,
) -> Result<(), &'static str> {
    if !account.is_cross() {
        return Err("Account is not in cross margin mode");
    }
    if amount <= 0 {
        return Err("Deposit amount must be positive");
    }

    let mut found = false;
    let mut new_positions = Vec::new(env);

    for i in 0..account.positions.len() {
        let mut pos = account.positions.get(i).unwrap();
        if pos.asset == asset {
            pos.amount = pos.amount.saturating_add(amount);
            found = true;
        }
        new_positions.push_back(pos);
    }

    if !found {
        let entry_price = configs
            .get(asset.clone())
            .map(|c| c.price)
            .unwrap_or(10_000_000);
        new_positions.push_back(Position {
            asset,
            amount,
            debt: 0,
            entry_price,
        });
    }

    account.positions = new_positions;
    let summary = calculate_multi_collateral_summary(env, account, configs);
    account.total_collateral_value = summary.total_collateral_value;
    account.total_debt_value = summary.total_debt_value;

    Ok(())
}

/// Withdraw collateral of a specific asset from a cross-margin account,
/// ensuring health factor remains >= 1.0 (Safe or Warning).
pub fn withdraw_cross_collateral(
    env: &Env,
    account: &mut MarginAccount,
    asset: Address,
    amount: i128,
    configs: &Map<Address, CollateralAssetConfig>,
) -> Result<(), &'static str> {
    if !account.is_cross() {
        return Err("Account is not in cross margin mode");
    }
    if amount <= 0 {
        return Err("Withdrawal amount must be positive");
    }

    let mut found = false;
    let mut new_positions = Vec::new(env);

    for i in 0..account.positions.len() {
        let mut pos = account.positions.get(i).unwrap();
        if pos.asset == asset {
            if pos.amount < amount {
                return Err("Insufficient collateral of this asset");
            }
            pos.amount -= amount;
            found = true;
        }
        // Retain position if it still has collateral or debt
        if pos.amount > 0 || pos.debt > 0 {
            new_positions.push_back(pos);
        }
    }

    if !found {
        return Err("Asset position not found");
    }

    // Verify resulting health across all multiple collaterals
    let prev_positions = account.positions.clone();
    account.positions = new_positions;

    let summary = calculate_multi_collateral_summary(env, account, configs);
    if summary.is_liquidatable {
        // Revert positions change if withdrawal causes liquidation risk
        account.positions = prev_positions;
        return Err("Withdrawal would cause account undercollateralization");
    }

    account.total_collateral_value = summary.total_collateral_value;
    account.total_debt_value = summary.total_debt_value;

    Ok(())
}

/// Borrow an asset against cross-margin multiple collateral types.
pub fn borrow_cross_margin(
    env: &Env,
    account: &mut MarginAccount,
    borrow_asset: Address,
    amount: i128,
    configs: &Map<Address, CollateralAssetConfig>,
) -> Result<(), &'static str> {
    if !account.is_cross() {
        return Err("Account is not in cross margin mode");
    }
    if amount <= 0 {
        return Err("Borrow amount must be positive");
    }

    let config = configs
        .get(borrow_asset.clone())
        .ok_or("Borrow asset configuration not found")?;
    let new_debt_val = calculate_asset_value(amount, config.price, config.decimals);

    let summary = calculate_multi_collateral_summary(env, account, configs);
    let projected_debt = summary.total_debt_value.saturating_add(new_debt_val);

    if projected_debt > summary.weighted_borrow_power {
        return Err("Borrow exceeds multi-collateral borrowing capacity");
    }

    let mut found = false;
    let mut new_positions = Vec::new(env);

    for i in 0..account.positions.len() {
        let mut pos = account.positions.get(i).unwrap();
        if pos.asset == borrow_asset {
            pos.debt = pos.debt.saturating_add(amount);
            found = true;
        }
        new_positions.push_back(pos);
    }

    if !found {
        new_positions.push_back(Position {
            asset: borrow_asset,
            amount: 0,
            debt: amount,
            entry_price: config.price,
        });
    }

    account.positions = new_positions;
    let new_summary = calculate_multi_collateral_summary(env, account, configs);
    account.total_collateral_value = new_summary.total_collateral_value;
    account.total_debt_value = new_summary.total_debt_value;

    Ok(())
}

/// Liquidate a distressed cross-margin account across its multiple collateral assets.
pub fn liquidate_cross_margin_account(
    env: &Env,
    account: &mut MarginAccount,
) -> Result<(), &'static str> {
    if !account.is_cross() {
        return Err("Account is not in cross margin mode");
    }

    let health = check_cross_margin_health(env, account);
    if health == MarginCallLevel::Safe || health == MarginCallLevel::Warning {
        return Err("Account is healthy, cannot liquidate");
    }

    account.positions = Vec::new(env);
    account.total_collateral_value = 0;
    account.total_debt_value = 0;

    Ok(())
}

/// Liquidate multi-collateral cross-margin account, seizing collateral proportionally
/// or wiping bad debt while preserving excess collateral.
pub fn liquidate_cross_margin_multi_collateral(
    env: &Env,
    account: &mut MarginAccount,
    repay_asset: Address,
    repay_amount: i128,
    seize_asset: Address,
    configs: &Map<Address, CollateralAssetConfig>,
) -> Result<i128, &'static str> {
    if !account.is_cross() {
        return Err("Account is not in cross margin mode");
    }

    let summary = calculate_multi_collateral_summary(env, account, configs);
    if !summary.is_liquidatable {
        return Err("Account is healthy, cannot liquidate");
    }

    let repay_cfg = configs
        .get(repay_asset.clone())
        .ok_or("Repay asset configuration not found")?;
    let seize_cfg = configs
        .get(seize_asset.clone())
        .ok_or("Seize asset configuration not found")?;

    let repay_value = calculate_asset_value(repay_amount, repay_cfg.price, repay_cfg.decimals);
    // 5% liquidation bonus (105%)
    let seize_value = (repay_value.saturating_mul(105)) / 100;
    let seize_amount =
        (seize_value.saturating_mul(10i128.pow(seize_cfg.decimals))) / seize_cfg.price.max(1);

    let mut new_positions = Vec::new(env);
    let mut debt_repaid = false;
    let mut collateral_seized = false;

    for i in 0..account.positions.len() {
        let mut pos = account.positions.get(i).unwrap();
        if pos.asset == repay_asset && pos.debt > 0 {
            pos.debt = (pos.debt - repay_amount).max(0);
            debt_repaid = true;
        }
        if pos.asset == seize_asset && pos.amount > 0 {
            if pos.amount < seize_amount {
                return Err("Insufficient collateral of chosen seize asset");
            }
            pos.amount -= seize_amount;
            collateral_seized = true;
        }
        if pos.amount > 0 || pos.debt > 0 {
            new_positions.push_back(pos);
        }
    }

    if !debt_repaid || !collateral_seized {
        return Err("Failed to match repay debt or seize collateral asset");
    }

    account.positions = new_positions;
    let post_summary = calculate_multi_collateral_summary(env, account, configs);
    account.total_collateral_value = post_summary.total_collateral_value;
    account.total_debt_value = post_summary.total_debt_value;

    Ok(seize_amount)
}
