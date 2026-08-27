/**
 * Tests for Feed Correlation Service
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
  FeedCorrelation,
  CorrelationEventType,
  CorrelationSeverity,
  createFeedCorrelation,
} from '../src/services/feed-correlation.js';

describe('FeedCorrelation', () => {
  let correlation: FeedCorrelation;

  beforeEach(() => {
    correlation = createFeedCorrelation({
      rollingWindowSize: 50,
      minSamples: 10,
      coordinatedMoveThreshold: 0.85,
      correlationBreakdownThreshold: 0.4,
      minHistoricalCorrelation: 0.7,
      maxEvents: 100,
      correlatedGroups: [
        ['BTC', 'ETH'],
        ['USDC', 'USDT'],
      ],
      eventCooldownSeconds: 0, // Disable for testing
    });
  });

  describe('recordPrice', () => {
    it('should return empty array with insufficient data', () => {
      for (let i = 0; i < 5; i++) {
        const events = correlation.recordPrice('BTC', BigInt(50000 + i * 10), BigInt(50000 + (i - 1) * 10), Date.now() / 1000 + i);
        expect(events).toHaveLength(0);
      }
    });

    it('should track price returns', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      correlation.recordPrice('BTC', BigInt(50000), null, baseTime);
      correlation.recordPrice('BTC', BigInt(51000), BigInt(50000), baseTime + 1);

      const returns = correlation.getReturns('BTC');
      expect(returns).toHaveLength(1);
      expect(returns[0]).toBeCloseTo(2.0, 1); // 2% increase
    });

    it('should track returns for multiple assets', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      correlation.recordPrice('BTC', BigInt(50000), null, baseTime);
      correlation.recordPrice('ETH', BigInt(3000), null, baseTime);
      correlation.recordPrice('BTC', BigInt(51000), BigInt(50000), baseTime + 1);
      correlation.recordPrice('ETH', BigInt(3060), BigInt(3000), baseTime + 1);

      expect(correlation.getReturns('BTC')).toHaveLength(1);
      expect(correlation.getReturns('ETH')).toHaveLength(1);
    });
  });

  describe('computeCorrelation', () => {
    it('should return null with insufficient data', () => {
      expect(correlation.computeCorrelation('BTC', 'ETH')).toBeNull();
    });

    it('should compute high correlation for perfectly correlated assets', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      // Feed perfectly correlated returns (both increase by 1%)
      for (let i = 0; i < 20; i++) {
        const price1 = BigInt(50000 + i * 500);
        const price2 = BigInt(3000 + i * 30);
        const prev1 = i > 0 ? BigInt(50000 + (i - 1) * 500) : null;
        const prev2 = i > 0 ? BigInt(3000 + (i - 1) * 30) : null;
        correlation.recordPrice('BTC', price1, prev1, baseTime + i);
        correlation.recordPrice('ETH', price2, prev2, baseTime + i);
      }

      const pair = correlation.computeCorrelation('BTC', 'ETH');
      expect(pair).not.toBeNull();
      expect(pair!.correlation).toBeCloseTo(1.0, 1);
      expect(pair!.isSignificant).toBe(true);
    });

    it('should compute low correlation for uncorrelated assets', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      // Feed alternating patterns (anti-correlated)
      for (let i = 0; i < 20; i++) {
        const btcPrice = BigInt(50000 + (i % 2 === 0 ? 500 : -500));
        const ethPrice = BigInt(3000 + (i % 2 === 0 ? -50 : 50));
        const prevBtc = i > 0 ? BigInt(50000 + ((i - 1) % 2 === 0 ? 500 : -500)) : null;
        const prevEth = i > 0 ? BigInt(3000 + ((i - 1) % 2 === 0 ? -50 : 50)) : null;
        correlation.recordPrice('BTC', btcPrice, prevBtc, baseTime + i);
        correlation.recordPrice('ETH', ethPrice, prevEth, baseTime + i);
      }

      const pair = correlation.computeCorrelation('BTC', 'ETH');
      expect(pair).not.toBeNull();
      expect(pair!.correlation).toBeLessThan(0);
    });
  });

  describe('getCorrelationMatrix', () => {
    it('should return empty matrix with no data', () => {
      const matrix = correlation.getCorrelationMatrix();
      expect(matrix.assets).toHaveLength(0);
      expect(matrix.matrix).toHaveLength(0);
    });

    it('should return correct matrix dimensions', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      for (let i = 0; i < 15; i++) {
        correlation.recordPrice('BTC', BigInt(50000 + i * 100), i > 0 ? BigInt(50000 + (i - 1) * 100) : null, baseTime + i);
        correlation.recordPrice('ETH', BigInt(3000 + i * 10), i > 0 ? BigInt(3000 + (i - 1) * 10) : null, baseTime + i);
      }

      const matrix = correlation.getCorrelationMatrix();
      expect(matrix.assets).toHaveLength(2);
      expect(matrix.matrix).toHaveLength(2);
      expect(matrix.matrix[0]).toHaveLength(2);
      expect(matrix.matrix[0]![0]).toBe(1); // Self-correlation
      expect(matrix.matrix[1]![1]).toBe(1); // Self-correlation
    });
  });

  describe('areExpectedCorrelates', () => {
    it('should return true for correlated pair', () => {
      expect(correlation.areExpectedCorrelates('BTC', 'ETH')).toBe(true);
    });

    it('should return false for uncorrelated pair', () => {
      expect(correlation.areExpectedCorrelates('BTC', 'XLM')).toBe(false);
    });

    it('should be case-insensitive', () => {
      expect(correlation.areExpectedCorrelates('btc', 'eth')).toBe(true);
    });
  });

  describe('events', () => {
    it('should store and retrieve events', () => {
      const events = correlation.getEvents();
      expect(Array.isArray(events)).toBe(true);
    });

    it('should filter events by asset', () => {
      const events = correlation.getEvents('BTC');
      expect(Array.isArray(events)).toBe(true);
    });

    it('should filter recent events by time', () => {
      const events = correlation.getRecentEvents(3600);
      expect(Array.isArray(events)).toBe(true);
    });
  });

  describe('clearReturns', () => {
    it('should clear returns for specific asset', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      correlation.recordPrice('BTC', BigInt(50000), null, baseTime);
      correlation.recordPrice('BTC', BigInt(51000), BigInt(50000), baseTime + 1);
      expect(correlation.getReturns('BTC')).toHaveLength(1);

      correlation.clearReturns('BTC');
      expect(correlation.getReturns('BTC')).toHaveLength(0);
    });

    it('should clear all returns', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      correlation.recordPrice('BTC', BigInt(50000), null, baseTime);
      correlation.recordPrice('ETH', BigInt(3000), null, baseTime);

      correlation.clearReturns();
      expect(correlation.getReturns('BTC')).toHaveLength(0);
      expect(correlation.getReturns('ETH')).toHaveLength(0);
    });
  });

  describe('configuration', () => {
    it('should return current config', () => {
      const config = correlation.getConfig();
      expect(config.rollingWindowSize).toBe(50);
      expect(config.correlatedGroups).toHaveLength(2);
    });

    it('should update config at runtime', () => {
      correlation.updateConfig({ coordinatedMoveThreshold: 0.9 });
      expect(correlation.getConfig().coordinatedMoveThreshold).toBe(0.9);
    });
  });

  describe('stats', () => {
    it('should return correct stats', () => {
      const stats = correlation.getStats();
      expect(stats.trackedAssets).toBe(0);
      expect(stats.totalEvents).toBe(0);
      expect(stats.correlatedGroups).toBe(2);
    });

    it('should track assets', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      correlation.recordPrice('BTC', BigInt(50000), null, baseTime);
      correlation.recordPrice('ETH', BigInt(3000), null, baseTime);
      // Second call with previousPrice to actually record a return
      correlation.recordPrice('BTC', BigInt(51000), BigInt(50000), baseTime + 1);
      correlation.recordPrice('ETH', BigInt(3060), BigInt(3000), baseTime + 1);

      const stats = correlation.getStats();
      expect(stats.trackedAssets).toBe(2);
    });
  });
});
