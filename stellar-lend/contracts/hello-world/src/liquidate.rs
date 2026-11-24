//! # Liquidation Module
//!
//! Handles liquidation of undercollateralized positions in the lending protocol.
//!
//! Liquidators can repay a portion of a borrower's debt in exchange for their
//! collateral plus a liquidation incentive (bonus). This module uses the risk
//! management system to determine:
//! - Whether a position is eligible for liquidation (below liquidation threshold)
//! - The maximum liquidatable amount (controlled by the close factor)
//! - The liquidation incentive awarded to the liquidator
//!
//! ## State Machine
//! All liquidation paths (single and batch) flow through a single
//! `LiquidationStateMachine` with explicit phases:
//! 1. **PreChecks** — amount, reentrancy, pause, asset validation
//! 2. **LoadPosition** — fetch position, accrue interest
//! 3. **Eligibility** — verify position is undercollateralized
//! 4. **Calculate** — close factor, incentive, collateral seized
//! 5. **Execute** — token transfers, state mutations
//! 6. **Finalize** — analytics, events, credit score
//!
//! This eliminates duplicated validation/execution logic between single and
//! batch paths — both go through the same state machine.
//!
//! ## Cross-Asset Liquidation
//! When debt and collateral are different assets, oracle prices are used to
//! convert between asset values. A default price of 1.0 (8 decimals) is used
//! as fallback when oracle prices are not configured.
//!
//! ## Invariants
//! - Only undercollateralized positions (below liquidation threshold) can be liquidated.
//! - Liquidation amount cannot exceed the close factor percentage of total debt.
//! - Collateral seized cannot exceed the borrower's available collateral.
//! - Interest is accrued on the borrower's position before liquidation.

#![allow(unused)]
use crate::events::{
    emit_batch_liquidation, emit_liquidation, emit_liquidation_fee_collected,
    BatchLiquidationEvent, LiquidationEvent, LiquidationFeeCollectedEvent,
};
use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Map, Symbol, Val, Vec};
use stellarlend_math::checked::apply_bps;
use stellarlend_math::liquidation::{
    collateral_value_in_debt as math_collateral_value_in_debt,
    debt_value_in_collateral as math_debt_value_in_collateral, is_profitable as math_is_profitable,
    priority_score as math_priority_score, seize_amount as math_seize_amount,
};
use stellarlend_math::MathError;

use crate::deposit::{
    add_activity_log, emit_analytics_updated_event, emit_position_updated_event,
    emit_user_activity_tracked_event, update_protocol_analytics, AssetParams, DepositDataKey,
    Position, ProtocolAnalytics, UserAnalytics,
};
use crate::oracle::{get_liquidation_price, OracleError};
use crate::risk_management::{
    is_emergency_paused, is_operation_paused, require_operation_not_paused, RiskManagementError,
};
use crate::risk_params::{
    can_be_liquidated, get_close_factor, get_liquidation_incentive,
    get_liquidation_incentive_amount, get_max_liquidatable_amount, get_risk_params,
};

// ── Liquidation State Machine ──────────────────────────────────────────────

/// Explicit phases of the liquidation state machine.
/// Both single and batch liquidations flow through these phases in order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiquidationPhase {
    PreChecks,
    LoadPosition,
    Eligibility,
    Calculate,
    Execute,
    Finalize,
}

/// Context carried through the liquidation state machine.
struct LiquidationContext {
    liquidator: Address,
    borrower: Address,
    debt_asset: Option<Address>,
    collateral_asset: Option<Address>,
    debt_amount: i128,
    timestamp: u64,
    position: Position,
    total_debt: i128,
    collateral_balance: i128,
    collateral_value: i128,
    actual_debt_liquidated: i128,
    actual_collateral_seized: i128,
    incentive_amount: i128,
    protocol_fee: i128,
    is_multi_asset: bool,
}

/// Result of a completed liquidation.
pub struct LiquidationOutcome {
    pub debt_liquidated: i128,
    pub collateral_seized: i128,
    pub incentive_amount: i128,
}

