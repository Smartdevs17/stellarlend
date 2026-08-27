/**
 * Tests for Anomaly Detector Service
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  AnomalyDetector,
  AnomalySeverity,
  AnomalyMethod,
  createAnomalyDetector,
} from '../src/services/anomaly-detector.js';

describe('AnomalyDetector', () => {
  let detector: AnomalyDetector;

  beforeEach(() => {
    detector = createAnomalyDetector({
      rollingWindowSize: 50,
      zScoreWarningThreshold: 2.5,
      zScoreCriticalThreshold: 4.0,
      iqrMultiplier: 1.5,
      velocityBpsPerSecond: 500,
      velocityWindowSeconds: 60,
      adaptiveThresholds: false,
      minSamples: 10,
      maxEventsPerAsset: 100,
      anomalyCooldownSeconds: 0, // Disable cooldown for testing
    });
  });

  describe('ingestPrice', () => {
    it('should return empty array when below minimum samples', () => {
      for (let i = 0; i < 5; i++) {
        const anomalies = detector.ingestPrice('BTC', BigInt(50000 + i * 10), Date.now() / 1000 + i);
        expect(anomalies).toHaveLength(0);
      }
    });

    it('should return empty array for normal price variations', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      // Feed 20 normal prices around 50000
      for (let i = 0; i < 20; i++) {
        detector.ingestPrice('BTC', BigInt(50000 + Math.round(Math.sin(i) * 100)), baseTime + i);
      }
      const anomalies = detector.ingestPrice('BTC', BigInt(50005), baseTime + 20);
      expect(anomalies).toHaveLength(0);
    });

    it('should detect Z-score warning anomaly', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      // Feed stable prices around 50000
      for (let i = 0; i < 20; i++) {
        detector.ingestPrice('BTC', BigInt(50000 + Math.round(Math.sin(i) * 50)), baseTime + i);
      }
      // Now feed a significantly different price
      const anomalies = detector.ingestPrice('BTC', BigInt(52000), baseTime + 20);
      expect(anomalies.length).toBeGreaterThanOrEqual(1);
      expect(anomalies.some((a) => a.method === AnomalyMethod.Z_SCORE)).toBe(true);
    });

    it('should detect Z-score critical anomaly', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      // Feed very stable prices
      for (let i = 0; i < 20; i++) {
        detector.ingestPrice('BTC', BigInt(50000), baseTime + i);
      }
      // Feed an extreme outlier
      const anomalies = detector.ingestPrice('BTC', BigInt(60000), baseTime + 20);
      expect(anomalies.length).toBeGreaterThanOrEqual(1);
      expect(
        anomalies.some(
          (a) => a.severity === AnomalySeverity.CRITICAL && a.method === AnomalyMethod.Z_SCORE
        )
      ).toBe(true);
    });

    it('should detect IQR outlier', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      // Feed prices with some spread
      const prices = [100, 101, 99, 102, 98, 103, 97, 104, 96, 105, 95, 106, 94];
      for (let i = 0; i < prices.length; i++) {
        detector.ingestPrice('ETH', BigInt(prices[i]!), baseTime + i);
      }
      // Feed an IQR outlier
      const anomalies = detector.ingestPrice('ETH', BigInt(200), baseTime + prices.length);
      expect(anomalies.length).toBeGreaterThanOrEqual(1);
      expect(anomalies.some((a) => a.method === AnomalyMethod.IQR)).toBe(true);
    });

    it('should detect velocity anomaly on sudden price change', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      // Feed stable prices
      for (let i = 0; i < 15; i++) {
        detector.ingestPrice('XLM', BigInt(150000), baseTime + i);
      }
      // Rapid price change: >50% jump in 2 seconds = 25000+ bps/s (>500 threshold)
      const anomalies = detector.ingestPrice('XLM', BigInt(250000), baseTime + 16);
      expect(anomalies.length).toBeGreaterThanOrEqual(1);
      expect(anomalies.some((a) => a.method === AnomalyMethod.VELOCITY)).toBe(true);
    });
  });

  describe('getRollingStats', () => {
    it('should return null with insufficient data', () => {
      detector.ingestPrice('BTC', BigInt(50000), Date.now() / 1000);
      expect(detector.getRollingStats('BTC')).toBeNull();
    });

    it('should compute correct statistics', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      const prices = [100, 200, 300, 400, 500];
      for (let i = 0; i < prices.length; i++) {
        detector.ingestPrice('BTC', BigInt(prices[i]!), baseTime + i);
      }
      const stats = detector.getRollingStats('BTC');
      expect(stats).not.toBeNull();
      expect(stats!.count).toBe(5);
      expect(stats!.mean).toBe(300);
      expect(stats!.min).toBe(100);
      expect(stats!.max).toBe(500);
      expect(stats!.median).toBe(300);
    });

    it('should compute correct quartiles', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      const prices = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
      for (let i = 0; i < prices.length; i++) {
        detector.ingestPrice('ETH', BigInt(prices[i]!), baseTime + i);
      }
      const stats = detector.getRollingStats('ETH');
      expect(stats).not.toBeNull();
      expect(stats!.q1).toBeLessThan(stats!.q3);
      expect(stats!.iqr).toBeGreaterThan(0);
    });
  });

  describe('adaptive thresholds', () => {
    it('should adjust thresholds based on volatility', () => {
      const adaptiveDetector = createAnomalyDetector({
        rollingWindowSize: 50,
        zScoreWarningThreshold: 2.5,
        adaptiveThresholds: true,
        minSamples: 10,
        anomalyCooldownSeconds: 0,
      });

      const baseTime = Math.floor(Date.now() / 1000);
      // Feed low-volatility prices first
      for (let i = 0; i < 20; i++) {
        adaptiveDetector.ingestPrice('BTC', BigInt(50000), baseTime + i);
      }
      // Then feed high-volatility prices
      for (let i = 0; i < 15; i++) {
        const price = 50000 + (i % 2 === 0 ? 1000 : -1000);
        adaptiveDetector.ingestPrice('BTC', BigInt(price), baseTime + 20 + i);
      }

      const state = adaptiveDetector.getAdaptiveThreshold('BTC');
      expect(state).not.toBeNull();
      expect(state!.volatilityMultiplier).toBeGreaterThanOrEqual(1);
    });
  });

  describe('events management', () => {
    it('should store and retrieve events', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      for (let i = 0; i < 15; i++) {
        detector.ingestPrice('BTC', BigInt(50000 + i), baseTime + i);
      }
      detector.ingestPrice('BTC', BigInt(60000), baseTime + 15);
      const events = detector.getEvents('BTC');
      expect(events.length).toBeGreaterThan(0);
    });

    it('should filter recent events by time', () => {
      const events = detector.getRecentEvents(3600, 'BTC');
      expect(Array.isArray(events)).toBe(true);
    });

    it('should clear events for an asset', () => {
      detector.clearEvents('BTC');
      expect(detector.getEvents('BTC')).toHaveLength(0);
    });

    it('should clear all events', () => {
      detector.clearEvents();
      expect(detector.getEvents()).toHaveLength(0);
    });
  });

  describe('hasCriticalAnomaly', () => {
    it('should return false when no critical anomalies', () => {
      expect(detector.hasCriticalAnomaly('BTC')).toBe(false);
    });
  });

  describe('resetAsset', () => {
    it('should clear all state for an asset', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      for (let i = 0; i < 15; i++) {
        detector.ingestPrice('BTC', BigInt(50000), baseTime + i);
      }
      detector.resetAsset('BTC');
      expect(detector.getRollingStats('BTC')).toBeNull();
      expect(detector.getEvents('BTC')).toHaveLength(0);
    });
  });

  describe('configuration', () => {
    it('should return current config', () => {
      const config = detector.getConfig();
      expect(config.rollingWindowSize).toBe(50);
      expect(config.zScoreWarningThreshold).toBe(2.5);
    });

    it('should update config at runtime', () => {
      detector.updateConfig({ zScoreWarningThreshold: 3.0 });
      expect(detector.getConfig().zScoreWarningThreshold).toBe(3.0);
    });
  });

  describe('stats', () => {
    it('should return correct stats', () => {
      const stats = detector.getStats();
      expect(stats.trackedAssets).toBe(0);
      expect(stats.totalEvents).toBe(0);
    });

    it('should track multiple assets', () => {
      const baseTime = Math.floor(Date.now() / 1000);
      for (let i = 0; i < 5; i++) {
        detector.ingestPrice('BTC', BigInt(50000), baseTime + i);
        detector.ingestPrice('ETH', BigInt(3000), baseTime + i);
      }
      const stats = detector.getStats();
      expect(stats.trackedAssets).toBe(2);
    });
  });
});
