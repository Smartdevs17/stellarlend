/**
 * Unit tests for the pool-utilization materialised-view service.
 * Issue #1650 — pool-utilization time series via materialized view.
 *
 * All tests use the injectable repository so no real database is required.
 */

import {
  setPoolUtilizationMvRepository,
  getPoolUtilizationTimeSeries,
  getAllPoolsUtilizationTimeSeries,
  refreshPoolUtilizationTimeSeries,
  periodToDateRange,
  parsePeriod,
  type PoolUtilizationMvRepository,
  type UtilizationBucket,
} from '../services/poolUtilizationMv.service';
import { redisCacheService } from '../services/redisCache.service';

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeBucket(overrides: Partial<UtilizationBucket> = {}): UtilizationBucket {
  return {
    bucket:            '2026-11-19T10:00:00.000Z',
    poolAddress:       'pool_xlm_001',
    avgUtilization:    0.65,
    minUtilization:    0.60,
    maxUtilization:    0.70,
    medianUtilization: 0.65,
    stddevUtilization: 0.03,
    avgTvl:            2_000_000,
    avgTotalBorrows:   1_300_000,
    avgTotalDeposits:  2_000_000,
    avgBorrowApy:      0.08,
    avgSupplyApy:      0.055,
    sampleCount:       3,
    ...overrides,
  };
}

function makeRepo(buckets: UtilizationBucket[] = [], opts: { failQuery?: boolean } = {}): PoolUtilizationMvRepository {
  return {
    async queryByPool(_poolAddress, _from, _to) {
      if (opts.failQuery) throw new Error('DB error');
      return buckets.filter((b) => b.poolAddress === _poolAddress);
    },
    async queryAll(_from, _to) {
      if (opts.failQuery) throw new Error('DB error');
      return buckets;
    },
    async triggerManualRefresh() {
      if (opts.failQuery) throw new Error('DB error');
    },
  };
}

// ── Setup / teardown ──────────────────────────────────────────────────────────

beforeEach(() => {
  setPoolUtilizationMvRepository(null);
  // Silence cache; return null for every get so we always hit the view in tests
  jest.spyOn(redisCacheService, 'get').mockResolvedValue(null);
  jest.spyOn(redisCacheService, 'set').mockResolvedValue(undefined);
  jest.spyOn(redisCacheService, 'buildKey').mockImplementation((...parts) => parts.join(':'));
});

afterEach(() => {
  jest.restoreAllMocks();
  setPoolUtilizationMvRepository(null);
});

// ── parsePeriod ───────────────────────────────────────────────────────────────

describe('parsePeriod', () => {
  it.each(['7d', '30d', '90d', '1y'] as const)('accepts valid period %s', (p) => {
    expect(parsePeriod(p)).toBe(p);
  });

  it('defaults unknown values to 30d', () => {
    expect(parsePeriod('180d')).toBe('30d');
    expect(parsePeriod('')).toBe('30d');
  });
});

// ── periodToDateRange ─────────────────────────────────────────────────────────

describe('periodToDateRange', () => {
  it('returns a from/to pair where from is before to', () => {
    const { from, to } = periodToDateRange('7d');
    expect(from.getTime()).toBeLessThan(to.getTime());
  });

  it('7d range is approximately 7 days', () => {
    const { from, to } = periodToDateRange('7d');
    const diffDays = (to.getTime() - from.getTime()) / (24 * 3_600_000);
    expect(diffDays).toBeCloseTo(7, 0);
  });

  it('1y range is approximately 365 days', () => {
    const { from, to } = periodToDateRange('1y');
    const diffDays = (to.getTime() - from.getTime()) / (24 * 3_600_000);
    expect(diffDays).toBeCloseTo(365, 0);
  });
});

// ── getPoolUtilizationTimeSeries ──────────────────────────────────────────────

