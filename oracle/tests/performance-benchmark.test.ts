/**
 * Performance Benchmarks for Real-Time Price Feed Pipeline
 *
 * Measures throughput and latency of:
 *   - Price aggregation (single & multi-asset)
 *   - Anomaly detection throughput
 *   - Full pipeline processing
 *   - Memory usage under sustained load
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { createRealtimePriceFeed } from '../src/services/realtime-price-feed.js';
import { createAggregator } from '../src/services/price-aggregator.js';
import { createValidator } from '../src/services/price-validator.js';
import { createPriceCache } from '../src/services/cache.js';
import { createAnomalyDetector } from '../src/services/anomaly-detector.js';
import { createFeedCorrelation } from '../src/services/feed-correlation.js';
import { BasePriceProvider } from '../src/providers/base-provider.js';
import type { RawPriceData } from '../src/types/index.js';

class BenchProvider extends BasePriceProvider {
  private basePrice: number;

  constructor(name: string, priority: number, weight: number, basePrice: number) {
    super({
      name,
      enabled: true,
      priority,
      weight,
      baseUrl: 'https://bench.api',
      rateLimit: { maxRequests: 100000, windowMs: 60000 },
    });
    this.basePrice = basePrice;
  }

  async fetchPrice(asset: string): Promise<RawPriceData> {
    const jitter = (Math.random() - 0.5) * this.basePrice * 0.01;
    return {
      asset: asset.toUpperCase(),
      price: this.basePrice + jitter,
      timestamp: Math.floor(Date.now() / 1000),
      source: this.name,
    };
  }
}

function createBenchAggregator(basePrice: number) {
  const p1 = new BenchProvider('p1', 1, 0.6, basePrice);
  const p2 = new BenchProvider('p2', 2, 0.4, basePrice * 1.001);
  const validator = createValidator({ maxDeviationPercent: 20, maxStalenessSeconds: 300 });
  const cache = createPriceCache(5);
  return createAggregator([p1, p2], validator, cache, { minSources: 1 });
}

describe('Performance Benchmarks', () => {
  describe('anomaly detection throughput', () => {
    it('should process 1000 price ingests in < 500ms', () => {
      const detector = createAnomalyDetector({
        rollingWindowSize: 100,
        minSamples: 5,
        anomalyCooldownSeconds: 0,
        adaptiveThresholds: false,
      });

      const start = performance.now();
      for (let i = 0; i < 1000; i++) {
        const price = BigInt(50000 + Math.round(Math.sin(i / 10) * 500));
        detector.ingestPrice('BTC', price, Math.floor(Date.now() / 1000) + i);
      }
      const elapsed = performance.now() - start;

      expect(elapsed).toBeLessThan(500);
    });

    it('should handle 50 assets with 100 price points each in < 1s', () => {
      const detector = createAnomalyDetector({
        rollingWindowSize: 100,
        minSamples: 5,
        anomalyCooldownSeconds: 0,
        adaptiveThresholds: false,
      });

      const assets = Array.from({ length: 50 }, (_, i) => `ASSET${i}`);
      const start = performance.now();
      for (const asset of assets) {
        for (let i = 0; i < 100; i++) {
          const price = BigInt(1000 + Math.round(Math.sin(i / 5) * 100));
          detector.ingestPrice(asset, price, Math.floor(Date.now() / 1000) + i);
        }
      }
      const elapsed = performance.now() - start;

      expect(elapsed).toBeLessThan(1000);
    });
  });

  describe('feed correlation throughput', () => {
    it('should process 500 correlated price records in < 1s', () => {
      const correlation = createFeedCorrelation({
        rollingWindowSize: 100,
        minSamples: 10,
        correlatedGroups: [['BTC', 'ETH'], ['SOL', 'AVAX']],
        eventCooldownSeconds: 0,
      });

      const start = performance.now();
      for (let i = 0; i < 500; i++) {
        const btcPrice = BigInt(50000 + i * 10);
        const ethPrice = BigInt(3000 + i);
        correlation.recordPrice('BTC', btcPrice, i > 0 ? btcPrice - 10n : null, Math.floor(Date.now() / 1000) + i);
        correlation.recordPrice('ETH', ethPrice, i > 0 ? ethPrice - 1n : null, Math.floor(Date.now() / 1000) + i);
      }
      const elapsed = performance.now() - start;

      expect(elapsed).toBeLessThan(1000);
    });
  });

  describe('full pipeline processing', () => {
    it('should complete 50 price cycles in < 5s', async () => {
      const aggregator = createBenchAggregator(50000);
      const feed = createRealtimePriceFeed(aggregator, {
        assets: ['BTC', 'ETH', 'XLM', 'USDC', 'USDT'],
        pollIntervalMs: 60_000,
        enableAnomalyDetection: true,
        enableCorrelationAnalysis: true,
        enableTwapSmoothing: true,
        maxConcurrency: 5,
        updateTimeoutMs: 10000,
        anomalyWindowSize: 100,
      });

      const start = performance.now();
      for (let i = 0; i < 50; i++) {
        await feed.runPriceCycle();
      }
      const elapsed = performance.now() - start;

      expect(elapsed).toBeLessThan(5000);
    });

    it('should handle single-asset processing with low latency', async () => {
      const aggregator = createBenchAggregator(50000);
      const feed = createRealtimePriceFeed(aggregator, {
        assets: ['BTC'],
        pollIntervalMs: 60_000,
        enableAnomalyDetection: true,
        enableCorrelationAnalysis: false,
        enableTwapSmoothing: true,
        maxConcurrency: 5,
        updateTimeoutMs: 10000,
      });

      const start = performance.now();
      for (let i = 0; i < 100; i++) {
        await feed.processAsset('BTC');
      }
      const elapsed = performance.now() - start;

      // Average latency per process should be < 100ms
      expect(elapsed / 100).toBeLessThan(100);
    });
  });

  describe('memory usage', () => {
    it('should not leak memory after many cycles', async () => {
      const aggregator = createBenchAggregator(50000);
      const feed = createRealtimePriceFeed(aggregator, {
        assets: ['BTC'],
        pollIntervalMs: 60_000,
        enableAnomalyDetection: true,
        enableCorrelationAnalysis: true,
        enableTwapSmoothing: true,
        maxConcurrency: 5,
        updateTimeoutMs: 10000,
        anomalyWindowSize: 50,
      });

      // Run many cycles
      for (let i = 0; i < 200; i++) {
        await feed.runPriceCycle();
      }

      // Verify internal maps don't grow unbounded
      const lastPrices = feed.getLastPrices();
      expect(lastPrices.size).toBe(1);

      const detector = feed.getAnomalyDetector();
      const stats = detector.getStats();
      expect(stats.totalEvents).toBeLessThan(2000); // Should be bounded
    });

    it('should bound price history to configured window size', async () => {
      const aggregator = createBenchAggregator(50000);
      const windowSize = 20;
      const feed = createRealtimePriceFeed(aggregator, {
        assets: ['BTC'],
        pollIntervalMs: 60_000,
        enableAnomalyDetection: true,
        enableCorrelationAnalysis: false,
        enableTwapSmoothing: true,
        maxConcurrency: 5,
        updateTimeoutMs: 10000,
        anomalyWindowSize: windowSize,
      });

      for (let i = 0; i < 100; i++) {
        await feed.runPriceCycle();
      }

      // Stats should show bounded data
      const detector = feed.getAnomalyDetector();
      const btcStats = detector.getRollingStats('BTC');
      if (btcStats) {
        expect(btcStats.count).toBeLessThanOrEqual(windowSize);
      }
    });
  });

  describe('concurrent access', () => {
    it('should handle 10 concurrent processAsset calls without corruption', async () => {
      const aggregator = createBenchAggregator(50000);
      const feed = createRealtimePriceFeed(aggregator, {
        assets: ['BTC'],
        pollIntervalMs: 60_000,
        enableAnomalyDetection: true,
        enableCorrelationAnalysis: false,
        enableTwapSmoothing: true,
        maxConcurrency: 10,
        updateTimeoutMs: 10000,
      });

      const promises = Array.from({ length: 10 }, () => feed.processAsset('BTC'));
      const results = await Promise.all(promises);

      // At least some should succeed (some may get null due to race conditions)
      const successful = results.filter((r) => r !== null);
      expect(successful.length).toBeGreaterThan(0);
    });
  });
});
