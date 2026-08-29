//! # Lazy Pool State Module (#721)
//!
//! ## Motivation
//!
//! Pool-level state used to be eagerly materialized: every dependent
//! sub-system (interest-rate config, risk parameters, liquidity analytics,
//! reserves) was written at `initialize` time, and every consumer that wanted
//! a consolidated view had to walk each of those independent storage entries
//! itself. That made both startup and reads slower than necessary.
//!
//! This module replaces that with a lazily-built, cached aggregate:
//!
//! * **Lazy initialization** – the consolidated [`PoolStateSnapshot`] for a
//!   pool is only materialized the first time it is actually requested through
//!   [`load`]. Nothing pool-state-related is written during contract
//!   `initialize`.
//! * **On-demand loading** – individual components are resolved only while a
//!   snapshot is being built, never ahead of time. Missing sub-config falls
//!   back to protocol defaults instead of erroring.
//! * **Caching** – a resolved snapshot is memoised in short-lived
//!   (`temporary`) storage keyed by `(pool, epoch)`, so repeated reads inside
//!   the cache window skip the rebuild entirely.
//! * **Invalidation** – a global monotonic *epoch* is bumped by every mutation
//!   that can change a snapshot (see [`bump_epoch`]). Because the epoch is part
//!   of the cache key, stale entries are ignored automatically; [`invalidate`]
//!   additionally lets an admin force a rebuild for one pool.
//! * **Consistency guarantees** – a snapshot is always internally consistent:
//!   every component is read within the same [`load`] call against a single
//!   `epoch`, and the `epoch` is embedded in the returned value so callers can
//!   detect staleness. A snapshot is never partially updated in place.
//! * **Monitoring** – hit / miss / rebuild / invalidation counters are kept in
//!   persistent storage and exposed through [`metrics`].
//!
//! ## Storage layout
//!
//! | Key | Storage | Value |
//! | --- | --- | --- |
//! | `PoolStateKey::Epoch` | persistent | `u64` |
//! | `PoolStateKey::Initialized(pool)` | persistent | `bool` |
//! | `PoolStateKey::Metrics` | persistent | [`PoolStateMetrics`] |
//! | `PoolStateTempKey::Snapshot(pool, epoch)` | temporary | [`PoolStateSnapshot`] |

use soroban_sdk::{contracttype, Address, Env};

use crate::storage::{PoolStateKey, PoolStateTempKey};

/// Number of ledgers a cached snapshot is kept alive for (~20 min at 5s/ledger).
const CACHE_TTL_LEDGERS: u32 = 240;
/// TTL floor before the cache entry is refreshed.
const CACHE_TTL_THRESHOLD: u32 = 120;

/// Basis-point default fallbacks, mirroring the module defaults used by
/// `interest_rate` / `risk_params` when their config has not been written yet.
const DEFAULT_BORROW_RATE_BPS: i128 = 100;
const DEFAULT_SPREAD_BPS: i128 = 200;
const DEFAULT_MIN_COLLATERAL_RATIO_BPS: i128 = 11_000;
const DEFAULT_LIQUIDATION_THRESHOLD_BPS: i128 = 10_500;
const DEFAULT_CLOSE_FACTOR_BPS: i128 = 5_000;
const DEFAULT_LIQUIDATION_INCENTIVE_BPS: i128 = 1_000;
const INDEX_SCALE: i128 = 1_000_000_000_000;

/// Cache-hit / miss / rebuild / invalidation counters for lazy pool-state loads.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolStateMetrics {
    /// Snapshot requests served from the cache.
    pub hits: u64,
    /// Snapshot requests that missed the cache and triggered a rebuild.
    pub misses: u64,
    /// Total snapshots materialized on demand.
    pub rebuilds: u64,
    /// Explicit `invalidate` calls performed.
    pub invalidations: u64,
    /// Number of distinct pools lazily initialized so far.
    pub pools_initialized: u64,
    /// Current global epoch.
    pub epoch: u64,
}

impl PoolStateMetrics {
    fn zero() -> Self {
        Self {
            hits: 0,
            misses: 0,
            rebuilds: 0,
            invalidations: 0,
            pools_initialized: 0,
            epoch: 0,
        }
    }
}

