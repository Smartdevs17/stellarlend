//! # Treasury Module
//!
//! Manages protocol fee collection and treasury for the StellarLend protocol.
//!
//! ## Overview
//! The treasury module provides a central place for:
//! - Configuring a protocol treasury address (admin-only)
//! - Viewing accumulated protocol reserves per asset
//! - Withdrawing reserves to a recipient (admin-only)
//! - Configuring fee percentages (interest spread fee, liquidation fee)
//!
//! ## Fee Sources
//! - **Borrow fees**: Collected at borrow time (`borrow_fee_bps` in `AssetParams`)
//! - **Interest spread**: A percentage of repaid interest goes to `ProtocolReserve`
//! - **Liquidation bonus fee**: A percentage of the liquidation incentive is retained
//! - **Flash loan fees**: Entire flash loan fee is credited to `ProtocolReserve`
//!
//! ## Access Control
//! All write operations require the caller to be the protocol admin.

#![allow(unused)]
use soroban_sdk::{contracterror, contracttype, Address, Env};

use crate::deposit::DepositDataKey;
use crate::events::{emit_fee_config_updated, emit_reserves_claimed, emit_treasury_set};

/// Errors that can occur during treasury operations
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TreasuryError {
    /// Caller is not the protocol admin
    Unauthorized = 1,
    /// Amount must be greater than zero
    InvalidAmount = 2,
    /// Requested amount exceeds protocol reserves
    InsufficientReserve = 3,
    /// Overflow occurred during calculation
    Overflow = 4,
    /// Treasury address has not been configured
    TreasuryNotSet = 5,
    /// Fee value out of valid range (0–10000 bps)
    InvalidFee = 6,
}

/// Storage keys for treasury-related data
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum TreasuryDataKey {
    /// Protocol treasury address
    TreasuryAddress,
    /// Fee configuration for interest and liquidation fees
    FeeConfig,
}

/// Fee configuration for protocol revenue collection
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TreasuryFeeConfig {
    /// Percentage of repaid interest routed to protocol reserve (in basis points)
    /// e.g. 1000 = 10% of interest paid goes to reserve
    pub interest_fee_bps: i128,
    /// Percentage of liquidation bonus retained by protocol (in basis points)
    /// e.g. 1000 = 10% of the liquidation incentive amount stays in reserve
    pub liquidation_fee_bps: i128,
}

/// Default: 10% of interest goes to reserve
const DEFAULT_INTEREST_FEE_BPS: i128 = 1000;
/// Default: 10% of liquidation bonus goes to reserve
const DEFAULT_LIQUIDATION_FEE_BPS: i128 = 1000;

/// Return the default fee configuration used when none has been set
pub fn default_fee_config() -> TreasuryFeeConfig {
    TreasuryFeeConfig {
        interest_fee_bps: DEFAULT_INTEREST_FEE_BPS,
        liquidation_fee_bps: DEFAULT_LIQUIDATION_FEE_BPS,
    }
}

// ============================================================================
// Treasury Address
// ============================================================================

/// Set the protocol treasury address (admin-only)
///
/// The treasury is the destination for claimed reserves when no explicit
/// recipient is provided to `claim_reserves`.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `caller` - Must be the protocol admin
/// * `treasury` - The new treasury address
pub fn set_treasury(
    env: &Env,
    caller: Address,
    treasury: Address,
) -> Result<(), TreasuryError> {
    caller.require_auth();
    crate::admin::require_admin(env, &caller).map_err(|_| TreasuryError::Unauthorized)?;

    env.storage()
        .persistent()
        .set(&TreasuryDataKey::TreasuryAddress, &treasury);

    emit_treasury_set(
        env,
        crate::events::TreasurySetEvent {
            admin: caller,
            treasury: treasury.clone(),
            timestamp: env.ledger().timestamp(),
        },
    );

    Ok(())
}

/// Return the configured treasury address, if any
pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage()
        .persistent()
        .get::<TreasuryDataKey, Address>(&TreasuryDataKey::TreasuryAddress)
}

// ============================================================================
// Fee Configuration
// ============================================================================

