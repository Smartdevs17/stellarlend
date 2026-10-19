//! Per-update price-jump bound for push reports.
//!
//! The aggregation-level deviation check compares sources *against each
//! other*. It says nothing about how far one source moves *between its own
//! reports*, so a single compromised or buggy reporter could move its slot
//! from 1.00 to 100.00 in one ledger and the hub would store it. With a thin
//! source set (one feed, or a quorum the attacker controls) that price is
//! then served.
//!
//! This module bounds that move. Every push report is compared with the last
//! price accepted on the same `(asset, slot)`; beyond the configured band the
//! report is rejected with [`OracleHubError::PriceJumpExceeded`] and nothing
//! is stored, so the slot keeps its previous price, earns no heartbeat and no
//! reward.
//!
//! # Configuration
//!
//! ```text
//! DefaultMaxPriceJump   -> i128  // hub-wide bound in bps, default 2_000 (20 %)
//! MaxPriceJump(Bytes)   -> i128  // per-asset override
//! ```
//!
//! A bound of `0` disables the check for that scope. The bound is capped at
//! [`BPS_DENOM`] (a 100 % move), which already admits a doubling in one step.
//!
//! # Rebase
//!
//! A legitimate move larger than the bound (a redenomination, a depeg, a
//! long outage) would otherwise lock the slot: every honest report is
//! rejected against the stale reference. Governance unlocks it with
//! [`crate::OracleHubContract::rebase_price`], which overwrites the slot's
//! reference with an explicitly chosen price and publishes a
//! [`PriceRebasedEvent`]. Subsequent reports are bounded against the new
//! reference. The operational runbook is in the crate README.

use crate::storage::DataKey;
use crate::types::{MaxPriceJumpUpdatedEvent, BPS_DENOM};
use crate::OracleHubError;
use soroban_sdk::{panic_with_error, Bytes, Env};

/// Default per-update bound: a report may move its slot by at most 20 %.
pub const DEFAULT_MAX_PRICE_JUMP_BPS: i128 = 2_000;

/// Set the hub-wide bound, or one asset's bound when `asset` is `Some`.
pub fn set_bound(env: &Env, asset: Option<&Bytes>, max_jump_bps: i128) {
    if !(0..=BPS_DENOM).contains(&max_jump_bps) {
        panic_with_error!(env, OracleHubError::InvalidConfig);
    }
    match asset {
        Some(asset) => env
            .storage()
            .instance()
            .set(&DataKey::MaxPriceJump(asset.clone()), &max_jump_bps),
        None => env
            .storage()
            .instance()
            .set(&DataKey::DefaultMaxPriceJump, &max_jump_bps),
    }
    MaxPriceJumpUpdatedEvent {
        asset: asset.cloned(),
        max_jump_bps,
    }
    .publish(env);
}

/// Effective bound for an asset: its override, else the hub-wide default.
pub fn bound(env: &Env, asset: &Bytes) -> i128 {
    let storage = env.storage().instance();
    storage
        .get(&DataKey::MaxPriceJump(asset.clone()))
        .or_else(|| storage.get(&DataKey::DefaultMaxPriceJump))
        .unwrap_or(DEFAULT_MAX_PRICE_JUMP_BPS)
}

/// Whether `price` lies further than `max_jump_bps` from `reference`.
///
/// Compared exactly as `|price - reference| * 10_000 > max_jump_bps *
/// reference`, so a move a fraction of a basis point past the bound is still
/// caught instead of being rounded down onto it. Overflow saturates, which
/// can only make a report look *more* deviant, never less.
pub fn exceeds_bound(price: i128, reference: i128, max_jump_bps: i128) -> bool {
    let diff = price.saturating_sub(reference).saturating_abs();
    diff.saturating_mul(BPS_DENOM) > max_jump_bps.saturating_mul(reference)
}

/// Reject `price` when it moves the slot further than the asset's bound from
/// `reference`, the slot's last accepted price.
pub fn enforce(env: &Env, asset: &Bytes, price: i128, reference: Option<i128>) {
    let reference = match reference {
        Some(reference) if reference > 0 => reference,
        // The first report of a slot has nothing to be measured against.
        _ => return,
    };
    let max_jump_bps = bound(env, asset);
    if max_jump_bps != 0 && exceeds_bound(price, reference, max_jump_bps) {
        panic_with_error!(env, OracleHubError::PriceJumpExceeded);
    }
}
