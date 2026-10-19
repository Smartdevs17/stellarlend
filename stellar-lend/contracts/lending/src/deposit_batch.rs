//! # Batch Deposits (issue #1044)
//!
//! Depositing into several assets (or topping up the same asset in several
//! tranches) previously needed one transaction per deposit. Each of those paid
//! for its own authorization check, reentrancy-guard round trip, pause lookup,
//! packed deposit-state read/write and user-position read/write.
//!
//! [`deposit_batch`] applies up to [`MAX_BATCH_DEPOSITS`] deposits in a single
//! invocation while touching shared state only once:
//!
//! | cost                         | N × `deposit` | `deposit_batch` |
//! |------------------------------|---------------|-----------------|
//! | `require_auth`               | N             | 1               |
//! | reentrancy guard             | N             | 1               |
//! | pause lookup                 | N             | 1               |
//! | packed deposit state (r + w) | 2N            | 2               |
//! | user position (r + w)        | 2N            | 2               |
//!
//! ## Semantics
//!
//! A batch is **all-or-nothing**: every entry is validated (positive, not dust,
//! within the deposit cap *cumulatively*) before anything is written, and any
//! failure aborts the whole invocation. The resulting state is identical to
//! calling `deposit` once per entry, in order — including the running balance
//! reported by each entry's `VaultDepositEvent` and the position's `asset`
//! field, which ends up as the asset of the last entry.

use crate::deposit::{emit_deposit_event, get_deposit_position, save_deposit_position, DepositError};
use crate::dust::is_dust_amount;
use crate::events::BatchDepositEvent;
use crate::hot_storage::DepositHotSlot;
use crate::pause::{self, PauseType};
use crate::reentrancy::ReentrancyGuard;
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Upper bound on entries per batch, keeping a batch inside Soroban's
/// per-transaction CPU and event budgets.
pub const MAX_BATCH_DEPOSITS: u32 = 20;

/// A single entry in a batch deposit.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DepositRequest {
    pub asset: Address,
    pub amount: i128,
}

/// Outcome of a successful batch deposit.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchDepositResult {
    /// Number of deposits applied.
    pub count: u32,
    /// Sum of all deposited amounts.
    pub total_amount: i128,
    /// User's collateral balance after the batch.
    pub new_balance: i128,
}

/// Apply several deposits for `user` atomically in one invocation.
pub fn deposit_batch(
    env: &Env,
    user: Address,
    requests: Vec<DepositRequest>,
) -> Result<BatchDepositResult, DepositError> {
    deposit_batch_with_auth(env, user, requests, true)
}

pub(crate) fn deposit_batch_with_auth(
    env: &Env,
    user: Address,
    requests: Vec<DepositRequest>,
    require_auth: bool,
) -> Result<BatchDepositResult, DepositError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| DepositError::ReentrancyDetected)?;

    if require_auth {
        user.require_auth();
    }

    if pause::is_paused(env, PauseType::Deposit) {
        return Err(DepositError::DepositPaused);
    }

    let count = requests.len();
    if count == 0 {
        return Err(DepositError::EmptyBatch);
    }
    if count > MAX_BATCH_DEPOSITS {
        return Err(DepositError::BatchTooLarge);
    }

    let mut hot = DepositHotSlot::load(env);

    // Pass 1: validate every entry against the cumulative totals before any
    // state is touched, so a bad entry late in the batch fails fast.
    let mut batch_total: i128 = 0;
    for req in requests.iter() {
        if req.amount <= 0 || is_dust_amount(req.amount, hot.state.min) {
            return Err(DepositError::InvalidAmount);
        }
        batch_total = batch_total
            .checked_add(req.amount)
            .ok_or(DepositError::Overflow)?;
    }

    let new_total = hot
        .state
        .total
        .checked_add(batch_total)
        .ok_or(DepositError::Overflow)?;
    if new_total > hot.state.cap {
        return Err(DepositError::ExceedsDepositCap);
    }

    // Pass 2: apply in memory, emitting one event per entry with the running
    // balance so indexers see exactly what N sequential deposits would emit.
    let first_asset = requests.get_unchecked(0).asset;
    let mut position = get_deposit_position(env, &user, &first_asset);
    let now = env.ledger().timestamp();

    for req in requests.iter() {
        position.amount = position
            .amount
            .checked_add(req.amount)
            .ok_or(DepositError::Overflow)?;
        position.asset = req.asset.clone();
        emit_deposit_event(env, user.clone(), req.asset, req.amount, position.amount);
    }
    position.last_deposit_time = now;

    // Single write for each piece of shared state.
    save_deposit_position(env, &user, &position);
    hot.state.total = new_total;
    hot.commit(env);

    BatchDepositEvent {
        user,
        count,
        total_amount: batch_total,
        new_balance: position.amount,
        timestamp: now,
    }
    .publish(env);

    Ok(BatchDepositResult {
        count,
        total_amount: batch_total,
        new_balance: position.amount,
    })
}
