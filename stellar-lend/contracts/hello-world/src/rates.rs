//! # Borrow-Rate Curve Memoization (issue #1640)
//!
//! Evaluating the piecewise borrow-rate curve on every call site inside a
//! single transaction is redundant: utilization and rate-curve parameters do
//! not change between invocations within the same ledger sequence.
//!
//! This module wraps [`crate::interest_rate`] with a **per-block memo-cache**
//! backed by Soroban's `temporary` storage tier.  Temporary entries live for
//! exactly one ledger; there is no need to invalidate them manually.
//!
//! ## Cache key
//! ```text
//! RateCacheKey::BorrowRate(sequence)   → i128  (borrow rate bps)
//! RateCacheKey::SupplyRate(sequence)   → i128  (supply rate bps)
//! RateCacheKey::Utilization(sequence)  → i128  (utilization bps)
//! ```
//!
//! ## Usage
//! Replace direct calls to `interest_rate::calculate_borrow_rate(env)` with
//! `rates::get_borrow_rate(env)`.  The first call within a ledger hits the
//! hot path and stores the result; every subsequent call returns the cached
//! value with a single temporary-storage read.
//!
//! ## Observability
//! [`RateCacheMetrics`] accumulates hit/miss/error counters in temporary
//! storage.  Callers can read them via [`get_cache_metrics`] for dashboards
//! and CI assertions.

use soroban_sdk::{contracttype, Env};

use crate::interest_rate::{
    calculate_borrow_rate, calculate_supply_rate, calculate_utilization, InterestRateError,
};

// ── Storage keys ─────────────────────────────────────────────────────────────

/// Temporary-storage keys used by this module.
///
/// Each key is tagged with the current ledger `sequence` number so the entry
/// is logically scoped to a single block even before Soroban's TTL evicts it.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum RateCacheKey {
    /// Cached borrow rate for `sequence`.
    BorrowRate(u32),
    /// Cached supply rate for `sequence`.
    SupplyRate(u32),
    /// Cached utilization for `sequence`.
    Utilization(u32),
    /// Hit/miss/error counters (not keyed by sequence; accumulates forever
    /// within the transaction but is cheap — it's temporary storage).
    Metrics,
}

// ── Metrics ───────────────────────────────────────────────────────────────────

/// Lightweight hit/miss/error counters for observability.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateCacheMetrics {
    /// Number of times a cached value was returned without recomputing.
    pub hits: u32,
    /// Number of times the cache was cold and the curve had to be evaluated.
    pub misses: u32,
    /// Number of times the underlying rate computation returned an error.
    pub errors: u32,
}

