/**
 * oraclePrice.ts — Issue #1769
 *
 * Per-block memoization for backend oracle reads. Price adapters are called
 * from several guards during one request; this cache shares a single fetch
 * per asset per ledger block and drops entries when the block advances, so
 * staleness is bounded by one ledger rather than by wall-clock guessing.
 */

export interface PriceQuote {
  asset: string;
  price: number;
  decimals: number;
  timestamp: number;
  confidence: number;
}

export interface BlockClock {
  block(): number;
  now(): number;
}

export interface OraclePriceLogEvent {
  event: 'oracle.price.read';
  asset: string;
  block: number;
  cacheHit: boolean;
  durationMs: number;
}

const DEFAULT_MAX_ENTRIES = 256;

const systemClock: BlockClock = {
  block: () => 0,
  now: () => Date.now(),
};

interface CacheEntry {
  block: number;
  quote: PriceQuote;
}

export class OraclePriceCache {
  private readonly entries = new Map<string, CacheEntry>();
  private readonly inFlight = new Map<string, Promise<PriceQuote>>();

  constructor(
    private readonly clock: BlockClock = systemClock,
    private readonly maxEntries = DEFAULT_MAX_ENTRIES,
    private readonly logger: (event: OraclePriceLogEvent) => void = (event) =>
      console.info(JSON.stringify(event)),
  ) {}

  /** Returns the quote for the current block, fetching at most once per asset. */
  async getOrFetch(
    asset: string,
    fetch: () => Promise<PriceQuote>,
  ): Promise<PriceQuote> {
    const block = this.clock.block();
    const cached = this.entries.get(asset);
    if (cached && cached.block === block) {
      this.logger({
        event: 'oracle.price.read',
        asset,
        block,
        cacheHit: true,
        durationMs: 0,
      });
      return cached.quote;
    }

    const pending = this.inFlight.get(asset);
    if (pending) return pending;

    const startedAt = this.clock.now();
    const promise = fetch().then(
      (quote) => {
        this.inFlight.delete(asset);
        this.entries.delete(asset);
        this.entries.set(asset, { block, quote });
        this.evictIfNeeded();
        this.logger({
          event: 'oracle.price.read',
          asset,
          block,
          cacheHit: false,
          durationMs: this.clock.now() - startedAt,
        });
        return quote;
      },
      (error: unknown) => {
        this.inFlight.delete(asset);
        throw error;
      },
    );

    this.inFlight.set(asset, promise);
    return promise;
  }

  /** Drops one asset, or every asset when called without an argument. */
  invalidate(asset?: string): void {
    if (asset) {
      this.entries.delete(asset);
      return;
    }
    this.entries.clear();
  }

  get size(): number {
    return this.entries.size;
  }

  private evictIfNeeded(): void {
    while (this.entries.size > this.maxEntries) {
      const oldest = this.entries.keys().next().value as string;
      this.entries.delete(oldest);
    }
  }
}

export const oraclePriceCache = new OraclePriceCache();
