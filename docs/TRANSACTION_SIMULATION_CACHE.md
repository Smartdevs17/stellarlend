# Transaction Simulation Cache for Pool Operations

The API simulates every pool operation (`deposit`, `borrow`, `repay`,
`withdraw`) against Soroban RPC before a user signs it, to estimate its
resource cost. Wallets and dashboards often ask for the same estimate several
times in a row: when a form re-renders, when the user edits and reverts an
amount, or when the gas estimator refreshes. Without a cache, each request is
a full `simulateTransaction` round trip.

`StellarService.estimateGas` now caches the simulation result for a short
time, so an identical request is answered from the cache.

Related: [GAS_ESTIMATION_SYSTEM.md](GAS_ESTIMATION_SYSTEM.md),
[POOL_STATE_LAZY_LOADING.md](POOL_STATE_LAZY_LOADING.md).

## How it works

- **Key.** Each entry is keyed by user, operation, asset and amount:
  `stellarlend:simulation:<user>:<operation>:<asset|native>:<amount>`. Only an
  identical request reuses a result.
- **Store.** Entries go through `redisCacheService`. That is Redis when
  `REDIS_ENABLED=true`, otherwise the in-memory fallback, so every API
  instance behind the same Redis shares the cache.
- **What is cached.** Only successful simulations
  (`cpuInstructions`, `memoryBytes`, `minResourceFee`). A failed simulation is
  never cached, so the next request simulates again.
- **Invalidation.** Entries expire after `SIMULATION_CACHE_TTL_MS`. They are
  also dropped for everyone whenever a transaction is submitted successfully
  through `POST /api/lending/submit`, together with the position, pool and
  protocol caches, because ledger state has just changed.
- **Concurrency.** Identical concurrent requests were already merged by
  `requestCoalescingService`. The cache now also covers requests that arrive
  one after another.

Every path that estimates a pool operation benefits, including
`POST /api/gas/estimate`, which calls `StellarService.estimateGas`.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SIMULATION_CACHE_TTL_MS` | `10000` (10 s) | How long a simulation result is reused. Stored in whole seconds, minimum 1 s. |

Keep the TTL short: a simulation reflects the ledger at the moment it ran. The
default of 10 seconds covers about two ledgers.

## Performance

A cache hit costs one Redis `GET` (or one in-memory lookup). It skips the
Horizon account lookup and the Soroban `simulateTransaction` call, which are
the slow part of an estimate. The hit/miss counters are included in
`redisCacheService.getMetrics()`.

## Testing

`api/src/__tests__/stellar.service.test.ts` → `estimateGas › simulation cache`
covers:

- an identical request reuses the cached result
- different parameters are simulated separately
- a failed simulation is not cached
- the cache refills after invalidation
