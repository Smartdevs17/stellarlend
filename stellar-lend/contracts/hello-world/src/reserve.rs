//! # Reserve and Treasury Module
//!
//! Manages protocol reserves and treasury operations for the StellarLend lending protocol.
//!
//! ## Overview
//! This module implements the reserve factor mechanism that allocates a portion of protocol
//! interest income to the treasury. The reserve factor determines what percentage of interest
//! accrued from borrowers is retained by the protocol versus distributed to lenders.
//!
//! ## Key Concepts
//!
//! ### Reserve Factor
//! - A percentage (in basis points) of interest income allocated to protocol reserves
//! - Example: 1000 bps (10%) means 10% of interest goes to reserves, 90% to lenders
//! - Configurable per asset by admin
//! - Range: 0 - 5000 bps (0% - 50%)
//!
//! ### Reserve Accrual
//! - Reserves accrue automatically when interest is calculated during repayment
//! - Formula: `reserve_amount = total_interest * reserve_factor / 10000`
//! - Tracked separately per asset in persistent storage
//!
//! ### Treasury Withdrawal
//! - Admin can withdraw accrued reserves to a treasury address
//! - Withdrawals are bounded by the actual reserve balance
//! - Cannot withdraw user funds (collateral or principal)
//! - All withdrawals are logged via events
//!
//! ## Storage Layout
//! - `ReserveBalance(asset)` — accumulated reserve per asset
//! - `ReserveFactor(asset)` — reserve factor per asset (basis points)
//! - `TreasuryAddress` — destination address for reserve withdrawals
//!
//! ## Security Invariants
//! - Reserve factor must be between 0 and 5000 bps (0% - 50%)
//! - Only admin can modify reserve factors or withdraw reserves
//! - Withdrawals cannot exceed accrued reserve balance
//! - User funds (collateral, principal) are never accessible via treasury operations
//! - All state changes emit events for transparency and auditability

#![allow(unused)]
use soroban_sdk::{contracterror, contracttype, Address, Env, Symbol};

use crate::deposit::DepositDataKey;
use crate::reserve_factor;

/// Maximum allowed reserve factor (50% = 5000 basis points)
/// This ensures that at least 50% of interest always goes to lenders
pub const MAX_RESERVE_FACTOR_BPS: i128 = 5000;

/// Default reserve factor (10% = 1000 basis points)
pub const DEFAULT_RESERVE_FACTOR_BPS: i128 = 1000;

/// Basis points scale (100% = 10000 basis points)
pub const BASIS_POINTS_SCALE: i128 = 10000;

/// Errors that can occur during reserve and treasury operations
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReserveError {
    /// Caller is not authorized (not admin)
    Unauthorized = 1,
    /// Reserve factor exceeds maximum allowed value
    InvalidReserveFactor = 2,
    /// Withdrawal amount exceeds available reserve balance
    InsufficientReserve = 3,
    /// Invalid asset address
    InvalidAsset = 4,
    /// Invalid treasury address
    InvalidTreasury = 5,
    /// Withdrawal amount must be greater than zero
    InvalidAmount = 6,
    /// Arithmetic overflow occurred
    Overflow = 7,
    /// Treasury address not configured
    TreasuryNotSet = 8,
}

/// Storage keys for reserve and treasury data
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum ReserveDataKey {
    /// Reserve factor per asset: ReserveFactor(asset) -> i128
    /// Percentage of interest allocated to reserves (in basis points)
    ReserveFactor(Option<Address>),

    /// Optional AMM integration target per asset.
    /// Allows governance/admin to route protocol reserves into AMM liquidity.
    ReserveAmmTarget(Option<Address>),

    /// Virtual LP token balance tracked per asset for AMM deployments.
    /// (Accounting only; actual LP token custody is managed by the AMM contract / ops layer.)
    ReserveAmmLpBalance(Option<Address>),
    /// Dynamic reserve factor curve per asset.
    /// Value type: ReserveFactorCurve
    ReserveFactorCurve(Option<Address>),
}

