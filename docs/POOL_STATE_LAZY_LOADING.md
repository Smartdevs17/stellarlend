# Lazy Pool State: On-Demand Loading, Caching, and Monitoring

This document describes the lazy pool-state architecture introduced for the
StellarLend protocol on Soroban. It is the reference for how pool snapshots are
built, cached, invalidated, and monitored, and it documents the performance
targets and benchmark story for the feature.

Related: [storage.md](storage.md), [gas-benchmarks.md](gas-benchmarks.md).

## Problem

Pool-level state (aggregate supply, borrow, utilization, health, and
cross-asset metrics) is needed by most views and by the liquidator. Eagerly
recomputing it on every mutation is expensive: every deposit, borrow, repay, or
health check would pay for rebuilding the full aggregate even when nothing else
reads it. The previous design recomputed the snapshot unconditionally and
stored it under a single key, so reads were cheap but writes were asymptotically
costly and the value went stale across unrelated mutations.

## Design

Pool state is **lazy**:

1. **On-demand construction** — the snapshot is only built when it is first
   requested (`pool_state::load`), not on every mutation.
2. **Epoch-keyed caching** — snapshots are cached under a *temporary* key that
   includes the current epoch:
   `PoolStateTempKey::Snapshot(pool, epoch)`. Temporary entries survive only for
   the current ledger run, so a snapshot is valid for the epoch in which it was
   produced and is never stale across epoch boundaries.
3. **Explicit invalidation** — mutations that change aggregate values call
   `pool_state::bump_epoch`, which advances `PoolStateKey::Epoch`. Reads fall
   back to rebuilding when the cached entry's epoch does not match. Targeted
   invalidation of a single pool is available via `pool_state::invalidate`.
4. **Initialization marker** — `PoolStateKey::Initialized(pool)` records that a
   pool has been initialized; `default_pool_state_loader` presets safe defaults
   for uninitialized pools so that `get_pool_state` never panics on a fresh or
   partially-initialized pool.

### Contract surface

All state lives in `contracts/hello-world/src/pool_state.rs` with storage keys
in `contracts/hello-world/src/storage.rs`:

| Key (Type) | Storage | Value |
|------------|---------|-------|
| `PoolStateKey::Epoch` | persistent | `u64` |
| `PoolStateKey::Initialized(pool)` | persistent | `bool` |
| `PoolStateKey::Metrics` | persistent | `PoolStateMetrics` |
| `PoolStateTempKey::Snapshot(pool, epoch)` | temporary | `PoolStateSnapshot` |

Entrypoints (`contracts/hello-world/src/lib.rs`):

- `get_pool_state(env, asset) -> PoolStateSnapshot` — build or return the
  cached snapshot.
- `get_multiple_pool_states(env, assets: Vec<Option<Address>>) -> Vec<PoolStateSnapshot>`.
- `is_pool_state_initialized(env, asset) -> bool`.
- `get_pool_state_epoch(env) -> u64`.
- `get_pool_state_metrics(env) -> PoolStateMetrics`.
- `invalidate_pool_state(env)` — advance the epoch (admin-gated).

Epoch bumping is wired into the mutation paths that change aggregates (the
`bump_epoch`/`invalidate` hooks in `lib.rs`, e.g. the borrow/repay/withdrawal
and pool-management flows), keeping cache coherence without a central scheduler.

## API service

The API layer mirrors the contract with its own bounded cache in
`api/src/services/stellar.service.ts`, keyed constants `NATIVE_POOL_KEY` and
`POOL_STATE_EPOCH_KEY`:

- `getPoolState` / `getMultiplePoolStates` — reads with response coalescing.
- `getPoolStateEpoch` — cached epoch used for request fencing.
- `invalidatePoolStateCache` — clears the in-memory and Redis caches.
- `getPoolStateCacheMetrics` — hit rate and cost observability.

The contract epoch is the source of truth; the service cache is best-effort and
always fallible, so the UI remains correct even when caching is bypassed.

## Consistency

- Snapshot identity is `(pool, epoch)`. Any write that should invalidate a
  snapshot advances the epoch before subsequent reads see the new value.
- Temporary storage guarantees cross-ledger freshness: a snapshot cached in a
  previous run is not used, satisfying the "never stale across ledgers" rule
  without extra bookkeeping.
- Deeper consistency and reentrancy guarantees are documented in
  [REENTRANCY_GUARANTEES.md](../stellar-lend/docs/REENTRANCY_GUARANTEES.md).

## Performance target: <50 ms

The acceptance target for a single `get_pool_state` call is **under 50
milliseconds** of CPU time, including the cold-miss rebuild path.

Evidence strategy (benchmarks/):

- `stellar-lend/benchmarks/src/pool_state_benchmarks.rs` exercises cold
  (uncached) load, warm cached load, invalidation-then-reload, and metric reads,
  reporting instruction counts via the host cost estimator
  (`framework.rs`).
- The elapsed-time runner measures wall-clock latency so the <50 ms constraint
  is checked directly, not only via instruction counts.

### Current status / blocker

The benchmark runner currently cannot build: hello-world fails to compile when
the workspace enables the soroban-sdk `testutils` feature (which any contract-
driving harness requires). `#[contracttype]` under testutils generates
struct→`ScVal` conversions that need a plain `From<T> for ScVal` for every
field, which SDK 27 does not provide for custom types or raw `Val` fields. The
affected contracttypes are:

- `contracts/hello-world/src/storage.rs:3` — `SnapshotValue { value: Val, .. }`
- `contracts/hello-world/src/rate_limiter.rs:134` — `CongestionState` with
  `Option<CongestionReport>` / `Option<LedgerIntervalSample>`
- `contracts/hello-world/src/mev_protection.rs:70` — `PendingCommit` with
  `Option<Address>`

This is a pre-existing upstream issue, unrelated to pool-state itself, and is
tracked as a follow-up. Once resolved, running
`cargo run --bin run_benchmarks -- pool_state` produces the load numbers and
the elapsed-time assertion (`<50 ms`) is evaluated by the runner. Until then,
the margin argument rests on the instruction-count scale of the load path: a
single-pool snapshot touches a bounded set of keys and aggregates, orders of
magnitude below Soroban's per-invocation instruction ceiling, and cached reads
are a single temporary-storage lookup.

## Monitoring

`PoolStateMetrics` (persisted under `PoolStateKey::Metrics`) aggregates hit/miss
and rebuild counters across calls. `get_pool_state_metrics` exposes them
on-chain, and the API service exposes cache hit rate via
`getPoolStateCacheMetrics`. Existing dashboards may consume either endpoint; see
[RISK_MONITORING_DASHBOARD.md](RISK_MONITORING_DASHBOARD.md).

## Deployment notes

The feature ships behind the existing upgrade mechanism (`docs/upgrade-
mechanism.md`): a contract upgrade carries the new `pool_state.rs` module; no
data migration is needed because epoch and initialized markers are new keys and
the old snapshot key(s) are not read after upgrade.