describe('getPoolUtilizationTimeSeries', () => {
  it('returns buckets from the injected repository for the matching pool', async () => {
    const buckets = [
      makeBucket({ poolAddress: 'pool_xlm_001', bucket: '2026-11-19T10:00:00.000Z' }),
      makeBucket({ poolAddress: 'pool_xlm_001', bucket: '2026-11-19T11:00:00.000Z' }),
      makeBucket({ poolAddress: 'pool_usdc_001', bucket: '2026-11-19T10:00:00.000Z' }),
    ];
    setPoolUtilizationMvRepository(makeRepo(buckets));

    const result = await getPoolUtilizationTimeSeries('pool_xlm_001', '7d');

    expect(result.servedFromView).toBe(true);
    expect(result.poolAddress).toBe('pool_xlm_001');
    expect(result.period).toBe('7d');
    expect(result.buckets).toHaveLength(2);
    expect(result.buckets.every((b) => b.poolAddress === 'pool_xlm_001')).toBe(true);
  });

  it('defaults unknown period to 30d', async () => {
    setPoolUtilizationMvRepository(makeRepo([]));
    const result = await getPoolUtilizationTimeSeries('pool_xlm_001', 'bad-period');
    expect(result.period).toBe('30d');
  });

  it('returns empty buckets array when view has no data yet', async () => {
    setPoolUtilizationMvRepository(makeRepo([]));
    const result = await getPoolUtilizationTimeSeries('pool_xlm_001', '30d');
    expect(result.buckets).toEqual([]);
    expect(result.servedFromView).toBe(true);
  });

  it('propagates repository errors', async () => {
    setPoolUtilizationMvRepository(makeRepo([], { failQuery: true }));
    await expect(getPoolUtilizationTimeSeries('pool_xlm_001', '7d')).rejects.toThrow('DB error');
  });

  it('caches the result and returns cached value on second call', async () => {
    const buckets = [makeBucket()];
    const repo = makeRepo(buckets);
    const spy = jest.spyOn(repo, 'queryByPool');
    setPoolUtilizationMvRepository(repo);

    // First call — populate cache
    await getPoolUtilizationTimeSeries('pool_xlm_001', '30d');

    // Simulate a warm cache hit on the second call
    jest.spyOn(redisCacheService, 'get').mockResolvedValueOnce({
      poolAddress: 'pool_xlm_001',
      period: '30d',
      buckets,
      servedFromView: true,
    });

    const cached = await getPoolUtilizationTimeSeries('pool_xlm_001', '30d');
    // Repository should not have been called a second time
    expect(spy).toHaveBeenCalledTimes(1);
    expect(cached.buckets).toHaveLength(1);
  });

  it('bucket fields are all numeric and within expected ranges', async () => {
    setPoolUtilizationMvRepository(makeRepo([makeBucket()]));
    const result = await getPoolUtilizationTimeSeries('pool_xlm_001', '7d');
    const b = result.buckets[0]!;
    expect(b.avgUtilization).toBeGreaterThanOrEqual(0);
    expect(b.avgUtilization).toBeLessThanOrEqual(1);
    expect(b.minUtilization).toBeLessThanOrEqual(b.avgUtilization);
    expect(b.maxUtilization).toBeGreaterThanOrEqual(b.avgUtilization);
    expect(b.sampleCount).toBeGreaterThan(0);
  });
});

// ── getAllPoolsUtilizationTimeSeries ──────────────────────────────────────────

describe('getAllPoolsUtilizationTimeSeries', () => {
  it('returns buckets for all pools', async () => {
    const buckets = [
      makeBucket({ poolAddress: 'pool_xlm_001' }),
      makeBucket({ poolAddress: 'pool_usdc_001' }),
    ];
    setPoolUtilizationMvRepository(makeRepo(buckets));

    const result = await getAllPoolsUtilizationTimeSeries('30d');
    expect(result).toHaveLength(2);
  });

  it('returns empty array when no data exists', async () => {
    setPoolUtilizationMvRepository(makeRepo([]));
    const result = await getAllPoolsUtilizationTimeSeries('7d');
    expect(result).toEqual([]);
  });

  it('propagates repository errors', async () => {
    setPoolUtilizationMvRepository(makeRepo([], { failQuery: true }));
    await expect(getAllPoolsUtilizationTimeSeries('30d')).rejects.toThrow('DB error');
  });
});

// ── refreshPoolUtilizationTimeSeries ─────────────────────────────────────────

describe('refreshPoolUtilizationTimeSeries', () => {
  it('calls triggerManualRefresh and returns a refreshedAt timestamp', async () => {
    const repo = makeRepo([]);
    const spy = jest.spyOn(repo, 'triggerManualRefresh').mockResolvedValue();
    setPoolUtilizationMvRepository(repo);

    const result = await refreshPoolUtilizationTimeSeries();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(typeof result.refreshedAt).toBe('string');
    expect(new Date(result.refreshedAt).toISOString()).toBe(result.refreshedAt);
  });

  it('propagates errors from triggerManualRefresh', async () => {
    setPoolUtilizationMvRepository(makeRepo([], { failQuery: true }));
    await expect(refreshPoolUtilizationTimeSeries()).rejects.toThrow('DB error');
  });
});
