/**
 * Oracle Failure Scenario Stress Tests (#691)
 *
 * Existing coverage (tests/failure-scenarios.test.ts) checks that a single
 * failing provider is handled. This suite pushes the same paths under sustained
 * load and under failure modes that answer successfully with bad data, where
 * the transport cannot help and the validator has to carry the weight.
 *
 * Invariant under test: the aggregator either returns a price that passed
 * validation, or it returns null. It must never return an unvalidated price,
 * no matter how the upstreams misbehave.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { createAggregator } from '../../src/services/price-aggregator.js';
import { createValidator } from '../../src/services/price-validator.js';
import { PriceHistoryService } from '../../src/services/price-history.js';
import { scalePrice } from '../../src/config.js';
import {
  StressProvider,
  FailureMode,
  driveLoad,
  summarizeLatency,
  recordScenario,
  nonCachingPriceCache,
} from './harness.js';

const ASSET = 'XLM';
const TRUE_PRICE = 0.15;
const ITERATIONS = 200;
const CONCURRENCY = 16;

/** Cache TTL 0 so every iteration exercises the providers, not the cache. */
function buildStack(providers: StressProvider[], minSources = 1) {
  const validator = createValidator({
    maxDeviationPercent: 10,
    maxStalenessSeconds: 300,
  });
  const cache = nonCachingPriceCache();
  const history = new PriceHistoryService();
  return createAggregator(providers, validator, cache, history, {
    minSources,
    useWeightedMedian: true,
    // Effectively off: this suite isolates failure handling, and the breaker
    // has its own suite.
    circuitBreaker: { failureThreshold: 1_000_000 },
  });
}

