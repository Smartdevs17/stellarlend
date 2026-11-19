-- Migration: 20261119000000_pool_utilization_timeseries_mv.sql
-- Issue #1650: Materialised view for pool-utilization time series.
--
-- The `pool_snapshots` table is written by the hourly cron job
-- (api/src/jobs/poolSnapshot.job.ts) and already carries utilization,
-- TVL, borrow/supply APY, and total borrows/deposits per pool.
-- A full table scan over that growing table on every dashboard request
-- is the measured tail-latency source; this view pre-aggregates the
-- data into hourly buckets so the hot path becomes an indexed seek.
--
-- Refresh strategy
-- ─────────────────
-- CONCURRENTLY is used so reads are never blocked.  It requires the
-- UNIQUE index on (pool_address, bucket) created below.  The throttled
-- trigger (last section) keeps the view warm: it fires AFTER INSERT on
-- pool_snapshots but only actually refreshes when the view has not been
-- refreshed in the last hour, matching the hourly snapshot cadence.

-- ── 1. Materialized view ─────────────────────────────────────────────────────

CREATE MATERIALIZED VIEW IF NOT EXISTS pool_utilization_timeseries AS
SELECT
    pool_address,
    date_trunc('hour', snapshot_at)                           AS bucket,
    AVG(utilization)                                          AS avg_utilization,
    MIN(utilization)                                          AS min_utilization,
    MAX(utilization)                                          AS max_utilization,
    PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY utilization)  AS median_utilization,
    STDDEV_POP(utilization)                                   AS stddev_utilization,
    AVG(tvl)                                                  AS avg_tvl,
    AVG(total_borrows)                                        AS avg_total_borrows,
    AVG(total_deposits)                                       AS avg_total_deposits,
    AVG(borrow_apy)                                           AS avg_borrow_apy,
    AVG(supply_apy)                                           AS avg_supply_apy,
    COUNT(*)                                                  AS sample_count
FROM pool_snapshots
GROUP BY pool_address, date_trunc('hour', snapshot_at)
WITH DATA;

-- ── 2. Indexes ───────────────────────────────────────────────────────────────

-- Primary lookup: pool + time range (also satisfies the CONCURRENTLY constraint)
CREATE UNIQUE INDEX IF NOT EXISTS idx_pool_util_ts_pool_bucket
    ON pool_utilization_timeseries (pool_address, bucket DESC);

-- Global time-range scans (cross-pool dashboards)
CREATE INDEX IF NOT EXISTS idx_pool_util_ts_bucket
    ON pool_utilization_timeseries (bucket DESC);

-- High-utilization alert queries: fast filter without reading all rows
CREATE INDEX IF NOT EXISTS idx_pool_util_ts_avg_util
    ON pool_utilization_timeseries (avg_utilization DESC);

-- ── 3. Refresh function ──────────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION refresh_pool_utilization_timeseries()
RETURNS VOID
LANGUAGE plpgsql
AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY pool_utilization_timeseries;
END;
$$;

-- ── 4. Throttled auto-refresh trigger ────────────────────────────────────────
-- Fires AFTER INSERT on pool_snapshots (once per statement, not per row).
-- Checks when the view was last refreshed; only calls the refresh function
-- if more than 55 minutes have elapsed (slightly under the 1-hour snapshot
-- cadence to avoid missing any bucket boundary).  The timestamp is tracked
-- in the `pool_mv_refresh_log` control table created below.

CREATE TABLE IF NOT EXISTS pool_mv_refresh_log (
    view_name        TEXT        PRIMARY KEY,
    last_refreshed   TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01T00:00:00Z'
);

INSERT INTO pool_mv_refresh_log (view_name, last_refreshed)
VALUES ('pool_utilization_timeseries', '1970-01-01T00:00:00Z')
ON CONFLICT (view_name) DO NOTHING;

CREATE OR REPLACE FUNCTION trigger_refresh_pool_utilization_timeseries()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    v_last_refreshed TIMESTAMPTZ;
BEGIN
    SELECT last_refreshed
      INTO v_last_refreshed
      FROM pool_mv_refresh_log
     WHERE view_name = 'pool_utilization_timeseries';

    IF v_last_refreshed IS NULL
       OR v_last_refreshed < NOW() - INTERVAL '55 minutes'
    THEN
        PERFORM refresh_pool_utilization_timeseries();

        UPDATE pool_mv_refresh_log
           SET last_refreshed = NOW()
         WHERE view_name = 'pool_utilization_timeseries';
    END IF;

    RETURN NULL;
END;
$$;

-- Drop first so re-running the migration is idempotent
DROP TRIGGER IF EXISTS trg_refresh_pool_utilization_timeseries ON pool_snapshots;

CREATE TRIGGER trg_refresh_pool_utilization_timeseries
    AFTER INSERT ON pool_snapshots
    FOR EACH STATEMENT
    EXECUTE FUNCTION trigger_refresh_pool_utilization_timeseries();

-- ── 5. Comments (runbook) ────────────────────────────────────────────────────

COMMENT ON MATERIALIZED VIEW pool_utilization_timeseries IS
    'Hourly pre-aggregated utilization time series per pool (issue #1650). '
    'Refresh via: SELECT refresh_pool_utilization_timeseries(); '
    'Last-refresh time: SELECT * FROM pool_mv_refresh_log;';

COMMENT ON TABLE pool_mv_refresh_log IS
    'Control table: tracks when each API materialized view was last refreshed '
    'so throttled triggers do not over-refresh under bursts.';
