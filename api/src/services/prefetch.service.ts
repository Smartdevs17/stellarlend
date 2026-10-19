import { config } from '../config';
import logger from '../utils/logger';
import { redisCacheService, HotCacheKeyKind } from './redisCache.service';

/**
 * Prefetch service.
 *
 * Tracks how often each cache entry is read and refreshes the hot ones in the
 * background before they expire, so the request path serves them from cache
 * instead of paying an RPC round trip. Access counts decay every window
 * (halved) so keys that stop being read cool down and drop out.
 *
 * Loaders are supplied by callers of the read cache (see readCache.service)
 * or registered explicitly for entries that must always stay warm.
 */

export type PrefetchLoader = () => Promise<unknown>;

interface TrackedEntry {
  kind: HotCacheKeyKind;
  id: string;
  ttlMs: number;
  loader: PrefetchLoader | null;
  pinned: boolean;
  count: number;
  lastAccessAt: number;
  lastRefreshAt: number;
  refreshes: number;
  failures: number;
}

export interface PrefetchRunResult {
  refreshed: number;
  failed: number;
  skipped: number;
  durationMs: number;
}

export interface PrefetchHotKey {
  key: string;
  kind: HotCacheKeyKind;
  id: string;
  count: number;
  pinned: boolean;
  hasLoader: boolean;
  refreshes: number;
  failures: number;
  lastRefreshAt: string | null;
}

export interface PrefetchStats {
  enabled: boolean;
  running: boolean;
  trackedKeys: number;
  hotKeys: PrefetchHotKey[];
  runs: number;
  refreshes: number;
  failures: number;
  evictions: number;
  lastRunAt: string | null;
  lastRunMs: number;
  config: {
    intervalMs: number;
    windowMs: number;
    hotThreshold: number;
    maxTrackedKeys: number;
    maxKeysPerRun: number;
  };
}

const HOT_KEY_REPORT_LIMIT = 20;

function ttlSeconds(ttlMs: number): number {
  return Math.max(1, Math.floor(ttlMs / 1000));
}

export class PrefetchService {
  private readonly entries = new Map<string, TrackedEntry>();
  private timer: NodeJS.Timeout | null = null;
  private lastDecayAt = Date.now();
  private runInFlight: Promise<PrefetchRunResult> | null = null;
  private runs = 0;
  private refreshes = 0;
  private failures = 0;
  private evictions = 0;
  private lastRunAt: string | null = null;
  private lastRunMs = 0;

  buildKey(kind: HotCacheKeyKind, id: string): string {
    return redisCacheService.buildKey(kind, id);
  }

  /** Count one read of a cache entry. The loader, when given, enables refreshes. */
  recordAccess(kind: HotCacheKeyKind, id: string, ttlMs: number, loader?: PrefetchLoader): void {
    this.decayIfDue();
    const entry = this.getOrCreate(kind, id, ttlMs);
    entry.count += 1;
    entry.lastAccessAt = Date.now();
    entry.ttlMs = ttlMs;
    if (loader && !entry.pinned) {
      entry.loader = loader;
    }
  }

  /** Mark an entry as always hot; it is refreshed on every run regardless of traffic. */
  registerLoader(kind: HotCacheKeyKind, id: string, ttlMs: number, loader: PrefetchLoader): void {
    const entry = this.getOrCreate(kind, id, ttlMs);
    entry.pinned = true;
    entry.loader = loader;
    entry.ttlMs = ttlMs;
  }

  unregister(kind: HotCacheKeyKind, id: string): boolean {
    return this.entries.delete(this.buildKey(kind, id));
  }

  /** Record that the request path just loaded this entry, so it is not refreshed again immediately. */
  noteLoaded(kind: HotCacheKeyKind, id: string): void {
    const entry = this.entries.get(this.buildKey(kind, id));
    if (entry) {
      entry.lastRefreshAt = Date.now();
    }
  }

  isHot(kind: HotCacheKeyKind, id: string): boolean {
    const entry = this.entries.get(this.buildKey(kind, id));
    return entry ? this.entryIsHot(entry) : false;
  }

  /** Refresh every hot entry that has a loader. Concurrent calls share one run. */
  async runOnce(): Promise<PrefetchRunResult> {
    if (this.runInFlight) {
      return this.runInFlight;
    }
    this.runInFlight = this.execute().finally(() => {
      this.runInFlight = null;
    });
    return this.runInFlight;
  }

  start(): boolean {
    if (this.timer || !config.prefetch.enabled) {
      return false;
    }
    this.timer = setInterval(() => {
      void this.runOnce();
    }, config.prefetch.intervalMs);
    if (typeof this.timer.unref === 'function') {
      this.timer.unref();
    }
    logger.info('Prefetch scheduler started', {
      intervalMs: config.prefetch.intervalMs,
      hotThreshold: config.prefetch.hotThreshold,
    });
    return true;
  }

  stop(): void {
    if (!this.timer) {
      return;
    }
    clearInterval(this.timer);
    this.timer = null;
    logger.info('Prefetch scheduler stopped');
  }

  isRunning(): boolean {
    return this.timer !== null;
  }

