/**
 * Tests for CircuitBreaker service
 *
 * Covers all state transitions and acceptance criteria from the issue:
 *   - Opens after N consecutive failures
 *   - Skips requests while OPEN (backoff period)
 *   - Transitions to HALF_OPEN after backoff expires
 *   - Closes on success from HALF_OPEN
 *   - Re-opens on failure from HALF_OPEN
 *   - Metrics are updated on every transition
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import {
  CircuitBreaker,
  CircuitState,
  createCircuitBreaker,
} from '../src/services/circuit-breaker.js';

describe('CircuitBreaker', () => {
  let cb: CircuitBreaker;

  beforeEach(() => {
    vi.useFakeTimers();
    cb = createCircuitBreaker({
      providerName: 'test-provider',
      failureThreshold: 3,
      backoffMs: 10_000,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  // ── Initial state ──────────────────────────────────────────────────────

  it('starts in CLOSED state', () => {
    expect(cb.currentState).toBe(CircuitState.CLOSED);
  });

  it('allows requests when CLOSED', () => {
    expect(cb.isAllowed()).toBe(true);
  });

  // ── CLOSED → OPEN ──────────────────────────────────────────────────────

  it('opens after reaching the failure threshold', () => {
    cb.recordFailure();
    cb.recordFailure();
    expect(cb.currentState).toBe(CircuitState.CLOSED); // not yet

    cb.recordFailure(); // threshold = 3
    expect(cb.currentState).toBe(CircuitState.OPEN);
  });

  it('does not open before the failure threshold', () => {
    cb.recordFailure();
    cb.recordFailure();
    expect(cb.currentState).toBe(CircuitState.CLOSED);
  });

  it('blocks requests when OPEN', () => {
    cb.recordFailure();
    cb.recordFailure();
    cb.recordFailure();

    expect(cb.isAllowed()).toBe(false);
  });

  // ── OPEN → HALF_OPEN ───────────────────────────────────────────────────

  it('transitions to HALF_OPEN after backoff period', () => {
    cb.recordFailure();
    cb.recordFailure();
    cb.recordFailure();
    expect(cb.currentState).toBe(CircuitState.OPEN);

    vi.advanceTimersByTime(10_000);

    expect(cb.isAllowed()).toBe(true);
    expect(cb.currentState).toBe(CircuitState.HALF_OPEN);
  });

  it('stays OPEN before backoff period expires', () => {
    cb.recordFailure();
    cb.recordFailure();
    cb.recordFailure();

    vi.advanceTimersByTime(9_999);

    expect(cb.isAllowed()).toBe(false);
    expect(cb.currentState).toBe(CircuitState.OPEN);
  });

  // ── HALF_OPEN → CLOSED (recovery) ─────────────────────────────────────

  it('closes on success from HALF_OPEN', () => {
    cb.recordFailure();
    cb.recordFailure();
    cb.recordFailure();
    vi.advanceTimersByTime(10_000);
    cb.isAllowed(); // triggers HALF_OPEN transition

    cb.recordSuccess();
    expect(cb.currentState).toBe(CircuitState.CLOSED);
  });

  it('resets consecutive failures on success', () => {
    cb.recordFailure();
    cb.recordFailure();
    cb.recordFailure();
    vi.advanceTimersByTime(10_000);
    cb.isAllowed();

    cb.recordSuccess();

    const metrics = cb.getMetrics();
    expect(metrics.consecutiveFailures).toBe(0);
  });

  // ── HALF_OPEN → OPEN (probe fails) ────────────────────────────────────

  it('re-opens when probe request fails in HALF_OPEN', () => {
    cb.recordFailure();
    cb.recordFailure();
    cb.recordFailure();
    vi.advanceTimersByTime(10_000);
    cb.isAllowed(); // HALF_OPEN

    cb.recordFailure(); // probe fails
    expect(cb.currentState).toBe(CircuitState.OPEN);
  });

  // ── Success in CLOSED resets counter ──────────────────────────────────

  it('resets consecutive failure count on success while CLOSED', () => {
    cb.recordFailure();
    cb.recordFailure();
    cb.recordSuccess();
    cb.recordFailure();
    cb.recordFailure();

    // Only 2 consecutive failures after the success – should still be CLOSED
    expect(cb.currentState).toBe(CircuitState.CLOSED);
  });

  // ── Metrics ───────────────────────────────────────────────────────────

  it('tracks total failures and successes', () => {
    cb.recordFailure();
    cb.recordFailure();
    cb.recordSuccess();
    cb.recordFailure();

    const m = cb.getMetrics();
    expect(m.totalFailures).toBe(3);
    expect(m.totalSuccesses).toBe(1);
  });

  it('records lastFailureTime on failure', () => {
    vi.setSystemTime(new Date('2024-01-01T00:00:00Z'));
    cb.recordFailure();

    expect(cb.getMetrics().lastFailureTime).toBe(new Date('2024-01-01T00:00:00Z').getTime());
  });

  it('includes providerName in metrics', () => {
    expect(cb.getMetrics().providerName).toBe('test-provider');
  });

  it('updates lastStateChangeTime on transition', () => {
    const before = cb.getMetrics().lastStateChangeTime;

    vi.advanceTimersByTime(1_000);
    cb.recordFailure();
    cb.recordFailure();
    cb.recordFailure();

    expect(cb.getMetrics().lastStateChangeTime).toBeGreaterThan(before);
  });

  // ── Factory ───────────────────────────────────────────────────────────

  it('createCircuitBreaker returns a CircuitBreaker instance', () => {
    const instance = createCircuitBreaker({ providerName: 'p' });
    expect(instance).toBeInstanceOf(CircuitBreaker);
  });

  it('uses default threshold of 3 when not specified', () => {
    const defaultCb = createCircuitBreaker({ providerName: 'p' });
    defaultCb.recordFailure();
    defaultCb.recordFailure();
    expect(defaultCb.currentState).toBe(CircuitState.CLOSED);
    defaultCb.recordFailure();
    expect(defaultCb.currentState).toBe(CircuitState.OPEN);
  });
});
