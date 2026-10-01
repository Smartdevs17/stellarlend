/**
 * Price Aggregator Service
 *
 * Fetches prices from multiple providers and aggregates them
 * using weighted median calculation.
 */
import type { AggregatedPrice } from '@/types';
import { BasePriceProvider } from '@/providers/base-provider';
import { PriceValidator } from './price-validator';
import { PriceCache } from './cache';
import { PriceHistoryService } from './price-history';
import type { CircuitBreakerConfig, CircuitBreakerMetrics } from './circuit-breaker';
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
 * Price Aggregator
 */
export declare class PriceAggregator {
    private providers;
    private validator;
    private cache;
    private priceHistory;
    private config;
    private circuitBreakers;
    constructor(providers: BasePriceProvider[], validator: PriceValidator, cache: PriceCache, priceHistory: PriceHistoryService, config?: Partial<AggregatorConfig>);
    /**
     * Fetch and aggregate price for a single asset
     */
    getPrice(asset: string): Promise<AggregatedPrice | null>;
    /**
     * Fetch prices for multiple assets
     */
    getPrices(assets: string[]): Promise<Map<string, AggregatedPrice>>;
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
    private fetchWithFallback;
    /**
     * Priority-based failover: try providers in priority order and return as
     * soon as the highest-available provider succeeds.  Lower-priority providers
     * are only consulted when all higher-priority ones are unavailable or fail.
     *
     * Recovery is automatic: once a higher-priority provider's circuit breaker
     * closes it will be tried first again on the next request.
     */
    private fetchWithPriorityFailover;
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
    private fetchFromAllProviders;
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
    private screenAgainstConsensus;
    /**
     * Aggregate prices from multiple sources
     */
    private aggregate;
    /**
     * Calculate weighted median of prices
     */
    private weightedMedian;
    /**
     * Calculate simple median of prices
     */
    private simpleMedian;
    /**
     * Get price history service
     */
    getPriceHistory(): PriceHistoryService;
    /**
     * Get circuit breaker metrics for all providers
     */
    getCircuitBreakerMetrics(): CircuitBreakerMetrics[];
    /**
     * Get list of enabled providers
     */
    getProviders(): string[];
    /**
     * Returns true when the aggregator is running in priority-based failover mode.
     */
    isFailoverMode(): boolean;
    /**
     * Get aggregator statistics
     */
    getStats(): {
        enabledProviders: number;
        failoverMode: boolean;
        cacheStats: {
            size: number;
            hits: number;
            misses: number;
            hitRate: number;
            evictions: number;
        };
        priceHistoryStats: {
            trackedAssets: number;
            totalEntries: number;
            maxEntriesPerAsset: number;
            assets: string[];
        };
        circuitBreakerMetrics: CircuitBreakerMetrics[];
        circuitBreakers: CircuitBreakerMetrics[];
    };
}
/**
 * Create a price aggregator
 */
export declare function createAggregator(providers: BasePriceProvider[], validator: PriceValidator, cache: PriceCache, priceHistoryOrConfig?: PriceHistoryService | Partial<AggregatorConfig>, config?: Partial<AggregatorConfig>): PriceAggregator;
//# sourceMappingURL=price-aggregator.d.ts.map