/// State machine driver — executes all phases in order.
fn run_liquidation_sm(
    env: &Env,
    ctx: &mut LiquidationContext,
) -> Result<LiquidationOutcome, LiquidationError> {
    // Phase 1: PreChecks (amount, reentrancy, pause, asset validation)
    pre_checks(env, ctx)?;
    // Phase 2: LoadPosition (fetch + accrue interest)
    load_position(env, ctx)?;
    // Phase 3: Eligibility (undercollateralized check)
    check_eligibility(env, ctx)?;
    // Phase 4: Calculate (close factor, incentive, collateral)
    calculate_amounts(env, ctx)?;
    // Phase 5: Execute (transfers + state mutations)
    execute_transfers(env, ctx)?;
    // Phase 6: Finalize (analytics, events)
    finalize(env, ctx)?;

    Ok(LiquidationOutcome {
        debt_liquidated: ctx.actual_debt_liquidated,
        collateral_seized: ctx.actual_collateral_seized,
        incentive_amount: ctx.incentive_amount,
    })
}

fn pre_checks(env: &Env, ctx: &LiquidationContext) -> Result<(), LiquidationError> {
    if ctx.debt_amount <= 0 {
        return Err(LiquidationError::InvalidAmount);
    }
    let _guard =
        crate::reentrancy::ReentrancyGuard::new(env).map_err(|_| LiquidationError::Reentrancy)?;
    let liquidation_allowed =
        crate::circuit_breaker::is_liquidation_allowed(env, &ctx.liquidator).unwrap_or(true);
    if !liquidation_allowed {
        return Err(LiquidationError::LiquidationPaused);
    }
    if is_emergency_paused(env) {
        return Err(LiquidationError::LiquidationPaused);
    }
    require_operation_not_paused(env, Symbol::new(env, "pause_liquidate")).map_err(|e| {
        match e {
            RiskManagementError::OperationPaused => LiquidationError::LiquidationPaused,
            RiskManagementError::EmergencyPaused => LiquidationError::LiquidationPaused,
            _ => LiquidationError::LiquidationPaused,
        }
    })?;
    if let Some(ref debt_addr) = ctx.debt_asset {
        if debt_addr == &env.current_contract_address() {
            return Err(LiquidationError::InvalidDebtAsset);
        }
    }
    if let Some(ref collateral_addr) = ctx.collateral_asset {
        if collateral_addr == &env.current_contract_address() {
            return Err(LiquidationError::InvalidCollateralAsset);
        }
    }
    Ok(())
}

fn load_position(env: &Env, ctx: &mut LiquidationContext) -> Result<(), LiquidationError> {
    let position_key = DepositDataKey::Position(ctx.borrower.clone());
    let mut position = env
        .storage()
        .persistent()
        .get::<DepositDataKey, Position>(&position_key)
        .ok_or(LiquidationError::NotLiquidatable)?;
    accrue_interest(env, &ctx.borrower, &mut position)?;
    ctx.position = position;
    ctx.total_debt = calculate_debt_value(ctx.position.debt, ctx.position.borrow_interest)?;
    ctx.is_multi_asset = crate::multi_collateral::has_multi_asset_collateral(env, &ctx.borrower);

    ctx.collateral_balance = if let Some(ref collateral_addr) = ctx.collateral_asset {
        if ctx.is_multi_asset {
            crate::multi_collateral::get_user_asset_collateral(env, &ctx.borrower, collateral_addr)
        } else {
            let key = DepositDataKey::CollateralBalance(ctx.borrower.clone());
            env.storage().persistent().get::<DepositDataKey, i128>(&key).unwrap_or(0)
        }
    } else {
        let key = DepositDataKey::CollateralBalance(ctx.borrower.clone());
        env.storage().persistent().get::<DepositDataKey, i128>(&key).unwrap_or(0)
    };
    Ok(())
}

fn check_eligibility(env: &Env, ctx: &mut LiquidationContext) -> Result<(), LiquidationError> {
    ctx.collateral_value = if ctx.is_multi_asset {
        crate::multi_collateral::calculate_total_collateral_value(env, &ctx.borrower)
            .map_err(|_| LiquidationError::Overflow)?
    } else if ctx.debt_asset.is_none() && ctx.collateral_asset.is_none() {
        ctx.collateral_balance
    } else {
        let d = if let Some(ref debt_addr) = ctx.debt_asset {
            get_asset_price(env, debt_addr)?
        } else {
            1i128
        };
        let c = if let Some(ref collateral_addr) = ctx.collateral_asset {
            get_asset_price(env, collateral_addr)?
        } else {
            1i128
        };
        calculate_collateral_value(ctx.collateral_balance, c, d)?
    };
    let can_liquidate = can_be_liquidated(env, ctx.collateral_value, ctx.total_debt)
        .map_err(|_| LiquidationError::NotLiquidatable)?;
    if !can_liquidate {
        return Err(LiquidationError::NotLiquidatable);
    }
    Ok(())
}

