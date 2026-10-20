//! Storage TTL management for the hub's persistent entries.
//!
//! Soroban rents persistent storage by the ledger: an entry with no remaining
//! TTL is archived, and a price feed that has gone cold *and* had its data
//! archived cannot be read back until somebody pays to restore it. The hub is
//! exactly the kind of long-lived contract that hits this: prices are written
//! continuously, but reads are bursty, and a quiet asset's last accepted price
//! is the one a liquidation later depends on.
//!
//! This module makes that lifecycle explicit rather than implicit:
//!
//! - **Policy.** [`TtlPolicy`] declares a low-water `threshold` and an
//!   `extend_to` window, in ledgers. Governance sets a hub-wide default and
//!   may override it per asset, the same shape as every other knob in the hub.
//! - **Extension.** [`extend_entry`] calls the host's `extend_ttl`, which only
//!   bumps an entry whose remaining TTL has fallen below the threshold. That
//!   makes bumping idempotent by construction: calling it twice in a row is a
//!   no-op the second time, so a keeper may run it every ledger.
//! - **Amortized on the write path.** Every price write also refreshes the
//!   entry it just touched (see `lib.rs`), so managed entries stay live
//!   without a separate keeper transaction, and
//!   [`crate::OracleHubContract::bump_storage`] exists for the assets that
//!   stopped being written to.
//!
//! # What is managed, and what is not
//!
//! Only ledger-rent *persistent* entries are managed: the mirrored latest
//! price per slot and the retained price-history mirrors. Instance storage
//! (governance, config) shares the contract instance's own rent and is never
//! bumped here. The window is deliberately independent of a quote's freshness
//! budget: the goal is preserving the audit trail and the last reference
//! price, not keeping a stale quote servable.

use crate::storage::DataKey;
use crate::types::{
    TtlConfig, TtlConfigUpdatedEvent, TtlExtendedEvent, TtlPolicy, MAX_TTL_EXTEND_TO,
    MIN_TTL_THRESHOLD,
};
use crate::OracleHubError;
use soroban_sdk::{panic_with_error, Bytes, Env};

/// Hub-wide default policy, falling back to [`TtlPolicy::default_policy`].
pub fn default_policy(env: &Env) -> TtlPolicy {
    env.storage()
        .instance()
        .get(&DataKey::DefaultTtlPolicy)
        .unwrap_or_else(TtlPolicy::default_policy)
}

/// Effective policy for an asset: its own override, else the hub-wide default.
pub fn policy(env: &Env, asset: &Bytes) -> TtlPolicy {
    env.storage()
        .instance()
        .get(&DataKey::TtlPolicy(asset.clone()))
        .unwrap_or_else(|| default_policy(env))
}

/// Reject a policy that could not describe working storage management.
fn validate(env: &Env, policy: &TtlPolicy) {
    if !policy.enabled {
        return;
    }
    let coherent = policy.extend_to > policy.threshold
        && policy.threshold >= MIN_TTL_THRESHOLD
        && policy.extend_to <= MAX_TTL_EXTEND_TO;
    if !coherent {
        panic_with_error!(env, OracleHubError::InvalidConfig);
    }
}

/// Store a policy for one asset, or for every asset when `asset` is `None`.
pub fn set_policy(env: &Env, asset: Option<&Bytes>, policy: &TtlPolicy) {
    validate(env, policy);
    match asset {
        Some(asset) => env
            .storage()
            .instance()
            .set(&DataKey::TtlPolicy(asset.clone()), policy),
        None => env
            .storage()
            .instance()
            .set(&DataKey::DefaultTtlPolicy, policy),
    }
    TtlConfigUpdatedEvent {
        asset: asset.cloned().unwrap_or_else(|| Bytes::new(env)),
        enabled: policy.enabled,
        threshold: policy.threshold,
        extend_to: policy.extend_to,
    }
    .publish(env);
}

/// Read-only policy view for an asset, plus the ledger its latest price was
/// last refreshed at (or `0` when nothing is stored).
pub fn config(env: &Env, asset: &Bytes) -> TtlConfig {
    TtlConfig {
        asset: asset.clone(),
        policy: policy(env, asset),
        last_bump_ledger: last_bumped(env, asset),
    }
}

/// Ledger at which the asset's latest price was last refreshed, or `0`.
fn last_bumped(env: &Env, asset: &Bytes) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::LastTtlBump(asset.clone()))
        .unwrap_or(0)
}

/// Extend one persistent entry to the policy window when it is below the
/// threshold.
///
/// The host's `extend_ttl` is a no-op when the entry is already inside the
/// window, so this is idempotent. It reveals nothing about whether it acted,
/// which is why callers that need a count compare the stamped ledger instead;
/// here we return whether the entry existed and management was on.
fn extend_entry(env: &Env, key: &DataKey, policy: &TtlPolicy) -> bool {
    if !policy.enabled {
        return false;
    }
    if !env.storage().persistent().has(key) {
        return false;
    }
    env.storage()
        .persistent()
        .extend_ttl(key, policy.threshold, policy.extend_to);
    true
}

/// Bump every managed persistent entry of an asset and publish one event.
///
/// Returns how many entries were present and refreshed. The host extends only
/// the entries below the threshold, so a call in the middle of a window does no
/// storage work even though it reports the managed entries it walked.
pub fn bump_asset(env: &Env, asset: &Bytes) -> u32 {
    let policy = policy(env, asset);
    if !policy.enabled {
        return 0;
    }

    let mut bumped = 0u32;

    // Latest price per registered slot.
    for slot in crate::feeds::feed_index(env, asset).iter() {
        let key = DataKey::PersistentLatestPrice(asset.clone(), slot);
        if extend_entry(env, &key, &policy) {
            bumped += 1;
        }
    }

    // Retained history ring mirrors.
    let written = crate::history::count_for_ttl(env, asset);
    for i in 0..written {
        let key = DataKey::PersistentHistoryEntry(asset.clone(), i % crate::history::RING_SLOTS);
        if extend_entry(env, &key, &policy) {
            bumped += 1;
        }
    }

    if bumped > 0 {
        env.storage().instance().set(
            &DataKey::LastTtlBump(asset.clone()),
            &env.ledger().sequence(),
        );
        TtlExtendedEvent {
            asset: asset.clone(),
            ledger: env.ledger().sequence(),
            extend_to: policy.extend_to,
            bumped,
        }
        .publish(env);
    }
    bumped
}

/// Refresh the persistent latest-price entry of one slot right after it is
/// written. Cheap and idempotent, so the write path can call it unconditionally.
pub fn touch_latest_price(env: &Env, asset: &Bytes, slot: u32) {
    let policy = policy(env, asset);
    if !policy.enabled {
        return;
    }
    extend_entry(
        env,
        &DataKey::PersistentLatestPrice(asset.clone(), slot),
        &policy,
    );
}

/// Refresh a single persistent history entry after it is written.
pub fn touch_history_entry(env: &Env, asset: &Bytes, slot: u32) {
    let policy = policy(env, asset);
    if !policy.enabled {
        return;
    }
    extend_entry(
        env,
        &DataKey::PersistentHistoryEntry(asset.clone(), slot),
        &policy,
    );
}
