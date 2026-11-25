import { describe, expect, it, vi } from 'vitest';
import { OraclePriceCache, type BlockClock, type PriceQuote } from '../oraclePrice.js';

function clock(block: number): BlockClock & { setBlock(next: number): void } {
  let current = block;
  return {
    block: () => current,
    now: () => current * 1000,
    setBlock(next) {
      current = next;
    },
  };
}

function quote(asset: string, price: number): PriceQuote {
  return { asset, price, decimals: 7, timestamp: 1_700_000_000, confidence: 100 };
}

describe('OraclePriceCache', () => {
  it('shares one fetch across concurrent reads in the same block', async () => {
    const fakeClock = clock(10);
    const fetch = vi.fn().mockResolvedValue(quote('XLM', 100));
    const cache = new OraclePriceCache(fakeClock, 10, vi.fn());

    const [first, second] = await Promise.all([
      cache.getOrFetch('XLM', fetch),
      cache.getOrFetch('XLM', fetch),
    ]);

    expect(first.price).toBe(100);
    expect(second.price).toBe(100);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it('refetches after the block advances', async () => {
    const fakeClock = clock(10);
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(quote('XLM', 100))
      .mockResolvedValueOnce(quote('XLM', 120));
    const cache = new OraclePriceCache(fakeClock, 10, vi.fn());

    await cache.getOrFetch('XLM', fetch);
    fakeClock.setBlock(11);
    const second = await cache.getOrFetch('XLM', fetch);

    expect(second.price).toBe(120);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it('clears cached quotes on invalidate', async () => {
    const fakeClock = clock(10);
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(quote('XLM', 100))
      .mockResolvedValueOnce(quote('XLM', 130));
    const cache = new OraclePriceCache(fakeClock, 10, vi.fn());

    await cache.getOrFetch('XLM', fetch);
    cache.invalidate('XLM');
    const second = await cache.getOrFetch('XLM', fetch);

    expect(second.price).toBe(130);
    expect(cache.size).toBe(1);
  });

  it('retries after a rejected fetch', async () => {
    const cache = new OraclePriceCache(clock(10), 10, vi.fn());
    const fetch = vi
      .fn()
      .mockRejectedValueOnce(new Error('rpc down'))
      .mockResolvedValueOnce(quote('XLM', 100));

    await expect(cache.getOrFetch('XLM', fetch)).rejects.toThrow('rpc down');
    await expect(cache.getOrFetch('XLM', fetch)).resolves.toMatchObject({ price: 100 });
    expect(fetch).toHaveBeenCalledTimes(2);
  });
});
