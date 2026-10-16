//! # Utilization Circuit Breaker
//!
//! Throttles new borrowing once protocol utilization (`total_debt / debt_ceiling`, in basis
//! points) climbs past an admin-configured threshold, so a pool can no longer be driven to
//! 100% utilization in a single burst.
//!
//! ## Behaviour
//!
//! | state     | borrow is accepted when                                    |
//! |-----------|------------------------------------------------------------|
//! | disabled  | always (breaker is inert; the default)                     |
//! | armed     | post-borrow utilization `< trip_utilization_bps`, **or**   |
//! |           | `amount <= max_borrow_while_tripped`                       |
//! | tripped   | `amount <= max_borrow_while_tripped`                       |
//!
//! * The breaker **trips** after any debt change that leaves utilization at or above
//!   `trip_utilization_bps`, and emits [`UtilBreakerTrippedEvent`].
//! * A tripped breaker **resets** automatically once utilization falls below
//!   `reset_utilization_bps` (repayments, dust sweeps) and emits
//!   [`UtilBreakerResetEvent`]. Keeping `reset < trip` gives hysteresis so the breaker
//!   does not flap around the threshold.
//! * Setting `max_borrow_while_tripped = 0` turns the throttle into a hard stop.
//! * Repayments are never throttled: they can only lower utilization.
//!
//! ## Reversibility
//!
//! Every effect is reversible by the admin without an upgrade:
//! * `set_utilization_breaker` with `enabled = false` lifts the throttle immediately (a tripped
//!   breaker is reset and the reset event is emitted).
//! * `set_utilization_breaker` with new thresholds re-evaluates the latch against current
//!   utilization.
//! * `reset_utilization_breaker` clears the latch manually. If utilization is still at or above
//!   the trip threshold the next borrow re-trips it.
//!
//! A rejected borrow reverts the whole invocation, so no event is emitted for it; indexers see
//! the `UtilizationThrottled` error code on the failed transaction instead.

use soroban_sdk::{contracterror, contracttype, Address, Env};

use crate::borrow::{get_admin, get_debt_ceiling, get_total_debt, BorrowError};
use crate::events::{UtilBreakerConfiguredEvent, UtilBreakerResetEvent, UtilBreakerTrippedEvent};

const BPS_SCALE: u32 = 10_000;

/// Admin-configurable circuit breaker parameters.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UtilizationBreakerConfig {
    /// Master switch. When `false` the breaker never throttles and never trips.
    pub enabled: bool,
    /// Utilization (bps of the debt ceiling) at or above which the breaker trips. `1..=10000`.
    pub trip_utilization_bps: u32,
    /// Utilization below which a tripped breaker re-arms automatically. `<= trip`.
    pub reset_utilization_bps: u32,
    /// Largest single borrow accepted while throttled. `0` blocks all new borrows.
    pub max_borrow_while_tripped: i128,
}

/// Read-only snapshot for frontends and monitoring.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UtilizationBreakerStatus {
    /// Active configuration; a disabled all-zero config when never set.
    pub config: UtilizationBreakerConfig,
    pub tripped: bool,
    /// Ledger timestamp of the most recent trip (0 when never tripped or after a reset).
    pub tripped_at: u64,
    pub utilization_bps: u32,
}

#[contracttype]
#[derive(Clone)]
pub(crate) enum UtilizationBreakerKey {
    Config,
    /// Ledger timestamp at which the breaker tripped; absent while armed.
    TrippedAt,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum UtilizationBreakerError {
    /// Caller is not the protocol admin
    Unauthorized = 1,
    /// Thresholds are out of range or inconsistent
    InvalidConfig = 2,
}

/// Utilization in bps for the given totals, clamped to `[0, 10000]`.
pub(crate) fn utilization_bps_for(total_debt: i128, debt_ceiling: i128) -> u32 {
    if debt_ceiling <= 0 || total_debt <= 0 {
        return 0;
    }
    let bps = total_debt
        .saturating_mul(BPS_SCALE as i128)
        .saturating_div(debt_ceiling);
    bps.clamp(0, BPS_SCALE as i128) as u32
}

fn active_config(env: &Env) -> Option<UtilizationBreakerConfig> {
    get_config(env).filter(|c| c.enabled)
}

pub fn get_config(env: &Env) -> Option<UtilizationBreakerConfig> {
    env.storage()
        .persistent()
        .get(&UtilizationBreakerKey::Config)
}

fn tripped_at(env: &Env) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&UtilizationBreakerKey::TrippedAt)
}

pub fn is_tripped(env: &Env) -> bool {
    tripped_at(env).is_some()
}

pub fn get_status(env: &Env) -> UtilizationBreakerStatus {
    UtilizationBreakerStatus {
        config: get_config(env).unwrap_or(UtilizationBreakerConfig {
            enabled: false,
            trip_utilization_bps: 0,
            reset_utilization_bps: 0,
            max_borrow_while_tripped: 0,
        }),
        tripped: is_tripped(env),
        tripped_at: tripped_at(env).unwrap_or(0),
        utilization_bps: utilization_bps_for(get_total_debt(env), get_debt_ceiling(env)),
    }
}

