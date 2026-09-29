import { config } from '../config';
import { PrefetchService } from '../services/prefetch.service';
import { redisCacheService } from '../services/redisCache.service';

describe('PrefetchService', () => {
  const originalPrefetch = { ...config.prefetch };
  let service: PrefetchService;

  beforeEach(() => {
    jest.useFakeTimers();
    jest.setSystemTime(new Date('2026-09-29T12:00:00Z'));
    Object.assign(config.prefetch, {
      enabled: true,
      intervalMs: 1000,
      windowMs: 60000,
      hotThreshold: 3,
      maxTrackedKeys: 4,
      maxKeysPerRun: 10,
    });
    redisCacheService.clearAllForTests();
    service = new PrefetchService();
  });

  afterEach(() => {
    service.stop();
    Object.assign(config.prefetch, originalPrefetch);
    jest.useRealTimers();
  });

  it('marks a key hot once its access count reaches the threshold', () => {
    service.recordAccess('position', 'GUSER', 15000);
    service.recordAccess('position', 'GUSER', 15000);
    expect(service.isHot('position', 'GUSER')).toBe(false);

    service.recordAccess('position', 'GUSER', 15000);
    expect(service.isHot('position', 'GUSER')).toBe(true);
    expect(service.getStats().hotKeys).toEqual([
      expect.objectContaining({ kind: 'position', id: 'GUSER', count: 3, pinned: false }),
    ]);
  });

  it('refreshes hot keys with a loader, stores the value and skips keys without one', async () => {
    const loader = jest.fn().mockResolvedValue({ collateral: '10' });
    for (let i = 0; i < 3; i += 1) service.recordAccess('position', 'GHOT', 15000, loader);
    for (let i = 0; i < 3; i += 1) service.recordAccess('position', 'GNOLOADER', 15000);
    service.recordAccess('position', 'GCOLD', 15000, loader);

    const result = await service.runOnce();

    expect(result).toMatchObject({ refreshed: 1, failed: 0, skipped: 1 });
    expect(loader).toHaveBeenCalledTimes(1);
    await expect(
      redisCacheService.get(redisCacheService.buildKey('position', 'GHOT'))
    ).resolves.toEqual({ collateral: '10' });
    await expect(
      redisCacheService.get(redisCacheService.buildKey('position', 'GCOLD'))
    ).resolves.toBeNull();
    expect(service.getStats()).toMatchObject({ runs: 1, refreshes: 1, failures: 0 });
  });

  it('always refreshes pinned loaders and keeps them when traffic supplies another loader', async () => {
    const pinned = jest.fn().mockResolvedValue({ tvl: '1' });
    const fromTraffic = jest.fn().mockResolvedValue({ tvl: 'stale' });
    service.registerLoader('protocol', 'stats', 30000, pinned);
    service.recordAccess('protocol', 'stats', 30000, fromTraffic);

    await service.runOnce();

    expect(pinned).toHaveBeenCalledTimes(1);
    expect(fromTraffic).not.toHaveBeenCalled();
    expect(service.getStats().hotKeys[0]).toMatchObject({ id: 'stats', pinned: true });
  });

  it('does not refresh an entry that was loaded less than half a TTL ago', async () => {
    const loader = jest.fn().mockResolvedValue({ v: 1 });
    service.registerLoader('protocol', 'stats', 30000, loader);
    service.noteLoaded('protocol', 'stats');

    expect(await service.runOnce()).toMatchObject({ refreshed: 0, skipped: 1 });

    jest.advanceTimersByTime(15000);
    expect(await service.runOnce()).toMatchObject({ refreshed: 1, skipped: 0 });
    expect(loader).toHaveBeenCalledTimes(1);
  });

  it('counts loader failures without aborting the run', async () => {
    service.registerLoader(
      'protocol',
      'stats',
      30000,
      jest.fn().mockRejectedValue(new Error('rpc down'))
    );
    service.registerLoader('pool', 'native', 30000, jest.fn().mockResolvedValue({ ok: true }));

    const result = await service.runOnce();

    expect(result).toMatchObject({ refreshed: 1, failed: 1 });
    expect(service.getStats()).toMatchObject({ failures: 1, refreshes: 1 });
  });

  it('does not cache null loader results', async () => {
    service.registerLoader('protocol', 'stats', 30000, jest.fn().mockResolvedValue(null));
    await service.runOnce();
    await expect(
      redisCacheService.get(redisCacheService.buildKey('protocol', 'stats'))
    ).resolves.toBeNull();
  });

  it('halves access counts every window and drops keys that cool to zero', () => {
    for (let i = 0; i < 4; i += 1) service.recordAccess('position', 'GBUSY', 15000);
    service.recordAccess('position', 'GONCE', 15000);

    jest.advanceTimersByTime(60000);
    service.recordAccess('pool', 'native', 30000);

    const stats = service.getStats();
    expect(stats.trackedKeys).toBe(2);
    expect(stats.hotKeys).toEqual([]);
    expect(service.isHot('position', 'GBUSY')).toBe(false);

    service.recordAccess('position', 'GBUSY', 15000);
    expect(service.isHot('position', 'GBUSY')).toBe(true);
  });

  it('evicts the coldest unpinned key when the tracked-key cap is reached', () => {
    service.registerLoader('protocol', 'stats', 30000, jest.fn());
    for (let i = 0; i < 3; i += 1) service.recordAccess('position', 'GA', 15000);
    service.recordAccess('position', 'GB', 15000);
    service.recordAccess('position', 'GC', 15000);
    expect(service.getStats().trackedKeys).toBe(4);

    service.recordAccess('position', 'GD', 15000);

    const stats = service.getStats();
    expect(stats.trackedKeys).toBe(4);
    expect(stats.evictions).toBe(1);
    expect(service.isHot('position', 'GA')).toBe(true);
    expect(service.getStats().hotKeys.map((k) => k.id)).toEqual(['stats', 'GA']);
  });

  it('limits refreshes per run to maxKeysPerRun, hottest first', async () => {
    config.prefetch.maxKeysPerRun = 1;
    const hot = jest.fn().mockResolvedValue(1);
    const warm = jest.fn().mockResolvedValue(2);
    for (let i = 0; i < 5; i += 1) service.recordAccess('position', 'GHOT', 15000, hot);
    for (let i = 0; i < 3; i += 1) service.recordAccess('position', 'GWARM', 15000, warm);

    await service.runOnce();

    expect(hot).toHaveBeenCalledTimes(1);
    expect(warm).not.toHaveBeenCalled();
  });

  it('shares one run between concurrent callers', async () => {
    const loader = jest.fn().mockResolvedValue(1);
    service.registerLoader('protocol', 'stats', 30000, loader);

    const [a, b] = await Promise.all([service.runOnce(), service.runOnce()]);

    expect(a).toBe(b);
    expect(loader).toHaveBeenCalledTimes(1);
    expect(service.getStats().runs).toBe(1);
  });

  it('starts the scheduler once, runs on the interval and stops cleanly', async () => {
    const loader = jest.fn().mockResolvedValue(1);
    service.registerLoader('protocol', 'stats', 30000, loader);

    expect(service.start()).toBe(true);
    expect(service.start()).toBe(false);
    expect(service.isRunning()).toBe(true);

    await jest.advanceTimersByTimeAsync(1000);
    expect(loader).toHaveBeenCalledTimes(1);

    service.stop();
    expect(service.isRunning()).toBe(false);
    await jest.advanceTimersByTimeAsync(5000);
    expect(loader).toHaveBeenCalledTimes(1);
  });

  it('refuses to start when prefetching is disabled', () => {
    config.prefetch.enabled = false;
    expect(service.start()).toBe(false);
    expect(service.getStats()).toMatchObject({ enabled: false, running: false });
  });

  it('unregisters keys and resets state for tests', () => {
    service.registerLoader('protocol', 'stats', 30000, jest.fn());
    expect(service.unregister('protocol', 'stats')).toBe(true);
    expect(service.unregister('protocol', 'stats')).toBe(false);

    service.recordAccess('pool', 'native', 30000);
    service.resetForTests();
    expect(service.getStats()).toMatchObject({ trackedKeys: 0, runs: 0, evictions: 0 });
  });
});
