/**
 * Oracle Upgrade Stress Tests (#691)
 *
 * An oracle upgrade — swapping a provider, changing weights, tightening
 * thresholds, rolling a new aggregation mode — happens on a live protocol with
 * open positions. The risk is not that the new configuration is wrong; it is
 * that the *transition* drops the feed or publishes a discontinuity, and a
 * discontinuity liquidates people.
 *
 * These scenarios rehearse each kind of upgrade against a running feed and
 * assert the two properties that make an upgrade safe:
 *
 *   - continuity: the feed keeps answering across the change, and
 *   - equivalence: the price does not jump purely because the config changed.
 *
 * Rollback is covered too, because an upgrade you cannot reverse is not a
 * deployment strategy.
 */

import { describe, it, expect } from 'vitest';
import { createAggregator } from '../../src/services/price-aggregator.js';
import { createValidator } from '../../src/services/price-validator.js';
import { PriceHistoryService } from '../../src/services/price-history.js';
import { scalePrice } from '../../src/config.js';
import {
  StressProvider,
  FailureMode,
  driveLoad,
  summarizeLatency,
  nonCachingPriceCache,
  recordScenario,
} from './harness.js';

const ASSET = 'XLM';
const PRICE = 0.15;

interface BuildOptions {
  minSources?: number;
  failoverMode?: boolean;
  useWeightedMedian?: boolean;
  maxDeviationPercent?: number;
}

function buildAggregator(providers: StressProvider[], options: BuildOptions = {}) {
  return createAggregator(
    providers,
    createValidator({
      maxDeviationPercent: options.maxDeviationPercent ?? 10,
      maxStalenessSeconds: 300,
    }),
    nonCachingPriceCache(),
    new PriceHistoryService(),
    {
      minSources: options.minSources ?? 1,
      useWeightedMedian: options.useWeightedMedian ?? true,
      failoverMode: options.failoverMode ?? false,
      circuitBreaker: { failureThreshold: 1_000_000 },
    }
  );
}

function provider(name: string, priority: number, weight: number, seed: number): StressProvider {
  return new StressProvider({ name, priority, weight, seed }).setPrice(ASSET, PRICE);
}

