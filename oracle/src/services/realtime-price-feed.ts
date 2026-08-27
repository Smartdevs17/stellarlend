/**
 * Real-Time Price Feed Service
 *
 * Orchestrates real-time price feed aggregation with anomaly detection.
 * Combines polling-based fetching with WebSocket streaming support and
 * integrates anomaly detection, correlation analysis, and TWAP into a
 * unified, production-ready pipeline.
 *
 * Architecture:
 *   External APIs → PriceAggregator → [AnomalyDetector, FeedCorrelation, TWAPService]
 *                                    → AggregatedPrice (enriched with anomaly metadata)
 *                                    → ContractUpdater
 *
 * Key features:
 *   - Real-time streaming with configurable poll intervals
 *   - Automatic anomaly detection on every price update
 *   - Cross-feed correlation monitoring
 *   - Health monitoring with liveness/readiness probes
 *   - Event-driven architecture with typed event callbacks
 *   - Graceful degradation when subsystems fail
 */

import { EventEmitter } from 'events';
import type { AggregatedPrice, PriceData } from '../types/index.js';
import { PriceAggregator } from './price-aggregator.js';
import { AnomalyDetector, AnomalySeverity } from './anomaly-detector.js';
import { FeedCorrelation, CorrelationSeverity } from './feed-correlation.js';
import { TWAPService } from './twap.service.js';
import { PriceHistoryService } from './price-history.js';
import { ManipulationDetector } from './manipulation-detector.js';
import { logger } from '../utils/logger.js';

/**
 * Real-time feed event types
 */
export enum FeedEventType {
  PRICE_UPDATE = 'price_update',
  ANOMALY_DETECTED = 'anomaly_detected',
  CORRELATION_ALERT = 'correlation_alert',
  FEED_HEALTH_CHANGE = 'feed_health_change',
  AGGREGATION_COMPLETE = 'aggregation_complete',
  FEED_ERROR = 'feed_error',
}

/**
 * Enriched price with anomaly metadata
 */
export interface EnrichedPrice {
  /** The aggregated price */
  aggregated: AggregatedPrice;
  /** TWAP-smoothed price */
  twapPrice: bigint;
  /** Anomalies detected for this price */
  anomalies: AnomalyEvent[];
  /** Whether any critical anomaly was detected */
  hasCriticalAnomaly: boolean;
  /** Price health score (0-100, higher is healthier) */
  healthScore: number;
  /** Processing latency in milliseconds */
  processingLatencyMs: number;
}

// Re-import for local use
import type { AnomalyEvent } from './anomaly-detector.js';

/**
 * Feed health status
 */
export interface FeedHealthStatus {
  asset: string;
  isHealthy: boolean;
  lastUpdateAge: number;
  anomalyCount: number;
  criticalAnomalyCount: number;
  consecutiveFailures: number;
  averageLatencyMs: number;
}

/**
 * Real-time feed configuration
 */
export interface RealtimeFeedConfig {
  /** Assets to track */
  assets: string[];
  /** Polling interval in milliseconds */
  pollIntervalMs: number;
  /** Maximum allowed price update age before marking stale (seconds) */
  maxPriceAgeSeconds: number;
  /** Enable anomaly detection */
  enableAnomalyDetection: boolean;
  /** Enable correlation analysis */
  enableCorrelationAnalysis: boolean;
  /** Enable TWAP smoothing */
  enableTwapSmoothing: boolean;
  /** Maximum concurrent price fetches */
  maxConcurrency: number;
  /** Number of historical prices to keep for anomaly detection */
  anomalyWindowSize: number;
  /** Price update timeout in milliseconds */
  updateTimeoutMs: number;
}

