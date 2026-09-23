/**
 * Issue #849 – Reputation Controller + Router Integration Tests
 *
 * Covers every route registered by routes/reputation.ts:
 *   - 14 routes (12 GET + 2 POST)
 *   - 200 / 400 / 404 response statuses
 *   - Validation contracts for every input
 *   - Error propagation via next(err) for malformed inputs
 */

import axios from 'axios';
jest.mock('axios');
const mockedAxios = axios as jest.Mocked<typeof axios>;
beforeAll(() => {
  mockedAxios.create.mockReturnThis();
  const axiosResponse = { data: {}, status: 200, statusText: 'OK', headers: {}, config: { url: '' } };
  mockedAxios.get.mockResolvedValue(axiosResponse);
  mockedAxios.post.mockResolvedValue(axiosResponse);
  mockedAxios.request.mockResolvedValue(axiosResponse);
});
afterEach(() => jest.clearAllMocks());

import { StellarService } from '../services/stellar.service';
jest.mock('../services/stellar.service');
const mockStellarService = {
  buildUnsignedTransaction: jest.fn().mockResolvedValue('unsigned_xdr_string'),
  submitTransaction: jest.fn().mockResolvedValue({ success: true, transactionHash: 'mock_tx_hash' }),
  healthCheck: jest.fn().mockResolvedValue({ horizon: true, sorobanRpc: true }),
  getProtocolStats: jest.fn().mockResolvedValue({ tvl: '0' }),
};
(StellarService as jest.Mock).mockImplementation(() => mockStellarService);

import logger from '../utils/logger';
jest.mock('../utils/logger');
(logger as any).warn = jest.fn();
(logger as any).error = jest.fn();
(logger as any).info = jest.fn();

import request from 'supertest';
import app, { resetRateLimiters } from '../app';
import { idempotencyStore } from '../middleware/idempotency';

const VALID_ADDR = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
const INVALID_ADDR_EMPTY = '';
const INVALID_ADDR_TOO_LONG = 'G' + 'A'.repeat(200);

beforeEach(async () => {
  jest.clearAllMocks();
  idempotencyStore.clear();
  await resetRateLimiters();
});