describe('Oracle stress: failure scenarios', () => {
  let primary: StressProvider;
  let secondary: StressProvider;
  let tertiary: StressProvider;

  beforeEach(() => {
    primary = new StressProvider({ name: 'primary', priority: 1, weight: 0.5, seed: 1 });
    secondary = new StressProvider({ name: 'secondary', priority: 2, weight: 0.3, seed: 2 });
    tertiary = new StressProvider({ name: 'tertiary', priority: 3, weight: 0.2, seed: 3 });
    for (const p of [primary, secondary, tertiary]) {
      p.setPrice(ASSET, TRUE_PRICE);
    }
  });

  it('survives a total outage of every provider without returning a price', async () => {
    for (const p of [primary, secondary, tertiary]) {
      p.setFailure(1, FailureMode.NETWORK_ERROR);
    }
    const aggregator = buildStack([primary, secondary, tertiary]);

    const load = await driveLoad(ITERATIONS, CONCURRENCY, () => aggregator.getPrice(ASSET));

    // Every result must be null — never a fabricated or cached-forever price.
    const nonNull = load.results.filter((r) => r !== null);
    const passed = nonNull.length === 0 && load.successes === 0;

    recordScenario({
      category: 'failure-scenarios',
      name: 'Total outage — all providers down',
      passed,
      iterations: ITERATIONS,
      successes: load.successes,
      failures: load.failures,
      latency: summarizeLatency(load.latencies),
      notes: {
        failureMode: FailureMode.NETWORK_ERROR,
        pricesReturned: nonNull.length,
        invariant: 'null returned, never an unvalidated price',
      },
    });

    expect(nonNull).toHaveLength(0);
    expect(load.failures).toBe(ITERATIONS);
  });

  it('holds a correct price through a 70% intermittent failure rate', async () => {
    // Each provider flaps independently; on most iterations at least one
    // answers, and the aggregate must still be right when it does.
    for (const p of [primary, secondary, tertiary]) {
      p.setFailure(0.7, FailureMode.SERVER_ERROR);
    }
    const aggregator = buildStack([primary, secondary, tertiary]);

    const load = await driveLoad(ITERATIONS, CONCURRENCY, () => aggregator.getPrice(ASSET));
    const prices = load.results.filter((r) => r !== null);
    const expected = scalePrice(TRUE_PRICE);
    const allCorrect = prices.every((p) => p!.price === expected);

    recordScenario({
      category: 'failure-scenarios',
      name: 'Intermittent 70% failure rate across all providers',
      passed: allCorrect && prices.length > 0,
      iterations: ITERATIONS,
      successes: prices.length,
      failures: ITERATIONS - prices.length,
      latency: summarizeLatency(load.latencies),
      notes: {
        failureRate: '70%',
        expectedScaledPrice: expected.toString(),
        invariant: 'every returned price equals the true price',
      },
    });

    // With 3 independent providers at p(fail)=0.7, p(all fail) ≈ 0.34, so a
    // meaningful majority of iterations should still produce a price.
    expect(prices.length).toBeGreaterThan(ITERATIONS * 0.3);
    expect(allCorrect).toBe(true);
  });

  it('rejects malformed payloads rather than propagating them', async () => {
    // Malformed answers succeed at the transport layer — only validation stops
    // them, which is exactly the regression this guards.
    for (const p of [primary, secondary, tertiary]) {
      p.setFailure(1, FailureMode.MALFORMED);
    }
    const aggregator = buildStack([primary, secondary, tertiary]);

    const load = await driveLoad(ITERATIONS, CONCURRENCY, () => aggregator.getPrice(ASSET));
    const nonNull = load.results.filter((r) => r !== null);

    recordScenario({
      category: 'failure-scenarios',
      name: 'Malformed payloads (negative price) from every provider',
      passed: nonNull.length === 0,
      iterations: ITERATIONS,
      successes: 0,
      failures: ITERATIONS,
      latency: summarizeLatency(load.latencies),
      notes: {
        failureMode: FailureMode.MALFORMED,
        invariant: 'negative prices never reach the consumer',
      },
    });

    expect(nonNull).toHaveLength(0);
  });

  it('rejects stale payloads under sustained load', async () => {
    for (const p of [primary, secondary, tertiary]) {
      p.setFailure(1, FailureMode.STALE).setStaleOffset(86_400);
    }
    const aggregator = buildStack([primary, secondary, tertiary]);

    const load = await driveLoad(ITERATIONS, CONCURRENCY, () => aggregator.getPrice(ASSET));
    const nonNull = load.results.filter((r) => r !== null);

    recordScenario({
      category: 'failure-scenarios',
      name: 'Day-old timestamps from every provider',
      passed: nonNull.length === 0,
      iterations: ITERATIONS,
      successes: 0,
      failures: ITERATIONS,
      latency: summarizeLatency(load.latencies),
      notes: {
        staleBySeconds: 86_400,
        maxStalenessSeconds: 300,
        invariant: 'stale prices never reach the consumer',
      },
    });

    expect(nonNull).toHaveLength(0);
  });

  it('outvotes a byzantine provider that reports a plausible but wrong price', async () => {
    // The hardest failure: well-formed, fresh, and wrong. Two honest providers
    // must dominate the weighted median.
    primary.setFailure(1, FailureMode.BYZANTINE);
    const aggregator = buildStack([primary, secondary, tertiary]);

    const load = await driveLoad(ITERATIONS, CONCURRENCY, () => aggregator.getPrice(ASSET));
    const prices = load.results.filter((r) => r !== null);
    const expected = scalePrice(TRUE_PRICE);
    const uncorrupted = prices.filter((p) => p!.price === expected);

    recordScenario({
      category: 'failure-scenarios',
      name: 'Byzantine provider reporting +40% off true price',
      passed: uncorrupted.length === prices.length && prices.length > 0,
      iterations: ITERATIONS,
      successes: prices.length,
      failures: ITERATIONS - prices.length,
      latency: summarizeLatency(load.latencies),
      notes: {
        byzantineProvider: 'primary',
        byzantineSkew: '+40%',
        honestProviders: 2,
        invariant: 'honest majority determines the aggregate',
      },
    });

    expect(prices.length).toBeGreaterThan(0);
    expect(uncorrupted.length).toBe(prices.length);
  });

  it('refuses to publish when minSources cannot be met', async () => {
    // Two of three down, quorum of 3 required — the aggregator must not
    // silently lower its own quorum.
    secondary.setFailure(1, FailureMode.NETWORK_ERROR);
    tertiary.setFailure(1, FailureMode.NETWORK_ERROR);
    const aggregator = buildStack([primary, secondary, tertiary], 3);

    const load = await driveLoad(50, 8, () => aggregator.getPrice(ASSET));
    const nonNull = load.results.filter((r) => r !== null);

    recordScenario({
      category: 'failure-scenarios',
      name: 'Quorum not met (1 of 3 required sources alive)',
      passed: nonNull.length === 0,
      iterations: 50,
      successes: 0,
      failures: 50,
      latency: summarizeLatency(load.latencies),
      notes: {
        minSources: 3,
        healthySources: 1,
        invariant: 'quorum is never silently lowered',
      },
    });

    expect(nonNull).toHaveLength(0);
  });
});
