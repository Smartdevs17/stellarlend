/**
 * Integration tests: CircuitBreaker inside PriceAggregator
 *
 * Verifies that the aggregator correctly skips providers with open circuits
 * and recovers when a provider comes back online.
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { PriceAggregator, createAggregator } from '../src/services/price-aggregator.js';
import { createValidator } from '../src/services/price-validator.js';
import { createPriceCache } from '../src/services/cache.js';
import { CircuitState } from '../src/services/circuit-breaker.js';
import { BasePriceProvider } from '../src/providers/base-provider.js';
import type { RawPriceData } from '../src/types/index.js';

class MockProvider extends BasePriceProvider {
  private prices: Map<string, number>;
  private _fail = false;

  constructor(name: string, priority: number, prices: Record<string, number> = {}) {
    super({
      name,
      enabled: true,
      priority,
      weight: 0.5,
      baseUrl: 'https://mock',
      rateLimit: { maxRequests: 1000, windowMs: 60000 },
    });
    this.prices = new Map(Object.entries(prices).map(([k, v]) => [k.toUpperCase(), v]));
  }

  async fetchPrice(asset: string): Promise<RawPriceData> {
    if (this._fail) throw new Error(`${this.name} is down`);
    const price = this.prices.get(asset.toUpperCase());
    if (price === undefined) throw new Error(`${asset} not found`);
    return {
      asset: asset.toUpperCase(),
      price,
      timestamp: Math.floor(Date.now() / 1000),
      source: this.name,
    };
  }

  setFail(v: boolean) {
    this._fail = v;
  }
}

function makeAggregator(providers: MockProvider[], backoffMs = 10_000) {
  return createAggregator(
    providers,
    createValidator({ maxDeviationPercent: 50, maxStalenessSeconds: 300 }),
    createPriceCache(1), // 1-second TTL so cache doesn't interfere
    { minSources: 1, circuitBreaker: { failureThreshold: 3, backoffMs } }
  );
}

describe('PriceAggregator + CircuitBreaker integration', () => {
  let p1: MockProvider;
  let p2: MockProvider;
  let aggregator: PriceAggregator;

  beforeEach(() => {
    vi.useFakeTimers();
    p1 = new MockProvider('primary', 1, { XLM: 0.15 });
    p2 = new MockProvider('fallback', 2, { XLM: 0.15 });
    aggregator = makeAggregator([p1, p2]);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns prices normally when all providers are healthy', async () => {
    const result = await aggregator.getPrice('XLM');
    expect(result).not.toBeNull();
  });

  it('opens circuit for a failing provider after threshold failures', async () => {
    p1.setFail(true);

    // Trigger 3 failures to open the circuit
    for (let i = 0; i < 3; i++) {
      await aggregator.getPrice('XLM');
      // Bust cache between calls
      vi.advanceTimersByTime(2_000);
    }

    const metrics = aggregator.getCircuitBreakerMetrics();
    const p1Metrics = metrics.find((m) => m.providerName === 'primary');
    expect(p1Metrics?.state).toBe(CircuitState.OPEN);
  });

  it('still returns prices from healthy provider when one circuit is open', async () => {
    p1.setFail(true);

    for (let i = 0; i < 3; i++) {
      await aggregator.getPrice('XLM');
      vi.advanceTimersByTime(2_000);
    }

    // p1 circuit is now OPEN; p2 should still serve prices
    const result = await aggregator.getPrice('XLM');
    expect(result).not.toBeNull();
    expect(result?.sources.every((s) => s.source !== 'primary')).toBe(true);
  });

  it('transitions to HALF_OPEN after backoff and closes on recovery', async () => {
    p1.setFail(true);

    for (let i = 0; i < 3; i++) {
      await aggregator.getPrice('XLM');
      vi.advanceTimersByTime(2_000);
    }

    // Advance past backoff
    vi.advanceTimersByTime(10_000);

    // Provider recovers
    p1.setFail(false);

    // Probe request – should succeed and close the circuit
    await aggregator.getPrice('XLM');

    const metrics = aggregator.getCircuitBreakerMetrics();
    const p1Metrics = metrics.find((m) => m.providerName === 'primary');
    expect(p1Metrics?.state).toBe(CircuitState.CLOSED);
  });

  it('getStats includes circuit breaker metrics', async () => {
    const stats = aggregator.getStats();
    expect(stats.circuitBreakers).toBeDefined();
    expect(Array.isArray(stats.circuitBreakers)).toBe(true);
    expect(stats.circuitBreakers.length).toBe(2);
  });
});
