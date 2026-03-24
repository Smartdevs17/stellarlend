/**
 * Tests for Cache Service
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { Cache, PriceCache, createCache, createPriceCache } from '../src/services/cache.js';

describe('Cache', () => {
  let cache: Cache;

  beforeEach(() => {
    cache = createCache({
      defaultTtlSeconds: 10,
      maxEntries: 100,
    });
  });

  describe('get/set', () => {
    it('should store and retrieve values', () => {
      cache.set('key1', 'value1');

      expect(cache.get('key1')).toBe('value1');
    });

    it('should return undefined for missing keys', () => {
      expect(cache.get('nonexistent')).toBeUndefined();
    });

    it('should handle different data types', () => {
      cache.set('string', 'hello');
      cache.set('number', 42);
      cache.set('object', { foo: 'bar' });
      cache.set('array', [1, 2, 3]);
      cache.set('bigint', 12345678901234567890n);

      expect(cache.get('string')).toBe('hello');
      expect(cache.get('number')).toBe(42);
      expect(cache.get('object')).toEqual({ foo: 'bar' });
      expect(cache.get('array')).toEqual([1, 2, 3]);
      expect(cache.get('bigint')).toBe(12345678901234567890n);
    });
  });

  describe('TTL expiration', () => {
    it('should expire entries after TTL', async () => {
      cache = createCache({ defaultTtlSeconds: 0.1 });
      cache.set('temp', 'value');

      expect(cache.get('temp')).toBe('value');

      await new Promise((r) => setTimeout(r, 150));

      expect(cache.get('temp')).toBeUndefined();
    });

    it('should use custom TTL when provided', async () => {
      cache.set('custom', 'value', 0.05);

      expect(cache.get('custom')).toBe('value');

      await new Promise((r) => setTimeout(r, 100));

      expect(cache.get('custom')).toBeUndefined();
    });
  });

  describe('has', () => {
    it('should return true for existing keys', () => {
      cache.set('exists', 'value');

      expect(cache.has('exists')).toBe(true);
    });

    it('should return false for missing keys', () => {
      expect(cache.has('missing')).toBe(false);
    });

    it('should return false for expired keys', async () => {
      cache = createCache({ defaultTtlSeconds: 0.05 });
      cache.set('expires', 'value');

      await new Promise((r) => setTimeout(r, 100));

      expect(cache.has('expires')).toBe(false);
    });
  });

  describe('delete', () => {
    it('should delete existing keys', () => {
      cache.set('toDelete', 'value');

      expect(cache.delete('toDelete')).toBe(true);
      expect(cache.get('toDelete')).toBeUndefined();
    });

    it('should return false for non-existent keys', () => {
      expect(cache.delete('nonexistent')).toBe(false);
    });
  });

  describe('clear', () => {
    it('should remove all entries', () => {
      cache.set('key1', 'value1');
      cache.set('key2', 'value2');
      cache.set('key3', 'value3');

      cache.clear();

      expect(cache.get('key1')).toBeUndefined();
      expect(cache.get('key2')).toBeUndefined();
      expect(cache.get('key3')).toBeUndefined();
    });
  });

  describe('stats', () => {
    it('should track hits and misses', () => {
      cache.set('hit', 'value');

      cache.get('hit');
      cache.get('hit');
      cache.get('miss');

      const stats = cache.getStats();

      expect(stats.hits).toBe(2);
      expect(stats.misses).toBe(1);
      expect(stats.hitRate).toBeCloseTo(0.667, 2);
    });

    it('should track size', () => {
      cache.set('a', 1);
      cache.set('b', 2);
      cache.set('c', 3);

      const stats = cache.getStats();

      expect(stats.size).toBe(3);
    });

    it('should report eviction count', () => {
      // maxEntries=3, batch=10% => ceil(0.3)=1 eviction per batch
      cache = createCache({ maxEntries: 3, evictBatchFraction: 0.1 });

      cache.set('a', 1);
      cache.set('b', 2);
      cache.set('c', 3);
      cache.set('d', 4); // triggers eviction

      const stats = cache.getStats();
      expect(stats.evictions).toBeGreaterThanOrEqual(1);
    });
  });

  describe('LRU eviction', () => {
    it('should evict least recently used entry first (single eviction)', () => {
      // maxEntries=3, batch fraction=0.1 => ceil(3*0.1)=ceil(0.3)=1 eviction
      cache = createCache({ maxEntries: 3, evictBatchFraction: 0.1 });

      cache.set('first', 1);
      cache.set('second', 2);
      cache.set('third', 3);

      // Access 'first' to make it recently used — 'second' becomes LRU
      cache.get('first');

      // Adding 'fourth' should evict 'second' (LRU)
      cache.set('fourth', 4);

      expect(cache.get('second')).toBeUndefined(); // evicted
      expect(cache.get('first')).toBe(1);
      expect(cache.get('third')).toBe(3);
      expect(cache.get('fourth')).toBe(4);
    });

    it('should evict a batch of LRU entries when at capacity', () => {
      // maxEntries=10, batch=10% => ceil(1)=1; use 50% to evict 5
      cache = createCache({ maxEntries: 10, evictBatchFraction: 0.5 });

      for (let i = 0; i < 10; i++) {
        cache.set(`key${i}`, i);
      }

      // Access keys 5-9 to make them recently used; keys 0-4 are LRU
      for (let i = 5; i < 10; i++) {
        cache.get(`key${i}`);
      }

      // Adding one more triggers batch eviction of 5 LRU entries (keys 0-4)
      cache.set('new', 99);

      for (let i = 0; i < 5; i++) {
        expect(cache.get(`key${i}`)).toBeUndefined();
      }
      for (let i = 5; i < 10; i++) {
        expect(cache.get(`key${i}`)).toBe(i);
      }
      expect(cache.get('new')).toBe(99);
    });

    it('should evict 10% batch by default when at capacity', () => {
      // maxEntries=10, default 10% => ceil(1)=1 eviction per trigger
      cache = createCache({ maxEntries: 10, evictBatchFraction: 0.1 });

      for (let i = 0; i < 10; i++) {
        cache.set(`key${i}`, i);
      }

      // key0 is LRU; adding one more should evict key0
      cache.set('extra', 100);

      expect(cache.get('key0')).toBeUndefined();
      expect(cache.getStats().evictions).toBe(1);
    });

    it('should update LRU order when a key is overwritten', () => {
      // maxEntries=3, fraction=0.1 => ceil(0.3)=1 eviction per batch
      cache = createCache({ maxEntries: 3, evictBatchFraction: 0.1 });

      cache.set('a', 1);
      cache.set('b', 2);
      cache.set('c', 3);

      // Overwrite 'a' — it should move to most-recently-used position
      cache.set('a', 10);

      // 'b' is now LRU; adding 'd' should evict 'b'
      cache.set('d', 4);

      expect(cache.get('b')).toBeUndefined();
      expect(cache.get('a')).toBe(10);
      expect(cache.get('c')).toBe(3);
      expect(cache.get('d')).toBe(4);
    });
  });

  describe('eviction under load', () => {
    it('should handle rapid insertions without exceeding maxEntries by more than batchSize', () => {
      const maxEntries = 100;
      const batchFraction = 0.1;
      cache = createCache({ maxEntries, evictBatchFraction: batchFraction });

      // Insert 200 entries — cache should never grow beyond maxEntries
      for (let i = 0; i < 200; i++) {
        cache.set(`load-key-${i}`, i);
        expect(cache.getStats().size).toBeLessThanOrEqual(maxEntries);
      }

      const stats = cache.getStats();
      expect(stats.evictions).toBeGreaterThan(0);
      expect(stats.size).toBeLessThanOrEqual(maxEntries);
    });

    it('should maintain high hit rate when recently set keys are accessed', () => {
      cache = createCache({ maxEntries: 50, evictBatchFraction: 0.1 });

      // Fill cache
      for (let i = 0; i < 50; i++) {
        cache.set(`k${i}`, i);
      }

      // Access all keys (hits)
      for (let i = 0; i < 50; i++) {
        cache.get(`k${i}`);
      }

      const stats = cache.getStats();
      expect(stats.hitRate).toBeGreaterThan(0.8);
    });
  });

  describe('cleanup', () => {
    it('should remove expired entries', async () => {
      cache = createCache({ defaultTtlSeconds: 0.05 });

      cache.set('expire1', 1);
      cache.set('expire2', 2);

      await new Promise((r) => setTimeout(r, 100));

      const cleaned = cache.cleanup();

      expect(cleaned).toBe(2);
      expect(cache.getStats().size).toBe(0);
    });
  });
});

describe('PriceCache', () => {
  let priceCache: PriceCache;

  beforeEach(() => {
    priceCache = createPriceCache(30);
  });

  describe('price operations', () => {
    it('should store and retrieve prices as bigint', () => {
      const price = 150000n;

      priceCache.setPrice('XLM', price);

      expect(priceCache.getPrice('XLM')).toBe(price);
    });

    it('should normalize asset symbols to uppercase', () => {
      priceCache.setPrice('xlm', 150000n);

      expect(priceCache.getPrice('XLM')).toBe(150000n);
      expect(priceCache.getPrice('xlm')).toBe(150000n);
    });

    it('should check if price exists', () => {
      priceCache.setPrice('BTC', 50000000000n);

      expect(priceCache.hasPrice('BTC')).toBe(true);
      expect(priceCache.hasPrice('ETH')).toBe(false);
    });
  });

  describe('clear', () => {
    it('should clear all prices', () => {
      priceCache.setPrice('XLM', 150000n);
      priceCache.setPrice('BTC', 50000000000n);

      priceCache.clear();

      expect(priceCache.hasPrice('XLM')).toBe(false);
      expect(priceCache.hasPrice('BTC')).toBe(false);
    });
  });

  describe('stats', () => {
    it('should return cache statistics', () => {
      priceCache.setPrice('XLM', 150000n);
      priceCache.getPrice('XLM');
      priceCache.getPrice('ETH');

      const stats = priceCache.getStats();

      expect(stats.hits).toBe(1);
      expect(stats.misses).toBe(1);
    });

    it('should include eviction count in stats', () => {
      const stats = priceCache.getStats();
      expect(stats.evictions).toBeDefined();
      expect(stats.evictions).toBeGreaterThanOrEqual(0);
    });
  });
});
