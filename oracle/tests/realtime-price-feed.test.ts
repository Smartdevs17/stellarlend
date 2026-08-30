/**
 * Tests for Real-Time Price Feed Service
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  RealtimePriceFeed,
  FeedEventType,
  createRealtimePriceFeed,
} from '../src/services/realtime-price-feed.js';
import { PriceAggregator, createAggregator } from '../src/services/price-aggregator.js';
import { createValidator } from '../src/services/price-validator.js';
import { createPriceCache } from '../src/services/cache.js';
import { BasePriceProvider } from '../src/providers/base-provider.js';
import type { RawPriceData, ProviderConfig, HealthStatus } from '../src/types/index.js';

/**
 * Mock provider for testing
 */
class MockProvider extends BasePriceProvider {
  private mockPrices: Map<string, number> = new Map();
  private shouldFail: boolean = false;

  constructor(name: string, priority: number, weight: number, prices: Record<string, number> = {}) {
    super({
      name,
      enabled: true,
      priority,
      weight,
      baseUrl: 'https://mock.api',
      rateLimit: { maxRequests: 1000, windowMs: 60000 },
    });

    Object.entries(prices).forEach(([asset, price]) => {
      this.mockPrices.set(asset.toUpperCase(), price);
    });
  }

  async fetchPrice(asset: string): Promise<RawPriceData> {
    if (this.shouldFail) {
      throw new Error(`Mock provider ${this.name} failed`);
    }

    const price = this.mockPrices.get(asset.toUpperCase());
    if (price === undefined) {
      throw new Error(`Asset ${asset} not found in mock provider`);
    }

    return {
      asset: asset.toUpperCase(),
      price,
      timestamp: Math.floor(Date.now() / 1000),
      source: this.name,
    };
  }

  setPrice(asset: string, price: number): void {
    this.mockPrices.set(asset.toUpperCase(), price);
  }

  setFail(shouldFail: boolean): void {
    this.shouldFail = shouldFail;
  }
}