  getStats(): PrefetchStats {
    this.decayIfDue();
    return {
      enabled: config.prefetch.enabled,
      running: this.isRunning(),
      trackedKeys: this.entries.size,
      hotKeys: this.selectHotEntries(HOT_KEY_REPORT_LIMIT).map((entry) => ({
        key: this.buildKey(entry.kind, entry.id),
        kind: entry.kind,
        id: entry.id,
        count: entry.count,
        pinned: entry.pinned,
        hasLoader: entry.loader !== null,
        refreshes: entry.refreshes,
        failures: entry.failures,
        lastRefreshAt: entry.lastRefreshAt ? new Date(entry.lastRefreshAt).toISOString() : null,
      })),
      runs: this.runs,
      refreshes: this.refreshes,
      failures: this.failures,
      evictions: this.evictions,
      lastRunAt: this.lastRunAt,
      lastRunMs: this.lastRunMs,
      config: {
        intervalMs: config.prefetch.intervalMs,
        windowMs: config.prefetch.windowMs,
        hotThreshold: config.prefetch.hotThreshold,
        maxTrackedKeys: config.prefetch.maxTrackedKeys,
        maxKeysPerRun: config.prefetch.maxKeysPerRun,
      },
    };
  }

  resetForTests(): void {
    this.stop();
    this.entries.clear();
    this.runInFlight = null;
    this.lastDecayAt = Date.now();
    this.runs = 0;
    this.refreshes = 0;
    this.failures = 0;
    this.evictions = 0;
    this.lastRunAt = null;
    this.lastRunMs = 0;
  }

  private async execute(): Promise<PrefetchRunResult> {
    const startedAt = Date.now();
    this.decayIfDue(startedAt);
    this.runs += 1;

    let refreshed = 0;
    let failed = 0;
    let skipped = 0;

    for (const entry of this.selectHotEntries(config.prefetch.maxKeysPerRun)) {
      if (!entry.loader) {
        skipped += 1;
        continue;
      }
      // Refresh-ahead: an entry loaded less than half a TTL ago is still fresh.
      const refreshDueAt = entry.lastRefreshAt + Math.floor(entry.ttlMs / 2);
      if (entry.lastRefreshAt > 0 && Date.now() < refreshDueAt) {
        skipped += 1;
        continue;
      }

      const key = this.buildKey(entry.kind, entry.id);
      try {
        const value = await entry.loader();
        if (value !== undefined && value !== null) {
          await redisCacheService.set(key, value, ttlSeconds(entry.ttlMs));
        }
        entry.lastRefreshAt = Date.now();
        entry.refreshes += 1;
        refreshed += 1;
        this.refreshes += 1;
      } catch (error) {
        entry.failures += 1;
        failed += 1;
        this.failures += 1;
        logger.warn('Prefetch refresh failed', { key, error });
      }
    }

    const durationMs = Date.now() - startedAt;
    this.lastRunAt = new Date().toISOString();
    this.lastRunMs = durationMs;
    return { refreshed, failed, skipped, durationMs };
  }

  private entryIsHot(entry: TrackedEntry): boolean {
    return entry.pinned || entry.count >= config.prefetch.hotThreshold;
  }

  private selectHotEntries(limit: number): TrackedEntry[] {
    return Array.from(this.entries.values())
      .filter((entry) => this.entryIsHot(entry))
      .sort((a, b) => {
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
        if (a.count !== b.count) return b.count - a.count;
        return b.lastAccessAt - a.lastAccessAt;
      })
      .slice(0, limit);
  }

  private getOrCreate(kind: HotCacheKeyKind, id: string, ttlMs: number): TrackedEntry {
    const key = this.buildKey(kind, id);
    const existing = this.entries.get(key);
    if (existing) {
      return existing;
    }
    this.makeRoom();
    const entry: TrackedEntry = {
      kind,
      id,
      ttlMs,
      loader: null,
      pinned: false,
      count: 0,
      lastAccessAt: 0,
      lastRefreshAt: 0,
      refreshes: 0,
      failures: 0,
    };
    this.entries.set(key, entry);
    return entry;
  }

  /** Evict the coldest unpinned entry once the tracked-key cap is reached. */
  private makeRoom(): void {
    if (this.entries.size < config.prefetch.maxTrackedKeys) {
      return;
    }
    let victimKey: string | null = null;
    let victim: TrackedEntry | null = null;
    for (const [key, entry] of this.entries) {
      if (entry.pinned) continue;
      if (
        !victim ||
        entry.count < victim.count ||
        (entry.count === victim.count && entry.lastAccessAt < victim.lastAccessAt)
      ) {
        victim = entry;
        victimKey = key;
      }
    }
    if (victimKey) {
      this.entries.delete(victimKey);
      this.evictions += 1;
    }
  }

  private decayIfDue(now = Date.now()): void {
    if (now - this.lastDecayAt < config.prefetch.windowMs) {
      return;
    }
    for (const [key, entry] of this.entries) {
      entry.count = Math.floor(entry.count / 2);
      if (entry.count === 0 && !entry.pinned) {
        this.entries.delete(key);
      }
    }
    this.lastDecayAt = now;
  }
}

export const prefetchService = new PrefetchService();
