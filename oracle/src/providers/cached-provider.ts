import type { HealthStatus, RawPriceData } from '../types/index.js';
import { logger } from '../utils/logger.js';
import type { PriceProvider } from './price-provider.js';

export interface ProviderCacheConfig {
  ttlSeconds: number;
  maxEntries?: number;
  /** Source-age bound, independent of the time spent in cache. */
  maxStalenessSeconds?: number;
}

/** Bounded, per-provider quote cache. Quotes retain their original source time. */
export class CachedPriceProvider implements PriceProvider {
  private readonly entries = new Map<
    string,
    { quote: RawPriceData; cachedAt: number; expiresAt: number }
  >();
  private readonly pending = new Map<string, Promise<RawPriceData>>();
  private readonly maxEntries: number;
  private readonly maxStalenessSeconds: number;

  constructor(
    private readonly provider: PriceProvider,
    private readonly config: ProviderCacheConfig
  ) {
    this.maxEntries = config.maxEntries ?? 1000;
    this.maxStalenessSeconds = config.maxStalenessSeconds ?? config.ttlSeconds;
    if (!Number.isFinite(config.ttlSeconds) || config.ttlSeconds < 0) {
      throw new Error('Provider cache TTL must be finite and non-negative');
    }
    if (!Number.isFinite(this.maxStalenessSeconds) || this.maxStalenessSeconds < 0) {
      throw new Error('Provider cache source age must be finite and non-negative');
    }
    if (!Number.isSafeInteger(this.maxEntries) || this.maxEntries < 1) {
      throw new Error('Provider cache capacity must be a positive integer');
    }
  }

  get name(): string {
    return this.provider.name;
  }
  get priority(): number {
    return this.provider.priority;
  }
  get weight(): number {
    return this.provider.weight;
  }
  get isEnabled(): boolean {
    return this.provider.isEnabled;
  }

  private read(asset: string): RawPriceData | undefined {
    const entry = this.entries.get(asset);
    if (!entry) return undefined;
    if (Date.now() >= entry.expiresAt || Date.now() < entry.cachedAt) {
      this.entries.delete(asset);
      return undefined;
    }
    this.entries.delete(asset);
    this.entries.set(asset, entry);
    logger.debug('Oracle adapter cache hit', { provider: this.name, asset });
    return { ...entry.quote };
  }

  private store(asset: string, quote: RawPriceData): void {
    // Invalid responses still reach the validator, but never poison the cache.
    if (
      this.config.ttlSeconds === 0 ||
      quote.asset.toUpperCase() !== asset ||
      quote.source !== this.name ||
      !Number.isFinite(quote.price) ||
      quote.price <= 0 ||
      !Number.isFinite(quote.timestamp) ||
      quote.timestamp < 0 ||
      quote.timestamp > Date.now() / 1000
    )
      return;
    const expiresAt = Math.min(
      Date.now() + this.config.ttlSeconds * 1000,
      (quote.timestamp + this.maxStalenessSeconds) * 1000
    );
    if (expiresAt <= Date.now()) return;
    this.entries.delete(asset);
    if (this.entries.size >= this.maxEntries) {
      const oldest = this.entries.keys().next().value;
      if (oldest !== undefined) this.entries.delete(oldest);
    }
    this.entries.set(asset, {
      quote: { ...quote },
      cachedAt: Date.now(),
      expiresAt,
    });
  }

  async fetchPrice(asset: string): Promise<RawPriceData> {
    const key = asset.toUpperCase();
    const cached = this.read(key);
    if (cached) return cached;
    const existing = this.pending.get(key);
    if (existing) return { ...(await existing) };
    logger.debug('Oracle adapter cache miss', { provider: this.name, asset: key });
    const request = Promise.resolve()
      .then(() => this.provider.fetchPrice(key))
      .then((quote) => {
        this.store(key, quote);
        return { ...quote };
      })
      .finally(() => this.pending.delete(key));
    this.pending.set(key, request);
    return { ...(await request) };
  }

  async fetchPrices(assets: string[]): Promise<RawPriceData[]> {
    const keys = [...new Set(assets.map((asset) => asset.toUpperCase()))];
    const quotes = new Map<string, RawPriceData>();
    const missing: string[] = [];
    for (const key of keys) {
      const cached = this.read(key);
      if (cached) quotes.set(key, cached);
      else missing.push(key);
    }
    // Preserve provider-specific batch endpoints and partial-success semantics.
    if (missing.length) {
      const fetched = await this.provider.fetchPrices(missing);
      for (const quote of fetched) {
        const key = quote.asset.toUpperCase();
        if (!missing.includes(key)) continue;
        this.store(key, quote);
        quotes.set(key, { ...quote });
      }
    }
    return keys.flatMap((key) => {
      const quote = quotes.get(key);
      return quote ? [{ ...quote }] : [];
    });
  }

  /** Health probes always reach the provider, even while quotes are cached. */
  healthCheck(): Promise<HealthStatus> {
    return this.provider.healthCheck();
  }
}