impl RateCacheMetrics {
    fn zero() -> Self {
        RateCacheMetrics {
            hits: 0,
            misses: 0,
            errors: 0,
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn load_metrics(env: &Env) -> RateCacheMetrics {
    env.storage()
        .temporary()
        .get::<RateCacheKey, RateCacheMetrics>(&RateCacheKey::Metrics)
        .unwrap_or_else(RateCacheMetrics::zero)
}

fn save_metrics(env: &Env, m: &RateCacheMetrics) {
    env.storage()
        .temporary()
        .set(&RateCacheKey::Metrics, m);
}

fn record_hit(env: &Env) {
    let mut m = load_metrics(env);
    m.hits = m.hits.saturating_add(1);
    save_metrics(env, &m);
}

fn record_miss(env: &Env) {
    let mut m = load_metrics(env);
    m.misses = m.misses.saturating_add(1);
    save_metrics(env, &m);
}

fn record_error(env: &Env) {
    let mut m = load_metrics(env);
    m.errors = m.errors.saturating_add(1);
    save_metrics(env, &m);
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Return the borrow rate (bps) for the current ledger, using the memo-cache.
///
/// # Cache behaviour
/// - **Hit**: ledger sequence unchanged since the last call → returns stored
///   value, increments `hits`.
/// - **Miss**: first call this ledger → evaluates the full curve, stores the
///   result in temporary storage, increments `misses`.
/// - **Error**: curve evaluation fails → propagates the error, increments
///   `errors`.
pub fn get_borrow_rate(env: &Env) -> Result<i128, InterestRateError> {
    let seq = env.ledger().sequence();
    let key = RateCacheKey::BorrowRate(seq);

    if let Some(cached) = env.storage().temporary().get::<RateCacheKey, i128>(&key) {
        record_hit(env);
        return Ok(cached);
    }

    record_miss(env);
    let rate = calculate_borrow_rate(env).map_err(|e| {
        record_error(env);
        e
    })?;
    env.storage().temporary().set(&key, &rate);
    Ok(rate)
}

/// Return the supply rate (bps) for the current ledger, using the memo-cache.
pub fn get_supply_rate(env: &Env) -> Result<i128, InterestRateError> {
    let seq = env.ledger().sequence();
    let key = RateCacheKey::SupplyRate(seq);

    if let Some(cached) = env.storage().temporary().get::<RateCacheKey, i128>(&key) {
        record_hit(env);
        return Ok(cached);
    }

    record_miss(env);
    let rate = calculate_supply_rate(env).map_err(|e| {
        record_error(env);
        e
    })?;
    env.storage().temporary().set(&key, &rate);
    Ok(rate)
}

/// Return the utilization (bps) for the current ledger, using the memo-cache.
pub fn get_utilization(env: &Env) -> Result<i128, InterestRateError> {
    let seq = env.ledger().sequence();
    let key = RateCacheKey::Utilization(seq);

    if let Some(cached) = env.storage().temporary().get::<RateCacheKey, i128>(&key) {
        record_hit(env);
        return Ok(cached);
    }

    record_miss(env);
    let util = calculate_utilization(env).map_err(|e| {
        record_error(env);
        e
    })?;
    env.storage().temporary().set(&key, &util);
    Ok(util)
}

/// Explicitly warm up all three cache entries for the current ledger in one
/// pass.  Call this at the top of any multi-step operation (e.g. a batch
/// liquidation) to ensure every downstream call within the same transaction
/// is a cache hit.
///
/// Returns `(borrow_rate, supply_rate, utilization)` in bps.
pub fn prime_cache(env: &Env) -> Result<(i128, i128, i128), InterestRateError> {
    let borrow = get_borrow_rate(env)?;
    let supply = get_supply_rate(env)?;
    let util = get_utilization(env)?;
    Ok((borrow, supply, util))
}

/// Invalidate any cached rate entries for the current ledger sequence.
///
/// Call this after an admin operation that changes curve parameters
/// mid-transaction (rare, but possible in test harnesses).  Under normal
/// protocol flow the cache auto-expires each ledger, so this is a no-op.
pub fn invalidate_cache(env: &Env) {
    let seq = env.ledger().sequence();
    env.storage()
        .temporary()
        .remove(&RateCacheKey::BorrowRate(seq));
    env.storage()
        .temporary()
        .remove(&RateCacheKey::SupplyRate(seq));
    env.storage()
        .temporary()
        .remove(&RateCacheKey::Utilization(seq));
}

/// Read the current hit/miss/error counters without modifying them.
pub fn get_cache_metrics(env: &Env) -> RateCacheMetrics {
    load_metrics(env)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Ledger as _;
    use soroban_sdk::Env;

    use crate::deposit::{DepositDataKey, ProtocolAnalytics};
    use crate::interest_rate::{
        initialize_interest_rate_config, InterestRateDataKey,
        InterestRateConfig, InterestRateModelKind,
    };
    use crate::admin;

    // ── Helpers ───────────────────────────────────────────────────────────

    /// Bootstrap the minimum state the rate-curve needs to compute a result.
    fn setup(env: &Env) {
        use soroban_sdk::testutils::Address as _;
        let admin_addr = soroban_sdk::Address::generate(env);
        admin::set_admin(env, admin_addr.clone(), None).unwrap();
        initialize_interest_rate_config(env, admin_addr).unwrap();

        // Seed analytics so utilization is non-zero (50%)
        env.storage().persistent().set(
            &DepositDataKey::ProtocolAnalytics,
            &ProtocolAnalytics {
                total_deposits: 10_000,
                total_borrows: 5_000,
                total_value_locked: 10_000,
            },
        );
    }

    // ── Happy path ────────────────────────────────────────────────────────

    #[test]
    fn first_call_is_a_miss_and_warms_cache() {
        let env = Env::default();
        setup(&env);

        let rate = get_borrow_rate(&env).expect("borrow rate");
        assert!(rate > 0, "rate should be positive at 50% utilization");

        let metrics = get_cache_metrics(&env);
        assert_eq!(metrics.misses, 1, "first call must be a miss");
        assert_eq!(metrics.hits, 0);
        assert_eq!(metrics.errors, 0);
    }

    #[test]
    fn second_call_same_sequence_is_a_hit() {
        let env = Env::default();
        setup(&env);

        let rate1 = get_borrow_rate(&env).unwrap();
        let rate2 = get_borrow_rate(&env).unwrap();

        assert_eq!(rate1, rate2, "cached value must equal computed value");

        let metrics = get_cache_metrics(&env);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.hits, 1, "second call must be a hit");
    }

    #[test]
    fn supply_and_utilization_caches_are_independent() {
        let env = Env::default();
        setup(&env);

        get_borrow_rate(&env).unwrap();  // miss
        get_supply_rate(&env).unwrap();  // miss
        get_utilization(&env).unwrap();  // miss

        get_borrow_rate(&env).unwrap();  // hit
        get_supply_rate(&env).unwrap();  // hit
        get_utilization(&env).unwrap();  // hit

        let m = get_cache_metrics(&env);
        assert_eq!(m.misses, 3);
        assert_eq!(m.hits, 3);
    }

    #[test]
    fn prime_cache_returns_all_three_values_and_warms_all_entries() {
        let env = Env::default();
        setup(&env);

        let (b, s, u) = prime_cache(&env).unwrap();
        assert!(b > 0);
        assert!(s > 0);
        assert!(u > 0);

        // All subsequent single-value calls should be hits.
        get_borrow_rate(&env).unwrap();
        get_supply_rate(&env).unwrap();
        get_utilization(&env).unwrap();

        let m = get_cache_metrics(&env);
        // prime_cache: 3 misses (each function sees cold cache)
        // 3 explicit follow-up calls: 3 hits
        assert_eq!(m.misses, 3);
        assert_eq!(m.hits, 3);
    }

    #[test]
    fn new_ledger_sequence_causes_cache_miss() {
        let env = Env::default();
        setup(&env);

        get_borrow_rate(&env).unwrap(); // miss at sequence N

        // Advance to the next ledger.
        env.ledger().with_mut(|li| {
            li.sequence_number += 1;
        });

        get_borrow_rate(&env).unwrap(); // miss at sequence N+1

        let m = get_cache_metrics(&env);
        assert_eq!(m.misses, 2, "each new sequence must be a fresh miss");
    }

    #[test]
    fn invalidate_cache_forces_next_call_to_miss() {
        let env = Env::default();
        setup(&env);

        get_borrow_rate(&env).unwrap(); // miss
        get_borrow_rate(&env).unwrap(); // hit

        invalidate_cache(&env);

        get_borrow_rate(&env).unwrap(); // miss again after invalidation

        let m = get_cache_metrics(&env);
        assert_eq!(m.misses, 2);
        assert_eq!(m.hits, 1);
    }

    // ── Failure modes ─────────────────────────────────────────────────────

    #[test]
    fn error_increments_error_counter_and_does_not_populate_cache() {
        let env = Env::default();
        // Deliberately skip setup() so there is no InterestRateConfig in
        // storage — calculate_borrow_rate will return InvalidParameter.

        let result = get_borrow_rate(&env);
        assert!(result.is_err(), "must fail without config");

        let m = get_cache_metrics(&env);
        assert_eq!(m.errors, 1, "error counter must be incremented");
        assert_eq!(m.misses, 1, "miss counted before error propagated");
        assert_eq!(m.hits, 0);

        // Cache must be cold — another call must also be a miss (and error).
        let _ = get_borrow_rate(&env);
        let m2 = get_cache_metrics(&env);
        assert_eq!(m2.errors, 2);
        assert_eq!(m2.misses, 2);
    }

    #[test]
    fn cache_returns_zero_utilization_when_no_deposits() {
        let env = Env::default();
        use soroban_sdk::testutils::Address as _;
        let admin_addr = soroban_sdk::Address::generate(&env);
        admin::set_admin(&env, admin_addr.clone(), None).unwrap();
        initialize_interest_rate_config(&env, admin_addr).unwrap();
        // No analytics stored → utilization defaults to 0.

        let u = get_utilization(&env).unwrap();
        assert_eq!(u, 0, "empty pool must have 0% utilization");

        let m = get_cache_metrics(&env);
        assert_eq!(m.misses, 1);
        assert_eq!(m.hits, 0);
    }

    #[test]
    fn metrics_saturate_instead_of_overflow() {
        // Ensure saturating_add protects the counters under extreme use.
        let m = RateCacheMetrics {
            hits: u32::MAX,
            misses: u32::MAX,
            errors: u32::MAX,
        };
        // These should not panic.
        let _ = m.hits.saturating_add(1);
        let _ = m.misses.saturating_add(1);
        let _ = m.errors.saturating_add(1);
    }
}