fn calculate_amounts(env: &Env, ctx: &mut LiquidationContext) -> Result<(), LiquidationError> {
    let max_liquidatable =
        get_max_liquidatable_amount(env, ctx.total_debt).map_err(|_| LiquidationError::Overflow)?;
    if ctx.debt_amount > max_liquidatable {
        return Err(LiquidationError::ExceedsCloseFactor);
    }
    ctx.actual_debt_liquidated = if ctx.debt_amount > ctx.total_debt {
        ctx.total_debt
    } else {
        ctx.debt_amount
    };
    let incentive_bps = calculate_dynamic_penalty(env, ctx.collateral_value, ctx.total_debt)?;
    ctx.incentive_amount = apply_bps(ctx.actual_debt_liquidated, incentive_bps)
        .map_err(map_math_error)?;
    let collateral_value_liquidated =
        if ctx.debt_asset.is_none() && ctx.collateral_asset.is_none() {
            ctx.actual_debt_liquidated
        } else {
            let d = if let Some(ref debt_addr) = ctx.debt_asset {
                get_asset_price(env, debt_addr)?
            } else {
                1i128
            };
            let c = if let Some(ref collateral_addr) = ctx.collateral_asset {
                get_asset_price(env, collateral_addr)?
            } else {
                1i128
            };
            math_debt_value_in_collateral(ctx.actual_debt_liquidated, d, c)
                .map_err(map_math_error)?
        };
    let collateral_seized =
        math_seize_amount(collateral_value_liquidated, incentive_bps)
            .map_err(map_math_error)?;
    ctx.actual_collateral_seized = if collateral_seized > ctx.collateral_balance {
        ctx.collateral_balance
    } else {
        collateral_seized
    };
    let fee_config = crate::treasury::get_fee_config(env);
    ctx.protocol_fee =
        apply_bps(ctx.incentive_amount, fee_config.liquidation_fee_bps)
            .map_err(map_math_error)?;
    abort_if_unprofitable(
        ctx.actual_debt_liquidated,
        collateral_seized,
        ctx.protocol_fee,
    )?;
    Ok(())
}

fn execute_transfers(env: &Env, ctx: &mut LiquidationContext) -> Result<(), LiquidationError> {
    // Debt transfer from liquidator
    if let Some(ref debt_addr) = ctx.debt_asset {
        let liquidator_balance = if let Some(cached) =
            crate::storage::get_temp_token_balance(env, debt_addr, &ctx.liquidator)
        {
            cached
        } else {
            let token_client = soroban_sdk::token::Client::new(env, debt_addr);
            let balance = token_client.balance(&ctx.liquidator);
            crate::storage::set_temp_token_balance(env, debt_addr, &ctx.liquidator, balance);
            balance
        };
        if liquidator_balance < ctx.actual_debt_liquidated {
            return Err(LiquidationError::InsufficientBalance);
        }
        let token_client = soroban_sdk::token::Client::new(env, debt_addr);
        token_client.transfer_from(
            &env.current_contract_address(),
            &ctx.liquidator,
            &env.current_contract_address(),
            &ctx.actual_debt_liquidated,
        );
    }
    // Collateral transfer to liquidator
    let liquidator_collateral = ctx.actual_collateral_seized
        .checked_sub(ctx.protocol_fee)
        .ok_or(LiquidationError::Overflow)?;
    if let Some(ref collateral_addr) = ctx.collateral_asset {
        let contract_balance = if let Some(cached) = crate::storage::get_temp_token_balance(
            env, collateral_addr, &env.current_contract_address(),
        ) {
            cached
        } else {
            let token_client = soroban_sdk::token::Client::new(env, collateral_addr);
            let balance = token_client.balance(&env.current_contract_address());
            crate::storage::set_temp_token_balance(
                env, collateral_addr, &env.current_contract_address(), balance,
            );
            balance
        };
        if contract_balance < ctx.actual_collateral_seized {
            return Err(LiquidationError::InsufficientBalance);
        }
        let token_client = soroban_sdk::token::Client::new(env, collateral_addr);
        token_client.transfer(
            &env.current_contract_address(),
            &ctx.liquidator,
            &liquidator_collateral,
        );
    }
    // Credit protocol fee
    if ctx.protocol_fee > 0 {
        let reserve_key = DepositDataKey::ProtocolReserve(ctx.collateral_asset.clone());
        let current_reserve = env
            .storage()
            .persistent()
            .get::<DepositDataKey, i128>(&reserve_key)
            .unwrap_or(0);
        let new_reserve = current_reserve
            .checked_add(ctx.protocol_fee)
            .ok_or(LiquidationError::Overflow)?;
        env.storage().persistent().set(&reserve_key, &new_reserve);
        emit_liquidation_fee_collected(
            env,
            LiquidationFeeCollectedEvent {
                asset: ctx.collateral_asset.clone(),
                fee_amount: ctx.protocol_fee,
                timestamp: ctx.timestamp,
            },
        );
    }
    // Update borrower position
    let interest_to_pay = if ctx.actual_debt_liquidated <= ctx.position.borrow_interest {
        ctx.actual_debt_liquidated
    } else {
        ctx.position.borrow_interest
    };
    let principal_to_pay = ctx.actual_debt_liquidated
        .checked_sub(interest_to_pay)
        .ok_or(LiquidationError::Overflow)?;
    ctx.position.borrow_interest = ctx.position.borrow_interest
        .checked_sub(interest_to_pay)
        .unwrap_or(0);
    ctx.position.debt = ctx.position.debt
        .checked_sub(principal_to_pay)
        .unwrap_or(0);
    ctx.position.last_accrual_time = ctx.timestamp;
    let aggregate_collateral_key = DepositDataKey::CollateralBalance(ctx.borrower.clone());
    let aggregate_collateral = if ctx.is_multi_asset {
        env.storage()
            .persistent()
            .get::<DepositDataKey, i128>(&aggregate_collateral_key)
            .unwrap_or(0)
    } else {
        ctx.collateral_balance
    };
    let new_aggregate_collateral = aggregate_collateral
        .checked_sub(ctx.actual_collateral_seized)
        .unwrap_or(0);
    env.storage().persistent().set(&aggregate_collateral_key, &new_aggregate_collateral);
    if let Some(ref collateral_addr) = ctx.collateral_asset {
        if ctx.is_multi_asset {
            crate::deposit::record_asset_withdrawal(
                env, &ctx.borrower, collateral_addr, ctx.actual_collateral_seized,
            ).map_err(|_| LiquidationError::Overflow)?;
        }
    }
    ctx.position.collateral = new_aggregate_collateral;
    let position_key = DepositDataKey::Position(ctx.borrower.clone());
    env.storage().persistent().set(&position_key, &ctx.position);
    Ok(())
}