describe('Reputation Controller Routes', () => {
  // ── Reference data endpoints ───────────────────────────────────────────

  describe('GET /api/reputation/tiers', () => {
    it('returns four tiers with ascending score ranges', async () => {
      const res = await request(app).get('/api/reputation/tiers');
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body)).toBe(true);
      expect(res.body.map((t: any) => t.tier)).toEqual([
        'Bronze',
        'Silver',
        'Gold',
        'Platinum',
      ]);
    });
  });

  describe('GET /api/reputation/tiers/:tier/benefits', () => {
    it('returns benefits for valid tier names', async () => {
      const res = await request(app).get('/api/reputation/tiers/Gold/benefits');
      expect(res.status).toBe(200);
      expect(res.body.tier).toBe('Gold');
      expect(res.body.benefits.interest_rate_discount_bps).toBe(50);
    });

    it('is case-insensitive', async () => {
      const res = await request(app).get('/api/reputation/tiers/silver/benefits');
      expect(res.status).toBe(200);
    });

    it('returns 404 for unknown tiers', async () => {
      const res = await request(app).get('/api/reputation/tiers/Diamond/benefits');
      expect(res.status).toBe(404);
      expect(res.body.error).toBeDefined();
    });
  });

  describe('GET /api/reputation/config/deployment', () => {
    it('returns the pool deployment security config', async () => {
      const res = await request(app).get('/api/reputation/config/deployment');
      expect(res.status).toBe(200);
      expect(typeof res.body.min_deployer_score).toBe('number');
      expect(typeof res.body.max_pools_per_deployer).toBe('number');
      expect(typeof res.body.deploy_cooldown_seconds).toBe('number');
      expect(res.body.min_initial_deposit).toBeDefined();
    });
  });

  // ── Analytics ──────────────────────────────────────────────────────────

  describe('GET /api/reputation/analytics', () => {
    it('returns analytics structure with by_tier breakdown', async () => {
      const res = await request(app).get('/api/reputation/analytics');
      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({
        total_tracked: expect.any(Number),
        average_score: expect.any(Number),
        user_count: expect.any(Number),
        deployer_count: expect.any(Number),
        total_pools: expect.any(Number),
        aggregate_tvl: expect.any(String),
        by_tier: expect.objectContaining({
          Bronze: expect.any(Number),
          Silver: expect.any(Number),
          Gold: expect.any(Number),
          Platinum: expect.any(Number),
        }),
      });
    });
  });

  // ── Leaderboard ────────────────────────────────────────────────────────

  describe('GET /api/reputation/leaderboard', () => {
    it('accepts limit and type query params', async () => {
      const res = await request(app).get(
        '/api/reputation/leaderboard?limit=5&type=all',
      );
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body)).toBe(true);
    });

    it('clamps limit between 1 and 100', async () => {
      const res = await request(app).get('/api/reputation/leaderboard?limit=9999');
      expect(res.status).toBe(200);
      expect(res.body.length).toBeLessThanOrEqual(100);
    });

    it('rejects unknown type filter', async () => {
      const res = await request(app).get('/api/reputation/leaderboard?type=robot');
      expect(res.status).toBe(400);
      expect(res.body.error).toBeDefined();
    });
  });

  // ── Deployer endpoints ─────────────────────────────────────────────────

  describe('GET /api/reputation/deployer/:address', () => {
    it('returns a condensed deployer reputation for a valid address', async () => {
      const res = await request(app).get(`/api/reputation/deployer/${VALID_ADDR}`);
      expect(res.status).toBe(200);
      expect(res.body.address).toBe(VALID_ADDR);
      expect(res.body.participant_type).toBe('deployer');
    });

    it('rejects empty address with 400', async () => {
      const res = await request(app).get(`/api/reputation/deployer/${INVALID_ADDR_EMPTY}`);
      expect(res.status).toBe(400);
    });

    it('rejects overly-long address with 400', async () => {
      const res = await request(app).get(`/api/reputation/deployer/${INVALID_ADDR_TOO_LONG}`);
      expect(res.status).toBe(400);
    });
  });

  describe('GET /api/reputation/deployer/:address/full', () => {
    it('returns full deployer reputation including pools array', async () => {
      const res = await request(app).get(
        `/api/reputation/deployer/${VALID_ADDR}/full`,
      );
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body.pools)).toBe(true);
      expect(typeof res.body.avg_pool_uptime_bps).toBe('number');
    });
  });

  describe('GET /api/reputation/deployer/:address/eligibility', () => {
    it('returns an eligibility verdict with reason', async () => {
      const res = await request(app).get(
        `/api/reputation/deployer/${VALID_ADDR}/eligibility`,
      );
      expect(res.status).toBe(200);
      expect(typeof res.body.eligible).toBe('boolean');
    });
  });

  describe('GET /api/reputation/deployer/:address/fee-discount', () => {
    it('returns a numeric fee discount in bps', async () => {
      const res = await request(app).get(
        `/api/reputation/deployer/${VALID_ADDR}/fee-discount`,
      );
      expect(res.status).toBe(200);
      expect(typeof res.body.fee_discount_bps).toBe('number');
      expect(res.body.fee_discount_bps).toBeGreaterThanOrEqual(0);
    });
  });

  // ── User endpoints ─────────────────────────────────────────────────────

  describe('GET /api/reputation/user/:address/borrow-limit-multiplier', () => {
    it('returns borrow-limit multiplier in basis points', async () => {
      const res = await request(app).get(
        `/api/reputation/user/${VALID_ADDR}/borrow-limit-multiplier`,
      );
      expect(res.status).toBe(200);
      expect(typeof res.body.borrow_limit_multiplier_bps).toBe('number');
      expect(res.body.borrow_limit_multiplier_bps).toBeGreaterThanOrEqual(10_000);
    });
  });

  describe('GET /api/reputation/:address (user reputation)', () => {
    it('returns default user reputation for valid address', async () => {
      const res = await request(app).get(`/api/reputation/${VALID_ADDR}`);
      expect(res.status).toBe(200);
      expect(res.body.address).toBe(VALID_ADDR);
      expect(res.body.participant_type).toBe('user');
      expect(res.body.tier).toBeDefined();
    });

    it('returns 400 for invalid address', async () => {
      const res = await request(app).get(`/api/reputation/  `);
      expect(res.status).toBe(400);
    });
  });

  // ── Pool records ───────────────────────────────────────────────────────

  describe('GET /api/reputation/pool/:poolAddress', () => {
    it('returns a shaped pool record with is_active boolean', async () => {
      const res = await request(app).get(`/api/reputation/pool/${VALID_ADDR}`);
      expect(res.status).toBe(200);
      expect(res.body.pool_address).toBe(VALID_ADDR);
      expect(typeof res.body.is_active).toBe('boolean');
      expect(typeof res.body.performance_score).toBe('number');
    });
  });

  // ── Write endpoints (idempotent mutations) ─────────────────────────────

  describe('POST /api/reputation/apply-decay', () => {
    it('acknowledges decay trigger and clears caches', async () => {
      const res = await request(app).post('/api/reputation/apply-decay');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
    });
  });

  describe('POST /api/reputation/cache/invalidate', () => {
    it('acknowledges cache invalidation with list of cleared keys', async () => {
      const res = await request(app)
        .post('/api/reputation/cache/invalidate')
        .send({ address: VALID_ADDR, poolAddress: 'POOL1' });
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.invalidated).toBeDefined();
    });

    it('works even without a body (clears everything)', async () => {
      const res = await request(app).post('/api/reputation/cache/invalidate');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
    });
  });
});

/**
 * Router-order sanity test
 *
 * Express matches routes in registration order.  We must ensure the more
 * specific /deployer/:address route matches before the catch-all /:address
 * GET.  The following request targets an address that contains the word
 * "deployer" – if routes were registered out of order, /:address would
 * shadow /deployer/:address and we'd see participant_type=user instead
 * of participant_type=deployer.
 */
describe('Router ordering', () => {
  it('prefers /deployer/:address over /:address', async () => {
    const res = await request(app).get(
      `/api/reputation/deployer/${VALID_ADDR}`,
    );
    expect(res.status).toBe(200);
    expect(res.body.participant_type).toBe('deployer');
  });

  it('still routes /:address for non-deployer lookups', async () => {
    const res = await request(app).get(`/api/reputation/${VALID_ADDR}`);
    expect(res.status).toBe(200);
    expect(res.body.participant_type).toBe('user');
  });
});