describe('Oracle stress: upgrade', () => {
  it('keeps serving while a provider is swapped out for a replacement', async () => {
    // Blue/green on a single source: the old provider is drained, a new one is
    // introduced, and the feed must not miss a beat.
    const stable = provider('stable', 1, 0.5, 1001);
    const outgoing = provider('outgoing-v1', 2, 0.5, 1002);
    const incoming = provider('incoming-v2', 2, 0.5, 1003);

    const before = buildAggregator([stable, outgoing]);
    const beforeResult = await before.getPrice(ASSET);

    // Both old and new run side by side during the cutover window.
    const during = buildAggregator([stable, outgoing, incoming]);
    const duringResult = await during.getPrice(ASSET);

    // Old provider is removed.
    const after = buildAggregator([stable, incoming]);
    const afterResult = await after.getPrice(ASSET);

    const expected = scalePrice(PRICE);
    const continuous =
      beforeResult?.price === expected &&
      duringResult?.price === expected &&
      afterResult?.price === expected;

    recordScenario({
      category: 'upgrade',
      name: 'Provider swap (blue/green) keeps the feed continuous',
      passed: continuous,
      iterations: 3,
      successes: [beforeResult, duringResult, afterResult].filter(Boolean).length,
      failures: [beforeResult, duringResult, afterResult].filter((r) => !r).length,
      notes: {
        before: beforeResult?.price.toString() ?? 'null',
        during: duringResult?.price.toString() ?? 'null',
        after: afterResult?.price.toString() ?? 'null',
        invariant: 'no gap and no discontinuity across the cutover',
      },
    });

    expect(continuous).toBe(true);
  });

  it('does not move the price when provider weights are reconfigured', async () => {
    // Reweighting is the most common oracle change and the easiest to get
    // wrong. When every source agrees, no weighting can justify a different
    // answer — if the price moves here, the weights are being applied to
    // something other than a median.
    const expected = scalePrice(PRICE);

    const evenWeights = [
      provider('a', 1, 0.34, 1011),
      provider('b', 2, 0.33, 1012),
      provider('c', 3, 0.33, 1013),
    ];
    const skewedWeights = [
      provider('a', 1, 0.8, 1014),
      provider('b', 2, 0.15, 1015),
      provider('c', 3, 0.05, 1016),
    ];

    const evenResult = await buildAggregator(evenWeights).getPrice(ASSET);
    const skewedResult = await buildAggregator(skewedWeights).getPrice(ASSET);

    const unchanged = evenResult?.price === expected && skewedResult?.price === expected;

    recordScenario({
      category: 'upgrade',
      name: 'Weight reconfiguration does not move an agreed price',
      passed: unchanged,
      iterations: 2,
      successes: 2,
      failures: 0,
      notes: {
        evenWeights: '0.34 / 0.33 / 0.33',
        skewedWeights: '0.80 / 0.15 / 0.05',
        evenPrice: evenResult?.price.toString() ?? 'null',
        skewedPrice: skewedResult?.price.toString() ?? 'null',
        invariant: 'weights cannot change a unanimous answer',
      },
    });

    expect(unchanged).toBe(true);
  });

  it('survives switching between aggregation and failover mode', async () => {
    const expected = scalePrice(PRICE);
    const providers = [
      provider('a', 1, 0.4, 1021),
      provider('b', 2, 0.35, 1022),
      provider('c', 3, 0.25, 1023),
    ];

    const aggregationResult = await buildAggregator(providers, {
      failoverMode: false,
    }).getPrice(ASSET);
    const failoverResult = await buildAggregator(providers, {
      failoverMode: true,
    }).getPrice(ASSET);

    const equivalent =
      aggregationResult?.price === expected && failoverResult?.price === expected;

    recordScenario({
      category: 'upgrade',
      name: 'Switching aggregation ⇄ failover mode preserves the price',
      passed: equivalent,
      iterations: 2,
      successes: 2,
      failures: 0,
      notes: {
        aggregationPrice: aggregationResult?.price.toString() ?? 'null',
        failoverPrice: failoverResult?.price.toString() ?? 'null',
        aggregationSources: aggregationResult?.sources.length ?? 0,
        failoverSources: failoverResult?.sources.length ?? 0,
        invariant: 'mode changes availability characteristics, not the value',
      },
    });

    expect(equivalent).toBe(true);
  });

  it('survives switching between weighted and simple median', async () => {
    const expected = scalePrice(PRICE);
    const providers = [
      provider('a', 1, 0.5, 1031),
      provider('b', 2, 0.3, 1032),
      provider('c', 3, 0.2, 1033),
    ];

    const weighted = await buildAggregator(providers, { useWeightedMedian: true }).getPrice(ASSET);
    const simple = await buildAggregator(providers, { useWeightedMedian: false }).getPrice(ASSET);

    const equivalent = weighted?.price === expected && simple?.price === expected;

    recordScenario({
      category: 'upgrade',
      name: 'Switching weighted ⇄ simple median preserves the price',
      passed: equivalent,
      iterations: 2,
      successes: 2,
      failures: 0,
      notes: {
        weightedPrice: weighted?.price.toString() ?? 'null',
        simplePrice: simple?.price.toString() ?? 'null',
        invariant: 'both median strategies agree when sources agree',
      },
    });

    expect(equivalent).toBe(true);
  });

  it('applies a tightened deviation threshold immediately', async () => {
    // Tightening a threshold is a security upgrade; it has to take effect on
    // the very next round, not after some warm-up.
    const lenientProviders = [
      provider('a', 1, 0.4, 1041),
      provider('b', 2, 0.35, 1042),
      provider('c', 3, 0.25, 1043),
    ];
    const strictProviders = [
      provider('a', 1, 0.4, 1044),
      provider('b', 2, 0.35, 1045),
      provider('c', 3, 0.25, 1046),
    ];

    const lenient = buildAggregator(lenientProviders, { maxDeviationPercent: 50 });
    const strict = buildAggregator(strictProviders, { maxDeviationPercent: 2 });

    // Establish a reference on both, then present a 20% move.
    await lenient.getPrice(ASSET);
    await strict.getPrice(ASSET);

    const moved = PRICE * 1.2;
    for (const p of lenientProviders) p.setPrice(ASSET, moved);
    for (const p of strictProviders) p.setPrice(ASSET, moved);

    const lenientResult = await lenient.getPrice(ASSET);
    const strictResult = await strict.getPrice(ASSET);

    // The lenient config tolerates the move; the strict one must reject it.
    const thresholdEnforced = lenientResult !== null && strictResult === null;

    recordScenario({
      category: 'upgrade',
      name: 'Tightened deviation threshold takes effect immediately',
      passed: thresholdEnforced,
      iterations: 2,
      successes: lenientResult ? 1 : 0,
      failures: strictResult === null ? 1 : 0,
      notes: {
        movePercent: '+20%',
        lenientThreshold: '50%',
        strictThreshold: '2%',
        lenientOutcome: lenientResult ? 'published' : 'rejected',
        strictOutcome: strictResult === null ? 'rejected' : 'published',
        invariant: 'a tightened threshold is enforced on the next round',
      },
    });

    expect(thresholdEnforced).toBe(true);
  });

  it('raises the quorum requirement without dropping the feed', async () => {
    // Raising minSources is safe only while enough sources are live. The
    // upgrade must succeed with headroom and fail closed without it.
    const providers = [
      provider('a', 1, 0.25, 1051),
      provider('b', 2, 0.25, 1052),
      provider('c', 3, 0.25, 1053),
      provider('d', 4, 0.25, 1054),
    ];

    const withHeadroom = await buildAggregator(providers, { minSources: 3 }).getPrice(ASSET);

    // Now two go down and the raised quorum can no longer be met.
    providers[0]!.setFailure(1, FailureMode.NETWORK_ERROR);
    providers[1]!.setFailure(1, FailureMode.NETWORK_ERROR);
    const withoutHeadroom = await buildAggregator(providers, { minSources: 3 }).getPrice(ASSET);

    const correct = withHeadroom !== null && withoutHeadroom === null;

    recordScenario({
      category: 'upgrade',
      name: 'Raised quorum holds with headroom and fails closed without it',
      passed: correct,
      iterations: 2,
      successes: withHeadroom ? 1 : 0,
      failures: withoutHeadroom === null ? 1 : 0,
      notes: {
        newMinSources: 3,
        liveSourcesWithHeadroom: 4,
        liveSourcesWithoutHeadroom: 2,
        invariant: 'a raised quorum fails closed, never silently relaxes',
      },
    });

    expect(correct).toBe(true);
  });

  it('rolls back cleanly to the previous configuration', async () => {
    // A bad upgrade has to be reversible. Roll forward to a config that stops
    // the feed, then roll back and confirm the feed returns to exactly the
    // pre-upgrade value.
    const expected = scalePrice(PRICE);
    const providers = [
      provider('a', 1, 0.4, 1061),
      provider('b', 2, 0.35, 1062),
      provider('c', 3, 0.25, 1063),
    ];

    const v1 = buildAggregator(providers, { minSources: 1 });
    const beforeUpgrade = await v1.getPrice(ASSET);

    // Roll forward to a quorum the deployment cannot satisfy.
    const v2 = buildAggregator(providers, { minSources: 5 });
    const afterUpgrade = await v2.getPrice(ASSET);

    // Roll back.
    const rolledBack = buildAggregator(providers, { minSources: 1 });
    const afterRollback = await rolledBack.getPrice(ASSET);

    const recovered =
      beforeUpgrade?.price === expected &&
      afterUpgrade === null &&
      afterRollback?.price === expected;

    recordScenario({
      category: 'upgrade',
      name: 'Rollback restores the pre-upgrade behaviour exactly',
      passed: recovered,
      iterations: 3,
      successes: [beforeUpgrade, afterRollback].filter(Boolean).length,
      failures: afterUpgrade === null ? 1 : 0,
      notes: {
        beforeUpgrade: beforeUpgrade?.price.toString() ?? 'null',
        afterBadUpgrade: afterUpgrade === null ? 'null (feed stopped)' : 'published',
        afterRollback: afterRollback?.price.toString() ?? 'null',
        invariant: 'rollback is exact, not approximate',
      },
    });

    expect(recovered).toBe(true);
  });

  it('holds the feed through a rolling restart of every provider in turn', async () => {
    // A rolling upgrade takes each provider down briefly, one at a time. With
    // more than one source configured, the feed should never go dark.
    const providers = [
      provider('a', 1, 0.34, 1071),
      provider('b', 2, 0.33, 1072),
      provider('c', 3, 0.33, 1073),
    ];
    const aggregator = buildAggregator(providers, { minSources: 1 });
    const expected = scalePrice(PRICE);

    const prices: bigint[] = [];
    const latencies: number[] = [];
    let attempts = 0;

    // Each provider gets a turn being restarted; requests run against the rest.
    for (const restarting of providers) {
      restarting.setFailure(1, FailureMode.NETWORK_ERROR);

      const load = await driveLoad(6, 3, () => aggregator.getPrice(ASSET));
      attempts += 6;
      latencies.push(...load.latencies);
      for (const result of load.results) {
        if (result) prices.push(result.price);
      }

      restarting.recover();
    }

    const neverDark = prices.length === attempts;
    const allCorrect = prices.every((p) => p === expected);

    recordScenario({
      category: 'upgrade',
      name: 'Rolling restart of all 3 providers never darkens the feed',
      passed: neverDark && allCorrect,
      iterations: attempts,
      successes: prices.length,
      failures: attempts - prices.length,
      latency: summarizeLatency(latencies),
      notes: {
        providerCount: 3,
        requestsPerRestart: 6,
        expectedPrice: expected.toString(),
        invariant: 'a rolling upgrade is invisible to consumers',
      },
    });

    expect(neverDark).toBe(true);
    expect(allCorrect).toBe(true);
  });
});
