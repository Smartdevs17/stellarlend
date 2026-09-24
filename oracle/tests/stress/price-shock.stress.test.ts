/**
 * Price Spike / Crash Simulation Stress Tests (#691)
 *
 * Models the market conditions that break lending protocols: flash crashes,
 * vertical spikes, sustained volatility, and single-source manipulation.
 *
 * The feed has three independent guards against a bad print, and this suite
 * exercises all of them:
 *
 *   1. Round consensus — a quote that disagrees with the median of its peers
 *      is dropped before it can influence the aggregate.
 *   2. Cross-round drift — the validator rejects a quote that moves more than
 *      `maxDeviationPercent` from the last accepted price.
 *   3. TWAP guard — the aggregator suppresses an aggregate that deviates more
 *      than 5% from the 30-minute time-weighted average.
 *
 * Publishing a flash-crash print liquidates solvent positions, so suppression
 * is the correct outcome for an unconfirmed move. The property that makes
 * suppression safe rather than merely conservative is that the feed *recovers*
 * once the new level is corroborated over time — a guard that wedges the feed
 * permanently would leave the protocol blind exactly when it must act.
 *
 * ## Why fake timers
 *
 * Every guard here is time-dependent: the TWAP window ages samples out by
 * wall-clock, and the price cache serves a hit for the remainder of the current
 * millisecond. Run against the real clock, a whole price path executes inside
 * one or two milliseconds, so the cache freezes the feed and the TWAP window
 * never advances — the suite would measure its own execution speed instead of
 * the protocol. Each tick therefore advances the simulated clock by
 * `SECONDS_PER_TICK`, which both expires the cache and moves the TWAP window.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createAggregator } from '../../src/services/price-aggregator.js';
import { createValidator } from '../../src/services/price-validator.js';
import { createPriceCache } from '../../src/services/cache.js';
import { PriceHistoryService } from '../../src/services/price-history.js';
import { scalePrice } from '../../src/config.js';
import {
  StressProvider,
  FailureMode,
  generateShockPath,
  deviationPercent,
  recordScenario,
} from './harness.js';

const ASSET = 'BTC';
const START_PRICE = 50_000;
/** One minute of simulated time per tick; the TWAP window is 30 ticks wide. */
const SECONDS_PER_TICK = 60;

interface Stack {
  aggregator: ReturnType<typeof createAggregator>;
  providers: StressProvider[];
}

function buildStack(): Stack {
  const providers = [
    new StressProvider({ name: 'alpha', priority: 1, weight: 0.4, seed: 11 }),
    new StressProvider({ name: 'beta', priority: 2, weight: 0.35, seed: 22 }),
    new StressProvider({ name: 'gamma', priority: 3, weight: 0.25, seed: 33 }),
  ];
  for (const p of providers) p.setPrice(ASSET, START_PRICE);

  const aggregator = createAggregator(
    providers,
    createValidator({ maxDeviationPercent: 10, maxStalenessSeconds: 300 }),
    createPriceCache(0),
    new PriceHistoryService(),
    { minSources: 1, useWeightedMedian: true, circuitBreaker: { failureThreshold: 1_000_000 } }
  );

  return { aggregator, providers };
}

/** Advance simulated time, move every provider to `price`, take one reading. */
async function tick(stack: Stack, price: number) {
  vi.advanceTimersByTime(SECONDS_PER_TICK * 1000);
  for (const p of stack.providers) p.setPrice(ASSET, price);
  return stack.aggregator.getPrice(ASSET);
}