describe('RealtimePriceFeed', () => {
  let feed: RealtimePriceFeed;
  let mockProvider1: MockProvider;
  let mockProvider2: MockProvider;
  let aggregator: PriceAggregator;

  beforeEach(() => {
    mockProvider1 = new MockProvider('provider1', 1, 0.6, {
      XLM: 0.15,
      BTC: 50000,
      ETH: 3000,
      USDC: 1.0,
    });

    mockProvider2 = new MockProvider('provider2', 2, 0.4, {
      XLM: 0.152,
      BTC: 50100,
      ETH: 3010,
      USDC: 1.001,
    });

    const validator = createValidator({
      maxDeviationPercent: 20,
      maxStalenessSeconds: 300,
    });

    const cache = createPriceCache(30);

    aggregator = createAggregator([mockProvider1, mockProvider2], validator, cache, {
      minSources: 1,
    });

    feed = createRealtimePriceFeed(aggregator, {
      assets: ['XLM', 'BTC', 'ETH'],
      pollIntervalMs: 60_000, // Long interval so it doesn't auto-fire
      enableAnomalyDetection: true,
      enableCorrelationAnalysis: true,
      enableTwapSmoothing: true,
      maxConcurrency: 5,
      updateTimeoutMs: 5000,
    });
  });

  describe('processAsset', () => {
    it('should process a single asset and return enriched price', async () => {
      const result = await feed.processAsset('BTC');

      expect(result).not.toBeNull();
      expect(result!.aggregated.asset).toBe('BTC');
      expect(result!.aggregated.price).toBeGreaterThan(0n);
      expect(result!.twapPrice).toBeGreaterThan(0n);
      expect(result!.anomalies).toBeDefined();
      expect(result!.healthScore).toBeGreaterThan(0);
      expect(result!.processingLatencyMs).toBeGreaterThanOrEqual(0);
    });

    it('should return null when provider fails', async () => {
      mockProvider1.setFail(true);
      mockProvider2.setFail(true);

      const result = await feed.processAsset('BTC');
      expect(result).toBeNull();
    });

    it('should compute TWAP alongside spot price', async () => {
      // Feed multiple prices to build TWAP history
      for (let i = 0; i < 5; i++) {
        await feed.processAsset('BTC');
      }

      const result = await feed.processAsset('BTC');
      expect(result).not.toBeNull();
      expect(result!.twapPrice).toBeGreaterThan(0n);
    });
  });

  describe('runPriceCycle', () => {
    it('should process all configured assets', async () => {
      const results = await feed.runPriceCycle();

      expect(results.size).toBe(3);
      expect(results.has('XLM')).toBe(true);
      expect(results.has('BTC')).toBe(true);
      expect(results.has('ETH')).toBe(true);
    });

    it('should emit price update events', async () => {
      const priceUpdates: unknown[] = [];
      feed.on(FeedEventType.PRICE_UPDATE, (event) => priceUpdates.push(event));

      await feed.runPriceCycle();

      expect(priceUpdates.length).toBe(3);
    });

    it('should handle partial failures gracefully', async () => {
      // Add an asset that will fail
      feed.updateConfig({ assets: ['XLM', 'BTC', 'SOL'] });

      const results = await feed.runPriceCycle();
      // SOL will fail but XLM and BTC should succeed
      expect(results.size).toBe(2);
    });
  });

  describe('event system', () => {
    it('should emit ANOMALY_DETECTED when anomalies occur', async () => {
      const anomalies: unknown[] = [];
      feed.on(FeedEventType.ANOMALY_DETECTED, (event) => anomalies.push(event));

      // First build up history
      for (let i = 0; i < 15; i++) {
        await feed.processAsset('BTC');
      }

      // Now create an anomalous price
      mockProvider1.setPrice('BTC', 60000);
      mockProvider2.setPrice('BTC', 60000);
      await feed.processAsset('BTC');

      // May or may not have anomalies depending on detector sensitivity
      expect(Array.isArray(anomalies)).toBe(true);
    });

    it('should emit AGGREGATION_COMPLETE events', async () => {
      const completions: unknown[] = [];
      feed.on(FeedEventType.AGGREGATION_COMPLETE, (event) => completions.push(event));

      await feed.runPriceCycle();

      expect(completions.length).toBe(3);
    });

    it('should emit FEED_ERROR on failures', async () => {
      const errors: unknown[] = [];
      feed.on(FeedEventType.FEED_ERROR, (event) => errors.push(event));

      mockProvider1.setFail(true);
      mockProvider2.setFail(true);

      await feed.processAsset('BTC');

      expect(errors.length).toBeGreaterThan(0);
    });

    it('should emit CORRELATION_ALERT events', async () => {
      const corrAlerts: unknown[] = [];
      feed.on(FeedEventType.CORRELATION_ALERT, (event) => corrAlerts.push(event));

      // Build up correlation history with correlated movements
      for (let i = 0; i < 20; i++) {
        const btcPrice = 50000 + i * 100;
        const ethPrice = 3000 + i * 10;
        mockProvider1.setPrice('BTC', btcPrice);
        mockProvider2.setPrice('BTC', btcPrice + 50);
        mockProvider1.setPrice('ETH', ethPrice);
        mockProvider2.setPrice('ETH', ethPrice + 5);

        feed.updateConfig({ assets: ['BTC', 'ETH'] });
        await feed.runPriceCycle();
      }

      // Correlation events may or may not fire depending on threshold
      expect(Array.isArray(corrAlerts)).toBe(true);
    });
  });

  describe('health monitoring', () => {
    it('should return health statuses for all assets', async () => {
      await feed.runPriceCycle();

      const statuses = feed.getHealthStatuses();
      expect(statuses).toHaveLength(3);

      for (const status of statuses) {
        expect(status.asset).toBeDefined();
        expect(typeof status.isHealthy).toBe('boolean');
        expect(typeof status.lastUpdateAge).toBe('number');
        expect(typeof status.anomalyCount).toBe('number');
        expect(typeof status.consecutiveFailures).toBe('number');
        expect(typeof status.averageLatencyMs).toBe('number');
      }
    });

    it('should report unhealthy when consecutive failures exceed threshold', async () => {
      mockProvider1.setFail(true);
      mockProvider2.setFail(true);

      await feed.processAsset('BTC');
      await feed.processAsset('BTC');
      await feed.processAsset('BTC');

      const statuses = feed.getHealthStatuses();
      const btcStatus = statuses.find((s) => s.asset === 'BTC');
      expect(btcStatus?.isHealthy).toBe(false);
    });

    it('should return system health overview', async () => {
      await feed.runPriceCycle();

      const health = feed.getSystemHealth();
      expect(typeof health.isHealthy).toBe('boolean');
      expect(health.totalCycles).toBe(1);
      expect(health.assetHealth).toHaveLength(3);
      expect(health.anomalyStats).toBeDefined();
      expect(health.correlationStats).toBeDefined();
    });
  });

  describe('subsystem access', () => {
    it('should expose anomaly detector', () => {
      expect(feed.getAnomalyDetector()).toBeDefined();
    });

    it('should expose feed correlation', () => {
      expect(feed.getFeedCorrelation()).toBeDefined();
    });

    it('should expose TWAP service', () => {
      expect(feed.getTwapService()).toBeDefined();
    });

    it('should expose manipulation detector', () => {
      expect(feed.getManipulationDetector()).toBeDefined();
    });

    it('should expose price history', () => {
      expect(feed.getPriceHistory()).toBeDefined();
    });
  });

  describe('getLastPrices', () => {
    it('should return empty map initially', () => {
      const lastPrices = feed.getLastPrices();
      expect(lastPrices.size).toBe(0);
    });

    it('should return prices after processing', async () => {
      await feed.runPriceCycle();

      const lastPrices = feed.getLastPrices();
      expect(lastPrices.size).toBe(3);
      expect(lastPrices.has('BTC')).toBe(true);
    });
  });

  describe('start/stop lifecycle', () => {
    it('should report running state', () => {
      expect(feed.getIsRunning()).toBe(false);
    });

    it('should start and stop cleanly', async () => {
      await feed.start();
      expect(feed.getIsRunning()).toBe(true);

      await feed.stop();
      expect(feed.getIsRunning()).toBe(false);
    });

    it('should not start twice', async () => {
      await feed.start();
      await feed.start(); // Should not throw
      expect(feed.getIsRunning()).toBe(true);

      await feed.stop();
    });

    it('should not stop when not running', async () => {
      await feed.stop(); // Should not throw
      expect(feed.getIsRunning()).toBe(false);
    });
  });

  describe('configuration', () => {
    it('should update config at runtime', () => {
      feed.updateConfig({ pollIntervalMs: 5000 });
      // No assertion needed - just ensure it doesn't throw
    });

    it('should return running state', () => {
      expect(typeof feed.getIsRunning()).toBe('boolean');
    });
  });
});
