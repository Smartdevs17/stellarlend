import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CachedPriceProvider } from '../src/providers/cached-provider.js';
import type { PriceProvider } from '../src/providers/price-provider.js';
import type { RawPriceData } from '../src/types/index.js';
import { createAggregator } from '../src/services/price-aggregator.js';
import { createPriceCache } from '../src/services/cache.js';
import { createValidator } from '../src/services/price-validator.js';
import {
  asPriceProvider,
  createProviderContainer,
  ProviderContainer,
} from '../src/providers/provider-container.js';

vi.mock('../src/utils/logger.js', () => ({
  configureLogger: vi.fn(),
  logger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));

function quote(asset = 'XLM', source = 'test'): RawPriceData {
  return { asset, source, price: 1, timestamp: Math.floor(Date.now() / 1000) };
}

function provider(name = 'test') {
  return {
    name,
    priority: 1,
    weight: 1,
    isEnabled: true,
    fetchPrice: vi.fn(async (asset: string) => quote(asset, name)),
    fetchPrices: vi.fn(async (assets: string[]) => assets.map((asset) => quote(asset, name))),
    healthCheck: vi.fn(async () => ({ provider: name, healthy: true, lastCheck: Date.now() })),
  } satisfies PriceProvider;
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-11-22T00:00:00Z'));
});
afterEach(() => vi.useRealTimers());

describe('common cached price adapter', () => {
  it('preserves provider metadata and source timestamps without exposing mutable cache entries', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 10 });
    const first = await cached.fetchPrice('xlm');
    const original = { ...first };
    first.price = 999;
    vi.advanceTimersByTime(1000);
    expect(await cached.fetchPrice('XLM')).toEqual(original);
    expect(live.fetchPrice).toHaveBeenCalledTimes(1);
    expect(live.fetchPrice).toHaveBeenCalledWith('XLM');
    expect([cached.name, cached.priority, cached.weight, cached.isEnabled]).toEqual([
      'test',
      1,
      1,
      true,
    ]);
  });

  it('expires at the exact TTL boundary and supports disabling the cache', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 1 });
    await cached.fetchPrice('XLM');
    vi.advanceTimersByTime(999);
    await cached.fetchPrice('XLM');
    expect(live.fetchPrice).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    await cached.fetchPrice('XLM');
    expect(live.fetchPrice).toHaveBeenCalledTimes(2);
    const disabled = new CachedPriceProvider(live, { ttlSeconds: 0 });
    await disabled.fetchPrice('XLM');
    await disabled.fetchPrice('XLM');
    expect(live.fetchPrice).toHaveBeenCalledTimes(4);
  });

  it('coalesces concurrent single-asset requests', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 1 });
    const results = await Promise.all([cached.fetchPrice('XLM'), cached.fetchPrice('xlm')]);
    expect(live.fetchPrice).toHaveBeenCalledTimes(1);
    expect(results[0]).toEqual(results[1]);
    expect(results[0]).not.toBe(results[1]);
  });

  it('propagates errors to the caller and retries instead of caching a failure', async () => {
    const live = provider();
    live.fetchPrice.mockRejectedValueOnce(new Error('unavailable'));
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30 });
    await expect(cached.fetchPrice('XLM')).rejects.toThrow('unavailable');
    await expect(cached.fetchPrice('XLM')).resolves.toMatchObject({ price: 1 });
    expect(live.fetchPrice).toHaveBeenCalledTimes(2);
  });

  it('isolates assets and provider instances', async () => {
    const first = provider('first');
    const second = provider('second');
    const a = new CachedPriceProvider(first, { ttlSeconds: 30 });
    const b = new CachedPriceProvider(second, { ttlSeconds: 30 });
    await a.fetchPrice('XLM');
    expect(await a.fetchPrice('BTC')).toMatchObject({ asset: 'BTC', source: 'first' });
    expect(await b.fetchPrice('XLM')).toMatchObject({ source: 'second' });
    expect(first.fetchPrice).toHaveBeenCalledTimes(2);
    expect(second.fetchPrice).toHaveBeenCalledTimes(1);
  });

  it('evicts the least recently used quote at capacity', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30, maxEntries: 2 });
    await cached.fetchPrice('XLM');
    await cached.fetchPrice('BTC');
    await cached.fetchPrice('XLM');
    await cached.fetchPrice('ETH');
    await cached.fetchPrice('XLM');
    await cached.fetchPrice('BTC');
    expect(live.fetchPrice.mock.calls.map(([asset]) => asset)).toEqual([
      'XLM',
      'BTC',
      'ETH',
      'BTC',
    ]);
  });

  it('does not retain invalid or future-dated quotes', async () => {
    const live = provider();
    live.fetchPrice.mockResolvedValueOnce({ ...quote(), price: NaN });
    live.fetchPrice.mockResolvedValueOnce({ ...quote(), timestamp: Date.now() / 1000 + 100 });
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30 });
    await cached.fetchPrice('XLM');
    await cached.fetchPrice('XLM');
    await cached.fetchPrice('XLM');
    expect(live.fetchPrice).toHaveBeenCalledTimes(3);
  });

  it('does not extend cache lifetime when the clock moves backwards', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30 });
    await cached.fetchPrice('XLM');
    vi.setSystemTime(Date.now() - 1000);
    await cached.fetchPrice('XLM');
    expect(live.fetchPrice).toHaveBeenCalledTimes(2);
  });

  it('expires at the source-age limit even when the cache TTL is longer', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30, maxStalenessSeconds: 1 });
    await cached.fetchPrice('XLM');
    vi.advanceTimersByTime(1000);
    await cached.fetchPrice('XLM');
    expect(live.fetchPrice).toHaveBeenCalledTimes(2);
  });

  it('does not retain a successful response whose source timestamp is already stale', async () => {
    const live = provider();
    live.fetchPrice.mockResolvedValueOnce({ ...quote(), timestamp: Date.now() / 1000 - 100 });
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30 });
    await cached.fetchPrice('XLM');
    await cached.fetchPrice('XLM');
    expect(live.fetchPrice).toHaveBeenCalledTimes(2);
  });

  it('keeps native batch fetching and partial successes while reusing cached assets', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30 });
    await cached.fetchPrice('XLM');
    live.fetchPrices.mockResolvedValueOnce([quote('BTC')]);
    expect((await cached.fetchPrices(['xlm', 'BTC', 'ETH'])).map((p) => p.asset)).toEqual([
      'XLM',
      'BTC',
    ]);
    expect(live.fetchPrices).toHaveBeenCalledWith(['BTC', 'ETH']);
    expect((await cached.fetchPrices(['XLM', 'BTC'])).length).toBe(2);
    expect(live.fetchPrices).toHaveBeenCalledTimes(1);
  });

  it('does not mask a failed batch and always performs live health probes', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30 });
    await cached.fetchPrice('XLM');
    live.fetchPrices.mockRejectedValueOnce(new Error('batch unavailable'));
    await expect(cached.fetchPrices(['BTC'])).rejects.toThrow('batch unavailable');
    await cached.healthCheck();
    await cached.healthCheck();
    expect(live.healthCheck).toHaveBeenCalledTimes(2);
  });

  it('accepts structural adapters in the real aggregator and retains failure fallback', async () => {
    const primary = provider('primary');
    primary.fetchPrice.mockRejectedValue(new Error('down'));
    const fallback = provider('fallback');
    fallback.priority = 2;
    const aggregator = createAggregator(
      [
        new CachedPriceProvider(primary, { ttlSeconds: 30 }),
        new CachedPriceProvider(fallback, { ttlSeconds: 30 }),
      ],
      createValidator(),
      createPriceCache(0),
      { failoverMode: true }
    );
    const result = await aggregator.getPrice('XLM');
    expect(result?.sources.map((p) => p.source)).toEqual(['fallback']);
    expect(primary.fetchPrice).toHaveBeenCalledTimes(1);
    expect(fallback.fetchPrice).toHaveBeenCalledTimes(1);
  });

  it('rejects invalid TTL and capacity settings', () => {
    for (const ttlSeconds of [-1, Infinity, NaN]) {
      expect(() => new CachedPriceProvider(provider(), { ttlSeconds })).toThrow('TTL');
    }
    expect(() => new CachedPriceProvider(provider(), { ttlSeconds: 1, maxEntries: 0 })).toThrow(
      'capacity'
    );
  });
});

