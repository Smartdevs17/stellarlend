/**
 * Pool-utilization time-series service (Issue #1650).
 *
 * Queries the `pool_utilization_timeseries` materialised view, which
 * buckets `pool_snapshots` into hourly aggregates.  This replaces the
 * previously hot full-table-scan path that was the measured tail-latency
 * bottleneck on the utilization heatmap and time-series endpoints.
 *
 * The view is populated on first creation WITH DATA and kept fresh by the
 * throttled trigger defined in
 * api/src/db/migrations/20261119000000_pool_utilization_timeseries_mv.sql.
 *
 * Repository pattern
 * ──────────────────
 * Tests inject a `PoolUtilizationMvRepository` via
 * `setPoolUtilizationMvRepository()` so no real DB connection is needed.
 * Production uses the Prisma `$queryRaw` adapter below.
 */

import { PrismaClient, Prisma } from '@prisma/client';
import logger from '../utils/logger';
import { redisCacheService } from './redisCache.service';

export type UtilizationPeriod = '7d' | '30d' | '90d' | '1y';

export interface UtilizationBucket {
  /** ISO-8601 timestamp for the start of the hourly bucket */
  bucket: string;
  poolAddress: string;
  avgUtilization: number;
  minUtilization: number;
  maxUtilization: number;
  medianUtilization: number;
  stddevUtilization: number;
  avgTvl: number;
  avgTotalBorrows: number;
  avgTotalDeposits: number;
  avgBorrowApy: number;
  avgSupplyApy: number;
  sampleCount: number;
}

export interface UtilizationTimeSeriesResult {
  poolAddress: string;
  period: UtilizationPeriod;
  buckets: UtilizationBucket[];
  servedFromView: boolean;
}

// ── Repository interface (for testability) ───────────────────────────────────

export interface PoolUtilizationMvRepository {
  queryByPool(
    poolAddress: string,
    from: Date,
    to: Date,
  ): Promise<UtilizationBucket[]>;

  queryAll(from: Date, to: Date): Promise<UtilizationBucket[]>;

  triggerManualRefresh(): Promise<void>;
}

// ── Period helpers ────────────────────────────────────────────────────────────

const PERIOD_TO_MS: Record<UtilizationPeriod, number> = {
  '7d':  7  * 24 * 3_600_000,
  '30d': 30 * 24 * 3_600_000,
  '90d': 90 * 24 * 3_600_000,
  '1y': 365 * 24 * 3_600_000,
};

export function periodToDateRange(period: UtilizationPeriod): { from: Date; to: Date } {
  const to   = new Date();
  const from = new Date(to.getTime() - PERIOD_TO_MS[period]);
  return { from, to };
}

export function parsePeriod(value: string): UtilizationPeriod {
  if (value === '7d' || value === '30d' || value === '90d' || value === '1y') return value;
  return '30d';
}

// ── Prisma adapter ────────────────────────────────────────────────────────────

function toNumber(raw: unknown): number {
  if (raw === null || raw === undefined) return 0;
  const n = typeof raw === 'object' ? Number((raw as { toNumber?: () => number }).toNumber?.() ?? raw) : Number(raw);
  return Number.isFinite(n) ? n : 0;
}

function mapRow(row: Record<string, unknown>): UtilizationBucket {
  return {
    bucket:             row['bucket'] instanceof Date ? row['bucket'].toISOString() : String(row['bucket']),
    poolAddress:        String(row['pool_address'] ?? ''),
    avgUtilization:     toNumber(row['avg_utilization']),
    minUtilization:     toNumber(row['min_utilization']),
    maxUtilization:     toNumber(row['max_utilization']),
    medianUtilization:  toNumber(row['median_utilization']),
    stddevUtilization:  toNumber(row['stddev_utilization']),
    avgTvl:             toNumber(row['avg_tvl']),
    avgTotalBorrows:    toNumber(row['avg_total_borrows']),
    avgTotalDeposits:   toNumber(row['avg_total_deposits']),
    avgBorrowApy:       toNumber(row['avg_borrow_apy']),
    avgSupplyApy:       toNumber(row['avg_supply_apy']),
    sampleCount:        toNumber(row['sample_count']),
  };
}

export class PrismaPoolUtilizationMvRepository implements PoolUtilizationMvRepository {
  constructor(private readonly prisma: PrismaClient) {}

  async queryByPool(
    poolAddress: string,
    from: Date,
    to: Date,
  ): Promise<UtilizationBucket[]> {
    const rows = await this.prisma.$queryRaw<Record<string, unknown>[]>(
      Prisma.sql`
        SELECT
            pool_address,
            bucket,
            avg_utilization,
            min_utilization,
            max_utilization,
            median_utilization,
            stddev_utilization,
            avg_tvl,
            avg_total_borrows,
            avg_total_deposits,
            avg_borrow_apy,
            avg_supply_apy,
            sample_count
        FROM pool_utilization_timeseries
        WHERE pool_address = ${poolAddress}
          AND bucket >= ${from}
          AND bucket <= ${to}
        ORDER BY bucket ASC
      `,
    );
    return rows.map(mapRow);
  }