/// Consolidated, internally-consistent view of a single pool's on-chain state.
///
/// Every field is resolved within one [`load`] call against a single `epoch`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolStateSnapshot {
    /// Pool identifier (`None` = native XLM pool).
    pub pool: Option<Address>,
    /// Epoch this snapshot was built against.
    pub epoch: u64,
    /// Ledger timestamp at which the snapshot was materialized.
    pub built_at: u64,
    /// `true` when this `load` call was the one that first materialized the
    /// pool (i.e. lazy initialization happened here).
    pub lazily_initialized: bool,

    // ── Interest-rate component ───────────────────────────────────────────
    /// Current borrow rate (basis points / year).
    pub borrow_rate_bps: i128,
    /// Current supply rate (basis points / year).
    pub supply_rate_bps: i128,
    /// Current utilization (basis points).
    pub utilization_bps: i128,
    /// Global compound borrow index (scaled by 1e12).
    pub borrow_index: i128,
    /// Global compound supply index (scaled by 1e12).
    pub supply_index: i128,

    // ── Risk component ───────────────────────────────────────────────────
    /// Minimum collateral ratio (basis points).
    pub min_collateral_ratio_bps: i128,
    /// Liquidation threshold (basis points).
    pub liquidation_threshold_bps: i128,
    /// Close factor (basis points).
    pub close_factor_bps: i128,
    /// Liquidation incentive (basis points).
    pub liquidation_incentive_bps: i128,

    // ── Liquidity component ──────────────────────────────────────────────
    /// Total deposits across the protocol.
    pub total_deposits: i128,
    /// Total borrows across the protocol.
    pub total_borrows: i128,
    /// Total value locked.
    pub total_value_locked: i128,
    /// Deposits not currently lent out (`total_deposits - total_borrows`, floored at 0).
    pub available_liquidity: i128,

    // ── Reserve component ────────────────────────────────────────────────
    /// Accrued protocol reserve balance for the pool asset.
    pub reserve_balance: i128,
    /// Reserve factor for the pool asset (basis points).
    pub reserve_factor_bps: i128,
}

// ─── Epoch / invalidation ────────────────────────────────────────────────────

/// Return the current global pool-state epoch (0 before the first mutation).
pub fn current_epoch(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get::<PoolStateKey, u64>(&PoolStateKey::Epoch)
        .unwrap_or(0)
}

/// Bump the global epoch, invalidating every cached snapshot.
///
/// Called by public entrypoints after any mutation that can change a resolved
/// snapshot (risk params, interest-rate config, reserve factor, …). Returns the
/// new epoch.
pub fn bump_epoch(env: &Env) -> u64 {
    let next = current_epoch(env).saturating_add(1);
    env.storage()
        .persistent()
        .set(&PoolStateKey::Epoch, &next);
    let mut m = metrics(env);
    m.epoch = next;
    save_metrics(env, &m);
    next
}

/// Force a rebuild of one pool's snapshot on next access (admin-triggered).
///
/// Implemented as a global epoch bump plus an invalidation counter tick: the
/// stale entry for `_pool` (and every other pool) is dropped from the cache key
/// space and rebuilt lazily on the next [`load`].
pub fn invalidate(env: &Env, _pool: &Option<Address>) {
    let next = bump_epoch(env);
    let mut m = metrics(env);
    m.invalidations = m.invalidations.saturating_add(1);
    m.epoch = next;
    save_metrics(env, &m);
}

// ─── Lazy load ───────────────────────────────────────────────────────────────

/// Load the consolidated snapshot for `pool`, serving from cache when possible.
///
/// The first call for a given pool lazily materializes its `Initialized`
/// marker; subsequent calls within the cache window and epoch are served
/// without rebuilding.
pub fn load(env: &Env, pool: &Option<Address>) -> PoolStateSnapshot {
    let epoch = current_epoch(env);
    let cache_key = PoolStateTempKey::Snapshot(pool.clone(), epoch);

    if let Some(cached) = env
        .storage()
        .temporary()
        .get::<PoolStateTempKey, PoolStateSnapshot>(&cache_key)
    {
        env.storage().temporary().extend_ttl(
            &cache_key,
            CACHE_TTL_THRESHOLD,
            CACHE_TTL_LEDGERS,
        );
        record_hit(env);
        return cached;
    }

    record_miss(env);
    let just_initialized = mark_initialized(env, pool);
    let snapshot = build(env, pool, epoch, just_initialized);

    env.storage().temporary().set(&cache_key, &snapshot);
    env.storage().temporary().extend_ttl(
        &cache_key,
        CACHE_TTL_THRESHOLD,
        CACHE_TTL_LEDGERS,
    );

    snapshot
}

/// Whether `pool` has been lazily materialized at least once.
pub fn is_initialized(env: &Env, pool: &Option<Address>) -> bool {
    env.storage()
        .persistent()
        .get::<PoolStateKey, bool>(&PoolStateKey::Initialized(pool.clone()))
        .unwrap_or(false)
}

