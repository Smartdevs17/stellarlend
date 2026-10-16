/**
 * Circuit Breaker Validation Stress Tests (#691)
 *
 * The breaker is what stops a dead provider from costing a request on every
 * round. Validating it means proving three things under stress:
 *
 *   - it opens promptly when a provider is genuinely down,
 *   - it does not open on transient noise that the aggregate absorbs, and
 *   - it closes again once the provider recovers, without manual intervention.
 *
 * A breaker that fails the third property is worse than none at all: it turns a
 * five-minute outage into a permanent loss of a price source.
 *
 * Scenarios that depend on the backoff window elapsing use fake timers, because
 * a real 30-second backoff would make the suite unusable in CI.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createAggregator } from '../../src/services/price-aggregator.js';
import { createValidator } from '../../src/services/price-validator.js';
import { PriceHistoryService } from '../../src/services/price-history.js';
import { CircuitState, createCircuitBreaker } from '../../src/services/circuit-breaker.js';
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
const PRICE = 0.15;
const FAILURE_THRESHOLD = 3;
const BACKOFF_MS = 30_000;

function buildStack(providers: StressProvider[]) {
  return createAggregator(
    providers,
    createValidator({ maxDeviationPercent: 10, maxStalenessSeconds: 300 }),
    nonCachingPriceCache(),
    new PriceHistoryService(),
    {
      minSources: 1,
      useWeightedMedian: true,
      circuitBreaker: { failureThreshold: FAILURE_THRESHOLD, backoffMs: BACKOFF_MS },
    }
  );
}

function provider(name: string, priority: number, weight: number, seed: number): StressProvider {
  return new StressProvider({ name, priority, weight, seed }).setPrice(ASSET, PRICE);
}

function stateOf(aggregator: ReturnType<typeof createAggregator>, name: string): CircuitState {
  return aggregator.getCircuitBreakerMetrics().find((m) => m.providerName === name)!.state;
}

describe('Oracle stress: circuit breaker validation', () => {
  describe('breaker unit behaviour under repetition', () => {
    it('opens exactly at the failure threshold, not before', async () => {
      const breaker = createCircuitBreaker({ providerName: 'unit', failureThreshold: 3 });

      breaker.recordFailure();
      breaker.recordFailure();
      const beforeThreshold = breaker.currentState;
      breaker.recordFailure();
      const atThreshold = breaker.currentState;

      const correct =
        beforeThreshold === CircuitState.CLOSED && atThreshold === CircuitState.OPEN;

      recordScenario({
        category: 'circuit-breaker',
        name: 'Opens exactly at the configured threshold',
        passed: correct,
        iterations: 3,
        notes: {
          threshold: 3,
          stateAfter2Failures: beforeThreshold,
          stateAfter3Failures: atThreshold,
          invariant: 'no early trip, no late trip',
        },
      });

      expect(beforeThreshold).toBe(CircuitState.CLOSED);
      expect(atThreshold).toBe(CircuitState.OPEN);
    });

    it('resets the consecutive count on an interleaved success', async () => {
      // Flapping is not the same as being down. Two failures, a success, then
      // two more failures must not trip a threshold-3 breaker.
      const breaker = createCircuitBreaker({ providerName: 'flappy', failureThreshold: 3 });

      breaker.recordFailure();
      breaker.recordFailure();
      breaker.recordSuccess();
      breaker.recordFailure();
      breaker.recordFailure();

      const stillClosed = breaker.currentState === CircuitState.CLOSED;

      recordScenario({
        category: 'circuit-breaker',
        name: 'Interleaved success resets the consecutive failure count',
        passed: stillClosed,
        iterations: 5,
        notes: {
          sequence: 'fail, fail, success, fail, fail',
          threshold: 3,
          finalState: breaker.currentState,
          invariant: 'flapping below threshold does not trip the breaker',
        },
      });

      expect(stillClosed).toBe(true);
    });

    it('re-opens immediately when the half-open probe fails', async () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date('2026-01-01T00:00:00Z'));
      try {
        const breaker = createCircuitBreaker({
          providerName: 'probe',
          failureThreshold: 1,
          backoffMs: BACKOFF_MS,
        });

        breaker.recordFailure();
        expect(breaker.currentState).toBe(CircuitState.OPEN);

        // Still inside backoff — no probe allowed.
        const blockedDuringBackoff = breaker.isAllowed() === false;

        vi.advanceTimersByTime(BACKOFF_MS);
        const probeAllowed = breaker.isAllowed();
        const halfOpen = breaker.currentState === CircuitState.HALF_OPEN;

        // A failed probe must not consume the whole backoff again from CLOSED —
        // it goes straight back to OPEN.
        breaker.recordFailure();
        const reopened = breaker.currentState === CircuitState.OPEN;

        const correct = blockedDuringBackoff && probeAllowed && halfOpen && reopened;

        recordScenario({
          category: 'circuit-breaker',
          name: 'Half-open probe failure re-opens the breaker',
          passed: correct,
          iterations: 1,
          notes: {
            backoffMs: BACKOFF_MS,
            blockedDuringBackoff,
            probeAllowedAfterBackoff: probeAllowed,
            reachedHalfOpen: halfOpen,
            reopenedOnProbeFailure: reopened,
            invariant: 'a failed probe does not admit a second request',
          },
        });

        expect(correct).toBe(true);
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe('breaker behaviour inside the aggregator', () => {
    it('opens a dead provider and stops calling it', async () => {
      const dead = provider('dead', 1, 0.34, 801).setFailure(1, FailureMode.NETWORK_ERROR);
      const healthyA = provider('healthy-a', 2, 0.33, 802);
      const healthyB = provider('healthy-b', 3, 0.33, 803);
      const aggregator = buildStack([dead, healthyA, healthyB]);

      const load = await driveLoad(40, 1, () => aggregator.getPrice(ASSET));

      const callsWhileClosed = dead.callCount;
      const opened = stateOf(aggregator, 'dead') === CircuitState.OPEN;
      // The breaker should have stopped calling well before 40 rounds.
      const stoppedCalling = callsWhileClosed <= FAILURE_THRESHOLD + 1;

      recordScenario({
        category: 'circuit-breaker',
        name: 'Dead provider is opened and no longer called',
        passed: opened && stoppedCalling,
        iterations: 40,
        successes: load.successes,
        failures: load.failures,
        latency: summarizeLatency(load.latencies),
        notes: {
          roundsRun: 40,
          deadProviderCalls: callsWhileClosed,
          threshold: FAILURE_THRESHOLD,
          finalState: stateOf(aggregator, 'dead'),
          invariant: 'a dead provider costs a bounded number of calls, not one per round',
        },
      });

      expect(opened).toBe(true);
      expect(stoppedCalling).toBe(true);
    });

    it('keeps serving correct prices from healthy peers while a breaker is open', async () => {
      const dead = provider('dead', 1, 0.34, 811).setFailure(1, FailureMode.NETWORK_ERROR);
      const healthyA = provider('healthy-a', 2, 0.33, 812);
      const healthyB = provider('healthy-b', 3, 0.33, 813);
      const aggregator = buildStack([dead, healthyA, healthyB]);

      const load = await driveLoad(40, 1, () => aggregator.getPrice(ASSET));
      const prices = load.results.filter((r) => r !== null);
      const expected = scalePrice(PRICE);
      const allCorrect = prices.every((p) => p!.price === expected);

      recordScenario({
        category: 'circuit-breaker',
        name: 'Healthy peers keep serving while a breaker is open',
        passed: allCorrect && prices.length === 40,
        iterations: 40,
        successes: prices.length,
        failures: 40 - prices.length,
        latency: summarizeLatency(load.latencies),
        notes: {
          openedProvider: 'dead',
          expectedPrice: expected.toString(),
          invariant: 'an open breaker degrades capacity, never correctness',
        },
      });

      expect(prices.length).toBe(40);
      expect(allCorrect).toBe(true);
    });

    it('closes the breaker automatically once the provider recovers', async () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date('2026-01-01T00:00:00Z'));
      try {
        const flaky = provider('flaky', 1, 0.34, 821).setFailure(1, FailureMode.SERVER_ERROR);
        const healthyA = provider('healthy-a', 2, 0.33, 822);
        const healthyB = provider('healthy-b', 3, 0.33, 823);
        const aggregator = buildStack([flaky, healthyA, healthyB]);

        // Trip it.
        for (let i = 0; i < FAILURE_THRESHOLD + 1; i++) {
          vi.advanceTimersByTime(1000);
          await aggregator.getPrice(ASSET);
        }
        const openedState = stateOf(aggregator, 'flaky');

        // Provider comes back, but the breaker is still inside its backoff.
        flaky.recover();
        vi.advanceTimersByTime(1000);
        await aggregator.getPrice(ASSET);
        const stillOpen = stateOf(aggregator, 'flaky');

        // Backoff elapses — the probe succeeds and the breaker closes.
        vi.advanceTimersByTime(BACKOFF_MS);
        await aggregator.getPrice(ASSET);
        const recoveredState = stateOf(aggregator, 'flaky');

        const correct =
          openedState === CircuitState.OPEN &&
          stillOpen === CircuitState.OPEN &&
          recoveredState === CircuitState.CLOSED;

        recordScenario({
          category: 'circuit-breaker',
          name: 'Automatic recovery closes the breaker after backoff',
          passed: correct,
          iterations: FAILURE_THRESHOLD + 3,
          notes: {
            stateAfterOutage: openedState,
            stateWhileInBackoff: stillOpen,
            stateAfterBackoff: recoveredState,
            backoffMs: BACKOFF_MS,
            invariant: 'recovery is automatic — no manual reset required',
          },
        });

        expect(correct).toBe(true);
      } finally {
        vi.useRealTimers();
      }
    });

    it('opens breakers independently per provider', async () => {
      // A shared breaker would take down healthy venues with the failing one.
      const dead = provider('dead', 1, 0.34, 831).setFailure(1, FailureMode.NETWORK_ERROR);
      const healthyA = provider('healthy-a', 2, 0.33, 832);
      const healthyB = provider('healthy-b', 3, 0.33, 833);
      const aggregator = buildStack([dead, healthyA, healthyB]);

      await driveLoad(20, 1, () => aggregator.getPrice(ASSET));

      const metrics = aggregator.getCircuitBreakerMetrics();
      const deadOpen = metrics.find((m) => m.providerName === 'dead')!.state === CircuitState.OPEN;
      const peersClosed = metrics
        .filter((m) => m.providerName !== 'dead')
        .every((m) => m.state === CircuitState.CLOSED);

      recordScenario({
        category: 'circuit-breaker',
        name: 'Breakers are isolated per provider',
        passed: deadOpen && peersClosed,
        iterations: 20,
        notes: {
          deadProviderState: metrics.find((m) => m.providerName === 'dead')!.state,
          peerStates: metrics
            .filter((m) => m.providerName !== 'dead')
            .map((m) => `${m.providerName}=${m.state}`)
            .join(', '),
          invariant: 'one failing venue never trips its peers',
        },
      });

      expect(deadOpen).toBe(true);
      expect(peersClosed).toBe(true);
    });

    it('trips a provider that persistently disagrees with consensus', async () => {
      // Consensus screening records a disagreeing quote as a breaker failure,
      // so a compromised venue is eventually removed from rotation rather than
      // being re-screened on every single round.
      const liar = provider('liar', 1, 0.34, 841).setFailure(1, FailureMode.BYZANTINE);
      const honestA = provider('honest-a', 2, 0.33, 842);
      const honestB = provider('honest-b', 3, 0.33, 843);
      const aggregator = buildStack([liar, honestA, honestB]);

      const load = await driveLoad(20, 1, () => aggregator.getPrice(ASSET));

      const tripped = stateOf(aggregator, 'liar') === CircuitState.OPEN;
      const prices = load.results.filter((r) => r !== null);
      const allCorrect = prices.every((p) => p!.price === scalePrice(PRICE));

      recordScenario({
        category: 'circuit-breaker',
        name: 'Persistently disagreeing provider is taken out of rotation',
        passed: tripped && allCorrect,
        iterations: 20,
        successes: prices.length,
        failures: 20 - prices.length,
        notes: {
          liarState: stateOf(aggregator, 'liar'),
          liarSkew: '+40%',
          invariant: 'consensus disagreement counts toward the breaker',
        },
      });

      expect(tripped).toBe(true);
      expect(allCorrect).toBe(true);
    });
  });
});
