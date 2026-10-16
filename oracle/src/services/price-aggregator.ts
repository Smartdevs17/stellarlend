/**
 * Price Aggregator Service
 *
 * Fetches prices from multiple providers and aggregates them
 * using weighted median calculation.
 */

import type { RawPriceData, PriceData, AggregatedPrice } from '@/types';
import { BasePriceProvider } from '@/providers/base-provider';
import { PriceValidator } from './price-validator';
import { PriceCache } from './cache';
import { PriceHistoryService } from './price-history';
import { CircuitBreaker, CircuitState, createCircuitBreaker } from './circuit-breaker';
import type { CircuitBreakerConfig, CircuitBreakerMetrics } from './circuit-breaker';
import { scalePrice } from '@/config';
import { logger } from '@/utils/logger';

/**
 * Aggregator configuration
 */
export interface AggregatorConfig {
  minSources: number;
  useWeightedMedian: boolean;
  /**
   * Enable priority-based failover mode.
   *
   * When true, providers are tried in ascending priority order (1 = highest).
   * The aggregator stops as soon as it collects a valid price from the
   * highest-priority available provider and does NOT query lower-priority
   * providers — keeping latency minimal when the primary is healthy.
   *
   * Lower-priority providers are only consulted when all higher-priority
   * providers fail or have open circuit breakers.  When a previously-failed
   * provider recovers (circuit breaker closes), it is automatically preferred
   * again on the next request.
   *
   * When false (default), all enabled providers are queried and their results
   * are aggregated via weighted median.
   */
  failoverMode?: boolean;
  circuitBreaker?: Partial<Omit<CircuitBreakerConfig, 'providerName'>>;
}

/**
 * Default aggregator configuration
 */
const DEFAULT_CONFIG: AggregatorConfig = {
  minSources: 1,
  useWeightedMedian: true,
  failoverMode: false,
};

/**
 * Price Aggregator
 */
export class PriceAggregator {
  private providers: BasePriceProvider[];
  private validator: PriceValidator;
  private cache: PriceCache;
  private priceHistory: PriceHistoryService;
  private config: AggregatorConfig;
  private circuitBreakers: Map<string, CircuitBreaker>;

  constructor(
    providers: BasePriceProvider[],
    validator: PriceValidator,
    cache: PriceCache,
    priceHistory: PriceHistoryService,
    config: Partial<AggregatorConfig> = {}
  ) {
    this.providers = providers.filter((p) => p.isEnabled).sort((a, b) => a.priority - b.priority);

    this.validator = validator;
    this.cache = cache;
    this.priceHistory = priceHistory;
    this.config = { ...DEFAULT_CONFIG, ...config };

    // Create one circuit breaker per provider
    this.circuitBreakers = new Map(
      this.providers.map((p) => [
        p.name,
        createCircuitBreaker({
          providerName: p.name,
          ...this.config.circuitBreaker,
        }),
      ])
    );

    logger.info('Price aggregator initialized', {
      enabledProviders: this.providers.map((p) => p.name),
      minSources: this.config.minSources,
    });
  }

  /**
   * Fetch and aggregate price for a single asset
   */
  async getPrice(asset: string): Promise<AggregatedPrice | null> {
    const upperAsset = asset.toUpperCase();

    const cachedPrice = await this.cache.getPrice(upperAsset);
    if (cachedPrice !== undefined) {
      logger.debug(`Using cached price for ${upperAsset}`);
      return {
        asset: upperAsset,
        price: cachedPrice,
        sources: [],
        timestamp: Math.floor(Date.now() / 1000),
        confidence: 100,
      };
    }

    const validPrices = await this.fetchWithFallback(upperAsset);

    if (validPrices.length < this.config.minSources) {
      logger.error(`Not enough valid sources for ${upperAsset}`, {
        got: validPrices.length,
        required: this.config.minSources,
      });
      return null;
    }

    const aggregated = this.aggregate(upperAsset, validPrices);

    const twapResult = this.priceHistory.calculateTWAP(upperAsset, 1800);
    if (twapResult) {
      const twapSpot = Number(twapResult.twap) / 1e8;
      const spot = Number(aggregated.price) / 1e8;
      const deviation = Math.abs((spot - twapSpot) / twapSpot) * 100;
      if (deviation > 5) {
        logger.warn(`TWAP manipulation detected for ${upperAsset}`, {
          spot,
          twap: twapSpot,
          deviationPercent: deviation,
        });
        return null;
      }
    }

    this.cache.setPrice(upperAsset, aggregated.price);

    // Store in price history
    this.priceHistory.addAggregatedPrice(aggregated);

    return aggregated;
  }

