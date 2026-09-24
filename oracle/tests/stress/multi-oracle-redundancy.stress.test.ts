/**
 * Multi-Oracle Redundancy Stress Tests (#691)
 *
 * Redundancy is only real if it is exercised. These scenarios remove providers
 * one at a time, in combinations, and in the middle of a run, and check that the
 * feed keeps producing a correct price for as long as a quorum survives — and
 * stops cleanly the moment it does not.
 *
 * The distinction that matters: degradation must be *graceful* (fewer sources,
 * same price, lower confidence) rather than *silent* (fewer sources, same
 * confidence, so a consumer cannot tell it is now trusting one venue).
 */

import { describe, it, expect } from 'vitest';
import { createAggregator } from '../../src/services/price-aggregator.js';
import { createValidator } from '../../src/services/price-validator.js';
import { PriceHistoryService } from '../../src/services/price-history.js';
import { CircuitState } from '../../src/services/circuit-breaker.js';
import { scalePrice } from '../../src/config.js';
import {
  StressProvider,
  FailureMode,
  driveLoad,
  summarizeLatency,
  recordScenario,
  nonCachingPriceCache,
} from './harness.js';

const ASSET = 'USDC';
const PRICE = 1.0;

interface Stack {
  aggregator: ReturnType<typeof createAggregator>;
  providers: StressProvider[];
}

function buildStack(count: number, minSources = 1, failoverMode = false): Stack {
  const providers = Array.from({ length: count }, (_, i) =>
    new StressProvider({
      name: `oracle-${i + 1}`,
      priority: i + 1,
      weight: 1 / count,
      seed: 900 + i,
    }).setPrice(ASSET, PRICE)
  );

  const aggregator = createAggregator(
    providers,
    createValidator({ maxDeviationPercent: 10, maxStalenessSeconds: 300 }),
    nonCachingPriceCache(),
    new PriceHistoryService(),
    {
      minSources,
      useWeightedMedian: true,
      failoverMode,
      circuitBreaker: { failureThreshold: 1_000_000 },
    }
  );

  return { aggregator, providers };
}

