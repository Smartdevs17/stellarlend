# Query plan review

Tracks issue #1381. The review is static: it compares the indexes in `api/db/init`
and `api/prisma/schema.prisma` against the access patterns in the API code. No
`EXPLAIN` was run, because no database was available for this change. The
checks at the end should be run against a populated database before this is
treated as verified.

## Access patterns reviewed

| Table | Access pattern (source) | Index that serves it | Decision |
| --- | --- | --- | --- |
| `pool_performance_events` | `listEvents(poolAddress)` filters by pool (`analytics/pool-performance.ts`) | `idx_pool_perf_events_pool (pool_address, recorded_at DESC)` | Keep |
| `pool_performance_events` | `listEvents()` with no pool: all pools, no index on time alone | none | **Added** `idx_pool_perf_events_recorded_at (recorded_at DESC)` |
| `pool_performance_benchmarks` | `WHERE pool_address = ? AND period = ?` | `idx_pool_perf_bench_pool (pool_address, period)` | Keep |
| `asset_price_history` | lookups by `asset`, with `recorded_at` ordering | `idx_price_history_asset_time (asset, recorded_at DESC)` | **Dropped** `idx_price_history_asset (asset)`, a prefix of the composite |
| `position_risk_data` | lookups by `address` | `idx_position_risk_address_asset (address, asset)` | **Dropped** `idx_position_risk_address (address)`, a prefix of the composite |
| `pool_snapshots` (Prisma only) | `snapshotsInPeriod`: pool plus time cutoff | `@@index([poolAddress, snapshotAt])` | Keep. The table has no SQL definition in `api/db/init`, see below |

Dropping a prefix index removes write cost on every insert. It does not change
which queries can use an index, because the composite index serves them.

## Known gaps not addressed here

- **`pool_snapshots` has no SQL definition.** It exists only in
  `api/prisma/schema.prisma`. A deployment that initialises from
  `api/db/init` does not get this table, so the Prisma index does not apply
  there.
- **`comparePools` reads snapshots one pool at a time** (`getPoolSnapshots`
  inside a loop, see `services/poolPerformance.service.ts`). That is an N+1
  pattern. It is an application change, not an index change, and is left for a
  follow-up.
- **Existing deployments do not re-run init scripts.** The Docker init
  directory only runs on first boot. The `DROP INDEX` and `CREATE INDEX` in
  `09_query_plan_indexes.sql` must be applied to an existing database by hand.

## Checks to run against a populated database

```sql
EXPLAIN ANALYZE
SELECT * FROM pool_performance_events
WHERE recorded_at >= NOW() - INTERVAL '30 days'
ORDER BY recorded_at DESC
LIMIT 100;

EXPLAIN ANALYZE
SELECT * FROM asset_price_history
WHERE asset = 'XLM'
ORDER BY recorded_at DESC
LIMIT 100;

EXPLAIN ANALYZE
SELECT * FROM position_risk_data
WHERE address = 'G...';
```

Expected: the first query uses `idx_pool_perf_events_recorded_at`, and the second
and third use the composite indexes. None of them should show a sequential scan
on a table of realistic size.
