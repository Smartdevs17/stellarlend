//! event_monitor.rs — On-chain event monitoring for suspicious activity (issue #1391).
//!
//! # Context
//!
//! No monitoring existed for anomalous on-chain activity. This module adds:
//!
//! - **Detection rules** — threshold-based checks for large borrows, rapid
//!   liquidations, oracle-price manipulation attempts, and flash-loan abuse.
//! - **Alert emission** — every triggered rule publishes a typed Soroban event
//!   that off-chain alert consumers (Prometheus, PagerDuty, etc.) can subscribe
//!   to via the Horizon event stream or the indexing system.
//! - **Rate tracking** — per-user action counters stored in contract temporary
//!   storage allow the rules to catch burst activity within a configurable
//!   rolling window.
//!
//! # Alert severity levels
//!
//! | Level | Meaning |
//! |-------|---------|
//! | `Info` | Noteworthy but not immediately dangerous |
//! | `Warning` | Unusual pattern — warrants investigation |
//! | `Critical` | Strong signal of active exploit or manipulation |
//!
//! # Detection rules
//!
//! | Rule ID | Trigger | Default threshold |
//! |---------|---------|-------------------|
//! | `LARGE_BORROW` | Single borrow > threshold | 500 000 (protocol units) |
//! | `RAPID_LIQUIDATIONS` | > N liquidations of same user in window | 3 in 300 s |
//! | `PRICE_DEVIATION` | Oracle price deviates > X % from TWAP | 5 % |
//! | `FLASH_LOAN_ABUSE` | Flash loan with zero-or-negative profit net | always |
//! | `BURST_TRANSACTIONS` | > N actions from same address in window | 10 in 60 s |
//!
//! # Integration
//!
//! Call the check helpers from within the relevant contract operations:
//!
//! ```rust
//! // In borrow.rs, after a successful borrow:
//! event_monitor::check_large_borrow(&env, &user, amount);
//! event_monitor::record_action(&env, &user);
//! event_monitor::check_burst_transactions(&env, &user);
//! ```

#![allow(dead_code)]

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

// ─── Severity ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertSeverity {
    Info = 0,
    Warning = 1,
    Critical = 2,
}

// ─── Rule IDs ──────────────────────────────────────────────────────────────

/// Stable string identifiers for each detection rule.
pub mod rule {
    pub const LARGE_BORROW: &str = "LARGE_BORROW";
    pub const RAPID_LIQUIDATIONS: &str = "RAPID_LIQ";
    pub const PRICE_DEVIATION: &str = "PRICE_DEV";
    pub const FLASH_LOAN_ABUSE: &str = "FLASH_ABUSE";
    pub const BURST_TRANSACTIONS: &str = "BURST_TX";
    pub const ADMIN_ACTION: &str = "ADMIN_ACTION";
}

// ─── Default thresholds ────────────────────────────────────────────────────

/// Single-borrow alert threshold in protocol base units.
pub const DEFAULT_LARGE_BORROW_THRESHOLD: i128 = 500_000_000_000; // 500 000 (7 dp)

/// Number of liquidations of the same user within `LIQUIDATION_WINDOW_SECS`
/// that triggers an alert.
pub const DEFAULT_RAPID_LIQ_COUNT: u32 = 3;
pub const LIQUIDATION_WINDOW_SECS: u64 = 300;

/// Oracle deviation percentage (bps) that triggers an alert.
pub const DEFAULT_PRICE_DEVIATION_BPS: i128 = 500; // 5 %

/// Number of transactions from the same address within `BURST_WINDOW_SECS`
/// that triggers an alert.
pub const DEFAULT_BURST_TX_COUNT: u32 = 10;
pub const BURST_WINDOW_SECS: u64 = 60;

// ─── Storage key helpers ───────────────────────────────────────────────────

/// Temporary-storage key for per-user action count: `(EM_CNT, user)`.
fn action_count_key(user: &Address) -> (Symbol, Address) {
    (symbol_short!("EM_CNT"), user.clone())
}

/// Temporary-storage key for per-user window start timestamp: `(EM_WIN, user)`.
fn window_start_key(user: &Address) -> (Symbol, Address) {
    (symbol_short!("EM_WIN"), user.clone())
}

/// Temporary-storage key for per-user liquidation count: `(EM_LIQ, user)`.
fn liq_count_key(user: &Address) -> (Symbol, Address) {
    (symbol_short!("EM_LIQ"), user.clone())
}

/// Temporary-storage key for per-user liquidation window start: `(EM_LQW, user)`.
fn liq_window_key(user: &Address) -> (Symbol, Address) {
    (symbol_short!("EM_LQW"), user.clone())
}

// ─── Alert emitter ─────────────────────────────────────────────────────────

/// Emit a security alert event.
///
/// Events are published with topic `(security_alert, rule_id_symbol)` and a
/// payload of `(severity, actor, detail_i128)` for easy indexer parsing.
fn emit_alert(env: &Env, rule_id: Symbol, severity: AlertSeverity, actor: &Address, detail: i128) {
    env.events().publish(
        (symbol_short!("sec_alert"), rule_id),
        (severity as u32, actor.clone(), detail),
    );
}

// ─── Public check functions ────────────────────────────────────────────────

/// Check whether a borrow amount exceeds the large-borrow threshold.
///
/// Emits a `Critical` alert if `amount > threshold`.
pub fn check_large_borrow(env: &Env, user: &Address, amount: i128) {
    check_large_borrow_with_threshold(env, user, amount, DEFAULT_LARGE_BORROW_THRESHOLD);
}

