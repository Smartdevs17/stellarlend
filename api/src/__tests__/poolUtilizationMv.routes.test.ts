/**
 * Integration tests for the pool-utilization materialized-view routes.
 * Issue #1650 — pool-utilization time series via materialized view.
 *
 * Uses supertest against the real Express app; the repository is injected
 * so no database connection is required.
 */

import request from 'supertest';
import app from '../app';
import {
  setPoolUtilizationMvRepository,
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
    async queryByPool(poolAddress, _from, _to) {
      if (opts.failQuery) throw new Error('simulated DB failure');
      return buckets.filter((b) => b.poolAddress === poolAddress);
    },
    async queryAll(_from, _to) {
      if (opts.failQuery) throw new Error('simulated DB failure');
      return buckets;
    },
    async triggerManualRefresh() {
      if (opts.failQuery) throw new Error('simulated DB failure');
    },
  };
}

// ── Setup / teardown ──────────────────────────────────────────────────────────

beforeEach(() => {
  jest.spyOn(redisCacheService, 'get').mockResolvedValue(null);
  jest.spyOn(redisCacheService, 'set').mockResolvedValue(undefined);
  jest.spyOn(redisCacheService, 'buildKey').mockImplementation((...parts) => parts.join(':'));
});

afterEach(() => {
  jest.restoreAllMocks();
  setPoolUtilizationMvRepository(null);
});

// ── GET /api/pool-performance/utilization-timeseries/:poolAddress ─────────────

describe('GET /api/pool-performance/utilization-timeseries/:poolAddress', () => {
  it('returns 200 with buckets for a known pool', async () => {
    const buckets = [
      makeBucket({ poolAddress: 'pool_xlm_001', bucket: '2026-11-19T08:00:00.000Z' }),
      makeBucket({ poolAddress: 'pool_xlm_001', bucket: '2026-11-19T09:00:00.000Z' }),
    ];
    setPoolUtilizationMvRepository(makeRepo(buckets));

    const res = await request(app)
      .get('/api/pool-performance/utilization-timeseries/pool_xlm_001')
      .query({ period: '7d' });

    expect(res.status).toBe(200);
    expect(res.body.success).toBe(true);
    expect(res.body.data.poolAddress).toBe('pool_xlm_001');
    expect(res.body.data.period).toBe('7d');
    expect(res.body.data.buckets).toHaveLength(2);
    expect(res.body.data.servedFromView).toBe(true);
  });

  it('defaults to period 30d when no query param is provided', async () => {
    setPoolUtilizationMvRepository(makeRepo([]));

    const res = await request(app)
      .get('/api/pool-performance/utilization-timeseries/pool_xlm_001');

    expect(res.status).toBe(200);
    expect(res.body.data.period).toBe('30d');
  });

  it('returns empty buckets when pool has no view data', async () => {
    setPoolUtilizationMvRepository(makeRepo([]));

    const res = await request(app)
      .get('/api/pool-performance/utilization-timeseries/pool_new_001')
      .query({ period: '7d' });

    expect(res.status).toBe(200);
    expect(res.body.data.buckets).toEqual([]);
  });

  it('bucket shape contains all expected fields', async () => {
    setPoolUtilizationMvRepository(makeRepo([makeBucket()]));

    const res = await request(app)
      .get('/api/pool-performance/utilization-timeseries/pool_xlm_001')
      .query({ period: '7d' });

    const b = res.body.data.buckets[0];
    expect(b).toMatchObject({
      bucket:            expect.any(String),
      poolAddress:       expect.any(String),
      avgUtilization:    expect.any(Number),
      minUtilization:    expect.any(Number),
      maxUtilization:    expect.any(Number),
      medianUtilization: expect.any(Number),
      stddevUtilization: expect.any(Number),
      avgTvl:            expect.any(Number),
      avgTotalBorrows:   expect.any(Number),
      avgTotalDeposits:  expect.any(Number),
      avgBorrowApy:      expect.any(Number),
      avgSupplyApy:      expect.any(Number),
      sampleCount:       expect.any(Number),
    });
  });

  it('returns 500 when the repository throws', async () => {
    setPoolUtilizationMvRepository(makeRepo([], { failQuery: true }));

    const res = await request(app)
      .get('/api/pool-performance/utilization-timeseries/pool_xlm_001')
      .query({ period: '7d' });

    expect(res.status).toBe(500);
  });
});

// ── GET /api/pool-performance/utilization-timeseries ─────────────────────────

describe('GET /api/pool-performance/utilization-timeseries (all pools)', () => {
  it('returns 200 with buckets for all pools', async () => {
    const buckets = [
      makeBucket({ poolAddress: 'pool_xlm_001' }),
      makeBucket({ poolAddress: 'pool_usdc_001' }),
    ];
    setPoolUtilizationMvRepository(makeRepo(buckets));

    const res = await request(app)
      .get('/api/pool-performance/utilization-timeseries')
      .query({ period: '30d' });

    expect(res.status).toBe(200);
    expect(res.body.success).toBe(true);
    expect(res.body.data).toHaveLength(2);
  });

  it('returns an empty array when no data exists', async () => {
    setPoolUtilizationMvRepository(makeRepo([]));

    const res = await request(app)
      .get('/api/pool-performance/utilization-timeseries');

    expect(res.status).toBe(200);
    expect(res.body.data).toEqual([]);
  });

  it('returns 500 when the repository throws', async () => {
    setPoolUtilizationMvRepository(makeRepo([], { failQuery: true }));

    const res = await request(app)
      .get('/api/pool-performance/utilization-timeseries');

    expect(res.status).toBe(500);
  });
});

// ── POST /api/pool-performance/utilization-timeseries/refresh ────────────────

describe('POST /api/pool-performance/utilization-timeseries/refresh', () => {
  it('returns 200 with refreshedAt timestamp when refresh succeeds', async () => {
    setPoolUtilizationMvRepository(makeRepo([]));

    const res = await request(app)
      .post('/api/pool-performance/utilization-timeseries/refresh');

    expect(res.status).toBe(200);
    expect(res.body.success).toBe(true);
    expect(typeof res.body.data.refreshedAt).toBe('string');
    // refreshedAt must be a valid ISO-8601 timestamp
    expect(() => new Date(res.body.data.refreshedAt).toISOString()).not.toThrow();
  });

  it('returns 500 when the refresh call fails', async () => {
    setPoolUtilizationMvRepository(makeRepo([], { failQuery: true }));

    const res = await request(app)
      .post('/api/pool-performance/utilization-timeseries/refresh');

    expect(res.status).toBe(500);
  });
});
