import express from 'express';
import request from 'supertest';
import { config } from '../config';
import { buildReadCacheKey, cacheReadResponse } from '../middleware/readCache.middleware';
import { readCacheService } from '../services/readCache.service';
import { redisCacheService } from '../services/redisCache.service';

describe('cacheReadResponse middleware', () => {
  const originalReadCache = { ...config.readCache };
  let handlerCalls: number;

  function buildApp() {
    const app = express();
    app.use(express.json());
    app.get('/stats', cacheReadResponse({ ttlMs: 30000 }), (_req, res) => {
      handlerCalls += 1;
      res.setHeader('Cache-Control', 'public, max-age=30');
      res.json({ calls: handlerCalls });
    });
    app.get('/items', cacheReadResponse(), (req, res) => {
      handlerCalls += 1;
      res.json({ calls: handlerCalls, query: req.query });
    });
    app.get('/missing', cacheReadResponse(), (_req, res) => {
      handlerCalls += 1;
      res.status(404).json({ error: 'not found' });
    });
    app.get('/text', cacheReadResponse(), (_req, res) => {
      handlerCalls += 1;
      res.send('plain');
    });
    app.post('/stats', cacheReadResponse(), (_req, res) => {
      handlerCalls += 1;
      res.json({ calls: handlerCalls });
    });
    return app;
  }

  beforeEach(() => {
    Object.assign(config.readCache, { enabled: true, defaultTtlMs: 15000, maxQueryLength: 512 });
    redisCacheService.clearAllForTests();
    readCacheService.resetForTests();
    handlerCalls = 0;
  });

  afterEach(() => {
    Object.assign(config.readCache, originalReadCache);
  });

  it('serves the second identical GET from cache with X-Cache and the stored Cache-Control', async () => {
    const app = buildApp();

    const miss = await request(app).get('/stats');
    expect(miss.status).toBe(200);
    expect(miss.headers['x-cache']).toBe('MISS');
    expect(miss.body).toEqual({ calls: 1 });

    const hit = await request(app).get('/stats');
    expect(hit.status).toBe(200);
    expect(hit.headers['x-cache']).toBe('HIT');
    expect(hit.headers['cache-control']).toBe('public, max-age=30');
    expect(hit.body).toEqual({ calls: 1 });
    expect(handlerCalls).toBe(1);
    expect(readCacheService.getStats().kinds.http).toMatchObject({ hits: 1, misses: 1, stores: 1 });
  });

  it('keys entries by path and sorted query so parameter order does not matter', async () => {
    const app = buildApp();

    await request(app).get('/items?b=2&a=1');
    const hit = await request(app).get('/items?a=1&b=2');
    const other = await request(app).get('/items?a=1&b=3');

    expect(hit.headers['x-cache']).toBe('HIT');
    expect(other.headers['x-cache']).toBe('MISS');
    expect(handlerCalls).toBe(2);
  });

  it('bypasses the cache when the client sends Cache-Control: no-cache', async () => {
    const app = buildApp();
    await request(app).get('/stats');

    const bypass = await request(app).get('/stats').set('Cache-Control', 'no-cache');
    expect(bypass.headers['x-cache']).toBe('BYPASS');
    expect(bypass.body).toEqual({ calls: 2 });

    const hit = await request(app).get('/stats');
    expect(hit.headers['x-cache']).toBe('HIT');
    expect(hit.body).toEqual({ calls: 2 });
  });

  it('does not cache non-2xx responses', async () => {
    const app = buildApp();
    await request(app).get('/missing');
    const second = await request(app).get('/missing');
    expect(second.status).toBe(404);
    expect(second.headers['x-cache']).toBe('MISS');
    expect(handlerCalls).toBe(2);
  });

  it('leaves non-JSON responses and non-GET requests uncached', async () => {
    const app = buildApp();
    await request(app).get('/text');
    await request(app).get('/text');
    await request(app).post('/stats');
    const post = await request(app).post('/stats');
    expect(post.headers['x-cache']).toBeUndefined();
    expect(handlerCalls).toBe(4);
  });

  it('skips caching when the key is longer than the configured limit', async () => {
    config.readCache.maxQueryLength = 20;
    const app = buildApp();
    const res = await request(app).get('/items?filter=' + 'x'.repeat(40));
    expect(res.headers['x-cache']).toBe('SKIP');
    await request(app).get('/items?filter=' + 'x'.repeat(40));
    expect(handlerCalls).toBe(2);
  });

  it('runs every request through the handler when the read cache is disabled', async () => {
    config.readCache.enabled = false;
    const app = buildApp();
    await request(app).get('/stats');
    const res = await request(app).get('/stats');
    expect(res.headers['x-cache']).toBeUndefined();
    expect(handlerCalls).toBe(2);
  });

  it('honours a custom key builder', async () => {
    const app = express();
    app.get(
      '/user',
      cacheReadResponse({ keyBuilder: (req) => `user:${req.headers['x-user-address']}` }),
      (_req, res) => {
        handlerCalls += 1;
        res.json({ calls: handlerCalls });
      }
    );
    await request(app).get('/user').set('x-user-address', 'GA');
    const sameUser = await request(app).get('/user').set('x-user-address', 'GA');
    const otherUser = await request(app).get('/user').set('x-user-address', 'GB');
    expect(sameUser.headers['x-cache']).toBe('HIT');
    expect(otherUser.headers['x-cache']).toBe('MISS');
  });

  it('builds deterministic keys for array and object query values', () => {
    const req = {
      baseUrl: '/api/gas',
      path: '/compare',
      query: { ops: ['b', 'a'], filter: { min: 1 }, z: '1' },
    } as unknown as express.Request;
    expect(buildReadCacheKey(req)).toBe(
      '/api/gas/compare?filter=%7B%22min%22%3A1%7D&ops=b%2Ca&z=1'
    );
  });
});