/// Return the current fee configuration (falls back to defaults if unset)
pub fn get_fee_config(env: &Env) -> TreasuryFeeConfig {
    env.storage()
        .persistent()
        .get::<TreasuryDataKey, TreasuryFeeConfig>(&TreasuryDataKey::FeeConfig)
        .unwrap_or_else(default_fee_config)
}

/// Update the protocol fee configuration (admin-only)
///
/// Fee values must be in range [0, 10000] basis points.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `caller` - Must be the protocol admin
/// * `config` - New fee configuration
pub fn set_fee_config(
    env: &Env,
    caller: Address,
    config: TreasuryFeeConfig,
) -> Result<(), TreasuryError> {
    caller.require_auth();
    crate::admin::require_admin(env, &caller).map_err(|_| TreasuryError::Unauthorized)?;

    if config.interest_fee_bps < 0 || config.interest_fee_bps > 10000 {
        return Err(TreasuryError::InvalidFee);
    }
    if config.liquidation_fee_bps < 0 || config.liquidation_fee_bps > 10000 {
        return Err(TreasuryError::InvalidFee);
    }

    env.storage()
        .persistent()
        .set(&TreasuryDataKey::FeeConfig, &config);

    emit_fee_config_updated(
        env,
        crate::events::FeeConfigUpdatedEvent {
            admin: caller,
            interest_fee_bps: config.interest_fee_bps,
            liquidation_fee_bps: config.liquidation_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );

    Ok(())
}

// ============================================================================
// Reserve Balance
// ============================================================================

/// Return the accumulated protocol reserve for a given asset
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `asset` - The asset address (None for native XLM)
pub fn get_reserve_balance(env: &Env, asset: Option<Address>) -> i128 {
    env.storage()
        .persistent()
        .get::<DepositDataKey, i128>(&DepositDataKey::ProtocolReserve(asset))
        .unwrap_or(0)
}

// ============================================================================
// Claim Reserves
// ============================================================================

/// Withdraw accumulated protocol reserves to a recipient (admin-only)
///
/// Transfers `amount` tokens from the on-chain reserve tracking to `recipient`.
/// In non-test environments, the actual token transfer is executed.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `caller` - Must be the protocol admin
/// * `asset` - The asset to withdraw (None for native XLM)
/// * `recipient` - Destination address for the withdrawn tokens
/// * `amount` - Amount to withdraw (must be ≤ current reserve balance)
///
/// # Errors
/// * `TreasuryError::Unauthorized` - caller is not admin
/// * `TreasuryError::InvalidAmount` - amount ≤ 0
/// * `TreasuryError::InsufficientReserve` - amount exceeds reserve balance
pub fn claim_reserves(
    env: &Env,
    caller: Address,
    asset: Option<Address>,
    recipient: Address,
    amount: i128,
) -> Result<(), TreasuryError> {
    caller.require_auth();
    crate::admin::require_admin(env, &caller).map_err(|_| TreasuryError::Unauthorized)?;

    if amount <= 0 {
        return Err(TreasuryError::InvalidAmount);
    }

    let reserve_key = DepositDataKey::ProtocolReserve(asset.clone());
    let current_reserve = env
        .storage()
        .persistent()
        .get::<DepositDataKey, i128>(&reserve_key)
        .unwrap_or(0);

    if amount > current_reserve {
        return Err(TreasuryError::InsufficientReserve);
    }

    let new_reserve = current_reserve
        .checked_sub(amount)
        .ok_or(TreasuryError::Overflow)?;
    env.storage().persistent().set(&reserve_key, &new_reserve);

    // Execute the actual token transfer outside of tests
    #[cfg(not(test))]
    if let Some(ref asset_addr) = asset {
        let token_client = soroban_sdk::token::Client::new(env, asset_addr);
        token_client.transfer(&env.current_contract_address(), &recipient, &amount);
    }

    emit_reserves_claimed(
        env,
        crate::events::ReservesClaimedEvent {
            admin: caller,
            asset: asset.clone(),
            recipient: recipient.clone(),
            amount,
            timestamp: env.ledger().timestamp(),
        },
    );

    Ok(())
}
