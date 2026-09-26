pub use crate::events::VaultDepositEvent;

/// Backward-compatible name for vault deposit events (see [`VaultDepositEvent`]).
#[allow(dead_code)]
pub type DepositEvent = VaultDepositEvent;

use crate::dust::is_dust_amount;
use crate::hot_storage::DepositHotSlot;
use crate::pause::{self, PauseType};
use crate::reentrancy::ReentrancyGuard;
use soroban_sdk::{contracterror, contracttype, Address, Env};

/// Errors that can occur during deposit operations
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum DepositError {
    InvalidAmount = 1,
    DepositPaused = 2,
    Overflow = 3,
    AssetNotSupported = 4,
    ExceedsDepositCap = 5,
    Unauthorized = 6,
    ReentrancyDetected = 7,
    /// `deposit_batch` was called with no entries.
    EmptyBatch = 8,
    /// `deposit_batch` exceeded `MAX_BATCH_DEPOSITS` entries.
    BatchTooLarge = 9,
}

/// Storage keys for deposit-related data
#[contracttype]
#[derive(Clone)]
#[allow(clippy::enum_variant_names)]
pub enum DepositDataKey {
    UserCollateral(Address),
    /// Legacy per-field entry; superseded by the packed
    /// `HotStorageKey::DepositState` slot and only read for migration.
    TotalAmount,
    /// Legacy per-field entry (see `TotalAmount`).
    CapAmount,
    /// Legacy per-field entry (see `TotalAmount`).
    MinAmount,
}

/// User deposit position
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DepositCollateral {
    pub amount: i128,
    pub asset: Address,
    pub last_deposit_time: u64,
}

/// Deposit collateral into the protocol
///
/// # Arguments
/// * `env` - The contract environment
/// * `user` - The depositor's address
/// * `asset` - The collateral asset address
/// * `amount` - The amount to deposit
///
/// # Returns
/// Returns the updated collateral balance on success
pub fn deposit(
    env: &Env,
    user: Address,
    asset: Address,
    amount: i128,
) -> Result<i128, DepositError> {
    deposit_with_auth(env, user, asset, amount, true)
}

pub(crate) fn deposit_with_auth(
    env: &Env,
    user: Address,
    asset: Address,
    amount: i128,
    require_auth: bool,
) -> Result<i128, DepositError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| DepositError::ReentrancyDetected)?;

    if require_auth {
        user.require_auth();
    }

    if pause::is_paused(env, PauseType::Deposit) {
        return Err(DepositError::DepositPaused);
    }

    if amount <= 0 {
        return Err(DepositError::InvalidAmount);
    }

    // Hot path: cap, min and running total live in one packed entry (#1043),
    // so this is a single read here and a single write below.
    let mut hot = DepositHotSlot::load(env);

    if is_dust_amount(amount, hot.state.min) {
        return Err(DepositError::InvalidAmount);
    }

    let new_total = hot
        .state
        .total
        .checked_add(amount)
        .ok_or(DepositError::Overflow)?;

    if new_total > hot.state.cap {
        return Err(DepositError::ExceedsDepositCap);
    }

    let mut position = get_deposit_position(env, &user, &asset);
    position.amount = position
        .amount
        .checked_add(amount)
        .ok_or(DepositError::Overflow)?;
    position.last_deposit_time = env.ledger().timestamp();
    position.asset = asset.clone();

    save_deposit_position(env, &user, &position);
    hot.state.total = new_total;
    hot.commit(env);
    emit_deposit_event(env, user, asset, amount, position.amount);

    Ok(position.amount)
}

/// Initialize deposit settings
pub fn initialize_deposit_settings(
    env: &Env,
    deposit_cap: i128,
    min_deposit_amount: i128,
) -> Result<(), DepositError> {
    if deposit_cap <= 0 || min_deposit_amount <= 0 {
        return Err(DepositError::InvalidAmount);
    }

    let mut hot = DepositHotSlot::load(env);
    hot.state.cap = deposit_cap;
    hot.state.min = min_deposit_amount;
    hot.commit(env);
    Ok(())
}

pub fn get_user_collateral(env: &Env, user: &Address, asset: &Address) -> DepositCollateral {
    get_deposit_position(env, user, asset)
}

pub(crate) fn get_deposit_position(
    env: &Env,
    user: &Address,
    asset: &Address,
) -> DepositCollateral {
    env.storage()
        .persistent()
        .get(&DepositDataKey::UserCollateral(user.clone()))
        .unwrap_or(DepositCollateral {
            amount: 0,
            asset: asset.clone(),
            last_deposit_time: env.ledger().timestamp(),
        })
}

pub(crate) fn save_deposit_position(env: &Env, user: &Address, position: &DepositCollateral) {
    env.storage()
        .persistent()
        .set(&DepositDataKey::UserCollateral(user.clone()), position);
}

pub(crate) fn emit_deposit_event(
    env: &Env,
    user: Address,
    asset: Address,
    amount: i128,
    new_balance: i128,
) {
    VaultDepositEvent {
        user,
        asset,
        amount,
        new_balance,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}