/// Read the current monitoring counters.
pub fn metrics(env: &Env) -> PoolStateMetrics {
    env.storage()
        .persistent()
        .get::<PoolStateKey, PoolStateMetrics>(&PoolStateKey::Metrics)
        .unwrap_or_else(PoolStateMetrics::zero)
}

// ─── Internals ───────────────────────────────────────────────────────────────

/// Resolve every sub-component on demand and assemble a consistent snapshot.
fn build(
    env: &Env,
    pool: &Option<Address>,
    epoch: u64,
    lazily_initialized: bool,
) -> PoolStateSnapshot {
    let mut m = metrics(env);
    m.rebuilds = m.rebuilds.saturating_add(1);
    save_metrics(env, &m);

    // Interest-rate component — fall back to protocol defaults when the
    // interest config / rate calculation is not available yet.
    let ir_config = crate::interest_rate::get_interest_rate_config(env);
    let (default_borrow, default_spread) = match &ir_config {
        Some(c) => (c.base_rate_bps, c.spread_bps),
        None => (DEFAULT_BORROW_RATE_BPS, DEFAULT_SPREAD_BPS),
    };
    let borrow_rate_bps =
        crate::interest_rate::get_current_borrow_rate(env).unwrap_or(default_borrow);
    let supply_rate_bps = crate::interest_rate::get_current_supply_rate(env)
        .unwrap_or((borrow_rate_bps - default_spread).max(0));
    let utilization_bps = crate::interest_rate::get_current_utilization(env).unwrap_or(0);
    let index = crate::interest_rate::get_lending_index(env);

    // Risk component.
    let risk = crate::risk_params::get_risk_params(env);
    let (min_cr, liq_threshold, close_factor, liq_incentive) = match risk {
        Some(r) => (
            r.min_collateral_ratio,
            r.liquidation_threshold,
            r.close_factor,
            r.liquidation_incentive,
        ),
        None => (
            DEFAULT_MIN_COLLATERAL_RATIO_BPS,
            DEFAULT_LIQUIDATION_THRESHOLD_BPS,
            DEFAULT_CLOSE_FACTOR_BPS,
            DEFAULT_LIQUIDATION_INCENTIVE_BPS,
        ),
    };

    // Liquidity component.
    let analytics = crate::deposit::get_protocol_analytics(env);
    let available_liquidity = analytics
        .total_deposits
        .checked_sub(analytics.total_borrows)
        .unwrap_or(0)
        .max(0);

    // Reserve component.
    let reserve_balance = crate::reserve::get_reserve_balance(env, pool.clone());
    let reserve_factor_bps = crate::reserve::get_reserve_factor(env, pool.clone());

    PoolStateSnapshot {
        pool: pool.clone(),
        epoch,
        built_at: env.ledger().timestamp(),
        lazily_initialized,
        borrow_rate_bps,
        supply_rate_bps,
        utilization_bps,
        borrow_index: if index.borrow_index == 0 {
            INDEX_SCALE
        } else {
            index.borrow_index
        },
        supply_index: if index.supply_index == 0 {
            INDEX_SCALE
        } else {
            index.supply_index
        },
        min_collateral_ratio_bps: min_cr,
        liquidation_threshold_bps: liq_threshold,
        close_factor_bps: close_factor,
        liquidation_incentive_bps: liq_incentive,
        total_deposits: analytics.total_deposits,
        total_borrows: analytics.total_borrows,
        total_value_locked: analytics.total_value_locked,
        available_liquidity,
        reserve_balance,
        reserve_factor_bps,
    }
}

/// Record the first materialization of `pool`. Returns `true` when this call
/// performed the lazy initialization.
fn mark_initialized(env: &Env, pool: &Option<Address>) -> bool {
    let key = PoolStateKey::Initialized(pool.clone());
    if env
        .storage()
        .persistent()
        .get::<PoolStateKey, bool>(&key)
        .unwrap_or(false)
    {
        return false;
    }
    env.storage().persistent().set(&key, &true);
    let mut m = metrics(env);
    m.pools_initialized = m.pools_initialized.saturating_add(1);
    save_metrics(env, &m);
    true
}

fn record_hit(env: &Env) {
    let mut m = metrics(env);
    m.hits = m.hits.saturating_add(1);
    save_metrics(env, &m);
}

fn record_miss(env: &Env) {
    let mut m = metrics(env);
    m.misses = m.misses.saturating_add(1);
    save_metrics(env, &m);
}

fn save_metrics(env: &Env, m: &PoolStateMetrics) {
    env.storage().persistent().set(&PoolStateKey::Metrics, m);
}