const DEFAULT_FEED_CONFIG: RealtimeFeedConfig = {
  assets: ['XLM', 'USDC', 'BTC', 'ETH'],
  pollIntervalMs: 10_000,
  maxPriceAgeSeconds: 120,
  enableAnomalyDetection: true,
  enableCorrelationAnalysis: true,
  enableTwapSmoothing: true,
  maxConcurrency: 5,
  anomalyWindowSize: 100,
  updateTimeoutMs: 30_000,
};

/**
 * Real-Time Price Feed Service
 */
export class RealtimePriceFeed extends EventEmitter {
  private config: RealtimeFeedConfig;
  private aggregator: PriceAggregator;
  private anomalyDetector: AnomalyDetector;
  private feedCorrelation: FeedCorrelation;
  private twapService: TWAPService;
  private priceHistory: PriceHistoryService;
  private manipulationDetector: ManipulationDetector;

  private intervalId?: ReturnType<typeof setInterval>;
  private isRunning: boolean = false;
  private lastPrices: Map<string, AggregatedPrice> = new Map();
  private previousPrices: Map<string, bigint> = new Map();
  private consecutiveFailures: Map<string, number> = new Map();
  private latencies: Map<string, number[]> = new Map();
  private cycleCount: number = 0;

  constructor(
    aggregator: PriceAggregator,
    config: Partial<RealtimeFeedConfig> = {}
  ) {
    super();
    this.config = { ...DEFAULT_FEED_CONFIG, ...config };

    // Initialize subsystem services
    this.priceHistory = new PriceHistoryService({
      maxEntries: this.config.anomalyWindowSize,
    });

    this.anomalyDetector = new AnomalyDetector({
      rollingWindowSize: this.config.anomalyWindowSize,
      minSamples: Math.min(10, Math.floor(this.config.anomalyWindowSize / 3)),
    });

    this.feedCorrelation = new FeedCorrelation({
      rollingWindowSize: this.config.anomalyWindowSize,
      correlatedGroups: [
        ['BTC', 'ETH'],
        ['USDC', 'USDT'],
      ],
    });

    this.twapService = new TWAPService(this.priceHistory, {
      windowSeconds: 1800,
      maxDeviationBps: 500,
      minDataPoints: 3,
      fallbackToMedian: true,
      singleUpdatePerPeriod: true,
    });

    this.manipulationDetector = new ManipulationDetector({
      sourceAlertBps: 200,
      sourcePauseBps: 1000,
      twapSpotAlertBps: 500,
      twapSpotPauseBps: 2500,
      volatilityBps: 2000,
      volatilityWindowSeconds: 600,
    });

    this.aggregator = aggregator;

    logger.info('Real-time price feed initialized', {
      assets: this.config.assets,
      pollIntervalMs: this.config.pollIntervalMs,
      anomalyDetection: this.config.enableAnomalyDetection,
      correlationAnalysis: this.config.enableCorrelationAnalysis,
      twapSmoothing: this.config.enableTwapSmoothing,
    });
  }

  /**
   * Start the real-time price feed
   */
  async start(): Promise<void> {
    if (this.isRunning) {
      logger.warn('Real-time price feed is already running');
      return;
    }

    this.isRunning = true;
    logger.info('Starting real-time price feed', { assets: this.config.assets });

    // Initial price fetch
    await this.runPriceCycle();

    // Schedule periodic updates
    this.intervalId = setInterval(async () => {
      await this.runPriceCycle();
    }, this.config.pollIntervalMs);

    this.emit(FeedEventType.FEED_HEALTH_CHANGE, {
      status: 'started',
      assets: this.config.assets,
    });
  }

  /**
   * Stop the real-time price feed
   */
  async stop(): Promise<void> {
    if (!this.isRunning) {
      logger.warn('Real-time price feed is not running');
      return;
    }

    this.isRunning = false;

    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = undefined;
    }

