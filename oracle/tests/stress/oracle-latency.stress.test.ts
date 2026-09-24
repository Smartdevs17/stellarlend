/**
 * Oracle Latency Stress Tests (#691)
 *
 * Latency is an availability problem before it is a performance problem: a feed
 * that answers in 30 seconds during a liquidation cascade has already failed,
 * whatever it eventually returns.
 *
 * These scenarios run against the real clock — the whole point is to measure
 * elapsed wall time — so the latencies are kept small (tens of milliseconds)
 * and the assertions are about *relationships* (does a slow provider gate the
 * whole round? does failover actually skip it?) rather than absolute
 * millisecond budgets, which would be flaky on a loaded CI runner.
 */

import { describe, it, expect } from 'vitest';
import { createAggregator } from '../../src/services/price-aggregator.js';
import { createValidator } from '../../src/services/price-validator.js';
import { createPriceCache } from '../../src/services/cache.js';
import { PriceHistoryService } from '../../src/services/price-history.js';
import {
  StressProvider,
  FailureMode,
  driveLoad,
  summarizeLatency,
  timed,
  recordScenario,
} from './harness.js';

const ASSET = 'ETH';
const PRICE = 3_000;

function buildStack(
  providers: StressProvider[],
  config: { failoverMode?: boolean; minSources?: number } = {}
) {
  return createAggregator(
    providers,
    createValidator({ maxDeviationPercent: 10, maxStalenessSeconds: 300 }),
    // 60s TTL is deliberate here: several scenarios measure whether a cache hit
    // actually short-circuits the slow provider path.
    createPriceCache(60),
    new PriceHistoryService(),
    {
      minSources: config.minSources ?? 1,
      useWeightedMedian: true,
      failoverMode: config.failoverMode ?? false,
      circuitBreaker: { failureThreshold: 1_000_000 },
    }
  );
}

function provider(name: string, priority: number, weight: number, seed: number): StressProvider {
  return new StressProvider({ name, priority, weight, seed }).setPrice(ASSET, PRICE);
}

