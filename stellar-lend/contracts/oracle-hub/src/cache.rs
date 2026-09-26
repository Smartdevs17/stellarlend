//! Memoized price reads with epoch invalidation.
//!
//! Every read of an aggregated price used to walk the feed slots, and every
//! pull feed also paid for a cross-contract call. When several operations in
//! the same few seconds all need the same price — a borrow, a liquidation and
//! a health check in one block — that work is pure repetition.
//!
//! The cache stores the aggregate per asset together with
//!
//! - the ledger time it was produced,
//! - an **effective TTL** equal to the smaller of the configured TTL and the
//!   shortest freshness budget of the sources that fed it, so a memoized price
//!   can never outlive the stalest quote behind it, and
//! - the **configuration epoch** it was produced under.
//!
//! Governance changes (feeds, strategies, deviation parameters, decimals,
//! freezes) bump the epoch, which invalidates every entry in a single write
//! instead of walking the cache. Every read re-checks the epoch before
//! trusting an entry, so a memoized price can never outlive a governance
//! decision.
//!
//! Caching is opt-in: with a TTL of `0` (the default) the read path behaves
//! exactly as it did before, and governance turns it on with
//! `set_cache_ttl`.

use crate::storage::DataKey;
use crate::types::{
    AggregatedPrice, CacheServedEvent, CacheStats, CachedPrice, PriceCachedEvent, PriceSource,
    DEFAULT_CACHE_TTL_SECONDS, MAX_CACHE_TTL_SECONDS,
};
use soroban_sdk::{Bytes, Env};

/// Configured TTL in seconds. `0` means the cache is disabled.
pub fn cache_ttl(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get::<_, u64>(&DataKey::CacheTtlSeconds)
        .unwrap_or(DEFAULT_CACHE_TTL_SECONDS)
}

/// Whether a governance-supplied TTL is acceptable.
pub fn valid_ttl(ttl: u64) -> bool {
    ttl <= MAX_CACHE_TTL_SECONDS
}

/// Set the configured TTL.
pub fn set_cache_ttl(env: &Env, ttl: u64) {
    env.storage()
        .instance()
        .set(&DataKey::CacheTtlSeconds, &ttl);
}

/// Current configuration epoch.
pub fn epoch(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get::<_, u32>(&DataKey::CacheEpoch)
        .unwrap_or(0)
}

/// Invalidate every memoized price in one storage write.
///
/// Called by every governance mutation that can change what a correct price
/// is: feed registration and updates, strategy and deviation changes, decimal
/// changes, and freezes.
pub fn invalidate_all(env: &Env) {
    let next = epoch(env).saturating_add(1);
    env.storage().instance().set(&DataKey::CacheEpoch, &next);
}

/// Drop the entry for a single asset.
pub fn invalidate_asset(env: &Env, asset: &Bytes) {
    env.storage()
        .instance()
        .remove(&DataKey::CachedPrice(asset.clone()));
}

/// Stored entry for an asset, regardless of freshness.
pub fn entry(env: &Env, asset: &Bytes) -> Option<CachedPrice> {
    env.storage()
        .instance()
        .get::<_, CachedPrice>(&DataKey::CachedPrice(asset.clone()))
}

/// A fresh entry for an asset, or `None` when the cache is disabled, the
/// entry was written under an older configuration epoch, or the entry has
/// outlived its TTL.
///
/// A non-positive price is never served: the aggregator refuses to produce
/// one, so a stored non-positive value means corrupted state and counts as a
/// miss.
pub fn fresh_entry(env: &Env, asset: &Bytes, now: u64) -> Option<CachedPrice> {
    if cache_ttl(env) == 0 {
        return None;
    }
    let cached = entry(env, asset)?;
    if cached.epoch != epoch(env) || cached.price.price <= 0 {
        return None;
    }
    if now < cached.cached_at {
        return None;
    }
    if now - cached.cached_at > cached.ttl_seconds {
        return None;
    }
    Some(cached)
}

/// Store an aggregate, clamped to the freshness budget of its sources.
///
/// `source_budget` is the shortest `stale_threshold_seconds` among the sources
/// that fed `price`; the effective TTL is
/// `min(configured TTL, source_budget)`, so a cache entry can never extend
/// the life of the stalest quote behind it. Nothing is written when caching
/// is disabled or when the clamp leaves no window at all.
pub fn store(env: &Env, asset: &Bytes, mut price: AggregatedPrice, now: u64, source_budget: u64) {
    let ttl = cache_ttl(env);
    if ttl == 0 {
        return;
    }
    let effective = ttl.min(source_budget);
    if effective == 0 {
        return;
    }
    price.from_cache = false;
    price.source = PriceSource::Consensus;
    let stored_price = price.price;
    env.storage().instance().set(
        &DataKey::CachedPrice(asset.clone()),
        &CachedPrice {
            price,
            cached_at: now,
            ttl_seconds: effective,
            epoch: epoch(env),
        },
    );
    record_write(env);
    PriceCachedEvent {
        asset: asset.clone(),
        price: stored_price,
        ttl_seconds: effective,
    }
    .publish(env);
}

/// Rewrite a cache hit as the aggregate callers see.
pub fn as_hit(cached: &CachedPrice) -> AggregatedPrice {
    let mut price = cached.price.clone();
    price.from_cache = true;
    price.source = PriceSource::Cached;
    price
}

/// Cache counters. Reads are cheap; the counters make the hit rate
/// observable so governance can tune the TTL with data instead of guesses.
pub fn stats(env: &Env) -> CacheStats {
    env.storage()
        .instance()
        .get::<_, CacheStats>(&DataKey::CacheStats)
        .unwrap_or(CacheStats {
            hits: 0,
            misses: 0,
            writes: 0,
            pull_reads: 0,
        })
}

/// Persist updated counters.
pub fn set_stats(env: &Env, stats: &CacheStats) {
    env.storage().instance().set(&DataKey::CacheStats, stats);
}

/// Record a cache hit and publish `cache_served`.
pub fn record_hit(env: &Env, asset: &Bytes, cached: &CachedPrice, now: u64) -> AggregatedPrice {
    let mut stats = stats(env);
    stats.hits = stats.hits.saturating_add(1);
    set_stats(env, &stats);
    CacheServedEvent {
        asset: asset.clone(),
        price: cached.price.price,
        age_seconds: now.saturating_sub(cached.cached_at),
    }
    .publish(env);
    as_hit(cached)
}

/// Record a miss, flagging the ones that also paid for a provider call.
pub fn record_miss(env: &Env, pulled: bool) {
    let mut stats = stats(env);
    stats.misses = stats.misses.saturating_add(1);
    if pulled {
        stats.pull_reads = stats.pull_reads.saturating_add(1);
    }
    set_stats(env, &stats);
}

/// Record that an aggregate was written to the cache.
pub fn record_write(env: &Env) {
    let mut stats = stats(env);
    stats.writes = stats.writes.saturating_add(1);
    set_stats(env, &stats);
}