  /**
   * Fetch prices for multiple assets
   */
  async getPrices(assets: string[]): Promise<Map<string, AggregatedPrice>> {
    const results = new Map<string, AggregatedPrice>();

    const promises = assets.map(async (asset) => {
      const price = await this.getPrice(asset);
      if (price) {
        results.set(asset.toUpperCase(), price);
      }
    });

    await Promise.allSettled(promises);

    return results;
  }

  /**
   * Fetch price from providers with fallback logic.
   *
   * In **failover mode** providers are tried in priority order (lowest number
   * first).  As soon as a valid price is obtained from the highest-available
   * provider the method returns immediately — lower-priority providers are
   * never queried, keeping latency minimal when the primary is healthy.
   * If the current provider fails its circuit breaker opens and the next
   * lower-priority provider is tried automatically.  When the failed provider
   * recovers (circuit breaker transitions back to CLOSED) it will be preferred
   * again on the next call.
   *
   * In **aggregation mode** (default) all enabled providers are queried and
   * their results are combined via weighted median.
   */
  private async fetchWithFallback(asset: string): Promise<PriceData[]> {
    return this.config.failoverMode
      ? this.fetchWithPriorityFailover(asset)
      : this.fetchFromAllProviders(asset);
  }

  /**
   * Priority-based failover: try providers in priority order and return as
   * soon as the highest-available provider succeeds.  Lower-priority providers
   * are only consulted when all higher-priority ones are unavailable or fail.
   *
   * Recovery is automatic: once a higher-priority provider's circuit breaker
   * closes it will be tried first again on the next request.
   */
  private async fetchWithPriorityFailover(asset: string): Promise<PriceData[]> {
    // Providers are already sorted by ascending priority (1 = highest)
    for (const provider of this.providers) {
      const circuitBreaker = this.circuitBreakers.get(provider.name);

      if (circuitBreaker && !circuitBreaker.isAllowed()) {
        logger.warn(
          `[failover] Circuit breaker OPEN for ${provider.name} (priority ${provider.priority}), trying next provider`
        );
        continue;
      }

      try {
        const rawPrice = await provider.fetchPrice(asset);
        const validation = this.validator.validate(rawPrice);

        if (validation.isValid && validation.price) {
          circuitBreaker?.recordSuccess();

          logger.debug(
            `[failover] Got valid price from ${provider.name} (priority ${provider.priority}) for ${asset}`,
            { price: validation.price.price.toString() }
          );

          // Return immediately — do not query lower-priority providers
          return [validation.price];
        }

        // Invalid price counts as a failure
        circuitBreaker?.recordFailure();
        logger.warn(
          `[failover] Invalid price from ${provider.name} (priority ${provider.priority}) for ${asset}, trying next provider`,
          { errors: validation.errors }
        );
      } catch (error) {
        circuitBreaker?.recordFailure();
        logger.warn(
          `[failover] Provider ${provider.name} (priority ${provider.priority}) failed for ${asset}, trying next provider`,
          { error }
        );
      }
    }

    logger.error(`[failover] All providers failed for ${asset}`);
    return [];
  }