describe('structural adapter container integration', () => {
  it('resolves cached and live adapters in enabled priority order without base-class inheritance', async () => {
    const first = provider('first');
    const second = provider('second');
    second.priority = 2;
    const disabled = { ...provider('disabled'), isEnabled: false };
    const cached = new CachedPriceProvider(first, { ttlSeconds: 30 });
    const container = createProviderContainer()
      .register('first', () => asPriceProvider(cached))
      .register('second', () => second)
      .register('disabled', () => disabled);
    const chain = container.resolveChain(['second', 'disabled', 'first']);
    expect(chain.map((adapter) => adapter.name)).toEqual(['first', 'second']);
    await chain[0]!.fetchPrice('XLM');
    await container.resolve('first').fetchPrice('XLM');
    expect(first.fetchPrice).toHaveBeenCalledTimes(1);
  });

  it('rejects duplicate and unknown adapter registrations', () => {
    const container = new ProviderContainer().register('test', () => provider());
    expect(() => container.register('test', () => provider())).toThrow('already registered');
    expect(() => container.resolve('unknown')).toThrow('not registered');
  });

  it('returns live health results for enabled structural adapters', async () => {
    const live = provider();
    const cached = new CachedPriceProvider(live, { ttlSeconds: 30 });
    const container = new ProviderContainer().register('test', () => cached);
    await cached.fetchPrice('XLM');
    const health = await container.healthCheck(['test']);
    expect(health).toEqual([{ provider: 'test', healthy: true, lastCheck: Date.now() }]);
    expect(live.healthCheck).toHaveBeenCalledTimes(1);
  });
});