describe('Oracle stress: price spike and crash simulation', () => {
  let stack: Stack;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T00:00:00Z'));
    stack = buildStack();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('suppresses an instantaneous 60% flash crash', async () => {
    const baseline = await tick(stack, START_PRICE);
    expect(baseline).not.toBeNull();

    const crashPrice = START_PRICE * 0.4;
    const crashed = await tick(stack, crashPrice);

    recordScenario({
      category: 'price-shock',
      name: 'Instantaneous −60% flash crash',
      passed: crashed === null,
      iterations: 2,
      successes: crashed === null ? 0 : 1,
      failures: crashed === null ? 1 : 0,
      notes: {
        startPrice: START_PRICE,
        crashPrice,
        movePercent: `-${deviationPercent(START_PRICE, crashPrice).toFixed(1)}%`,
        outcome: crashed === null ? 'suppressed' : 'published',
        invariant: 'an uncorroborated crash print is never published',
      },
    });

    expect(crashed).toBeNull();
  });

  it('suppresses an instantaneous 150% spike', async () => {
    const baseline = await tick(stack, START_PRICE);
    expect(baseline).not.toBeNull();

    const spikePrice = START_PRICE * 2.5;
    const spiked = await tick(stack, spikePrice);

    recordScenario({
      category: 'price-shock',
      name: 'Instantaneous +150% spike',
      passed: spiked === null,
      iterations: 2,
      successes: spiked === null ? 0 : 1,
      failures: spiked === null ? 1 : 0,
      notes: {
        startPrice: START_PRICE,
        spikePrice,
        movePercent: `+${deviationPercent(START_PRICE, spikePrice).toFixed(1)}%`,
        outcome: spiked === null ? 'suppressed' : 'published',
        invariant: 'an uncorroborated spike print is never published',
      },
    });

    expect(spiked).toBeNull();
  });

  it('tracks a mild decline that stays inside every guard band', async () => {
    // A 4% drift over 10 minutes: inside the 10% drift band and inside the 5%
    // TWAP band. Suppressing this would blind the protocol to ordinary market
    // movement, so every tick must publish.
    const path = generateShockPath({ startPrice: START_PRICE, magnitude: -0.04, steps: 10 });
    const published: bigint[] = [];

    for (const price of path) {
      const result = await tick(stack, price);
      if (result) published.push(result.price);
    }

    const trackedAll = published.length === path.length;
    const endsAtTruth = published.at(-1) === scalePrice(path.at(-1)!);

    recordScenario({
      category: 'price-shock',
      name: 'Mild −4% drift over 10 ticks',
      passed: trackedAll && endsAtTruth,
      iterations: path.length,
      successes: published.length,
      failures: path.length - published.length,
      notes: {
        perTickMovePercent: `${deviationPercent(path[0]!, path[1]!).toFixed(2)}%`,
        totalMove: '-4%',
        finalPublished: published.at(-1)?.toString() ?? 'none',
        invariant: 'ordinary market movement is tracked tick for tick',
      },
    });

    expect(trackedAll).toBe(true);
    expect(endsAtTruth).toBe(true);
  });

  it('suppresses a sustained crash while it falls, then republishes once it settles', async () => {
    // The headline availability property. A −50% decline over 30 minutes
    // outruns the TWAP band, so the feed goes quiet mid-fall. Once the market
    // holds the new level long enough for the TWAP window to catch up, the feed
    // must come back — a permanently wedged feed is as dangerous as a wrong one.
    await tick(stack, START_PRICE);

    const path = generateShockPath({ startPrice: START_PRICE, magnitude: -0.5, steps: 30 });
    let publishedDuringFall = 0;
    for (const price of path) {
      if (await tick(stack, price)) publishedDuringFall++;
    }

    const settled = START_PRICE * 0.5;
    let recovered: bigint | null = null;
    let ticksToRecover = 0;
    for (let i = 0; i < 40; i++) {
      const result = await tick(stack, settled);
      if (result) {
        recovered = result.price;
        ticksToRecover = i + 1;
        break;
      }
    }

    const suppressedMidFall = publishedDuringFall < path.length;
    const republishedAtNewLevel = recovered === scalePrice(settled);

    recordScenario({
      category: 'price-shock',
      name: 'Sustained −50% crash, then recovery at the new level',
      passed: suppressedMidFall && republishedAtNewLevel,
      iterations: path.length + ticksToRecover,
      successes: publishedDuringFall + (recovered ? 1 : 0),
      failures: path.length - publishedDuringFall,
      notes: {
        publishedDuringFall,
        fallTicks: path.length,
        ticksToRecover,
        settledPrice: settled,
        recoveredPrice: recovered?.toString() ?? 'none',
        invariant: 'guards suppress temporarily; they never wedge the feed',
      },
    });

    expect(suppressedMidFall).toBe(true);
    expect(republishedAtNewLevel).toBe(true);
  });

  it('ignores a single-source spike that its peers do not confirm', async () => {
    // One venue printing a wick — a classic manipulation attempt. The other two
    // must hold the aggregate at the true price.
    await tick(stack, START_PRICE);

    vi.advanceTimersByTime(SECONDS_PER_TICK * 1000);
    stack.providers[0]!.setPrice(ASSET, START_PRICE * 3);
    const result = await stack.aggregator.getPrice(ASSET);

    const held = result !== null && result.price === scalePrice(START_PRICE);

    recordScenario({
      category: 'price-shock',
      name: 'Single-source +200% wick, unconfirmed by peers',
      passed: held,
      iterations: 1,
      successes: held ? 1 : 0,
      failures: held ? 0 : 1,
      notes: {
        manipulatedProvider: 'alpha',
        manipulatedPrice: START_PRICE * 3,
        aggregate: result?.price.toString() ?? 'null',
        expected: scalePrice(START_PRICE).toString(),
        invariant: 'consensus of honest peers absorbs a lone manipulated quote',
      },
    });

    expect(held).toBe(true);
  });

  it('never fabricates a value under sustained whipsaw volatility', async () => {
    // Alternating swings for 60 ticks. Whether a tick publishes or is
    // suppressed, any value that *is* published must be a price a provider
    // actually quoted — never an interpolation or a stale carry-over.
    await tick(stack, START_PRICE);

    const high = START_PRICE * 1.04;
    const low = START_PRICE * 0.96;
    const quoted = new Set(
      [START_PRICE, high, low].map((p) => scalePrice(p).toString())
    );

    const published: bigint[] = [];
    for (let i = 0; i < 60; i++) {
      const result = await tick(stack, i % 2 === 0 ? high : low);
      if (result) published.push(result.price);
    }

    const allQuoted = published.every((p) => quoted.has(p.toString()));

    recordScenario({
      category: 'price-shock',
      name: 'Sustained ±4% whipsaw over 60 ticks',
      passed: allQuoted && published.length > 0,
      iterations: 60,
      successes: published.length,
      failures: 60 - published.length,
      notes: {
        high,
        low,
        publishedCount: published.length,
        invariant: 'published values are always real quotes, never interpolations',
      },
    });

    expect(published.length).toBeGreaterThan(0);
    expect(allQuoted).toBe(true);
  });

  it('holds the honest level when a provider goes byzantine during a real decline', async () => {
    // Worst case: the market really is moving AND one feed is lying. The honest
    // pair must still determine every published aggregate.
    await tick(stack, START_PRICE);

    const path = generateShockPath({ startPrice: START_PRICE, magnitude: -0.04, steps: 15 });
    stack.providers[0]!.setFailure(1, FailureMode.BYZANTINE);

    const published: { expected: bigint; actual: bigint }[] = [];
    for (const price of path) {
      vi.advanceTimersByTime(SECONDS_PER_TICK * 1000);
      for (const p of stack.providers) p.setPrice(ASSET, price);
      const result = await stack.aggregator.getPrice(ASSET);
      if (result) published.push({ expected: scalePrice(price), actual: result.price });
    }

    const allHonest =
      published.length > 0 && published.every((p) => p.actual === p.expected);

    recordScenario({
      category: 'price-shock',
      name: 'Byzantine feed during a real −4% decline',
      passed: allHonest,
      iterations: path.length,
      successes: published.length,
      failures: path.length - published.length,
      notes: {
        byzantineProvider: 'alpha',
        byzantineSkew: '+40%',
        honestFinalPrice: path.at(-1)!.toFixed(2),
        publishedCount: published.length,
        invariant: 'a lying feed cannot steer the aggregate during real volatility',
      },
    });

    expect(allHonest).toBe(true);
  });
});