fn finalize(env: &Env, ctx: &LiquidationContext) -> Result<(), LiquidationError> {
    update_liquidation_analytics(
        env, &ctx.borrower, &ctx.liquidator,
        ctx.actual_debt_liquidated, ctx.actual_collateral_seized, ctx.timestamp,
    )?;
    add_activity_log(
        env, &ctx.borrower, Symbol::new(env, "liquidate"),
        ctx.actual_debt_liquidated, ctx.debt_asset.clone(), ctx.timestamp,
    ).map_err(|e| match e {
        crate::deposit::DepositError::Overflow => LiquidationError::Overflow,
        _ => LiquidationError::Overflow,
    })?;
    emit_liquidation(
        env,
        LiquidationEvent {
            liquidator: ctx.liquidator.clone(),
            borrower: ctx.borrower.clone(),
            debt_asset: ctx.debt_asset.clone(),
            collateral_asset: ctx.collateral_asset.clone(),
            debt_liquidated: ctx.actual_debt_liquidated,
            collateral_seized: ctx.actual_collateral_seized,
            incentive_amount: ctx.incentive_amount,
            timestamp: ctx.timestamp,
        },
    );
    let _ = crate::credit_score::update_score_on_liquidation(env, &ctx.borrower);
    emit_position_updated_event(env, &ctx.borrower, &ctx.position);
    emit_analytics_updated_event(
        env, &ctx.borrower, "liquidate",
        ctx.actual_debt_liquidated, ctx.timestamp,
    );
    emit_user_activity_tracked_event(
        env, &ctx.borrower, Symbol::new(env, "liquidate"),
        ctx.actual_debt_liquidated, ctx.timestamp,
    );
    Ok(())
}

/// Maximum liquidation penalty cap in basis points (20%).
///
/// Re-exported from the shared math library so the cap enforced here and the cap
/// enforced inside [`stellarlend_math::liquidation`] can never diverge.
pub use crate::liq_math::MAX_PENALTY_BPS;

/// Maps a [`MathError`] from the shared math library onto this module's errors.
fn map_math_error(err: MathError) -> LiquidationError {
    match err {
        MathError::DivisionByZero => LiquidationError::PriceNotAvailable,
        _ => LiquidationError::Overflow,
    }
}

/// Maximum number of positions that can be liquidated in a single batch
pub const MAX_BATCH_SIZE: u32 = 10;

