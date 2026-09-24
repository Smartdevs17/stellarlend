/**
 * Oracle Stress Test Harness (#691)
 *
 * Shared machinery for the oracle stress suite: a provider that can be driven
 * into any failure mode we care about, a latency sampler, price shock
 * generators, and the recorder that feeds the stress report.
 *
 * The real providers talk to CoinGecko/Binance over HTTP. Stress testing those
 * would measure the network, not the protocol, so every scenario here drives
 * `StressProvider` instead — deterministic, seeded, and able to reproduce the
 * pathological conditions (total outage, 30s tail latency, 90% flap rate,
 * flash-crash prints) that real feeds only produce during an incident.
 */

import { BasePriceProvider } from '../../src/providers/base-provider.js';
import { createPriceCache, type PriceCache } from '../../src/services/cache.js';
import type { RawPriceData } from '../../src/types/index.js';
import { recordScenario } from './report.js';

// ─── Deterministic RNG ──────────────────────────────────────────────────────

/**
 * mulberry32 — small, fast, seedable PRNG.
 *
 * Stress results have to be reproducible: a flaky stress suite gets muted, and
 * a muted suite protects nothing. Every scenario seeds its own generator.
 */
export function seededRandom(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// ─── Failure modes ──────────────────────────────────────────────────────────

/**
 * How a stressed provider fails. These mirror the failure classes seen in
 * production oracle incidents.
 */
export enum FailureMode {
  /** Provider answers normally. */
  NONE = 'NONE',
  /** Connection-level error — DNS, refused, reset. */
  NETWORK_ERROR = 'NETWORK_ERROR',
  /** Request hangs past the caller's patience. */
  TIMEOUT = 'TIMEOUT',
  /** Upstream returns 5xx. */
  SERVER_ERROR = 'SERVER_ERROR',
  /** Upstream returns 429. */
  RATE_LIMITED = 'RATE_LIMITED',
  /** Answers, but with a structurally invalid payload (NaN / negative / zero). */
  MALFORMED = 'MALFORMED',
  /** Answers with a well-formed but very old timestamp. */
  STALE = 'STALE',
  /** Answers with a plausible but wrong price — the hardest case to detect. */
  BYZANTINE = 'BYZANTINE',
}

export interface LatencyProfile {
  /** Floor for every response, in ms. */
  baseMs: number;
  /** Uniform jitter added on top of `baseMs`. */
  jitterMs: number;
  /** Fraction of calls (0–1) that hit the slow tail. */
  spikeProbability?: number;
  /** Latency applied when a call hits the tail. */
  spikeMs?: number;
}

export interface StressProviderOptions {
  name: string;
  priority: number;
  weight: number;
  /** Fraction of calls (0–1) that fail with `failureMode`. */
  failureRate?: number;
  failureMode?: FailureMode;
  latency?: LatencyProfile;
  seed?: number;
}

/**
 * A price provider that can be pushed into any failure mode on demand.
 *
 * Counters (`callCount`, `failureCount`, `latencies`) are cumulative so a
 * scenario can assert on the whole run, and `reset()` clears them between
 * phases of a multi-stage scenario.
 */
export class StressProvider extends BasePriceProvider {
  private prices = new Map<string, number>();
  private failureRate: number;
  private failureMode: FailureMode;
  private latency: LatencyProfile;
  private rng: () => number;
  private staleOffsetSeconds = 86_400;

  /** Every fetch attempt, including the ones that threw. */
  callCount = 0;
  /** Attempts that threw. */
  failureCount = 0;
  /** Observed latency per attempt, in ms. */
  latencies: number[] = [];

  constructor(options: StressProviderOptions) {
    super({
      name: options.name,
      enabled: true,
      priority: options.priority,
      weight: options.weight,
      baseUrl: 'https://stress.invalid',
      // Deliberately high: the base class rate limiter is not what we're
      // stressing here, and throttling would mask the scenario's own timing.
      rateLimit: { maxRequests: 1_000_000, windowMs: 60_000 },
    });

    this.failureRate = options.failureRate ?? 0;
    this.failureMode = options.failureMode ?? FailureMode.NONE;
    this.latency = options.latency ?? { baseMs: 0, jitterMs: 0 };
    this.rng = seededRandom(options.seed ?? 0x5eed);
  }

  setPrice(asset: string, price: number): this {
    this.prices.set(asset.toUpperCase(), price);
    return this;
  }

  setFailure(rate: number, mode: FailureMode): this {
    this.failureRate = rate;
    this.failureMode = mode;
    return this;
  }

  setLatency(latency: LatencyProfile): this {
    this.latency = latency;
    return this;
  }

  /** How far in the past a `STALE` response is dated. */
  setStaleOffset(seconds: number): this {
    this.staleOffsetSeconds = seconds;
    return this;
  }

  /** Restore healthy behaviour — used to test recovery. */
  recover(): this {
    this.failureRate = 0;
    this.failureMode = FailureMode.NONE;
    return this;
  }

  reset(): this {
    this.callCount = 0;
    this.failureCount = 0;
    this.latencies = [];
    return this;
  }

  /** Mean observed latency in ms, or 0 when nothing was recorded. */
  get meanLatencyMs(): number {
    if (this.latencies.length === 0) return 0;
    return this.latencies.reduce((a, b) => a + b, 0) / this.latencies.length;
  }

  async fetchPrice(asset: string): Promise<RawPriceData> {
    this.callCount++;
    const started = Date.now();

    const delay = this.nextDelayMs();
    if (delay > 0) {
      await new Promise((resolve) => setTimeout(resolve, delay));
    }

    const failing = this.failureMode !== FailureMode.NONE && this.rng() < this.failureRate;

    if (failing && this.throwsOn(this.failureMode)) {
      this.failureCount++;
      this.latencies.push(Date.now() - started);
      throw this.errorFor(this.failureMode);
    }

    const upper = asset.toUpperCase();
    const price = this.prices.get(upper);
    if (price === undefined) {
      this.failureCount++;
      this.latencies.push(Date.now() - started);
      throw new Error(`Asset ${upper} not configured on ${this.name}`);
    }

    this.latencies.push(Date.now() - started);

    // Non-throwing failure modes answer successfully with bad data — the
    // validator, not the transport, has to catch these.
    if (failing) {
      return this.corruptedResponse(upper, price);
    }

    return {
      asset: upper,
      price,
      timestamp: Math.floor(Date.now() / 1000),
      source: this.name,
    };
  }

  // ── private ───────────────────────────────────────────────────────────────

  private nextDelayMs(): number {
    const { baseMs, jitterMs, spikeProbability = 0, spikeMs = 0 } = this.latency;
    if (spikeProbability > 0 && this.rng() < spikeProbability) {
      return spikeMs;
    }
    return baseMs + (jitterMs > 0 ? Math.floor(this.rng() * jitterMs) : 0);
  }

  /** True for modes that surface as a thrown transport error. */
  private throwsOn(mode: FailureMode): boolean {
    return (
      mode === FailureMode.NETWORK_ERROR ||
      mode === FailureMode.TIMEOUT ||
      mode === FailureMode.SERVER_ERROR ||
      mode === FailureMode.RATE_LIMITED
    );
  }

  private errorFor(mode: FailureMode): Error {
    switch (mode) {
      case FailureMode.TIMEOUT:
        return new Error(`${this.name}: request timed out`);
      case FailureMode.SERVER_ERROR:
        return new Error(`${this.name}: upstream returned 503`);
      case FailureMode.RATE_LIMITED:
        return new Error(`${this.name}: upstream returned 429`);
      default:
        return new Error(`${this.name}: ECONNREFUSED`);
    }
  }

  private corruptedResponse(asset: string, price: number): RawPriceData {
    const now = Math.floor(Date.now() / 1000);
    switch (this.failureMode) {
      case FailureMode.MALFORMED:
        return { asset, price: -1, timestamp: now, source: this.name };
      case FailureMode.STALE:
        return {
          asset,
          price,
          timestamp: now - this.staleOffsetSeconds,
          source: this.name,
        };
      case FailureMode.BYZANTINE:
        // Off by 40%: well inside "looks like a number", well outside any
        // sane deviation band.
        return { asset, price: price * 1.4, timestamp: now, source: this.name };
      default:
        return { asset, price, timestamp: now, source: this.name };
    }
  }
}

// ─── Cache control ──────────────────────────────────────────────────────────

/**
 * A `PriceCache` that never serves a hit.
 *
 * `createPriceCache(0)` is not the same thing. A TTL of 0 sets
 * `expiresAt = Date.now()`, and the cache treats an entry as live until
 * `Date.now()` is strictly greater — so an entry written and read inside the
 * same millisecond *is* a hit. Stress scenarios issue their calls back to back
 * and would otherwise be served a frozen price for a run of iterations,
 * silently bypassing the providers they are trying to exercise, with the run
 * length depending on how fast the machine is.
 *
 * A negative TTL backdates `expiresAt`, so every lookup is a miss on every
 * machine. Use this in any scenario whose subject is provider behaviour; use a
 * real TTL only when the cache itself is what is under test.
 */
export function nonCachingPriceCache(): PriceCache {
  return createPriceCache(-1);
}

// ─── Price shock generators ─────────────────────────────────────────────────

export interface ShockOptions {
  /** Price before the shock. */
  startPrice: number;
  /** Total move, as a signed fraction (0.6 = +60%, -0.6 = −60%). */
  magnitude: number;
  /** Number of ticks the move is spread across. 1 = instantaneous. */
  steps: number;
}

/**
 * Generate a price path for a spike or crash.
 *
 * Returns `steps + 1` prices including the starting price, moving
 * geometrically so each tick is an equal percentage move — which is how a
 * cascading liquidation actually prints, rather than a straight line.
 */
export function generateShockPath(options: ShockOptions): number[] {
  const { startPrice, magnitude, steps } = options;
  if (steps < 1) throw new Error('generateShockPath requires steps >= 1');

  const endPrice = startPrice * (1 + magnitude);
  const ratio = (endPrice / startPrice) ** (1 / steps);

  const path: number[] = [startPrice];
  for (let i = 1; i <= steps; i++) {
    path.push(startPrice * ratio ** i);
  }
  return path;
}

/** Percentage move between two prices, always non-negative. */
export function deviationPercent(from: number, to: number): number {
  if (from === 0) return Infinity;
  return Math.abs((to - from) / from) * 100;
}

// ─── Latency statistics ─────────────────────────────────────────────────────

export interface LatencyStats {
  samples: number;
  minMs: number;
  maxMs: number;
  meanMs: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
}

/** Nearest-rank percentile over an unsorted sample set. */
export function percentile(samples: number[], p: number): number {
  if (samples.length === 0) return 0;
  const sorted = [...samples].sort((a, b) => a - b);
  const rank = Math.ceil((p / 100) * sorted.length);
  return sorted[Math.min(Math.max(rank, 1), sorted.length) - 1];
}

export function summarizeLatency(samples: number[]): LatencyStats {
  if (samples.length === 0) {
    return { samples: 0, minMs: 0, maxMs: 0, meanMs: 0, p50Ms: 0, p95Ms: 0, p99Ms: 0 };
  }
  return {
    samples: samples.length,
    minMs: Math.min(...samples),
    maxMs: Math.max(...samples),
    meanMs: samples.reduce((a, b) => a + b, 0) / samples.length,
    p50Ms: percentile(samples, 50),
    p95Ms: percentile(samples, 95),
    p99Ms: percentile(samples, 99),
  };
}

/** Run `fn` and return its result alongside the wall-clock time it took. */
export async function timed<T>(fn: () => Promise<T>): Promise<{ result: T; elapsedMs: number }> {
  const started = Date.now();
  const result = await fn();
  return { result, elapsedMs: Date.now() - started };
}

// ─── Load driver ────────────────────────────────────────────────────────────

export interface LoadResult<T> {
  results: T[];
  successes: number;
  failures: number;
  latencies: number[];
  totalElapsedMs: number;
}

/**
 * Drive `iterations` calls through `fn` at a bounded concurrency.
 *
 * A rejected or null-returning call counts as a failure rather than aborting
 * the run — under stress, partial failure is the expected outcome and the
 * suite needs the distribution, not the first error.
 */
export async function driveLoad<T>(
  iterations: number,
  concurrency: number,
  fn: (index: number) => Promise<T>
): Promise<LoadResult<T>> {
  const results: T[] = [];
  const latencies: number[] = [];
  let successes = 0;
  let failures = 0;
  let next = 0;

  const started = Date.now();

  const worker = async (): Promise<void> => {
    for (;;) {
      const index = next++;
      if (index >= iterations) return;

      const callStarted = Date.now();
      try {
        const result = await fn(index);
        latencies.push(Date.now() - callStarted);
        results.push(result);
        if (result === null || result === undefined) {
          failures++;
        } else {
          successes++;
        }
      } catch {
        latencies.push(Date.now() - callStarted);
        failures++;
      }
    }
  };

  await Promise.all(
    Array.from({ length: Math.min(concurrency, iterations) }, () => worker())
  );

  return {
    results,
    successes,
    failures,
    latencies,
    totalElapsedMs: Date.now() - started,
  };
}

export { recordScenario };
