import request from 'supertest';
import jwt from 'jsonwebtoken';
import app from '../app';
import { config } from '../config';
import { prefetchService } from '../services/prefetch.service';
import { readCacheService } from '../services/readCache.service';
import { redisCacheService } from '../services/redisCache.service';
import { StellarService, clearProtocolStatsCache } from '../services/stellar.service';

const ADMIN = 'GADMINCACHEROUTESTESTADDRESS';

function adminAuth() {
  return { Authorization: `Bearer ${jwt.sign({ address: ADMIN }, config.auth.jwtSecret)}` };
}

describe('Cache routes', () => {
  const originalAdmin = process.env.ADMIN_ADDRESS;

  beforeEach(() => {
    process.env.ADMIN_ADDRESS = ADMIN;
    redisCacheService.clearAllForTests();
    readCacheService.resetForTests();
    prefetchService.resetForTests();
  });

  afterAll(() => {
    process.env.ADMIN_ADDRESS = originalAdmin;
  });

  it('GET /api/cache/stats reports read cache, store and prefetch metrics', async () => {
    await readCacheService.getOrLoad('protocol', 'stats', 30000, async () => ({ tvl: '1' }));
    await readCacheService.getOrLoad('protocol', 'stats', 30000, async () => ({ tvl: '1' }));

    const res = await request(app).get('/api/cache/stats');

    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      timestamp: expect.any(String),
      readCache: {
        enabled: true,
        totals: { hits: 1, misses: 1 },
        kinds: { protocol: { loads: 1 } },
      },
      store: { hits: expect.any(Number), misses: expect.any(Number), errors: expect.any(Number) },
      prefetch: { trackedKeys: 1, running: false, hotKeys: expect.any(Array) },
    });
  });

  it('is mounted under the versioned system prefix', async () => {
    const res = await request(app).get('/api/v1/system/cache/stats');
    expect(res.status).toBe(200);
    expect(res.body.readCache).toBeDefined();
  });

  it('GET /api/cache/prefetch exposes the scheduler state', async () => {
    prefetchService.registerLoader('protocol', 'stats', 30000, async () => ({ tvl: '1' }));
    const res = await request(app).get('/api/cache/prefetch');
    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      enabled: config.prefetch.enabled,
      trackedKeys: 1,
      hotKeys: [expect.objectContaining({ kind: 'protocol', id: 'stats', pinned: true })],
    });
  });

  describe('POST /api/cache/invalidate', () => {
    it('requires the operator role', async () => {
      const anonymous = await request(app).post('/api/cache/invalidate').send({});
      expect(anonymous.status).toBe(401);

      const spoofed = await request(app)
        .post('/api/cache/invalidate')
        .set('x-user-role', 'operator')
        .send({});
      expect(spoofed.status).toBe(401);
    });

    it('drops every read kind when no kind is given', async () => {
      await redisCacheService.set(
        redisCacheService.buildKey('protocol', 'stats'),
        { tvl: '1' },
        60
      );
      await redisCacheService.set(
        redisCacheService.buildKey('http', '/api/protocol/stats'),
        { s: 200 },
        60
      );

      const res = await request(app).post('/api/cache/invalidate').set(adminAuth()).send({});

      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({ success: true, scope: 'all' });
      await expect(
        redisCacheService.get(redisCacheService.buildKey('protocol', 'stats'))
      ).resolves.toBeNull();
      await expect(
        redisCacheService.get(redisCacheService.buildKey('http', '/api/protocol/stats'))
      ).resolves.toBeNull();
    });

    it('drops one kind or one entry', async () => {
      await redisCacheService.set(redisCacheService.buildKey('position', 'GA'), 1, 60);
      await redisCacheService.set(redisCacheService.buildKey('position', 'GB'), 2, 60);

      const entry = await request(app)
        .post('/api/cache/invalidate')
        .set(adminAuth())
        .send({ kind: 'position', id: 'GA' });
      expect(entry.body).toMatchObject({
        success: true,
        scope: 'entry',
        kind: 'position',
        id: 'GA',
      });
      await expect(
        redisCacheService.get(redisCacheService.buildKey('position', 'GB'))
      ).resolves.toBe(2);

      const kind = await request(app)
        .post('/api/cache/invalidate')
        .set(adminAuth())
        .send({ kind: 'position' });
      expect(kind.body).toMatchObject({ success: true, scope: 'kind', kind: 'position' });
      await expect(
        redisCacheService.get(redisCacheService.buildKey('position', 'GB'))
      ).resolves.toBeNull();
    });

    it('rejects unknown kinds and ids without a kind', async () => {
      const unknown = await request(app)
        .post('/api/cache/invalidate')
        .set(adminAuth())
        .send({ kind: 'nonce' });
      expect(unknown.status).toBe(400);
      const orphanId = await request(app)
        .post('/api/cache/invalidate')
        .set(adminAuth())
        .send({ id: 'GA' });
      expect(orphanId.status).toBe(400);
    });
  });

  describe('POST /api/cache/prefetch/run', () => {
    it('requires the operator role', async () => {
      const res = await request(app).post('/api/cache/prefetch/run');
      expect(res.status).toBe(401);
    });

    it('runs one prefetch pass and reports the outcome', async () => {
      const loader = jest.fn().mockResolvedValue({ tvl: '9' });
      prefetchService.registerLoader('protocol', 'stats', 30000, loader);

      const res = await request(app).post('/api/cache/prefetch/run').set(adminAuth());

      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({ success: true, refreshed: 1, failed: 0, skipped: 0 });
      expect(loader).toHaveBeenCalledTimes(1);
      await expect(
        redisCacheService.get(redisCacheService.buildKey('protocol', 'stats'))
      ).resolves.toEqual({ tvl: '9' });
    });
  });

  it('serves protocol stats from the response cache on repeat reads', async () => {
    clearProtocolStatsCache();
    const load = jest.spyOn(StellarService.prototype, 'loadProtocolStats').mockResolvedValue({
      totalDeposits: '1',
      totalBorrows: '2',
      utilizationRate: '0.50',
      numberOfUsers: 3,
      tvl: '4',
    });

    const first = await request(app).get('/api/protocol/stats');
    const second = await request(app).get('/api/protocol/stats');

    expect(first.status).toBe(200);
    expect(first.headers['x-cache']).toBe('MISS');
    expect(second.status).toBe(200);
    expect(second.headers['x-cache']).toBe('HIT');
    expect(second.headers['cache-control']).toBe(first.headers['cache-control']);
    expect(second.body).toMatchObject({ tvl: '4', numberOfUsers: 3 });
    expect(load).toHaveBeenCalledTimes(1);
    load.mockRestore();
  });
});