/// Initialize reserve configuration for an asset
///
/// Sets the default reserve factor for a new asset. Should be called when
/// a new asset is added to the protocol.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `asset` - The asset address (None for native asset)
/// * `reserve_factor_bps` - Reserve factor in basis points (0-5000)
///
/// # Errors
/// * `ReserveError::InvalidReserveFactor` - If reserve factor > MAX_RESERVE_FACTOR_BPS
/// * `ReserveError::Overflow` - If arithmetic overflow occurs
///
/// # Security
/// * No authorization check - should be called internally during asset initialization
/// * Validates reserve factor is within acceptable bounds
#[allow(deprecated)]
pub fn initialize_reserve_config(
    env: &Env,
    asset: Option<Address>,
    reserve_factor_bps: i128,
) -> Result<(), ReserveError> {
    // Validate reserve factor
    if !(0..=MAX_RESERVE_FACTOR_BPS).contains(&reserve_factor_bps) {
        return Err(ReserveError::InvalidReserveFactor);
    }

    // Set reserve factor
    let factor_key = ReserveDataKey::ReserveFactor(asset.clone());
    env.storage()
        .persistent()
        .set(&factor_key, &reserve_factor_bps);

    // Emit initialization event
    let topics = (Symbol::new(env, "reserve_initialized"),);
    env.events().publish(topics, (asset, reserve_factor_bps));

    Ok(())
}

/// Set the reserve factor for an asset (admin only)
///
/// Updates the percentage of interest income allocated to protocol reserves.
/// This affects future interest accruals but does not retroactively change
/// existing reserve balances.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `caller` - The caller address (must be admin)
/// * `asset` - The asset address (None for native asset)
/// * `reserve_factor_bps` - New reserve factor in basis points (0-5000)
///
/// # Errors
/// * `ReserveError::Unauthorized` - If caller is not admin
/// * `ReserveError::InvalidReserveFactor` - If reserve factor > MAX_RESERVE_FACTOR_BPS
///
/// # Security
/// * Requires admin authorization
/// * Validates reserve factor bounds
/// * Emits event for transparency
#[allow(deprecated)]
pub fn set_reserve_factor(
    env: &Env,
    caller: Address,
    asset: Option<Address>,
    reserve_factor_bps: i128,
) -> Result<(), ReserveError> {
    // Require admin authorization
    caller.require_auth();
    require_admin(env, &caller)?;

    // Validate reserve factor
    if !(0..=MAX_RESERVE_FACTOR_BPS).contains(&reserve_factor_bps) {
        return Err(ReserveError::InvalidReserveFactor);
    }

    // Update reserve factor
    let factor_key = ReserveDataKey::ReserveFactor(asset.clone());
    env.storage()
        .persistent()
        .set(&factor_key, &reserve_factor_bps);

    // Emit event
    let topics = (Symbol::new(env, "reserve_factor_updated"), caller);
    env.events().publish(topics, (asset, reserve_factor_bps));

    Ok(())
}

/// Get the reserve factor, preferring the treasury fee config, falling back to static storage.
///
/// This allows the reserve factor to be configured through the treasury fee configuration,
/// providing a single source of truth for the reserve factor that integrates with fee management.
pub fn get_reserve_factor_from_fee_config(env: &Env, asset: Option<Address>) -> i128 {
    // Try treasury fee config first
    let fee_factor = get_static_reserve_factor(env, asset.clone());
    // The treasury fee config provides a default; if explicitly set in storage, use that
    let storage_factor = get_reserve_factor(env, asset);
    storage_factor
}

/// Get the reserve factor for an asset
///
/// Returns the current reserve factor, or the default if not explicitly set.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `asset` - The asset address (None for native asset)
///
/// # Returns
/// Reserve factor in basis points (0-5000)
/// Get the legacy reserve factor
pub fn get_legacy_reserve_factor(env: &Env, asset: Option<Address>) -> i128 {
    let factor_key = ReserveDataKey::ReserveFactor(asset);
    env.storage()
        .persistent()
        .get(&factor_key)
        .unwrap_or(DEFAULT_RESERVE_FACTOR_BPS)
}

/// Get the reserve factor from packed pool config (#713)
pub fn get_reserve_factor(env: &Env, asset: Option<Address>) -> i128 {
    crate::storage::migrate_from_legacy(env, &asset)
        .map(|c| c.reserve_factor_bps)
        .unwrap_or(DEFAULT_RESERVE_FACTOR_BPS)
}

