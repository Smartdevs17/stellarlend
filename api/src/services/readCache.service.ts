import { config } from '../config';
import { redisCacheService, HotCacheKeyKind } from './redisCache.service';
import { prefetchService } from './prefetch.service';

/**
 * Read cache.
 *
 * One read-through entry point for read-heavy lookups: check the cache, load
 * on a miss (deduplicating concurrent loads of the same key), store the value
 * with its TTL and report the access to the prefetch service so hot keys are
 * refreshed in the background. Invalidation is by kind, by single entry, or
 * across every read kind after a state-changing transaction.
 */

export type ReadCacheLoader<T> = () => Promise<T>;

export interface GetOrLoadOptions {
  /** Report the access to the prefetch service (default true). */
  track?: boolean;
}

export interface ReadCacheKindMetrics {
  hits: number;
  misses: number;
  loads: number;
  loadErrors: number;
  stores: number;
  invalidations: number;
}

export interface ReadCacheStats {
  enabled: boolean;
  defaultTtlMs: number;
  inFlightLoads: number;
  totals: ReadCacheKindMetrics & { hitRate: number };
  kinds: Record<string, ReadCacheKindMetrics>;
}

/** Cache kinds that hold read models and are safe to drop after a write. */
export const READ_CACHE_KINDS: readonly HotCacheKeyKind[] = [
  'protocol',
  'pool',
  'position',
  'gas',
  'price',
  'simulation',
  'http',
];

export function ttlSeconds(ttlMs: number): number {
  return Math.max(1, Math.floor(ttlMs / 1000));
}

export function isReadCacheKind(value: unknown): value is HotCacheKeyKind {
  return typeof value === 'string' && (READ_CACHE_KINDS as readonly string[]).includes(value);
}

function emptyMetrics(): ReadCacheKindMetrics {
  return { hits: 0, misses: 0, loads: 0, loadErrors: 0, stores: 0, invalidations: 0 };
}

export class ReadCacheService {
  private readonly metrics = new Map<string, ReadCacheKindMetrics>();
  private readonly inFlight = new Map<string, Promise<unknown>>();

  buildKey(kind: HotCacheKeyKind, id: string): string {
    return redisCacheService.buildKey(kind, id);
  }

  isEnabled(): boolean {
    return config.readCache.enabled;
  }

  /**
   * Return the cached value for kind/id or load, store and return it.
   * Null and undefined loader results are returned but never cached.
   */
  async getOrLoad<T>(
    kind: HotCacheKeyKind,
    id: string,
    ttlMs: number,
    loader: ReadCacheLoader<T>,
    options: GetOrLoadOptions = {}
  ): Promise<T> {
    const metrics = this.kindMetrics(kind);
    const key = this.buildKey(kind, id);

    if (options.track !== false) {
      prefetchService.recordAccess(kind, id, ttlMs, loader);
    }

    if (!this.isEnabled()) {
      metrics.misses += 1;
      return this.load(kind, id, ttlMs, loader, false);
    }

    const cached = await redisCacheService.get<T>(key);
    if (cached !== null) {
      metrics.hits += 1;
      return cached;
    }
    metrics.misses += 1;

    const pending = this.inFlight.get(key);
    if (pending) {
      return pending as Promise<T>;
    }

    const promise = this.load(kind, id, ttlMs, loader, true).finally(() => {
      this.inFlight.delete(key);
    });
    this.inFlight.set(key, promise);
    return promise;
  }

  /** Load fresh data and overwrite the cache entry regardless of its state. */
  async refresh<T>(
    kind: HotCacheKeyKind,
    id: string,
    ttlMs: number,
    loader: ReadCacheLoader<T>
  ): Promise<T> {
    return this.load(kind, id, ttlMs, loader, this.isEnabled());
  }

  async invalidate(kind: HotCacheKeyKind, id?: string): Promise<void> {
    this.kindMetrics(kind).invalidations += 1;
    if (id) {
      await redisCacheService.del(this.buildKey(kind, id));
      return;
    }
    await redisCacheService.delByPrefix(this.buildKey(kind, ''));
  }

  async invalidateAll(): Promise<void> {
    await Promise.all(READ_CACHE_KINDS.map((kind) => this.invalidate(kind)));
  }

  recordHit(kind: HotCacheKeyKind): void {
    this.kindMetrics(kind).hits += 1;
  }

  recordMiss(kind: HotCacheKeyKind): void {
    this.kindMetrics(kind).misses += 1;
  }

  recordStore(kind: HotCacheKeyKind): void {
    this.kindMetrics(kind).stores += 1;
  }

  getStats(): ReadCacheStats {
    const totals = { ...emptyMetrics(), hitRate: 0 };
    const kinds: Record<string, ReadCacheKindMetrics> = {};
    for (const [kind, metrics] of this.metrics) {
      kinds[kind] = { ...metrics };
      totals.hits += metrics.hits;
      totals.misses += metrics.misses;
      totals.loads += metrics.loads;
      totals.loadErrors += metrics.loadErrors;
      totals.stores += metrics.stores;
      totals.invalidations += metrics.invalidations;
    }
    const lookups = totals.hits + totals.misses;
    totals.hitRate = lookups === 0 ? 0 : Math.round((totals.hits / lookups) * 10000) / 10000;
    return {
      enabled: this.isEnabled(),
      defaultTtlMs: config.readCache.defaultTtlMs,
      inFlightLoads: this.inFlight.size,
      totals,
      kinds,
    };
  }

  resetForTests(): void {
    this.metrics.clear();
    this.inFlight.clear();
  }

  private async load<T>(
    kind: HotCacheKeyKind,
    id: string,
    ttlMs: number,
    loader: ReadCacheLoader<T>,
    store: boolean
  ): Promise<T> {
    const metrics = this.kindMetrics(kind);
    metrics.loads += 1;
    try {
      const value = await loader();
      if (store && value !== undefined && value !== null) {
        await redisCacheService.set(this.buildKey(kind, id), value, ttlSeconds(ttlMs));
        metrics.stores += 1;
        prefetchService.noteLoaded(kind, id);
      }
      return value;
    } catch (error) {
      metrics.loadErrors += 1;
      throw error;
    }
  }

  private kindMetrics(kind: HotCacheKeyKind): ReadCacheKindMetrics {
    let metrics = this.metrics.get(kind);
    if (!metrics) {
      metrics = emptyMetrics();
      this.metrics.set(kind, metrics);
    }
    return metrics;
  }
}

export const readCacheService = new ReadCacheService();