/// Divisor that normalizes a repayment amount into the batch priority score.
const PRIORITY_DEBT_SCALE: i128 = 1_000_000;

/// Input item for a batch liquidation call
#[contracttype]
#[derive(Clone)]
pub struct BatchLiquidationRequest {
    pub borrower: Address,
    pub debt_asset: Option<Address>,
    pub collateral_asset: Option<Address>,
    pub debt_amount: i128,
    /// Priority score for ordering (higher = process first).
    /// Typically based on profit potential. 0 = use default ordering.
    pub priority_score: u64,
}

/// Per-position result within a batch liquidation
#[contracttype]
#[derive(Clone)]
pub struct BatchLiquidationResult {
    pub borrower: Address,
    pub success: bool,
    pub debt_liquidated: i128,
    pub collateral_seized: i128,
    /// 0 on success; LiquidationError discriminant on failure
    pub error_code: u32,
}

/// Calculate a dynamic liquidation penalty that scales with health factor severity.
///
/// The penalty scales linearly between the base incentive and `MAX_PENALTY_BPS`
/// as the health factor drops from the liquidation threshold toward zero.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `collateral_value` - Current collateral value
/// * `total_debt` - Current total debt (principal + interest)
///
/// # Returns
/// Penalty in basis points (e.g. 1000 = 10%)
pub fn calculate_dynamic_penalty(
    env: &Env,
    collateral_value: i128,
    total_debt: i128,
) -> Result<i128, LiquidationError> {
    let params = get_risk_params(env).ok_or(LiquidationError::Overflow)?;
    crate::liq_math::dynamic_penalty_bps(
        collateral_value,
        total_debt,
        params.liquidation_incentive,
        params.liquidation_threshold,
    )
    .map_err(map_math_error)
}

/// Minimum net profit (in bps of the debt repaid) below which a liquidation is
/// aborted to save gas (issue #723). 20 bps = 0.2% of the repaid debt.
const MIN_LIQUIDATOR_PROFIT_BPS: i128 = 20;

/// Early-exit guard: abort a liquidation when the liquidator's net recovery
/// (seized collateral minus the protocol fee, in debt terms) does not clear
/// the repaid debt plus a minimum profit floor. This skips the gas-heavy
/// token-transfer, position-mutation and event path for unprofitable
/// positions. `batch_liquidate` records the resulting
/// `LiquidationError::UnprofitableLiquidation` per item instead of blocking
/// the whole batch.
fn abort_if_unprofitable(
    debt_repayed: i128,
    collateral_seized: i128,
    protocol_fee: i128,
) -> Result<(), LiquidationError> {
    if debt_repayed <= 0 {
        return Err(LiquidationError::InvalidAmount);
    }
    let net = collateral_seized.saturating_sub(protocol_fee);
    if !math_is_profitable(debt_repayed, net, MIN_LIQUIDATOR_PROFIT_BPS).map_err(map_math_error)? {
        return Err(LiquidationError::UnprofitableLiquidation);
    }
    Ok(())
}

/// Errors that can occur during liquidation operations
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum LiquidationError {
    /// Liquidation amount must be greater than zero
    InvalidAmount = 1,
    /// Asset address is invalid
    InvalidAsset = 2,
    /// Position is not undercollateralized
    NotLiquidatable = 3,
    /// Liquidation operations are currently paused
    LiquidationPaused = 4,
    /// Liquidation amount exceeds maximum allowed (close factor)
    ExceedsCloseFactor = 5,
    /// Insufficient balance to liquidate
    InsufficientBalance = 6,
    /// Overflow occurred during calculation
    Overflow = 7,
    /// Invalid collateral asset
    InvalidCollateralAsset = 8,
    /// Invalid debt asset
    InvalidDebtAsset = 9,
    /// Price not available for asset
    PriceNotAvailable = 10,
    /// Liquidation would leave position undercollateralized
    InsufficientLiquidation = 11,
    /// Reentrancy detected
    Reentrancy = 12,
    /// Liquidation is unprofitable for the liquidator (net recovery below floor)
    UnprofitableLiquidation = 13,
}