    logger.info('Real-time price feed stopped');
    this.emit(FeedEventType.FEED_HEALTH_CHANGE, {
      status: 'stopped',
    });
  }

  /**
   * Run a single price update cycle across all assets
   */
  async runPriceCycle(): Promise<Map<string, EnrichedPrice>> {
    const startTime = Date.now();
    this.cycleCount++;
    const results = new Map<string, EnrichedPrice>();

    logger.debug(`Starting price cycle #${this.cycleCount}`, {
      assets: this.config.assets,
    });

    // Process assets with concurrency limit
    const chunks = this.chunkArray(this.config.assets, this.config.maxConcurrency);

    for (const chunk of chunks) {
      const promises = chunk.map((asset) => this.processAsset(asset));
      const chunkResults = await Promise.allSettled(promises);

      for (let i = 0; i < chunkResults.length; i++) {
        const result = chunkResults[i]!;
        const asset = chunk[i]!;

        if (result.status === 'fulfilled' && result.value) {
          results.set(asset.toUpperCase(), result.value);
        } else {
          const error =
            result.status === 'rejected' ? result.reason : new Error('No result');
          this.handleAssetError(asset, error);
        }
      }
    }

    const cycleLatencyMs = Date.now() - startTime;
    logger.debug(`Price cycle #${this.cycleCount} complete in ${cycleLatencyMs}ms`, {
      assetsProcessed: results.size,
      anomaliesDetected: Array.from(results.values()).reduce(
        (sum, r) => sum + r.anomalies.length,
        0
      ),
    });

    return results;
  }

  /**
   * Process a single asset: fetch price, detect anomalies, compute TWAP
   */
  async processAsset(asset: string): Promise<EnrichedPrice | null> {
    const processStart = Date.now();
    const upperAsset = asset.toUpperCase();

    try {
      // Fetch aggregated price from the underlying aggregator
      const aggregated = await this.fetchWithTimeout(
        () => this.aggregator.getPrice(upperAsset),
        this.config.updateTimeoutMs
      );

      if (!aggregated) {
        this.incrementFailures(upperAsset);
        this.emit(FeedEventType.FEED_ERROR, {
          asset: upperAsset,
          error: `Failed to fetch aggregated price for ${upperAsset}`,
        });
        return null;
      }

      // Record in price history
      this.priceHistory.addAggregatedPrice(aggregated);

      // Run anomaly detection
      let anomalies: AnomalyEvent[] = [];
      if (this.config.enableAnomalyDetection) {
        anomalies = this.anomalyDetector.ingestPrice(
          upperAsset,
          aggregated.price,
          aggregated.timestamp
        );
      }

      // Run correlation analysis
      if (this.config.enableCorrelationAnalysis) {
        const prevPrice = this.previousPrices.get(upperAsset) ?? null;
        const corrEvents = this.feedCorrelation.recordPrice(
          upperAsset,
          aggregated.price,
          prevPrice,
          aggregated.timestamp
        );
        for (const event of corrEvents) {
          this.emit(FeedEventType.CORRELATION_ALERT, event);
        }
      }

      // Compute TWAP
      let twapPrice = aggregated.price;
      if (this.config.enableTwapSmoothing) {
        this.twapService.recordObservation(upperAsset, aggregated.price);
        const twapStatus = this.twapService.getTWAPStatus(upperAsset, aggregated.price);
        if (twapStatus) {
          twapPrice = twapStatus.twap;
          if (twapStatus.manipulationDetected) {
            logger.warn('TWAP manipulation detected', {
              asset: upperAsset,
              deviationBps: twapStatus.deviationBps,
            });
          }
        }
      }

      // Run manipulation detection across sources
      if (aggregated.sources.length > 1) {
        const median = aggregated.price;
        this.manipulationDetector.checkSourceDeviations(
          upperAsset,
          aggregated.sources,
          median
        );
      }

      // Compute health score
      const healthScore = this.computeHealthScore(
        upperAsset,
        aggregated,
        anomalies
      );

      const processingLatencyMs = Date.now() - processStart;

      // Track latency
      this.trackLatency(upperAsset, processingLatencyMs);

      // Reset consecutive failures
      this.consecutiveFailures.set(upperAsset, 0);

      // Store previous price for next cycle
      this.previousPrices.set(upperAsset, aggregated.price);
      this.lastPrices.set(upperAsset, aggregated);

      const enriched: EnrichedPrice = {
        aggregated,
        twapPrice,
        anomalies,
        hasCriticalAnomaly: anomalies.some(
          (a) => a.severity === AnomalySeverity.CRITICAL
        ),
        healthScore,
        processingLatencyMs,
      };

      // Emit events
      this.emit(FeedEventType.PRICE_UPDATE, enriched);

      if (anomalies.length > 0) {
        this.emit(FeedEventType.ANOMALY_DETECTED, {
          asset: upperAsset,
          anomalies,
        });
      }

      this.emit(FeedEventType.AGGREGATION_COMPLETE, {
        asset: upperAsset,
        price: aggregated.price.toString(),
        twapPrice: twapPrice.toString(),
        sources: aggregated.sources.length,
        healthScore,
      });

      return enriched;
    } catch (error) {
      this.incrementFailures(upperAsset);
      this.emit(FeedEventType.FEED_ERROR, {
        asset: upperAsset,
        error: error instanceof Error ? error.message : String(error),
      });
      return null;
    }
  }

  /**
   * Get enriched price for a specific asset
   */
  async getEnrichedPrice(asset: string): Promise<EnrichedPrice | null> {
    return this.processAsset(asset);
  }

  /**
   * Get the last known enriched prices for all assets
   */
  getLastPrices(): Map<string, AggregatedPrice> {
    return new Map(this.lastPrices);
  }

  /**
   * Get health status for all tracked assets
   */
  getHealthStatuses(): FeedHealthStatus[] {
    const now = Math.floor(Date.now() / 1000);

    return this.config.assets.map((asset) => {
      const upperAsset = asset.toUpperCase();
      const lastPrice = this.lastPrices.get(upperAsset);
      const lastUpdateAge = lastPrice ? now - lastPrice.timestamp : Infinity;
      const isStale = lastUpdateAge > this.config.maxPriceAgeSeconds;
      const anomalyEvents = this.anomalyDetector.getEvents(upperAsset);
      const consecutiveFails = this.consecutiveFailures.get(upperAsset) ?? 0;
      const avgLatency = this.getAverageLatency(upperAsset);

      return {
        asset: upperAsset,
        isHealthy: !isStale && consecutiveFails < 3,
        lastUpdateAge,
        anomalyCount: anomalyEvents.length,
        criticalAnomalyCount: anomalyEvents.filter(
          (e) => e.severity === AnomalySeverity.CRITICAL
        ).length,
        consecutiveFailures: consecutiveFails,
        averageLatencyMs: avgLatency,
      };
    });
  }

  /**
   * Get overall system health
   */
  getSystemHealth(): {
    isHealthy: boolean;
    uptime: number;
    totalCycles: number;
    assetHealth: FeedHealthStatus[];
    anomalyStats: ReturnType<AnomalyDetector['getStats']>;
    correlationStats: ReturnType<FeedCorrelation['getStats']>;
  } {
    const healthStatuses = this.getHealthStatuses();
    const isHealthy = healthStatuses.every((h) => h.isHealthy);

    return {
      isHealthy,
      uptime: this.isRunning ? this.cycleCount * (this.config.pollIntervalMs / 1000) : 0,
      totalCycles: this.cycleCount,
      assetHealth: healthStatuses,
      anomalyStats: this.anomalyDetector.getStats(),
      correlationStats: this.feedCorrelation.getStats(),
    };
  }

  /**
   * Get the underlying anomaly detector
   */
  getAnomalyDetector(): AnomalyDetector {
    return this.anomalyDetector;
  }

  /**
   * Get the underlying feed correlation service
   */
  getFeedCorrelation(): FeedCorrelation {
    return this.feedCorrelation;
  }

  /**
   * Get the underlying TWAP service
   */
  getTwapService(): TWAPService {
    return this.twapService;
  }

  /**
   * Get the underlying manipulation detector
   */
  getManipulationDetector(): ManipulationDetector {
    return this.manipulationDetector;
  }

  /**
   * Get the underlying price history service
   */
  getPriceHistory(): PriceHistoryService {
    return this.priceHistory;
  }

  /**
   * Update configuration at runtime
   */
  updateConfig(config: Partial<RealtimeFeedConfig>): void {
    this.config = { ...this.config, ...config };
    logger.info('Real-time price feed config updated', config);
  }

  /**
   * Check if the feed is currently running
   */
  getIsRunning(): boolean {
    return this.isRunning;
  }

  // ── Private: Price Processing ───────────────────────────────────────

  private async fetchWithTimeout<T>(
    fn: () => Promise<T>,
    timeoutMs: number
  ): Promise<T> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error(`Price fetch timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      fn()
        .then((result) => {
          clearTimeout(timer);
          resolve(result);
        })
        .catch((error) => {
          clearTimeout(timer);
          reject(error);
        });
    });
  }

  private computeHealthScore(
    asset: string,
    aggregated: AggregatedPrice,
    anomalies: AnomalyEvent[]
  ): number {
    let score = 100;

    // Deduct for each source missing
    const maxSources = 2; // CoinGecko + Binance
    const sourcePenalty = ((maxSources - aggregated.sources.length) / maxSources) * 30;
    score -= sourcePenalty;

    // Deduct for confidence
    score -= Math.max(0, (100 - aggregated.confidence) * 0.2);

    // Deduct for anomalies
    for (const anomaly of anomalies) {
      switch (anomaly.severity) {
        case AnomalySeverity.CRITICAL:
          score -= 30;
          break;
        case AnomalySeverity.WARNING:
          score -= 15;
          break;
        case AnomalySeverity.INFO:
          score -= 5;
          break;
      }
    }

    // Deduct for consecutive failures
    const failures = this.consecutiveFailures.get(asset) ?? 0;
    score -= failures * 10;

    return Math.max(0, Math.min(100, Math.round(score)));
  }

  private handleAssetError(asset: string, error: unknown): void {
    const errorMsg = error instanceof Error ? error.message : String(error);
    logger.error(`Failed to process asset ${asset}`, { error: errorMsg });

    this.emit(FeedEventType.FEED_ERROR, {
      asset: asset.toUpperCase(),
      error: errorMsg,
    });
  }

  private incrementFailures(asset: string): void {
    const current = this.consecutiveFailures.get(asset) ?? 0;
    this.consecutiveFailures.set(asset, current + 1);
  }

  private trackLatency(asset: string, latencyMs: number): void {
    let latencies = this.latencies.get(asset);
    if (!latencies) {
      latencies = [];
      this.latencies.set(asset, latencies);
    }
    latencies.push(latencyMs);
    while (latencies.length > 50) {
      latencies.shift();
    }
  }

  private getAverageLatency(asset: string): number {
    const latencies = this.latencies.get(asset);
    if (!latencies || latencies.length === 0) return 0;
    return latencies.reduce((s, l) => s + l, 0) / latencies.length;
  }

  private chunkArray<T>(array: T[], chunkSize: number): T[][] {
    const chunks: T[][] = [];
    for (let i = 0; i < array.length; i += chunkSize) {
      chunks.push(array.slice(i, i + chunkSize));
    }
    return chunks;
  }
}

/**
 * Create a real-time price feed service
 */
export function createRealtimePriceFeed(
  aggregator: PriceAggregator,
  config?: Partial<RealtimeFeedConfig>
): RealtimePriceFeed {
  return new RealtimePriceFeed(aggregator, config);
}