  async queryAll(from: Date, to: Date): Promise<UtilizationBucket[]> {
    const rows = await this.prisma.$queryRaw<Record<string, unknown>[]>(
      Prisma.sql`
        SELECT
            pool_address,
            bucket,
            avg_utilization,
            min_utilization,
            max_utilization,
            median_utilization,
            stddev_utilization,
            avg_tvl,
            avg_total_borrows,
            avg_total_deposits,
            avg_borrow_apy,
            avg_supply_apy,
            sample_count
        FROM pool_utilization_timeseries
        WHERE bucket >= ${from}
          AND bucket <= ${to}
        ORDER BY pool_address ASC, bucket ASC
      `,
    );
    return rows.map(mapRow);
  }

  async triggerManualRefresh(): Promise<void> {
    await this.prisma.$executeRaw`SELECT refresh_pool_utilization_timeseries()`;
  }
}

// ── Module-level state (injectable for tests) ─────────────────────────────────

let _repo: PoolUtilizationMvRepository | null = null;

export function setPoolUtilizationMvRepository(repo: PoolUtilizationMvRepository | null): void {
  _repo = repo;
}

function getRepo(): PoolUtilizationMvRepository {
  if (_repo) return _repo;
  // Lazy-load Prisma so tests that inject a mock never instantiate a real client
  const { PrismaClient: PC } = require('@prisma/client') as typeof import('@prisma/client');
  const prisma = new PC();
  _repo = new PrismaPoolUtilizationMvRepository(prisma);
  return _repo;
}

// ── Cache helpers ─────────────────────────────────────────────────────────────

const CACHE_TTL_S = 120; // 2 min — view refreshes at most once per 55 min

function mvCacheKey(kind: string, id: string): string {
  return redisCacheService.buildKey('pool-util-mv', `${kind}:${id}`);
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Return hourly utilization buckets for a single pool over the requested
 * period.  Results are served from Redis when available; the materialised
 * view is the authoritative source otherwise.
 */
export async function getPoolUtilizationTimeSeries(
  poolAddress: string,
  period: string,
): Promise<UtilizationTimeSeriesResult> {
  const resolvedPeriod = parsePeriod(period);
  const cacheKey = mvCacheKey('by-pool', `${poolAddress}:${resolvedPeriod}`);

  const cached = await redisCacheService.get<UtilizationTimeSeriesResult>(cacheKey);
  if (cached) {
    logger.debug('pool-util-mv cache hit', { poolAddress, period: resolvedPeriod });
    return cached;
  }

  const { from, to } = periodToDateRange(resolvedPeriod);
  const start = Date.now();

  let buckets: UtilizationBucket[];
  try {
    buckets = await getRepo().queryByPool(poolAddress, from, to);
  } catch (err) {
    logger.error('pool-util-mv query failed', { poolAddress, period: resolvedPeriod, err });
    throw err;
  }

  const latencyMs = Date.now() - start;
  logger.info('pool-util-mv served from view', {
    poolAddress,
    period: resolvedPeriod,
    buckets: buckets.length,
    latencyMs,
  });

  const result: UtilizationTimeSeriesResult = {
    poolAddress,
    period: resolvedPeriod,
    buckets,
    servedFromView: true,
  };

  await redisCacheService.set(cacheKey, result, CACHE_TTL_S);
  return result;
}

/**
 * Return hourly utilization buckets for every pool over the requested period.
 * Used by cross-pool comparison dashboards.
 */
export async function getAllPoolsUtilizationTimeSeries(
  period: string,
): Promise<UtilizationBucket[]> {
  const resolvedPeriod = parsePeriod(period);
  const cacheKey = mvCacheKey('all', resolvedPeriod);

  const cached = await redisCacheService.get<UtilizationBucket[]>(cacheKey);
  if (cached) {
    logger.debug('pool-util-mv all-pools cache hit', { period: resolvedPeriod });
    return cached;
  }

  const { from, to } = periodToDateRange(resolvedPeriod);
  const start = Date.now();

  let buckets: UtilizationBucket[];
  try {
    buckets = await getRepo().queryAll(from, to);
  } catch (err) {
    logger.error('pool-util-mv all-pools query failed', { period: resolvedPeriod, err });
    throw err;
  }

  const latencyMs = Date.now() - start;
  logger.info('pool-util-mv all-pools served from view', {
    period: resolvedPeriod,
    buckets: buckets.length,
    latencyMs,
  });

  await redisCacheService.set(cacheKey, buckets, CACHE_TTL_S);
  return buckets;
}

/**
 * Manually trigger a view refresh.  Intended for use by admin tooling or
 * the CI data-load step; in production the trigger handles this automatically.
 */
export async function refreshPoolUtilizationTimeSeries(): Promise<{ refreshedAt: string }> {
  const start = Date.now();
  await getRepo().triggerManualRefresh();
  const latencyMs = Date.now() - start;
  const refreshedAt = new Date().toISOString();
  logger.info('pool-util-mv manual refresh completed', { latencyMs, refreshedAt });
  return { refreshedAt };
}