/// Annual interest rate in basis points (e.g., 500 = 5% per year)
/// This matches the rate used in borrow.rs and repay.rs
// Interest rate is now calculated dynamically based on utilization
// See interest_rate module for details
/// Calculate interest accrued since last accrual time
/// Calculate accrued interest using dynamic interest rate
/// Uses the current borrow rate based on protocol utilization
fn calculate_accrued_interest(
    env: &Env,
    principal: i128,
    last_accrual_time: u64,
    current_time: u64,
) -> Result<i128, LiquidationError> {
    if principal == 0 {
        return Ok(0);
    }

    if current_time <= last_accrual_time {
        return Ok(0);
    }

    // Get current borrow rate (in basis points)
    let rate_bps =
        crate::interest_rate::calculate_borrow_rate(env).map_err(|_| LiquidationError::Overflow)?;

    // Calculate interest using the dynamic rate
    crate::interest_rate::calculate_accrued_interest(
        principal,
        last_accrual_time,
        current_time,
        rate_bps,
    )
    .map_err(|_| LiquidationError::Overflow)
}

/// Accrue compound interest on a position using the global borrow index.
fn accrue_interest(
    env: &Env,
    user: &Address,
    position: &mut Position,
) -> Result<(), LiquidationError> {
    let current_time = env.ledger().timestamp();

    if position.debt == 0 {
        position.borrow_interest = 0;
        position.last_accrual_time = current_time;
        let current_index = crate::interest_rate::update_lending_index(env)
            .map_err(|_| LiquidationError::Overflow)?
            .borrow_index;
        env.storage().persistent().set(
            &DepositDataKey::UserBorrowIndex(user.clone()),
            &current_index,
        );
        return Ok(());
    }

    let current_index = crate::interest_rate::update_lending_index(env)
        .map_err(|_| LiquidationError::Overflow)?
        .borrow_index;

    let user_index = env
        .storage()
        .persistent()
        .get::<DepositDataKey, i128>(&DepositDataKey::UserBorrowIndex(user.clone()))
        .unwrap_or(current_index);

    let new_interest =
        crate::interest_rate::compute_index_interest(position.debt, user_index, current_index)
            .map_err(|_| LiquidationError::Overflow)?;

    position.borrow_interest = position
        .borrow_interest
        .checked_add(new_interest)
        .ok_or(LiquidationError::Overflow)?;
    position.last_accrual_time = current_time;

    env.storage().persistent().set(
        &DepositDataKey::UserBorrowIndex(user.clone()),
        &current_index,
    );

    Ok(())
}

/// Get asset price from oracle
/// Returns price in base units (scaled by decimals)
fn get_asset_price(env: &Env, asset: &Address) -> Result<i128, LiquidationError> {
    get_liquidation_price(env, asset).map_err(|err| match err {
        OracleError::StalePrice => LiquidationError::PriceNotAvailable,
        _ => LiquidationError::PriceNotAvailable,
    })
}

/// Calculate collateral value in debt asset terms
/// Returns collateral_value = collateral_amount * collateral_price / debt_price
fn calculate_collateral_value(
    collateral_amount: i128,
    collateral_price: i128,
    debt_price: i128,
) -> Result<i128, LiquidationError> {
    math_collateral_value_in_debt(collateral_amount, collateral_price, debt_price)
        .map_err(map_math_error)
}

/// Calculate debt value
/// Returns debt_value = debt_amount + interest
fn calculate_debt_value(debt: i128, interest: i128) -> Result<i128, LiquidationError> {
    debt.checked_add(interest).ok_or(LiquidationError::Overflow)
}

/// Liquidate an undercollateralized position
///
/// Allows liquidators to liquidate undercollateralized positions by:
/// 1. Repaying debt on behalf of the borrower
/// 2. Receiving collateral plus a liquidation incentive
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `liquidator` - The address of the liquidator
/// * `borrower` - The address of the borrower being liquidated
/// * `debt_asset` - The address of the debt asset to repay (None for native XLM)
/// * `collateral_asset` - The address of the collateral asset to receive (None for native XLM)
/// * `debt_amount` - The amount of debt to liquidate
///
/// # Returns
/// Returns a tuple (debt_liquidated, collateral_seized, incentive_amount)
///
/// # Errors
/// * `LiquidationError::InvalidAmount` - If amount is zero or negative
/// * `LiquidationError::NotLiquidatable` - If position is not undercollateralized
/// * `LiquidationError::LiquidationPaused` - If liquidations are paused
/// * `LiquidationError::ExceedsCloseFactor` - If liquidation exceeds close factor limit
/// * `LiquidationError::InsufficientBalance` - If liquidator doesn't have enough balance
/// * `LiquidationError::Overflow` - If calculation overflow occurs
///
/// # Security
/// * Validates liquidation amount > 0
/// * Checks pause switches
/// * Validates position is undercollateralized
/// * Enforces close factor limits
/// * Accrues interest before liquidation
/// * Transfers debt asset from liquidator to contract
/// * Transfers collateral asset from contract to liquidator (with incentive)
/// * Updates debt and collateral balances
/// * Emits events for tracking
/// * Updates analytics
pub fn liquidate(
    env: &Env,
    liquidator: Address,
    borrower: Address,
    debt_asset: Option<Address>,
    collateral_asset: Option<Address>,
    debt_amount: i128,
) -> Result<(i128, i128, i128), LiquidationError> {
    let timestamp = env.ledger().timestamp();
    let mut ctx = LiquidationContext {
        liquidator,
        borrower,
        debt_asset,
        collateral_asset,
        debt_amount,
        timestamp,
        position: Position {
            debt: 0,
            borrow_interest: 0,
            collateral: 0,
            last_accrual_time: 0,
        },
        total_debt: 0,
        collateral_balance: 0,
        collateral_value: 0,
        actual_debt_liquidated: 0,
        actual_collateral_seized: 0,
        incentive_amount: 0,
        protocol_fee: 0,
        is_multi_asset: false,
    };
    let outcome = run_liquidation_sm(env, &mut ctx)?;
    Ok((outcome.debt_liquidated, outcome.collateral_seized, outcome.incentive_amount))
}