describe('Oracle stress: latency', () => {
  it('reports a latency distribution under concurrent load', async () => {
    const providers = [
      provider('alpha', 1, 0.4, 101).setLatency({ baseMs: 5, jitterMs: 10 }),
      provider('beta', 2, 0.35, 102).setLatency({ baseMs: 5, jitterMs: 10 }),
      provider('gamma', 3, 0.25, 103).setLatency({ baseMs: 5, jitterMs: 10 }),
    ];
    const aggregator = buildStack(providers);

    const load = await driveLoad(120, 12, () => aggregator.getPrice(ASSET));
    const stats = summarizeLatency(load.latencies);

    // The tail must stay within an order of magnitude of the median; a p99 that
    // detaches from p50 means requests are queueing behind each other.
    const tailControlled = stats.p99Ms <= Math.max(stats.p50Ms * 10, 250);

    recordScenario({
      category: 'latency',
      name: 'Latency distribution under 12-way concurrent load',
      passed: tailControlled && load.successes > 0,
      iterations: 120,
      successes: load.successes,
      failures: load.failures,
      latency: stats,
      notes: {
        providerLatency: '5–15ms each',
        concurrency: 12,
        invariant: 'p99 stays within 10x p50 — no request queueing',
      },
    });

    expect(load.successes).toBeGreaterThan(0);
    expect(tailControlled).toBe(true);
  });

  it('is gated by the slowest provider in aggregation mode', async () => {
    // Aggregation mode queries every provider, so a single slow venue sets the
    // floor for the whole round. This is a real cost of aggregation and the
    // reason failover mode exists — the test pins the behaviour so a change is
    // deliberate rather than accidental.
    const providers = [
      provider('fast-a', 1, 0.4, 201).setLatency({ baseMs: 2, jitterMs: 0 }),
      provider('fast-b', 2, 0.35, 202).setLatency({ baseMs: 2, jitterMs: 0 }),
      provider('slow', 3, 0.25, 203).setLatency({ baseMs: 120, jitterMs: 0 }),
    ];
    const aggregator = buildStack(providers);

    const { elapsedMs } = await timed(() => aggregator.getPrice(ASSET));

    const gated = elapsedMs >= 120;

    recordScenario({
      category: 'latency',
      name: 'Aggregation round gated by slowest provider',
      passed: gated,
      iterations: 1,
      successes: 1,
      failures: 0,
      notes: {
        slowProviderLatencyMs: 120,
        observedRoundMs: elapsedMs,
        invariant: 'aggregation waits for every provider — failover mode does not',
      },
    });

    expect(gated).toBe(true);
  });

  it('skips a slow provider entirely in failover mode', async () => {
    // Failover mode returns as soon as the highest-priority provider answers,
    // so a slow low-priority venue must not be on the critical path at all.
    const providers = [
      provider('primary-fast', 1, 0.5, 301).setLatency({ baseMs: 2, jitterMs: 0 }),
      provider('backup-slow', 2, 0.5, 302).setLatency({ baseMs: 300, jitterMs: 0 }),
    ];
    const aggregator = buildStack(providers, { failoverMode: true });

    const { result, elapsedMs } = await timed(() => aggregator.getPrice(ASSET));

    const skipped = result !== null && elapsedMs < 300;

    recordScenario({
      category: 'latency',
      name: 'Failover mode skips a slow backup provider',
      passed: skipped,
      iterations: 1,
      successes: result ? 1 : 0,
      failures: result ? 0 : 1,
      notes: {
        backupLatencyMs: 300,
        observedRoundMs: elapsedMs,
        invariant: 'a healthy primary keeps the slow backup off the critical path',
      },
    });

    expect(skipped).toBe(true);
  });

  it('pays the backup latency only when the primary is down', async () => {
    // The flip side: when the primary fails, the round *should* wait for the
    // backup. Slow-but-correct beats fast-and-absent.
    const primary = provider('primary-down', 1, 0.5, 401).setFailure(1, FailureMode.NETWORK_ERROR);
    const backup = provider('backup-slow', 2, 0.5, 402).setLatency({ baseMs: 80, jitterMs: 0 });
    const aggregator = buildStack([primary, backup], { failoverMode: true });

    const { result, elapsedMs } = await timed(() => aggregator.getPrice(ASSET));

    const servedByBackup = result !== null && elapsedMs >= 80;

    recordScenario({
      category: 'latency',
      name: 'Backup latency paid only on primary failure',
      passed: servedByBackup,
      iterations: 1,
      successes: result ? 1 : 0,
      failures: result ? 0 : 1,
      notes: {
        backupLatencyMs: 80,
        observedRoundMs: elapsedMs,
        invariant: 'correctness is preferred over latency when the primary is down',
      },
    });

    expect(servedByBackup).toBe(true);
  });

  it('absorbs a latency spike affecting a minority of calls', async () => {
    // 20% of calls hit a 150ms tail. The median must stay fast — if a spiking
    // minority drags p50 up, requests are serialising somewhere.
    const providers = [
      provider('spiky', 1, 0.4, 501).setLatency({
        baseMs: 2,
        jitterMs: 3,
        spikeProbability: 0.2,
        spikeMs: 150,
      }),
      provider('steady-a', 2, 0.35, 502).setLatency({ baseMs: 2, jitterMs: 3 }),
      provider('steady-b', 3, 0.25, 503).setLatency({ baseMs: 2, jitterMs: 3 }),
    ];
    const aggregator = buildStack(providers);

    const load = await driveLoad(60, 10, () => aggregator.getPrice(ASSET));
    const stats = summarizeLatency(load.latencies);

    const medianStaysFast = stats.p50Ms < 150;

    recordScenario({
      category: 'latency',
      name: 'Minority latency spike (20% of calls hit a 150ms tail)',
      passed: medianStaysFast,
      iterations: 60,
      successes: load.successes,
      failures: load.failures,
      latency: stats,
      notes: {
        spikeProbability: '20%',
        spikeMs: 150,
        invariant: 'a spiking minority does not drag the median',
      },
    });

    expect(medianStaysFast).toBe(true);
  });

  it('serves a cache hit without touching the providers', async () => {
    // The cache is the protocol's latency backstop. A hit must not reach a
    // provider at all — measured by call count, not by timing, so the
    // assertion holds on a loaded runner.
    const slow = provider('slow', 1, 1.0, 601).setLatency({ baseMs: 100, jitterMs: 0 });
    const aggregator = buildStack([slow]);

    const cold = await timed(() => aggregator.getPrice(ASSET));
    const callsAfterCold = slow.callCount;

    const warm = await timed(() => aggregator.getPrice(ASSET));
    const callsAfterWarm = slow.callCount;

    const servedFromCache = callsAfterWarm === callsAfterCold && warm.result !== null;

    recordScenario({
      category: 'latency',
      name: 'Cache hit bypasses a slow provider',
      passed: servedFromCache,
      iterations: 2,
      successes: 2,
      failures: 0,
      notes: {
        coldRoundMs: cold.elapsedMs,
        warmRoundMs: warm.elapsedMs,
        providerCallsCold: callsAfterCold,
        providerCallsWarm: callsAfterWarm,
        invariant: 'a cache hit issues zero provider calls',
      },
    });

    expect(servedFromCache).toBe(true);
  });

  it('does not let a hung provider block the rest of the round indefinitely', async () => {
    // A provider that answers only after a long delay still resolves, but the
    // round must complete and return a usable price from its peers rather than
    // failing outright.
    const providers = [
      provider('hung', 1, 0.34, 701).setLatency({ baseMs: 200, jitterMs: 0 }),
      provider('healthy-a', 2, 0.33, 702).setLatency({ baseMs: 1, jitterMs: 0 }),
      provider('healthy-b', 3, 0.33, 703).setLatency({ baseMs: 1, jitterMs: 0 }),
    ];
    const aggregator = buildStack(providers, { minSources: 2 });

    const { result, elapsedMs } = await timed(() => aggregator.getPrice(ASSET));

    const completed = result !== null;

    recordScenario({
      category: 'latency',
      name: 'Hung provider does not fail the round',
      passed: completed,
      iterations: 1,
      successes: completed ? 1 : 0,
      failures: completed ? 0 : 1,
      notes: {
        hungProviderLatencyMs: 200,
        observedRoundMs: elapsedMs,
        minSources: 2,
        invariant: 'the round completes with a valid price despite one slow venue',
      },
    });

    expect(completed).toBe(true);
  });
});
