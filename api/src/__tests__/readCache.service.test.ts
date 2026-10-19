import { config } from '../config';
import { ReadCacheService, READ_CACHE_KINDS, isReadCacheKind } from '../services/readCache.service';
import { prefetchService } from '../services/prefetch.service';
import { redisCacheService } from '../services/redisCache.service';

describe('ReadCacheService', () => {
  const originalEnabled = config.readCache.enabled;
  let cache: ReadCacheService;

  beforeEach(() => {
    config.readCache.enabled = true;
    redisCacheService.clearAllForTests();
    prefetchService.resetForTests();
    cache = new ReadCacheService();
  });

  afterEach(() => {
    config.readCache.enabled = originalEnabled;
    jest.useRealTimers();
  });

  it('loads on a miss, stores the value and serves the next read from cache', async () => {
    const loader = jest.fn().mockResolvedValue({ tvl: '100' });

    await expect(cache.getOrLoad('protocol', 'stats', 30000, loader)).resolves.toEqual({
      tvl: '100',
    });
    await expect(cache.getOrLoad('protocol', 'stats', 30000, loader)).resolves.toEqual({
      tvl: '100',
    });

    expect(loader).toHaveBeenCalledTimes(1);
    expect(cache.getStats()).toMatchObject({
      enabled: true,
      totals: { hits: 1, misses: 1, loads: 1, stores: 1, loadErrors: 0, hitRate: 0.5 },
      kinds: { protocol: { hits: 1, misses: 1 } },
    });
  });

  it('deduplicates concurrent loads of the same key', async () => {
    let resolveLoad: (value: unknown) => void = () => undefined;
    const loader = jest.fn(
      () =>
        new Promise((resolve) => {
          resolveLoad = resolve;
        })
    );

    const first = cache.getOrLoad('pool', 'native', 30000, loader);
    const second = cache.getOrLoad('pool', 'native', 30000, loader);
    await new Promise((resolve) => setImmediate(resolve));
    expect(cache.getStats().inFlightLoads).toBe(1);

    resolveLoad({ epoch: 1 });
    await expect(Promise.all([first, second])).resolves.toEqual([{ epoch: 1 }, { epoch: 1 }]);
    expect(loader).toHaveBeenCalledTimes(1);
    expect(cache.getStats().inFlightLoads).toBe(0);
  });

  it('expires entries after their TTL', async () => {
    jest.useFakeTimers();
    jest.setSystemTime(new Date('2026-09-29T12:00:00Z'));
    const loader = jest.fn().mockResolvedValue('v1').mockResolvedValueOnce('v0');

    await expect(cache.getOrLoad('gas', 'compare', 2000, loader)).resolves.toBe('v0');
    jest.setSystemTime(new Date('2026-09-29T12:00:01Z'));
    await expect(cache.getOrLoad('gas', 'compare', 2000, loader)).resolves.toBe('v0');
    jest.setSystemTime(new Date('2026-09-29T12:00:03Z'));
    await expect(cache.getOrLoad('gas', 'compare', 2000, loader)).resolves.toBe('v1');
    expect(loader).toHaveBeenCalledTimes(2);
  });

  it('never caches null or undefined loader results', async () => {
    const loader = jest.fn().mockResolvedValue(null);
    await expect(cache.getOrLoad('position', 'GEMPTY', 15000, loader)).resolves.toBeNull();
    await expect(cache.getOrLoad('position', 'GEMPTY', 15000, loader)).resolves.toBeNull();
    expect(loader).toHaveBeenCalledTimes(2);
    expect(cache.getStats().totals.stores).toBe(0);
  });

  it('counts and rethrows loader errors, leaving the cache empty', async () => {
    const loader = jest.fn().mockRejectedValue(new Error('rpc down'));
    await expect(cache.getOrLoad('protocol', 'stats', 30000, loader)).rejects.toThrow('rpc down');
    expect(cache.getStats().totals).toMatchObject({ loadErrors: 1, stores: 0 });
    await expect(
      redisCacheService.get(redisCacheService.buildKey('protocol', 'stats'))
    ).resolves.toBeNull();
    expect(cache.getStats().inFlightLoads).toBe(0);
  });

  it('bypasses the cache entirely when disabled', async () => {
    config.readCache.enabled = false;
    const loader = jest.fn().mockResolvedValue(1);
    await cache.getOrLoad('protocol', 'stats', 30000, loader);
    await cache.getOrLoad('protocol', 'stats', 30000, loader);
    expect(loader).toHaveBeenCalledTimes(2);
    expect(cache.getStats()).toMatchObject({ enabled: false, totals: { misses: 2, stores: 0 } });
  });

  it('reports accesses and loaders to the prefetch service', async () => {
    const loader = jest.fn().mockResolvedValue({ collateral: '1' });
    for (let i = 0; i < config.prefetch.hotThreshold; i += 1) {
      await cache.getOrLoad('position', 'GUSER', 15000, loader);
    }
    expect(prefetchService.isHot('position', 'GUSER')).toBe(true);
    expect(prefetchService.getStats().hotKeys[0]).toMatchObject({ id: 'GUSER', hasLoader: true });

    await cache.getOrLoad('position', 'GQUIET', 15000, loader, { track: false });
    expect(prefetchService.getStats().trackedKeys).toBe(1);
  });

  it('refresh overwrites an existing entry with fresh data', async () => {
    await cache.getOrLoad('protocol', 'stats', 30000, async () => ({ tvl: 'old' }));
    await expect(
      cache.refresh('protocol', 'stats', 30000, async () => ({ tvl: 'new' }))
    ).resolves.toEqual({
      tvl: 'new',
    });
    await expect(
      cache.getOrLoad('protocol', 'stats', 30000, async () => ({ tvl: 'unused' }))
    ).resolves.toEqual({ tvl: 'new' });
  });

  it('invalidates a single entry, a whole kind, or every read kind', async () => {
    const loader = jest.fn().mockResolvedValue('x');
    await cache.getOrLoad('position', 'GA', 15000, loader);
    await cache.getOrLoad('position', 'GB', 15000, loader);
    await cache.getOrLoad('protocol', 'stats', 30000, loader);
    await redisCacheService.set(redisCacheService.buildKey('nonce', 'GA'), 'keep', 60);

    await cache.invalidate('position', 'GA');
    await expect(
      redisCacheService.get(redisCacheService.buildKey('position', 'GA'))
    ).resolves.toBeNull();
    await expect(redisCacheService.get(redisCacheService.buildKey('position', 'GB'))).resolves.toBe(
      'x'
    );

    await cache.invalidate('position');
    await expect(
      redisCacheService.get(redisCacheService.buildKey('position', 'GB'))
    ).resolves.toBeNull();
    await expect(
      redisCacheService.get(redisCacheService.buildKey('protocol', 'stats'))
    ).resolves.toBe('x');

    await cache.invalidateAll();
    await expect(
      redisCacheService.get(redisCacheService.buildKey('protocol', 'stats'))
    ).resolves.toBeNull();
    await expect(redisCacheService.get(redisCacheService.buildKey('nonce', 'GA'))).resolves.toBe(
      'keep'
    );
    // one entry, one kind, and one more from invalidateAll
    expect(cache.getStats().kinds.position.invalidations).toBe(3);
  });

  it('exposes the read kinds and validates kind names', () => {
    expect(READ_CACHE_KINDS).toContain('http');
    expect(READ_CACHE_KINDS).not.toContain('nonce');
    expect(isReadCacheKind('protocol')).toBe(true);
    expect(isReadCacheKind('nonce')).toBe(false);
    expect(isReadCacheKind(42)).toBe(false);
  });
});