/// Gate a borrow of `amount` that would move total debt to `new_total`.
///
/// Called before any state is written, so a rejection leaves the ledger untouched.
pub(crate) fn check_borrow(
    env: &Env,
    amount: i128,
    new_total: i128,
    debt_ceiling: i128,
) -> Result<(), BorrowError> {
    let Some(cfg) = active_config(env) else {
        return Ok(());
    };

    let post_util = utilization_bps_for(new_total, debt_ceiling);
    let throttled = is_tripped(env) || post_util >= cfg.trip_utilization_bps;

    if throttled && amount > cfg.max_borrow_while_tripped {
        return Err(BorrowError::UtilizationThrottled);
    }
    Ok(())
}

/// Update the trip latch after total debt changed. Emits trip / reset events on transitions.
pub(crate) fn on_debt_changed(env: &Env, total_debt: i128, debt_ceiling: i128) {
    let Some(cfg) = active_config(env) else {
        return;
    };
    let util = utilization_bps_for(total_debt, debt_ceiling);

    if !is_tripped(env) {
        if util >= cfg.trip_utilization_bps {
            trip(env, &cfg, util, total_debt, debt_ceiling);
        }
    } else if util < cfg.reset_utilization_bps {
        reset(env, util, false);
    }
}

fn trip(
    env: &Env,
    cfg: &UtilizationBreakerConfig,
    util: u32,
    total_debt: i128,
    debt_ceiling: i128,
) {
    let now = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&UtilizationBreakerKey::TrippedAt, &now);
    UtilBreakerTrippedEvent {
        utilization_bps: util,
        trip_utilization_bps: cfg.trip_utilization_bps,
        total_debt,
        debt_ceiling,
        timestamp: now,
    }
    .publish(env);
}

fn reset(env: &Env, util: u32, manual: bool) {
    env.storage()
        .persistent()
        .remove(&UtilizationBreakerKey::TrippedAt);
    UtilBreakerResetEvent {
        utilization_bps: util,
        manual,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

fn require_admin(env: &Env, admin: &Address) -> Result<(), UtilizationBreakerError> {
    let stored = get_admin(env).ok_or(UtilizationBreakerError::Unauthorized)?;
    if *admin != stored {
        return Err(UtilizationBreakerError::Unauthorized);
    }
    admin.require_auth();
    Ok(())
}

fn validate(cfg: &UtilizationBreakerConfig) -> Result<(), UtilizationBreakerError> {
    if cfg.trip_utilization_bps == 0 || cfg.trip_utilization_bps > BPS_SCALE {
        return Err(UtilizationBreakerError::InvalidConfig);
    }
    if cfg.reset_utilization_bps > cfg.trip_utilization_bps {
        return Err(UtilizationBreakerError::InvalidConfig);
    }
    if cfg.max_borrow_while_tripped < 0 {
        return Err(UtilizationBreakerError::InvalidConfig);
    }
    Ok(())
}

/// Admin: install, change or disable the breaker.
///
/// The trip latch is re-evaluated against current utilization under the new config: disabling
/// (or raising the thresholds above current utilization) resets a tripped breaker, and lowering
/// the trip threshold below current utilization trips it immediately.
pub fn set_config(
    env: &Env,
    admin: Address,
    config: UtilizationBreakerConfig,
) -> Result<(), UtilizationBreakerError> {
    require_admin(env, &admin)?;
    validate(&config)?;

    env.storage()
        .persistent()
        .set(&UtilizationBreakerKey::Config, &config);

    UtilBreakerConfiguredEvent {
        admin,
        enabled: config.enabled,
        trip_utilization_bps: config.trip_utilization_bps,
        reset_utilization_bps: config.reset_utilization_bps,
        max_borrow_while_tripped: config.max_borrow_while_tripped,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    let total_debt = get_total_debt(env);
    let debt_ceiling = get_debt_ceiling(env);
    let util = utilization_bps_for(total_debt, debt_ceiling);

    if !config.enabled {
        if is_tripped(env) {
            reset(env, util, true);
        }
        return Ok(());
    }

    if is_tripped(env) && util < config.trip_utilization_bps {
        // New thresholds sit above current utilization: re-arm.
        reset(env, util, true);
    } else {
        on_debt_changed(env, total_debt, debt_ceiling);
    }
    Ok(())
}

/// Admin: clear the trip latch without changing configuration.
pub fn manual_reset(env: &Env, admin: Address) -> Result<(), UtilizationBreakerError> {
    require_admin(env, &admin)?;
    if is_tripped(env) {
        let util = utilization_bps_for(get_total_debt(env), get_debt_ceiling(env));
        reset(env, util, true);
    }
    Ok(())
}

// Unified error registry: protocol-wide global codes, messages and recovery
// suggestions for every variant (see `stellarlend_errors` and docs/ERROR_HANDLING.md).
stellarlend_errors::impl_contract_error! {
    UtilizationBreakerError => stellarlend_errors::domains::LENDING_UTILIZATION_BREAKER;
    Unauthorized => Unauthorized, "Caller is not authorized to perform this action";
    InvalidConfig => InvalidInput, "Utilization breaker thresholds are out of range or inconsistent";
}