/// Configure AMM reserve integration target (admin only).
///
/// Stores the AMM contract address that should be used for reserve deployments for a given asset.
#[allow(deprecated)]
pub fn set_reserve_amm_target(
    env: &Env,
    caller: Address,
    asset: Option<Address>,
    amm_contract: Address,
) -> Result<(), ReserveError> {
    caller.require_auth();
    require_admin(env, &caller)?;

    env.storage().persistent().set(
        &ReserveDataKey::ReserveAmmTarget(asset.clone()),
        &amm_contract,
    );

    let topics = (Symbol::new(env, "reserve_amm_target_set"), caller);
    env.events().publish(topics, (asset, amm_contract));

    Ok(())
}

/// Get AMM reserve integration target for an asset.
pub fn get_reserve_amm_target(env: &Env, asset: Option<Address>) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&ReserveDataKey::ReserveAmmTarget(asset))
}

/// Record a reserve deployment into an AMM position (admin only).
///
/// This is an accounting helper that moves value from `ReserveBalance` into `ReserveAmmLpBalance`
/// without requiring a specific AMM interface at the protocol contract layer.
#[allow(deprecated)]
pub fn record_reserve_deploy_to_amm(
    env: &Env,
    caller: Address,
    asset: Option<Address>,
    reserve_amount: i128,
    lp_tokens_received: i128,
) -> Result<(), ReserveError> {
    caller.require_auth();
    require_admin(env, &caller)?;

    if reserve_amount <= 0 || lp_tokens_received <= 0 {
        return Err(ReserveError::InvalidAmount);
    }

    let balance = crate::treasury::get_reserve_balance(env, asset.clone());
    if reserve_amount > balance {
        return Err(ReserveError::InsufficientReserve);
    }

    let reserve_key = crate::deposit::DepositDataKey::ProtocolReserve(asset.clone());
    env.storage()
        .persistent()
        .set(&reserve_key, &(balance - reserve_amount));

    let lp_key = ReserveDataKey::ReserveAmmLpBalance(asset.clone());
    let current_lp: i128 = env.storage().persistent().get(&lp_key).unwrap_or(0);
    let new_lp = current_lp
        .checked_add(lp_tokens_received)
        .ok_or(ReserveError::Overflow)?;
    env.storage().persistent().set(&lp_key, &new_lp);

    let topics = (Symbol::new(env, "reserve_deployed_to_amm"), caller);
    env.events()
        .publish(topics, (asset, reserve_amount, lp_tokens_received, new_lp));

    Ok(())
}

/// Get tracked AMM LP token balance for an asset.
pub fn get_reserve_amm_lp_balance(env: &Env, asset: Option<Address>) -> i128 {
    env.storage()
        .persistent()
        .get(&ReserveDataKey::ReserveAmmLpBalance(asset))
        .unwrap_or(0)
}



/// Helper function to require admin authorization
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `caller` - The caller address to check
///
/// # Errors
/// * `ReserveError::Unauthorized` - If caller is not admin
fn require_admin(env: &Env, caller: &Address) -> Result<(), ReserveError> {
    let admin_key = DepositDataKey::Admin;
    let admin = env
        .storage()
        .persistent()
        .get::<DepositDataKey, Address>(&admin_key)
        .ok_or(ReserveError::Unauthorized)?;

    if caller != &admin {
        return Err(ReserveError::Unauthorized);
    }

    Ok(())
}

/// Get reserve statistics for an asset
///
/// Returns comprehensive reserve information for reporting and analytics.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `asset` - The asset address (None for native asset)
///
/// # Returns
/// Tuple of (reserve_balance, reserve_factor_bps, treasury_address)
pub fn get_reserve_stats(env: &Env, asset: Option<Address>) -> (i128, i128, Option<Address>) {
    let balance = crate::treasury::get_reserve_balance(env, asset.clone());
    let factor = get_reserve_factor(env, asset);
    let treasury = crate::treasury::get_treasury(env);

    (balance, factor, treasury)
}
