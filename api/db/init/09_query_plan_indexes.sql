-- Query plan review (issue #1381)
--
-- Index changes chosen from the access patterns the API uses against these
-- tables. See api/docs/query-plan-review.md for the reviewed queries and the
-- EXPLAIN checks to run against a populated database.

-- Redundant prefix index: `idx_price_history_asset_time (asset, recorded_at DESC)`
-- already serves every lookup by `asset` alone, so this one only adds write cost.
DROP INDEX IF EXISTS idx_price_history_asset;

-- Redundant prefix index: `idx_position_risk_address_asset (address, asset)`
-- already serves every lookup by `address` alone.
DROP INDEX IF EXISTS idx_position_risk_address;

-- Unfiltered event feed. `idx_pool_perf_events_pool (pool_address, recorded_at DESC)`
-- only helps when a pool is named; a feed across all pools has to start from time.
CREATE INDEX IF NOT EXISTS idx_pool_perf_events_recorded_at
    ON pool_performance_events (recorded_at DESC);