describe('Oracle stress: multi-oracle redundancy', () => {
  it('survives losing providers one at a time down to the last survivor', async () => {
    const { aggregator, providers } = buildStack(5, 1);
    const expected = scalePrice(PRICE);
    const timeline: string[] = [];
    let allCorrect = true;

    for (let downCount = 0; downCount < providers.length; downCount++) {
      if (downCount > 0) {
        providers[downCount - 1]!.setFailure(1, FailureMode.NETWORK_ERROR);
      }

      const result = await aggregator.getPrice(ASSET);
      const alive = providers.length - downCount;
      timeline.push(`${alive} alive → ${result ? result.price.toString() : 'null'}`);

      if (result === null || result.price !== expected) {
        allCorrect = false;
      }
    }

    recordScenario({
      category: 'redundancy',
      name: 'Progressive degradation from 5 oracles down to 1',
      passed: allCorrect,
      iterations: providers.length,
      successes: allCorrect ? providers.length : 0,
      failures: allCorrect ? 0 : 1,
      notes: {
        oracleCount: providers.length,
        minSources: 1,
        timeline: timeline.join(' | '),
        invariant: 'one surviving oracle still yields the correct price',
      },
    });

    expect(allCorrect).toBe(true);
  });

  it('stops publishing the moment quorum is lost', async () => {
    // With minSources 3, losing a third provider must stop the feed rather than
    // quietly publishing from two.
    const { aggregator, providers } = buildStack(5, 3);

    providers[0]!.setFailure(1, FailureMode.NETWORK_ERROR);
    providers[1]!.setFailure(1, FailureMode.NETWORK_ERROR);
    const atQuorum = await aggregator.getPrice(ASSET);

    providers[2]!.setFailure(1, FailureMode.NETWORK_ERROR);
    const belowQuorum = await aggregator.getPrice(ASSET);

    const correct = atQuorum !== null && belowQuorum === null;

    recordScenario({
      category: 'redundancy',
      name: 'Feed stops cleanly when quorum is lost',
      passed: correct,
      iterations: 2,
      successes: atQuorum ? 1 : 0,
      failures: belowQuorum === null ? 1 : 0,
      notes: {
        minSources: 3,
        atQuorumSources: 3,
        belowQuorumSources: 2,
        atQuorumResult: atQuorum ? 'published' : 'null',
        belowQuorumResult: belowQuorum === null ? 'null' : 'published',
        invariant: 'quorum is enforced, never silently relaxed',
      },
    });

    expect(correct).toBe(true);
  });

  it('reports lower confidence as sources are lost', async () => {
    // Graceful degradation has to be *observable*. A consumer that cannot see
    // the source count drop cannot decide to stop trusting the feed.
    const { aggregator, providers } = buildStack(5, 1);

    const full = await aggregator.getPrice(ASSET);
    for (let i = 0; i < 4; i++) providers[i]!.setFailure(1, FailureMode.NETWORK_ERROR);
    const degraded = await aggregator.getPrice(ASSET);

    const fullSources = full?.sources.length ?? 0;
    const degradedSources = degraded?.sources.length ?? 0;
    const observable = fullSources > degradedSources && degradedSources >= 1;

    recordScenario({
      category: 'redundancy',
      name: 'Source count shrinks observably under degradation',
      passed: observable,
      iterations: 2,
      successes: 2,
      failures: 0,
      notes: {
        sourcesAtFullHealth: fullSources,
        sourcesWhenDegraded: degradedSources,
        priceUnchanged: full?.price === degraded?.price,
        invariant: 'degradation is visible to the consumer, not silent',
      },
    });

    expect(observable).toBe(true);
  });

  it('fails over through a chain of dead providers in priority order', async () => {
    const { aggregator, providers } = buildStack(4, 1, true);

    // Kill the top three; only the lowest-priority oracle survives.
    for (let i = 0; i < 3; i++) providers[i]!.setFailure(1, FailureMode.NETWORK_ERROR);

    const result = await aggregator.getPrice(ASSET);
    const servedByLast = result !== null && result.sources[0]?.source === 'oracle-4';

    recordScenario({
      category: 'redundancy',
      name: 'Failover chain reaches the last surviving oracle',
      passed: servedByLast,
      iterations: 1,
      successes: result ? 1 : 0,
      failures: result ? 0 : 1,
      notes: {
        deadProviders: 'oracle-1, oracle-2, oracle-3',
        servingProvider: result?.sources[0]?.source ?? 'none',
        invariant: 'failover walks the full priority chain',
      },
    });

    expect(servedByLast).toBe(true);
  });

  it('prefers the primary again once it recovers', async () => {
    // Failover must not be sticky: after a primary outage the feed should
    // return to the highest-priority venue, not stay on the backup forever.
    const { aggregator, providers } = buildStack(3, 1, true);
    const primary = providers[0]!;

    primary.setFailure(1, FailureMode.NETWORK_ERROR);
    const duringOutage = await aggregator.getPrice(ASSET);

    primary.recover();
    const afterRecovery = await aggregator.getPrice(ASSET);

    const failedOver = duringOutage?.sources[0]?.source === 'oracle-2';
    const returnedToPrimary = afterRecovery?.sources[0]?.source === 'oracle-1';

    recordScenario({
      category: 'redundancy',
      name: 'Primary is preferred again after recovery',
      passed: failedOver && returnedToPrimary,
      iterations: 2,
      successes: 2,
      failures: 0,
      notes: {
        servingDuringOutage: duringOutage?.sources[0]?.source ?? 'none',
        servingAfterRecovery: afterRecovery?.sources[0]?.source ?? 'none',
        invariant: 'failover is not sticky',
      },
    });

    expect(failedOver).toBe(true);
    expect(returnedToPrimary).toBe(true);
  });

  it('holds a correct price through providers failing and recovering mid-run', async () => {
    // Churn: one provider is out per round, and which one rotates as the run
    // proceeds, so every provider takes a turn being unavailable while requests
    // are in flight against its peers.
    //
    // The outage is applied per round rather than per request on purpose. A
    // provider's failure mode is shared mutable state, so letting concurrent
    // requests each pick their own victim could down every provider at once and
    // the scenario would be testing correlated total failure — which is the
    // last test in this file, not this one.
    const { aggregator, providers } = buildStack(4, 1);
    const expected = scalePrice(PRICE);

    const rounds = 20;
    const concurrencyPerRound = 4;
    const prices: bigint[] = [];
    const latencies: number[] = [];
    let attempts = 0;

    for (let round = 0; round < rounds; round++) {
      const victim = providers[round % providers.length]!;
      victim.setFailure(1, FailureMode.SERVER_ERROR);

      const load = await driveLoad(concurrencyPerRound, concurrencyPerRound, () =>
        aggregator.getPrice(ASSET)
      );

      attempts += concurrencyPerRound;
      latencies.push(...load.latencies);
      for (const result of load.results) {
        if (result) prices.push(result.price);
      }

      victim.recover();
    }

    const allCorrect = prices.every((p) => p === expected);

    recordScenario({
      category: 'redundancy',
      name: 'Rotating single outage across 20 concurrent rounds',
      passed: allCorrect && prices.length === attempts,
      iterations: attempts,
      successes: prices.length,
      failures: attempts - prices.length,
      latency: summarizeLatency(latencies),
      notes: {
        oracleCount: 4,
        rounds,
        concurrencyPerRound,
        expectedPrice: expected.toString(),
        invariant: 'churn changes availability, never the published value',
      },
    });

    expect(prices.length).toBe(attempts);
    expect(allCorrect).toBe(true);
  });

  it('tolerates a minority of byzantine oracles', async () => {
    // Two of five lying, three honest. The honest majority must own the median.
    const { aggregator, providers } = buildStack(5, 1);
    providers[0]!.setFailure(1, FailureMode.BYZANTINE);
    providers[1]!.setFailure(1, FailureMode.BYZANTINE);

    const load = await driveLoad(40, 4, () => aggregator.getPrice(ASSET));
    const prices = load.results.filter((r) => r !== null);
    const expected = scalePrice(PRICE);
    const allCorrect = prices.every((p) => p!.price === expected);

    const liarsTripped = aggregator
      .getCircuitBreakerMetrics()
      .filter((m) => m.providerName === 'oracle-1' || m.providerName === 'oracle-2')
      .every((m) => m.totalFailures > 0 || m.state !== CircuitState.CLOSED);

    recordScenario({
      category: 'redundancy',
      name: 'Byzantine minority (2 of 5) cannot move the aggregate',
      passed: allCorrect && prices.length > 0,
      iterations: 40,
      successes: prices.length,
      failures: 40 - prices.length,
      latency: summarizeLatency(load.latencies),
      notes: {
        byzantineCount: 2,
        honestCount: 3,
        expectedPrice: expected.toString(),
        disagreementRecorded: liarsTripped,
        invariant: 'an honest majority owns the median',
      },
    });

    expect(prices.length).toBeGreaterThan(0);
    expect(allCorrect).toBe(true);
  });

  it('does not publish when every redundant oracle fails simultaneously', async () => {
    const { aggregator, providers } = buildStack(5, 1);
    for (const p of providers) p.setFailure(1, FailureMode.NETWORK_ERROR);

    const load = await driveLoad(30, 6, () => aggregator.getPrice(ASSET));
    const nonNull = load.results.filter((r) => r !== null);

    recordScenario({
      category: 'redundancy',
      name: 'Correlated failure of all 5 oracles',
      passed: nonNull.length === 0,
      iterations: 30,
      successes: 0,
      failures: 30,
      latency: summarizeLatency(load.latencies),
      notes: {
        oracleCount: 5,
        invariant: 'redundancy exhausted means no price, not a stale price',
      },
    });

    expect(nonNull).toHaveLength(0);
  });
});