  /**
   * Aggregation mode: query all providers and collect every valid price for
   * weighted-median aggregation.
   *
   * Runs in three phases so that no single provider can anchor the others:
   *
   *   1. Fetch every available provider's quote (transport failures recorded
   *      against that provider's circuit breaker).
   *   2. Screen the round against its own median, dropping quotes that disagree
   *      with the consensus by more than the validator's deviation threshold.
   *   3. Validate the survivors, most consensus-aligned first.
   *
   * Phases 2 and 3 are separate on purpose. `PriceValidator` keeps a
   * last-accepted price per asset as its drift reference, so whichever quote it
   * sees first in a round becomes the yardstick for that quote's peers. Handing
   * it raw quotes in provider-priority order let a single outlier — a
   * misconfigured or compromised primary — set the reference and get an honest
   * majority rejected as "deviating", leaving its own price as the aggregate.
   * Screening against the median first means the reference is always a quote
   * the round agreed on.
   */
  private async fetchFromAllProviders(asset: string): Promise<PriceData[]> {
    const quotes: { provider: BasePriceProvider; raw: RawPriceData }[] = [];
    const errors: Map<string, Error> = new Map();

    // ── Phase 1: collect quotes ───────────────────────────────────────────
    for (const provider of this.providers) {
      const circuitBreaker = this.circuitBreakers.get(provider.name);

      // Check circuit breaker state
      if (circuitBreaker && !circuitBreaker.isAllowed()) {
        logger.warn(`Circuit breaker OPEN for ${provider.name}, skipping`);
        continue;
      }

      try {
        quotes.push({ provider, raw: await provider.fetchPrice(asset) });
      } catch (error) {
        // Record failure for circuit breaker
        circuitBreaker?.recordFailure();
        errors.set(provider.name, error instanceof Error ? error : new Error(String(error)));
        logger.warn(`Provider ${provider.name} failed for ${asset}`, { error });
      }
    }

    // ── Phase 2: screen against round consensus ───────────────────────────
    const accepted = this.screenAgainstConsensus(asset, quotes);

    // ── Phase 3: validate survivors, most consensus-aligned first ─────────
    const validPrices: PriceData[] = [];

    for (const { provider, raw } of accepted) {
      const circuitBreaker = this.circuitBreakers.get(provider.name);
      const validation = this.validator.validate(raw);

      if (validation.isValid && validation.price) {
        validPrices.push(validation.price);
        circuitBreaker?.recordSuccess();

        logger.debug(`Got valid price from ${provider.name} for ${asset}`, {
          price: validation.price.price.toString(),
        });
      } else {
        circuitBreaker?.recordFailure();
        logger.warn(`Invalid price from ${provider.name} for ${asset}`, {
          errors: validation.errors,
        });
      }
    }

    if (validPrices.length === 0 && errors.size > 0) {
      logger.error(`All providers failed for ${asset}`, {
        providers: Array.from(errors.keys()),
      });
    }

    return validPrices;
  }

  /**
   * Drop quotes that disagree with the round's median by more than the
   * validator's deviation threshold, and order the survivors by how closely
   * they track that median.
   *
   * The median is used rather than the mean because it does not move with an
   * outlier: with three quotes, one arbitrarily wrong value cannot shift it.
   * A rejected quote is recorded as a circuit-breaker failure, so a provider
   * that persistently disagrees with its peers is eventually taken out of
   * rotation rather than screened out afresh on every round.
   *
   * Rounds of one or two quotes are passed through untouched — with no third
   * opinion there is no majority to appeal to, and the validator's own
   * cross-round drift check remains the backstop.
   */
  private screenAgainstConsensus(
    asset: string,
    quotes: { provider: BasePriceProvider; raw: RawPriceData }[]
  ): { provider: BasePriceProvider; raw: RawPriceData }[] {
    if (quotes.length < 3) {
      return quotes;
    }

    const sorted = [...quotes].sort((a, b) => a.raw.price - b.raw.price);
    const mid = Math.floor(sorted.length / 2);
    const median =
      sorted.length % 2 === 0
        ? (sorted[mid - 1]!.raw.price + sorted[mid]!.raw.price) / 2
        : sorted[mid]!.raw.price;

    if (median <= 0) {
      return quotes;
    }

    const threshold = this.validator.maxDeviationPercent;
    const scored = quotes.map((quote) => ({
      quote,
      deviation: Math.abs((quote.raw.price - median) / median) * 100,
    }));

    const kept = scored.filter((entry) => {
      if (entry.deviation <= threshold) {
        return true;
      }

      logger.warn(`Quote from ${entry.quote.provider.name} disagrees with consensus for ${asset}`, {
        price: entry.quote.raw.price,
        consensusMedian: median,
        deviationPercent: entry.deviation,
        maxDeviationPercent: threshold,
      });
      this.circuitBreakers.get(entry.quote.provider.name)?.recordFailure();
      return false;
    });

    return kept.sort((a, b) => a.deviation - b.deviation).map((entry) => entry.quote);
  }

