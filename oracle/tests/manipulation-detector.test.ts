/**
 * Tests for Manipulation Detector Service
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
  ManipulationDetector,
  AlertSeverity,
  AlertType,
  createManipulationDetector,
} from '../src/services/manipulation-detector.js';
import type { PriceData } from '../src/types/index.js';

function makePriceData(source: string, price: bigint, asset = 'BTC'): PriceData {
  return {
    asset,
    price,
    timestamp: Math.floor(Date.now() / 1000),
    source,
    confidence: 100,
  };
}

describe('ManipulationDetector', () => {
  let detector: ManipulationDetector;

  beforeEach(() => {
    detector = createManipulationDetector({
      sourceAlertBps: 200,
      sourcePauseBps: 1000,
      twapSpotAlertBps: 500,
      twapSpotPauseBps: 2500,
      volatilityBps: 2000,
      volatilityWindowSeconds: 600,
      minSourcesForSafety: 2,
      maxAlerts: 100,
    });
  });

  describe('checkSourceDeviations', () => {
    it('should return empty when sources are aligned', () => {
      const prices: PriceData[] = [
        makePriceData('cg', 50000n),
        makePriceData('binance', 50100n),
      ];
      const median = 50050n;
      const alerts = detector.checkSourceDeviations('BTC', prices, median);
      expect(alerts).toHaveLength(0);
    });

    it('should detect source deviation exceeding alert threshold', () => {
      const prices: PriceData[] = [
        makePriceData('cg', 50000n),
        makePriceData('binance', 51100n), // ~2.2% deviation from median
      ];
      const median = 50000n;
      const alerts = detector.checkSourceDeviations('BTC', prices, median);
      expect(alerts.length).toBeGreaterThanOrEqual(1);
      expect(alerts.some((a) => a.type === AlertType.SOURCE_DEVIATION)).toBe(true);
      expect(alerts.some((a) => a.severity === AlertSeverity.WARNING)).toBe(true);
    });

    it('should detect source deviation exceeding pause threshold', () => {
      const prices: PriceData[] = [
        makePriceData('cg', 50000n),
        makePriceData('binance', 56000n), // 12% deviation from median
      ];
      const median = 50000n;
      const alerts = detector.checkSourceDeviations('BTC', prices, median);
      expect(alerts.length).toBeGreaterThanOrEqual(1);
      expect(alerts.some((a) => a.severity === AlertSeverity.CRITICAL)).toBe(true);
    });

    it('should skip zero or negative prices', () => {
      const prices: PriceData[] = [
        makePriceData('cg', 0n),
        makePriceData('binance', 50100n),
      ];
      const median = 50000n;
      const alerts = detector.checkSourceDeviations('BTC', prices, median);
      expect(alerts).toHaveLength(0);
    });

    it('should skip when median is zero', () => {
      const prices: PriceData[] = [
        makePriceData('cg', 50000n),
        makePriceData('binance', 50100n),
      ];
      const alerts = detector.checkSourceDeviations('BTC', prices, 0n);
      expect(alerts).toHaveLength(0);
    });

    it('should track consecutive deviations and flag suspicious sources', () => {
      const prices: PriceData[] = [
        makePriceData('cg', 50000n),
        makePriceData('binance', 51100n),
      ];
      const median = 50000n;

      // First deviation
      detector.checkSourceDeviations('BTC', prices, median);
      // Second deviation
      detector.checkSourceDeviations('BTC', prices, median);
      // Third deviation → should flag as suspicious
      detector.checkSourceDeviations('BTC', prices, median);

      const suspicious = detector.getSuspiciousSources('BTC');
      expect(suspicious).toContain('binance');
    });

    it('should reset deviation counter when source aligns', () => {
      const deviatedPrices: PriceData[] = [
        makePriceData('cg', 50000n),
        makePriceData('binance', 51100n),
      ];
      const alignedPrices: PriceData[] = [
        makePriceData('cg', 50000n),
        makePriceData('binance', 50050n),
      ];
      const median = 50000n;

      detector.checkSourceDeviations('BTC', deviatedPrices, median);
      detector.checkSourceDeviations('BTC', alignedPrices, median);

      const count = detector.getSourceDeviationCount('BTC', 'binance');
      expect(count).toBe(0);
    });
  });

  describe('getSuspiciousSources', () => {
    it('should return empty for unknown asset', () => {
      expect(detector.getSuspiciousSources('UNKNOWN')).toHaveLength(0);
    });
  });

  describe('getSourceDeviationCount', () => {
    it('should return 0 for unknown source', () => {
      expect(detector.getSourceDeviationCount('BTC', 'unknown')).toBe(0);
    });
  });

  describe('checkTWAPSpotDeviation', () => {
    it('should return null when deviation is within threshold', () => {
      const result = detector.checkTWAPSpotDeviation('BTC', 50000n, 50100n, ['cg', 'binance']);
      expect(result).toBeNull();
    });

    it('should detect alert-level deviation', () => {
      // 1% deviation = 100 bps < 500 bps alert threshold → no alert
      // Need > 500 bps (5%)
      const result = detector.checkTWAPSpotDeviation('BTC', 50000n, 53000n, ['cg', 'binance']);
      expect(result).not.toBeNull();
      expect(result!.type).toBe(AlertType.TWAP_SPOT_DEVIATION);
      expect(result!.severity).toBe(AlertSeverity.WARNING);
    });

    it('should detect pause-level deviation', () => {
      const result = detector.checkTWAPSpotDeviation('BTC', 50000n, 63000n, ['cg', 'binance']);
      expect(result).not.toBeNull();
      expect(result!.severity).toBe(AlertSeverity.CRITICAL);
    });

    it('should return null when twap is zero', () => {
      const result = detector.checkTWAPSpotDeviation('BTC', 0n, 50000n, ['cg']);
      expect(result).toBeNull();
    });

    it('should return null when spot price is zero', () => {
      const result = detector.checkTWAPSpotDeviation('BTC', 50000n, 0n, ['cg']);
      expect(result).toBeNull();
    });
  });

  describe('checkVolatilitySpike', () => {
    it('should return null when no historical prices', () => {
      const result = detector.checkVolatilitySpike('BTC', 50000n, []);
      expect(result).toBeNull();
    });

    it('should detect volatility spike', () => {
      const now = Math.floor(Date.now() / 1000);
      const historical = [
        { price: 50000n, timestamp: now - 30 },
        { price: 50100n, timestamp: now - 20 },
        { price: 50200n, timestamp: now - 10 },
      ];
      // 60% price jump → 6000 bps > 2000 bps threshold
      const result = detector.checkVolatilitySpike('BTC', 80000n, historical);
      expect(result).not.toBeNull();
      expect(result!.type).toBe(AlertType.VOLATILITY_SPIKE);
      expect(result!.severity).toBe(AlertSeverity.CRITICAL);
    });

    it('should return null when deviation is within threshold', () => {
      const now = Math.floor(Date.now() / 1000);
      const historical = [
        { price: 50000n, timestamp: now - 30 },
        { price: 50100n, timestamp: now - 20 },
      ];
      const result = detector.checkVolatilitySpike('BTC', 50200n, historical);
      expect(result).toBeNull();
    });

    it('should return null when current price is zero', () => {
      const result = detector.checkVolatilitySpike('BTC', 0n, []);
      expect(result).toBeNull();
    });

    it('should ignore historical prices outside window', () => {
      const now = Math.floor(Date.now() / 1000);
      const historical = [
        { price: 10000n, timestamp: now - 700 }, // outside 600s window
      ];
      const result = detector.checkVolatilitySpike('BTC', 50000n, historical);
      expect(result).toBeNull();
    });

    it('should ignore historical prices with zero price', () => {
      const now = Math.floor(Date.now() / 1000);
      const historical = [
        { price: 0n, timestamp: now - 10 },
      ];
      const result = detector.checkVolatilitySpike('BTC', 50000n, historical);
      expect(result).toBeNull();
    });
  });

  describe('checkSourceAvailability', () => {
    it('should return null when enough sources', () => {
      const result = detector.checkSourceAvailability('BTC', ['cg', 'binance']);
      expect(result).toBeNull();
    });

    it('should detect insufficient sources', () => {
      const result = detector.checkSourceAvailability('BTC', ['cg']);
      expect(result).not.toBeNull();
      expect(result!.type).toBe(AlertType.SOURCE_UNAVAILABILITY);
    });

    it('should detect no sources', () => {
      const result = detector.checkSourceAvailability('BTC', []);
      expect(result).not.toBeNull();
    });
  });

  describe('shouldFallbackToMedian', () => {
    it('should return false when no critical alerts', () => {
      expect(detector.shouldFallbackToMedian('BTC')).toBe(false);
    });

    it('should return true when critical alert exists', () => {
      detector.checkSourceAvailability('BTC', []);
      // Need a critical alert - use TWAP with high deviation
      detector.checkTWAPSpotDeviation('BTC', 50000n, 63000n, ['cg']);
      expect(detector.shouldFallbackToMedian('BTC')).toBe(true);
    });
  });

  describe('getAlerts', () => {
    it('should return all alerts', () => {
      detector.checkSourceAvailability('BTC', []);
      detector.checkSourceAvailability('ETH', []);
      const alerts = detector.getAlerts();
      expect(alerts.length).toBeGreaterThanOrEqual(2);
    });

    it('should filter by asset', () => {
      detector.checkSourceAvailability('BTC', []);
      detector.checkSourceAvailability('ETH', []);
      const btcAlerts = detector.getAlerts('BTC');
      expect(btcAlerts.every((a) => a.asset === 'BTC')).toBe(true);
    });
  });

  describe('getRecentAlerts', () => {
    it('should return recent alerts', () => {
      detector.checkSourceAvailability('BTC', []);
      const recent = detector.getRecentAlerts(300);
      expect(recent.length).toBeGreaterThanOrEqual(1);
    });

    it('should filter old alerts', () => {
      const oldAlerts = detector.getRecentAlerts(0);
      expect(oldAlerts).toHaveLength(0);
    });
  });

  describe('getConfig', () => {
    it('should return current config', () => {
      const config = detector.getConfig();
      expect(config.sourceAlertBps).toBe(200);
      expect(config.sourcePauseBps).toBe(1000);
    });
  });

  describe('updateConfig', () => {
    it('should update config at runtime', () => {
      detector.updateConfig({ sourceAlertBps: 300 });
      expect(detector.getConfig().sourceAlertBps).toBe(300);
    });
  });

  describe('clearAlerts', () => {
    it('should clear all alerts', () => {
      detector.checkSourceAvailability('BTC', []);
      expect(detector.getAlerts().length).toBeGreaterThan(0);
      detector.clearAlerts();
      expect(detector.getAlerts()).toHaveLength(0);
    });

    it('should clear alerts for specific asset', () => {
      detector.checkSourceAvailability('BTC', []);
      detector.checkSourceAvailability('ETH', []);
      detector.clearAlerts('BTC');
      expect(detector.getAlerts('BTC')).toHaveLength(0);
      expect(detector.getAlerts('ETH').length).toBeGreaterThan(0);
    });
  });
});
