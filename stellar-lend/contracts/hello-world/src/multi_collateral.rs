//! # Multi-Asset Collateral Module
//!
//! Provides multi-asset collateral support for the StellarLend protocol.
//!
//! ## Overview
//! Users can deposit multiple distinct asset types as collateral. Each asset is
//! tracked individually via `DepositDataKey::UserAssetCollateral(user, asset)` and
//! enumerated via `DepositDataKey::UserAssetList(user)`.
//!
//! ## Total Collateral Value
//! `calculate_total_collateral_value` aggregates all per-asset balances using
//! oracle prices and each asset's configured collateral factor:
//!
//! ```
//! total_value = Σ( amount_i * price_i * collateral_factor_i / (PRICE_DECIMALS * BPS_SCALE) )
//! ```
//!
//! where `price_i` is the oracle price with 8 decimals and `collateral_factor_i`
//! is in basis points.
//!
//! ## Backward Compatibility
//! When a user's `UserAssetList` is empty (legacy single-asset users), callers
//! fall back to the aggregate `CollateralBalance(user)` which is always maintained
//! alongside the per-asset records.

#![allow(unused)]
use soroban_sdk::{contracterror, Address, Env, Vec};

use crate::deposit::{AssetParams, DepositDataKey};

/// Errors specific to multi-asset collateral operations
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MultiCollateralError {
    /// Arithmetic overflow
    Overflow = 1,
    /// Asset not found in user's collateral list
    AssetNotFound = 2,
}

/// Scale factor for oracle prices (8 decimal places)
const PRICE_DECIMALS: i128 = 100_000_000; // 10^8

/// Scale factor for basis points
const BPS_SCALE: i128 = 10_000;

// ============================================================================
// View Functions
// ============================================================================

/// Return the collateral balance for a specific `(user, asset)` pair.
///
/// Returns 0 if the user has no position in this asset.
pub fn get_user_asset_collateral(env: &Env, user: &Address, asset: &Address) -> i128 {
    let key = DepositDataKey::UserAssetCollateral(user.clone(), asset.clone());
    env.storage()
        .persistent()
        .get::<DepositDataKey, i128>(&key)
        .unwrap_or(0)
}

/// Return the list of assets in which the user currently has collateral.
///
/// Empty for legacy users who have only used the single-asset flow.
pub fn get_user_asset_list(env: &Env, user: &Address) -> Vec<Address> {
    let key = DepositDataKey::UserAssetList(user.clone());
    env.storage()
        .persistent()
        .get::<DepositDataKey, Vec<Address>>(&key)
        .unwrap_or_else(|| Vec::new(env))
}

// ============================================================================
// Collateral Value Calculation
// ============================================================================

/// Get the collateral factor for an asset (in basis points, e.g. 7500 = 75%).
/// Falls back to 10000 (100%) if no `AssetParams` have been configured.
fn get_collateral_factor(env: &Env, asset: &Address) -> i128 {
    let key = DepositDataKey::AssetParams(asset.clone());
    env.storage()
        .persistent()
        .get::<DepositDataKey, AssetParams>(&key)
        .map(|p| p.collateral_factor)
        .unwrap_or(BPS_SCALE)
}

/// Get the oracle price for an asset, falling back to 1 PRICE_DECIMALS unit
/// (i.e. 1:1 ratio with the debt asset) when no price feed is configured.
fn get_oracle_price(env: &Env, asset: &Address) -> i128 {
    crate::oracle::get_price(env, asset).unwrap_or(PRICE_DECIMALS)
}

/// Calculate the total collateral value across all of a user's deposited assets,
/// weighted by oracle prices and collateral factors.
///
/// Formula per asset:
/// ```
/// asset_value = amount * oracle_price / PRICE_DECIMALS * collateral_factor / BPS_SCALE
/// ```
///
/// Returns the sum in "debt-unit" terms (same denomination as oracle prices).
///
/// Returns `0` if the user has no multi-asset positions (i.e. `UserAssetList`
/// is empty). Callers should fall back to `CollateralBalance(user)` in that case.
pub fn calculate_total_collateral_value(
    env: &Env,
    user: &Address,
) -> Result<i128, MultiCollateralError> {
    let asset_list = get_user_asset_list(env, user);
    let mut total: i128 = 0;

    for asset in asset_list.iter() {
        let amount = get_user_asset_collateral(env, user, &asset);
        if amount == 0 {
            continue;
        }

        let price = get_oracle_price(env, &asset);
        let collateral_factor = get_collateral_factor(env, &asset);

        // Step 1: amount * price  (could be large, use i128 which handles up to ~1.7 * 10^38)
        let value_with_price = amount
            .checked_mul(price)
            .ok_or(MultiCollateralError::Overflow)?;

        // Step 2: scale down by price decimals
        let value_in_base = value_with_price
            .checked_div(PRICE_DECIMALS)
            .ok_or(MultiCollateralError::Overflow)?;

        // Step 3: apply collateral factor
        let weighted_value = value_in_base
            .checked_mul(collateral_factor)
            .ok_or(MultiCollateralError::Overflow)?
            .checked_div(BPS_SCALE)
            .ok_or(MultiCollateralError::Overflow)?;

        total = total
            .checked_add(weighted_value)
            .ok_or(MultiCollateralError::Overflow)?;
    }

    Ok(total)
}

/// Return `true` when the user has any per-asset collateral records.
/// Used by borrowing/liquidation logic to choose between multi-asset and
/// legacy single-asset paths.
pub fn has_multi_asset_collateral(env: &Env, user: &Address) -> bool {
    !get_user_asset_list(env, user).is_empty()
}
