/**
 * Cache Service
 *
 * In-memory caching layer with TTL support and LRU eviction.
 * Supports Redis too.
 */

import type { CacheEntry } from '../types/index.js';
import { logger } from '../utils/logger.js';

/**
 * Cache config
 */
export interface CacheConfig {
  defaultTtlSeconds: number;
  maxEntries: number;
  /** Fraction of entries to evict in a batch when at capacity (0 < x <= 1) */
  evictBatchFraction: number;
  /** Redis URL (optional) */
  redisUrl?: string;
}

/**
 * Default cache configuration
 */
const DEFAULT_CONFIG: CacheConfig = {
  defaultTtlSeconds: 30,
  maxEntries: 1000,
  evictBatchFraction: 0.1,
};

/**
 * In-memory LRU cache implementation.
 *
 * Access order is maintained by deleting and re-inserting keys into the Map
 * on every read, so the Map's natural insertion order reflects LRU order
 * (oldest = first entry, most-recently-used = last entry).
 */
export class Cache {
  private config: CacheConfig;
  private store: Map<string, CacheEntry<unknown>> = new Map();
  private hits: number = 0;
  private misses: number = 0;
  private evictions: number = 0;

  constructor(config: Partial<CacheConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };

    logger.info('Cache initialized', {
      defaultTtlSeconds: this.config.defaultTtlSeconds,
      maxEntries: this.config.maxEntries,
      evictBatchFraction: this.config.evictBatchFraction,
    });
  }

  /**
   * Get a value from cache.
   * Moves the accessed entry to the "most recently used" position.
   */
  get<T>(key: string): T | undefined {
    const entry = this.store.get(key) as CacheEntry<T> | undefined;

    if (!entry) {
      this.misses++;
      return undefined;
    }

    // Check if expired
    if (Date.now() > entry.expiresAt) {
      this.store.delete(key);
      this.misses++;
      return undefined;
    }

    // Refresh LRU position: delete then re-insert moves key to end of Map
    this.store.delete(key);
    this.store.set(key, entry);

    this.hits++;
    return entry.data;
  }

  /**
   * Set a value in cache with optional TTL.
   * Performs LRU batch eviction when at capacity.
   */
  set<T>(key: string, value: T, ttlSeconds?: number): void {
    const ttl = ttlSeconds ?? this.config.defaultTtlSeconds;
    const now = Date.now();

    // If key already exists, remove it first so it gets a fresh LRU position
    if (this.store.has(key)) {
      this.store.delete(key);
    } else if (this.store.size >= this.config.maxEntries) {
      this.evictLRUBatch();
    }

    const entry: CacheEntry<T> = {
      data: value,
      cachedAt: now,
      expiresAt: now + ttl * 1000,
    };

    this.store.set(key, entry);
  }

  /**
   * Delete a specific key
   */
  delete(key: string): boolean {
    return this.store.delete(key);
  }

  /**
   * Clear all entries
   */
  clear(): void {
    this.store.clear();
    logger.info('Cache cleared');
  }

  /**
   * Check if key exists and is not expired
   */
  has(key: string): boolean {
    const entry = this.store.get(key);

    if (!entry) {
      return false;
    }

    if (Date.now() > entry.expiresAt) {
      this.store.delete(key);
      return false;
    }

    return true;
  }

  /**
   * Get cache statistics including hit rate and eviction count.
   */
  getStats(): {
    size: number;
    hits: number;
    misses: number;
    hitRate: number;
    evictions: number;
  } {
    const total = this.hits + this.misses;
    const hitRate = total > 0 ? this.hits / total : 0;

    logger.debug('Cache stats', {
      size: this.store.size,
      hits: this.hits,
      misses: this.misses,
      hitRate: hitRate.toFixed(4),
      evictions: this.evictions,
    });

    return {
      size: this.store.size,
      hits: this.hits,
      misses: this.misses,
      hitRate,
      evictions: this.evictions,
    };
  }

  /**
   * Evict a batch of least-recently-used entries.
   *
   * The Map preserves insertion order and we refresh position on every get,
   * so the first N keys are always the least recently used.
   * Batch size = ceil(maxEntries * evictBatchFraction), minimum 1.
   */
  private evictLRUBatch(): void {
    const batchSize = Math.max(
      1,
      Math.ceil(this.config.maxEntries * this.config.evictBatchFraction)
    );

    let evicted = 0;
    for (const key of this.store.keys()) {
      if (evicted >= batchSize) break;
      this.store.delete(key);
      evicted++;
    }

    this.evictions += evicted;
    logger.debug(`LRU batch eviction: removed ${evicted} entries`, {
      remaining: this.store.size,
      totalEvictions: this.evictions,
    });
  }

  /**
   * Clean up expired entries periodically
   */
  cleanup(): number {
    const now = Date.now();
    let cleaned = 0;

    for (const [key, entry] of this.store) {
      if (now > entry.expiresAt) {
        this.store.delete(key);
        cleaned++;
      }
    }

    if (cleaned > 0) {
      logger.debug(`Cleaned up ${cleaned} expired cache entries`);
    }

    return cleaned;
  }
}

/**
 * Price-specific cache wrapper
 */
export class PriceCache {
  private cache: Cache;
  private keyPrefix = 'price:';

  constructor(ttlSeconds: number = 30) {
    this.cache = new Cache({
      defaultTtlSeconds: ttlSeconds,
      maxEntries: 100,
    });
  }

  /**
   * Get cached price for an asset
   */
  getPrice(asset: string): bigint | undefined {
    return this.cache.get<bigint>(`${this.keyPrefix}${asset.toUpperCase()}`);
  }

  /**
   * Cache a price for an asset
   */
  setPrice(asset: string, price: bigint, ttlSeconds?: number): void {
    this.cache.set(`${this.keyPrefix}${asset.toUpperCase()}`, price, ttlSeconds);
  }

  /**
   * Check if we have a cached price
   */
  hasPrice(asset: string): boolean {
    return this.cache.has(`${this.keyPrefix}${asset.toUpperCase()}`);
  }

  /**
   * Get cache statistics
   */
  getStats() {
    return this.cache.getStats();
  }

  /**
   * Clear all cached prices
   */
  clear(): void {
    this.cache.clear();
  }
}

/**
 * Create a new cache instance
 */
export function createCache(config?: Partial<CacheConfig>): Cache {
  return new Cache(config);
}

/**
 * Create a price-specific cache
 */
export function createPriceCache(ttlSeconds?: number): PriceCache {
  return new PriceCache(ttlSeconds);
}