/// Update analytics after liquidation
fn update_liquidation_analytics(
    env: &Env,
    borrower: &Address,
    liquidator: &Address,
    debt_liquidated: i128,
    collateral_seized: i128,
    timestamp: u64,
) -> Result<(), LiquidationError> {
    // Update borrower analytics
    let borrower_analytics_key = DepositDataKey::UserAnalytics(borrower.clone());
    #[allow(clippy::unnecessary_lazy_evaluations)]
    let mut borrower_analytics = env
        .storage()
        .persistent()
        .get::<DepositDataKey, UserAnalytics>(&borrower_analytics_key)
        .unwrap_or_else(|| UserAnalytics {
            total_deposits: 0,
            total_borrows: 0,
            total_withdrawals: 0,
            total_repayments: 0,
            collateral_value: 0,
            debt_value: 0,
            collateralization_ratio: 0,
            activity_score: 0,
            transaction_count: 0,
            first_interaction: timestamp,
            last_activity: timestamp,
            risk_level: 0,
            loyalty_tier: 0,
        });

    // Update debt value (subtract liquidated amount)
    borrower_analytics.debt_value = borrower_analytics
        .debt_value
        .checked_sub(debt_liquidated)
        .unwrap_or(0);

    // Update collateral value (subtract seized amount)
    borrower_analytics.collateral_value = borrower_analytics
        .collateral_value
        .checked_sub(collateral_seized)
        .unwrap_or(0);

    // Recalculate collateralization ratio
    if borrower_analytics.debt_value > 0 && borrower_analytics.collateral_value > 0 {
        borrower_analytics.collateralization_ratio = borrower_analytics
            .collateral_value
            .checked_mul(10000)
            .and_then(|v| v.checked_div(borrower_analytics.debt_value))
            .unwrap_or(0);
    } else {
        borrower_analytics.collateralization_ratio = 0;
    }

    borrower_analytics.transaction_count = borrower_analytics.transaction_count.saturating_add(1);
    borrower_analytics.last_activity = timestamp;

    env.storage()
        .persistent()
        .set(&borrower_analytics_key, &borrower_analytics);

    // Update protocol analytics
    let protocol_analytics_key = DepositDataKey::ProtocolAnalytics;
    let mut protocol_analytics = env
        .storage()
        .persistent()
        .get::<DepositDataKey, ProtocolAnalytics>(&protocol_analytics_key)
        .unwrap_or(ProtocolAnalytics {
            total_deposits: 0,
            total_borrows: 0,
            total_value_locked: 0,
        });

    // Update total value locked (subtract seized collateral)
    protocol_analytics.total_value_locked = protocol_analytics
        .total_value_locked
        .checked_sub(collateral_seized)
        .unwrap_or(0);

    env.storage()
        .persistent()
        .set(&protocol_analytics_key, &protocol_analytics);

    Ok(())
}

