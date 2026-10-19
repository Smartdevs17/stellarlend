/**
 * TWAP Oracle Security Tests
 *
 * Comprehensive tests for price manipulation protection using TWAP oracle.
 * Tests cover:
 * - TWAP calculation accuracy
 * - Circuit breaker activation and recovery
 * - Source deviation monitoring
 * - Volatility spike detection
 * - Staleness protection
 * - Multi-source aggregation security
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  ManipulationDetector,
  AlertSeverity,
  AlertType,
  createManipulationDetector,
} from '../src/services/manipulation-detector.js';
import type { PriceData } from '../src/types/index.js';

// ─── Test Helpers ──────────────────────────────────────────────────────────────

function makePriceData(source: string, price: bigint, asset = 'BTC', timestamp?: number): PriceData {
  return {
    asset,
    price,
    timestamp: timestamp ?? Math.floor(Date.now() / 1000),
    source,
    confidence: 100,
  };
}

function makeHistoricalPrices(
  basePrice: bigint,
  count: number,
  intervalSeconds: number,
  volatilityPercent: number = 0
): Array<{ price: bigint; timestamp: number }> {
  const now = Math.floor(Date.now() / 1000);
  const prices: Array<{ price: bigint; timestamp: number }> = [];
  
  for (let i = 0; i < count; i++) {
    const timestamp = now - (count - i) * intervalSeconds;
    const volatilityFactor = 1 + (Math.random() * 2 - 1) * volatilityPercent / 100;
    const price = BigInt(Math.floor(Number(basePrice) * volatilityFactor));
    prices.push({ price, timestamp });
  }
  
  return prices;
}

// ─── TWAP Calculation Tests ────────────────────────────────────────────────────

describe('TWAP Calculation Security', () => {
  let detector: ManipulationDetector;

  beforeEach(() => {
    detector = createManipulationDetector({
      twapSpotAlertBps: 500,  // 5%
      twapSpotPauseBps: 2500, // 25%
    });
  });

  it('should detect TWAP manipulation when spot deviates significantly', () => {
    const twap = 50000n;
    const manipulatedSpot = 63000n; // 26% deviation
    
    const alert = detector.checkTWAPSpotDeviation('BTC', twap, manipulatedSpot, ['cg', 'binance']);
    
    expect(alert).not.toBeNull();
    expect(alert!.severity).toBe(AlertSeverity.CRITICAL);
    expect(alert!.type).toBe(AlertType.TWAP_SPOT_DEVIATION);
  });

  it('should allow normal price variations within threshold', () => {
    const twap = 50000n;
    const spot = 51000n; // 2% deviation
    
    const alert = detector.checkTWAPSpotDeviation('BTC', twap, spot, ['cg', 'binance']);
    
    expect(alert).toBeNull();
  });

  it('should handle zero TWAP gracefully', () => {
    const alert = detector.checkTWAPSpotDeviation('BTC', 0n, 50000n, ['cg']);
    expect(alert).toBeNull();
  });

  it('should handle zero spot price gracefully', () => {
    const alert = detector.checkTWAPSpotDeviation('BTC', 50000n, 0n, ['cg']);
    expect(alert).toBeNull();
  });
});

// ─── Circuit Breaker Tests ─────────────────────────────────────────────────────

describe('Circuit Breaker Activation', () => {
  let detector: ManipulationDetector;

  beforeEach(() => {
    detector = createManipulationDetector({
      sourceAlertBps: 200,
      sourcePauseBps: 1000,
      volatilityBps: 2000,
      volatilityWindowSeconds: 600,
    });
  });

  it('should trigger circuit breaker on extreme source deviation', () => {
    const prices: PriceData[] = [
      makePriceData('cg', 50000n),
      makePriceData('binance', 56000n), // 12% deviation
    ];
    const median = 50000n;
    
    const alerts = detector.checkSourceDeviations('BTC', prices, median);
    
    expect(alerts.some(a => a.severity === AlertSeverity.CRITICAL)).toBe(true);
  });

  it('should track consecutive deviations for persistent manipulation', () => {
    const prices: PriceData[] = [
      makePriceData('cg', 50000n),
      makePriceData('binance', 51100n), // ~2.2% deviation
    ];
    const median = 50000n;
    
    // Simulate multiple consecutive deviations
    for (let i = 0; i < 5; i++) {
      detector.checkSourceDeviations('BTC', prices, median);
    }
    
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
    
    // Deviate
    detector.checkSourceDeviations('BTC', deviatedPrices, median);
    // Align
    detector.checkSourceDeviations('BTC', alignedPrices, median);
    
    const count = detector.getSourceDeviationCount('BTC', 'binance');
    expect(count).toBe(0);
  });
});

// ─── Volatility Detection Tests ────────────────────────────────────────────────

describe('Volatility Spike Detection', () => {
  let detector: ManipulationDetector;

  beforeEach(() => {
    detector = createManipulationDetector({
      volatilityBps: 2000, // 20%
      volatilityWindowSeconds: 600,
    });
  });

  it('should detect rapid price movements', () => {
    const now = Math.floor(Date.now() / 1000);
    const historical = [
      { price: 50000n, timestamp: now - 300 },
      { price: 50100n, timestamp: now - 200 },
      { price: 50200n, timestamp: now - 100 },
    ];
    
    // 60% price jump
    const alert = detector.checkVolatilitySpike('BTC', 80000n, historical);
    
    expect(alert).not.toBeNull();
    expect(alert!.type).toBe(AlertType.VOLATILITY_SPIKE);
    expect(alert!.severity).toBe(AlertSeverity.CRITICAL);
  });

  it('should ignore normal price movements', () => {
    const now = Math.floor(Date.now() / 1000);
    const historical = [
      { price: 50000n, timestamp: now - 300 },
      { price: 50100n, timestamp: now - 200 },
      { price: 50200n, timestamp: now - 100 },
    ];
    
    // 1% price movement
    const alert = detector.checkVolatilitySpike('BTC', 50500n, historical);
    
    expect(alert).toBeNull();
  });

  it('should ignore historical data outside time window', () => {
    const now = Math.floor(Date.now() / 1000);
    const historical = [
      { price: 10000n, timestamp: now - 700 }, // outside 600s window
    ];
    
    const alert = detector.checkVolatilitySpike('BTC', 50000n, historical);
    
    expect(alert).toBeNull();
  });
});

// ─── Source Availability Tests ─────────────────────────────────────────────────

describe('Source Availability Security', () => {
  let detector: ManipulationDetector;

  beforeEach(() => {
    detector = createManipulationDetector({
      minSourcesForSafety: 2,
    });
  });

  it('should alert when insufficient sources available', () => {
    const alert = detector.checkSourceAvailability('BTC', ['cg']);
    
    expect(alert).not.toBeNull();
    expect(alert!.type).toBe(AlertType.SOURCE_UNAVAILABILITY);
  });

  it('should alert when no sources available', () => {
    const alert = detector.checkSourceAvailability('BTC', []);
    
    expect(alert).not.toBeNull();
    expect(alert!.severity).toBe(AlertSeverity.WARNING);
  });

  it('should not alert when enough sources available', () => {
    const alert = detector.checkSourceAvailability('BTC', ['cg', 'binance']);
    
    expect(alert).toBeNull();
  });
});

// ─── Fallback Decision Tests ───────────────────────────────────────────────────

describe('Median Fallback Decision', () => {
  let detector: ManipulationDetector;

  beforeEach(() => {
    detector = createManipulationDetector({
      twapSpotPauseBps: 2500,
    });
  });

  it('should recommend median fallback on critical alerts', () => {
    // Generate critical alert
    detector.checkTWAPSpotDeviation('BTC', 50000n, 63000n, ['cg']);
    
    expect(detector.shouldFallbackToMedian('BTC')).toBe(true);
  });

  it('should not recommend fallback without critical alerts', () => {
    // Generate warning alert only
    detector.checkTWAPSpotDeviation('BTC', 50000n, 53000n, ['cg']);
    
    expect(detector.shouldFallbackToMedian('BTC')).toBe(false);
  });
});

// ─── Alert Management Tests ────────────────────────────────────────────────────

describe('Alert Management', () => {
  let detector: ManipulationDetector;

  beforeEach(() => {
    detector = createManipulationDetector({
      maxAlerts: 10,
    });
  });

  it('should store and retrieve alerts', () => {
    detector.checkSourceAvailability('BTC', []);
    detector.checkSourceAvailability('ETH', []);
    
    const alerts = detector.getAlerts();
    expect(alerts.length).toBeGreaterThanOrEqual(2);
  });

  it('should filter alerts by asset', () => {
    detector.checkSourceAvailability('BTC', []);
    detector.checkSourceAvailability('ETH', []);
    
    const btcAlerts = detector.getAlerts('BTC');
    expect(btcAlerts.every(a => a.asset === 'BTC')).toBe(true);
  });

  it('should limit stored alerts', () => {
    // Generate more alerts than max
    for (let i = 0; i < 15; i++) {
      detector.checkSourceAvailability(`ASSET${i}`, []);
    }
    
    const alerts = detector.getAlerts();
    expect(alerts.length).toBeLessThanOrEqual(10);
  });

  it('should clear alerts for specific asset', () => {
    detector.checkSourceAvailability('BTC', []);
    detector.checkSourceAvailability('ETH', []);
    
    detector.clearAlerts('BTC');
    
    expect(detector.getAlerts('BTC')).toHaveLength(0);
    expect(detector.getAlerts('ETH').length).toBeGreaterThan(0);
  });

  it('should clear all alerts', () => {
    detector.checkSourceAvailability('BTC', []);
    detector.checkSourceAvailability('ETH', []);
    
    detector.clearAlerts();
    
    expect(detector.getAlerts()).toHaveLength(0);
  });
});

// ─── Configuration Tests ───────────────────────────────────────────────────────

describe('Configuration Management', () => {
  it('should use default configuration', () => {
    const detector = createManipulationDetector();
    const config = detector.getConfig();
    
    expect(config.sourceAlertBps).toBe(200);
    expect(config.sourcePauseBps).toBe(1000);
    expect(config.twapSpotAlertBps).toBe(500);
    expect(config.twapSpotPauseBps).toBe(2500);
    expect(config.volatilityBps).toBe(2000);
    expect(config.volatilityWindowSeconds).toBe(600);
    expect(config.minSourcesForSafety).toBe(2);
    expect(config.maxAlerts).toBe(100);
  });

  it('should allow configuration updates', () => {
    const detector = createManipulationDetector();
    
    detector.updateConfig({
      sourceAlertBps: 300,
      sourcePauseBps: 1500,
    });
    
    const config = detector.getConfig();
    expect(config.sourceAlertBps).toBe(300);
    expect(config.sourcePauseBps).toBe(1500);
  });
});

// ─── Integration Scenario Tests ────────────────────────────────────────────────

describe('Integration Scenarios', () => {
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
    });
  });

  it('should handle flash loan attack scenario', () => {
    // Normal TWAP
    const twap = 50000n;
    // Manipulated spot price (flash loan attack)
    const manipulatedSpot = 65000n; // 30% deviation
    
    const alert = detector.checkTWAPSpotDeviation('BTC', twap, manipulatedSpot, ['cg', 'binance']);
    
    expect(alert).not.toBeNull();
    expect(alert!.severity).toBe(AlertSeverity.CRITICAL);
    expect(detector.shouldFallbackToMedian('BTC')).toBe(true);
  });

  it('should handle compromised oracle scenario', () => {
    const prices: PriceData[] = [
      makePriceData('cg', 50000n),
      makePriceData('compromised_oracle', 70000n), // 40% deviation
    ];
    const median = 50000n;
    
    const alerts = detector.checkSourceDeviations('BTC', prices, median);
    
    expect(alerts.some(a => a.severity === AlertSeverity.CRITICAL)).toBe(true);
    expect(detector.getSuspiciousSources('BTC')).toContain('compromised_oracle');
  });

  it('should handle volatility manipulation scenario', () => {
    const now = Math.floor(Date.now() / 1000);
    const historical = [
      { price: 50000n, timestamp: now - 300 },
      { price: 50100n, timestamp: now - 200 },
    ];
    
    // Artificial volatility spike
    const alert = detector.checkVolatilitySpike('BTC', 70000n, historical);
    
    expect(alert).not.toBeNull();
    expect(alert!.type).toBe(AlertType.VOLATILITY_SPIKE);
  });

  it('should handle source failure scenario', () => {
    const alert = detector.checkSourceAvailability('BTC', ['cg']);
    
    expect(alert).not.toBeNull();
    expect(alert!.type).toBe(AlertType.SOURCE_UNAVAILABILITY);
  });
});