  /**
   * Aggregate prices from multiple sources
   */
  private aggregate(asset: string, prices: PriceData[]): AggregatedPrice {
    const now = Math.floor(Date.now() / 1000);

    if (prices.length === 1) {
      return {
        asset,
        price: prices[0]!.price,
        sources: prices,
        timestamp: now,
        confidence: prices[0]!.confidence,
      };
    }

    const aggregatedPrice = this.config.useWeightedMedian
      ? this.weightedMedian(prices)
      : this.simpleMedian(prices);

    const totalWeight = this.providers
      .filter((p) => prices.some((pr) => pr.source === p.name))
      .reduce((sum, p) => sum + p.weight, 0);

    const weightedConfidence =
      prices.reduce((sum, p) => {
        const provider = this.providers.find((pr) => pr.name === p.source);
        const weight = provider?.weight ?? 0.1;
        return sum + p.confidence * weight;
      }, 0) / totalWeight;

    return {
      asset,
      price: aggregatedPrice,
      sources: prices,
      timestamp: now,
      confidence: Math.round(weightedConfidence),
    };
  }

  /**
   * Calculate weighted median of prices
   */
  private weightedMedian(prices: PriceData[]): bigint {
    const sorted = [...prices].sort((a, b) => (a.price < b.price ? -1 : a.price > b.price ? 1 : 0));

    const weights = sorted.map((p) => {
      const provider = this.providers.find((pr) => pr.name === p.source);
      return provider?.weight ?? 0.1;
    });

    const totalWeight = weights.reduce((a, b) => a + b, 0);
    const halfWeight = totalWeight / 2;

    let cumWeight = 0;
    for (let i = 0; i < sorted.length; i++) {
      cumWeight += weights[i]!;
      if (cumWeight >= halfWeight) {
        return sorted[i]!.price;
      }
    }

    return sorted[sorted.length - 1]!.price;
  }

  /**
   * Calculate simple median of prices
   */
  private simpleMedian(prices: PriceData[]): bigint {
    const sorted = [...prices].sort((a, b) => (a.price < b.price ? -1 : a.price > b.price ? 1 : 0));

    const mid = Math.floor(sorted.length / 2);

    if (sorted.length % 2 === 0) {
      const avg = (sorted[mid - 1]!.price + sorted[mid]!.price) / 2n;
      return avg;
    }

    return sorted[mid]!.price;
  }

  /**
   * Get price history service
   */
  getPriceHistory(): PriceHistoryService {
    return this.priceHistory;
  }

  /**
   * Get circuit breaker metrics for all providers
   */
  getCircuitBreakerMetrics(): CircuitBreakerMetrics[] {
    const metrics: CircuitBreakerMetrics[] = [];

    for (const breaker of this.circuitBreakers.values()) {
      metrics.push(breaker.getMetrics());
    }

    return metrics;
  }

  /**
   * Get list of enabled providers
   */
  getProviders(): string[] {
    return this.providers.map((p) => p.name);
  }

  /**
   * Returns true when the aggregator is running in priority-based failover mode.
   */
  isFailoverMode(): boolean {
    return this.config.failoverMode ?? false;
  }

  /**
   * Get aggregator statistics
   */
  getStats() {
    return {
      enabledProviders: this.providers.length,
      failoverMode: this.isFailoverMode(),
      cacheStats: this.cache.getStats(),
      priceHistoryStats: this.priceHistory.getStats(),
      circuitBreakerMetrics: this.getCircuitBreakerMetrics(),
      circuitBreakers: this.getCircuitBreakerMetrics(),
    };
  }
}

function isAggregatorConfig(value: unknown): value is Partial<AggregatorConfig> {
  if (!value || typeof value !== 'object') {
    return false;
  }

  return (
    'minSources' in value ||
    'useWeightedMedian' in value ||
    'failoverMode' in value ||
    'circuitBreaker' in value
  );
}

function isPriceHistoryService(value: unknown): value is PriceHistoryService {
  return value instanceof PriceHistoryService;
}

/**
 * Create a price aggregator
 */
export function createAggregator(
  providers: BasePriceProvider[],
  validator: PriceValidator,
  cache: PriceCache,
  priceHistoryOrConfig?: PriceHistoryService | Partial<AggregatorConfig>,
  config?: Partial<AggregatorConfig>
): PriceAggregator {
  const priceHistory = isPriceHistoryService(priceHistoryOrConfig)
    ? priceHistoryOrConfig
    : new PriceHistoryService();
  const resolvedConfig = isPriceHistoryService(priceHistoryOrConfig)
    ? config
    : isAggregatorConfig(priceHistoryOrConfig)
      ? priceHistoryOrConfig
      : config;

  return new PriceAggregator(providers, validator, cache, priceHistory, resolvedConfig);
}