/// Sort batch liquidation requests by priority score (descending).
///
/// Uses insertion sort — efficient for small batches (MAX_BATCH_SIZE = 10).
/// Requests with higher priority_score are processed first, maximizing
/// profit potential per gas unit spent.
fn sort_by_priority(
    env: &Env,
    requests: &Vec<BatchLiquidationRequest>,
) -> Vec<BatchLiquidationRequest> {
    let n = requests.len();
    let mut sorted: Vec<BatchLiquidationRequest> = Vec::new(env);

    for i in 0..n {
        let item = requests.get(i).unwrap();
        let mut insert_pos = sorted.len();

        // Find insertion point (descending order by priority_score)
        for j in 0..sorted.len() {
            let existing = sorted.get(j).unwrap();
            if item.priority_score > existing.priority_score {
                insert_pos = j;
                break;
            }
        }

        // Insert at the correct position
        sorted.insert(insert_pos, item);
    }

    sorted
}

/// Calculate the profit potential score for a liquidation request.
///
/// Higher scores indicate more profitable liquidations. The score is based on
/// the ratio of debt to collateral and the incentive available.
pub fn calculate_priority_score(
    env: &Env,
    request: &BatchLiquidationRequest,
) -> Result<u64, LiquidationError> {
    let position_key = DepositDataKey::Position(request.borrower.clone());
    let position = env
        .storage()
        .persistent()
        .get::<DepositDataKey, Position>(&position_key);

    let Some(pos) = position else {
        return Ok(0);
    };

    let total_debt = pos.debt.saturating_add(pos.borrow_interest);
    if total_debt == 0 {
        return Ok(0);
    }

    // Get collateral value
    let collateral_key = DepositDataKey::CollateralBalance(request.borrower.clone());
    let collateral: i128 = env
        .storage()
        .persistent()
        .get::<DepositDataKey, i128>(&collateral_key)
        .unwrap_or(0);

    // Collateral-to-debt ratio in bps, plus the repayment size normalized by 1e6
    // so a single large position does not swamp the ratio term.
    Ok(math_priority_score(
        collateral,
        total_debt,
        request.debt_amount,
        PRIORITY_DEBT_SCALE,
    ))
}

/// Liquidate multiple undercollateralized positions in a single atomic transaction.
///
/// Processes up to `MAX_BATCH_SIZE` positions in one call, reducing the per-position
/// overhead of separate transaction submissions. Per-position failures are captured in
/// the result and do not abort the entire batch.
///
/// Requests are sorted by priority_score (descending) before processing,
/// ensuring the most profitable liquidations are executed first.
///
/// Authorization and rate-limiting are applied once in the contract entry point
/// (`lib.rs`), not here.
pub fn batch_liquidate(
    env: &Env,
    liquidator: Address,
    requests: Vec<BatchLiquidationRequest>,
) -> Result<Vec<BatchLiquidationResult>, LiquidationError> {
    if requests.is_empty() {
        return Err(LiquidationError::InvalidAmount);
    }
    if requests.len() as u32 > MAX_BATCH_SIZE {
        return Err(LiquidationError::InvalidAmount);
    }

    // Sort by priority (most profitable first)
    let sorted_requests = sort_by_priority(env, &requests);

    let timestamp = env.ledger().timestamp();
    let mut results: Vec<BatchLiquidationResult> = Vec::new(env);
    let mut total_debt_liquidated: i128 = 0;
    let mut total_collateral_seized: i128 = 0;
    let mut successful: u32 = 0;
    let mut failed: u32 = 0;

    let n = sorted_requests.len();
    for i in 0..n {
        let req = sorted_requests.get(i).unwrap();
        let outcome = liquidate(
            env,
            liquidator.clone(),
            req.borrower.clone(),
            req.debt_asset.clone(),
            req.collateral_asset.clone(),
            req.debt_amount,
        );
        let result = match outcome {
            Ok((debt_liq, col_seized, _incentive)) => {
                total_debt_liquidated = total_debt_liquidated
                    .checked_add(debt_liq)
                    .unwrap_or(i128::MAX);
                total_collateral_seized = total_collateral_seized
                    .checked_add(col_seized)
                    .unwrap_or(i128::MAX);
                successful += 1;
                BatchLiquidationResult {
                    borrower: req.borrower,
                    success: true,
                    debt_liquidated: debt_liq,
                    collateral_seized: col_seized,
                    error_code: 0,
                }
            }
            Err(e) => {
                failed += 1;
                BatchLiquidationResult {
                    borrower: req.borrower,
                    success: false,
                    debt_liquidated: 0,
                    collateral_seized: 0,
                    error_code: e as u32,
                }
            }
        };
        results.push_back(result);
    }

    emit_batch_liquidation(
        env,
        BatchLiquidationEvent {
            liquidator,
            total_positions: n as u32,
            successful,
            failed,
            total_debt_liquidated,
            total_collateral_seized,
            timestamp,
        },
    );

    Ok(results)
}