/// Parameterised version — useful in tests and for protocol-level overrides.
pub fn check_large_borrow_with_threshold(
    env: &Env,
    user: &Address,
    amount: i128,
    threshold: i128,
) {
    if amount > threshold {
        emit_alert(
            env,
            symbol_short!("LRG_BRW"),
            AlertSeverity::Critical,
            user,
            amount,
        );
    }
}

/// Record one action for a user and check for burst-transaction anomaly.
///
/// Uses contract **temporary** storage so counters expire automatically when
/// the ledger TTL elapses — no manual cleanup required.
///
/// Emits a `Warning` alert when the per-user action count within
/// `BURST_WINDOW_SECS` exceeds `DEFAULT_BURST_TX_COUNT`.
pub fn record_action(env: &Env, user: &Address) {
    let now = env.ledger().timestamp();
    let win_key = window_start_key(user);
    let cnt_key = action_count_key(user);

    let window_start: u64 = env
        .storage()
        .temporary()
        .get::<(Symbol, Address), u64>(&win_key)
        .unwrap_or(0);

    let count: u32 = env
        .storage()
        .temporary()
        .get::<(Symbol, Address), u32>(&cnt_key)
        .unwrap_or(0);

    if now.saturating_sub(window_start) > BURST_WINDOW_SECS {
        // Window expired — reset.
        env.storage()
            .temporary()
            .set::<(Symbol, Address), u64>(&win_key, &now);
        env.storage()
            .temporary()
            .set::<(Symbol, Address), u32>(&cnt_key, &1);
    } else {
        let new_count = count.saturating_add(1);
        env.storage()
            .temporary()
            .set::<(Symbol, Address), u32>(&cnt_key, &new_count);
    }
}

/// Check whether a user has exceeded the burst-transaction threshold.
pub fn check_burst_transactions(env: &Env, user: &Address) {
    let cnt_key = action_count_key(user);
    let count: u32 = env
        .storage()
        .temporary()
        .get::<(Symbol, Address), u32>(&cnt_key)
        .unwrap_or(0);

    if count > DEFAULT_BURST_TX_COUNT {
        emit_alert(
            env,
            symbol_short!("BURST_TX"),
            AlertSeverity::Warning,
            user,
            count as i128,
        );
    }
}

/// Record a liquidation event for `liquidatee` and check for rapid-liquidation
/// patterns (potential griefing or price manipulation).
///
/// Emits a `Critical` alert when `DEFAULT_RAPID_LIQ_COUNT` or more liquidations
/// of the same user occur within `LIQUIDATION_WINDOW_SECS`.
pub fn record_liquidation(env: &Env, liquidatee: &Address) {
    let now = env.ledger().timestamp();
    let win_key = liq_window_key(liquidatee);
    let cnt_key = liq_count_key(liquidatee);

    let window_start: u64 = env
        .storage()
        .temporary()
        .get::<(Symbol, Address), u64>(&win_key)
        .unwrap_or(0);

    let count: u32 = env
        .storage()
        .temporary()
        .get::<(Symbol, Address), u32>(&cnt_key)
        .unwrap_or(0);

    let new_count = if now.saturating_sub(window_start) > LIQUIDATION_WINDOW_SECS {
        env.storage()
            .temporary()
            .set::<(Symbol, Address), u64>(&win_key, &now);
        1u32
    } else {
        count.saturating_add(1)
    };

    env.storage()
        .temporary()
        .set::<(Symbol, Address), u32>(&cnt_key, &new_count);

    if new_count >= DEFAULT_RAPID_LIQ_COUNT {
        emit_alert(
            env,
            symbol_short!("RPD_LIQ"),
            AlertSeverity::Critical,
            liquidatee,
            new_count as i128,
        );
    }
}

/// Check for oracle price deviation from an expected reference value.
///
/// `current_price` and `reference_price` must be in the same unit.
/// Emits a `Critical` alert when deviation (in bps) exceeds
/// `DEFAULT_PRICE_DEVIATION_BPS`.
pub fn check_price_deviation(env: &Env, asset: &Address, current_price: i128, reference_price: i128) {
    if reference_price == 0 {
        // Cannot compute deviation; emit a warning to avoid silent failures.
        emit_alert(
            env,
            symbol_short!("PRC_DEV"),
            AlertSeverity::Warning,
            asset,
            0,
        );
        return;
    }

    let diff = (current_price - reference_price).abs();
    // deviation_bps = diff * 10000 / reference_price
    let deviation_bps = diff
        .saturating_mul(10_000)
        .checked_div(reference_price)
        .unwrap_or(i128::MAX);

    if deviation_bps > DEFAULT_PRICE_DEVIATION_BPS {
        emit_alert(
            env,
            symbol_short!("PRC_DEV"),
            AlertSeverity::Critical,
            asset,
            deviation_bps,
        );
    }
}

/// Check for flash-loan abuse: a flash loan where the net profit (fee paid to
/// the protocol) is zero or negative indicates an exploit attempt.
///
/// Emits a `Critical` alert when `protocol_fee <= 0`.
pub fn check_flash_loan(env: &Env, receiver: &Address, amount: i128, protocol_fee: i128) {
    if protocol_fee <= 0 {
        emit_alert(
            env,
            symbol_short!("FL_ABUSE"),
            AlertSeverity::Critical,
            receiver,
            amount,
        );
    }
}

/// Emit an informational alert whenever a privileged admin action is performed.
///
/// This creates an immutable audit trail of all governance operations on-chain.
pub fn record_admin_action(env: &Env, admin: &Address, action_code: i128) {
    emit_alert(
        env,
        symbol_short!("ADM_ACT"),
        AlertSeverity::Info,
        admin,
        action_code,
    );
}
