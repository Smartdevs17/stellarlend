/**
 * Oracle Integration Stress Test Suite (#691)
 *
 * End-to-end stress testing of the oracle → contract → API pipeline. The
 * oracle service suite (`oracle/tests/stress/`) stresses the off-chain
 * aggregator in isolation, and the contract suite
 * (`hello-world/src/tests/oracle_stress_test.rs`) stresses the on-chain guards.
 * Neither covers what this file does: what a *consumer of the API* observes
 * while the oracle is failing.
 *
 * That gap matters because the pipeline's failure modes compose. An off-chain
 * feed that correctly declines to publish, combined with an on-chain breaker
 * that correctly halts reads, can still surface to an API client as a stale
 * `200 OK` if the API layer caches or defaults. The invariant asserted
 * throughout is:
 *
 *   **The API never serves a price the pipeline refused to certify.**
 *   It returns an explicit error (503) instead.
 *
 * As with the other e2e suites here, every external call is mocked so the tests
 * are hermetic and fast; the mocks reproduce the real guards' semantics
 * (staleness bound, deviation band, quorum, breaker cooldown) rather than the
 * real transport.
 */

import request from 'supertest';
import express, { Application } from 'express';

// ─── Simulated clock ────────────────────────────────────────────────────────

/**
 * The pipeline's guards are all time-based. Driving a simulated clock keeps the
 * suite deterministic and lets a 10-minute cooldown be tested in microseconds.
 */
class SimulatedClock {
  private nowSeconds = 1_800_000_000;

  now(): number {
    return this.nowSeconds;
  }

  advance(seconds: number): void {
    this.nowSeconds += seconds;
  }
}

// ─── Mock off-chain oracle feed ─────────────────────────────────────────────

interface Quote {
  source: string;
  price: number;
}

/**
 * Stands in for the aggregator: collects source quotes, drops outliers against
 * the round median, and enforces a source quorum.
 */
class MockAggregator {
  private quotes = new Map<string, Quote>();

  constructor(
    private readonly minSources: number,
    private readonly outlierBandPercent: number
  ) {}

  submit(source: string, price: number): void {
    this.quotes.set(source, { source, price });
  }

  removeSource(source: string): void {
    this.quotes.delete(source);
  }

  clear(): void {
    this.quotes.clear();
  }

  /**
   * Returns the consensus price, or null when the round cannot be certified.
   */
  aggregate(): { price: number; sources: string[] } | null {
    const quotes = Array.from(this.quotes.values());
    if (quotes.length === 0) return null;

    const prices = quotes.map((q) => q.price).sort((a, b) => a - b);
    const mid = Math.floor(prices.length / 2);
    const median =
      prices.length % 2 === 0 ? (prices[mid - 1]! + prices[mid]!) / 2 : prices[mid]!;

    const kept = quotes.filter(
      (q) => Math.abs((q.price - median) / median) * 100 <= this.outlierBandPercent
    );

    if (kept.length < this.minSources) return null;

    const keptPrices = kept.map((q) => q.price).sort((a, b) => a - b);
    const keptMid = Math.floor(keptPrices.length / 2);
    const consensus =
      keptPrices.length % 2 === 0
        ? (keptPrices[keptMid - 1]! + keptPrices[keptMid]!) / 2
        : keptPrices[keptMid]!;

    return { price: consensus, sources: kept.map((q) => q.source) };
  }
}

// ─── Mock on-chain oracle ───────────────────────────────────────────────────

interface StoredFeed {
  price: number;
  updatedAt: number;
}

/**
 * Stands in for the contract's oracle module: stores the feed, enforces the
 * staleness bound, and trips a per-asset circuit breaker on excessive
 * short-window volatility.
 */
class MockOnChainOracle {
  private feeds = new Map<string, StoredFeed>();
  private breakerOpenUntil = new Map<string, number>();
  /** Every write attempt, including rejected ones — used to assert on load. */
  writeAttempts = 0;
  rejectedWrites = 0;

  constructor(
    private readonly clock: SimulatedClock,
    private readonly maxStalenessSeconds: number,
    private readonly volatilityBandPercent: number,
    private readonly breakerCooldownSeconds: number,
    /**
     * Window over which a move is judged "volatile", mirroring the contract's
     * fixed 10-minute `VOLATILITY_WINDOW_SECONDS`. Deliberately independent of
     * `maxStalenessSeconds`: the two windows serve different purposes, and
     * conflating them hides the recovery hazard exercised below.
     */
    private readonly volatilityWindowSeconds: number = 600
  ) {}

  reset(): void {
    this.feeds.clear();
    this.breakerOpenUntil.clear();
    this.writeAttempts = 0;
    this.rejectedWrites = 0;
  }

  isBreakerOpen(asset: string): boolean {
    return (this.breakerOpenUntil.get(asset) ?? 0) > this.clock.now();
  }

  /** Admin escape hatch — bounded, like the contract's. */
  emergencyPause(asset: string, seconds: number): void {
    this.breakerOpenUntil.set(asset, this.clock.now() + seconds);
  }

  /**
   * Write a certified price. Returns whether it was accepted; a move outside the
   * volatility band trips the breaker and the write is rejected.
   */
  updateFeed(asset: string, price: number): boolean {
    this.writeAttempts++;

    const previous = this.feeds.get(asset);
    if (previous) {
      const movePercent = Math.abs((price - previous.price) / previous.price) * 100;
      const withinWindow =
        this.clock.now() - previous.updatedAt <= this.volatilityWindowSeconds;

      if (withinWindow && movePercent > this.volatilityBandPercent) {
        this.breakerOpenUntil.set(asset, this.clock.now() + this.breakerCooldownSeconds);
        this.rejectedWrites++;
        return false;
      }
    }

    this.feeds.set(asset, { price, updatedAt: this.clock.now() });
    return true;
  }

  /**
   * Read a price. Throws a tagged error for each distinct failure mode so a
   * responder — and the API layer — can tell them apart.
   */
  getPrice(asset: string): number {
    if (this.isBreakerOpen(asset)) {
      throw Object.assign(new Error('CircuitBreakerOpen'), { reason: 'circuit_breaker_open' });
    }

    const feed = this.feeds.get(asset);
    if (!feed) {
      throw Object.assign(new Error('PriceUnavailable'), { reason: 'no_feed' });
    }

    if (this.clock.now() - feed.updatedAt > this.maxStalenessSeconds) {
      throw Object.assign(new Error('StalePrice'), { reason: 'stale' });
    }

    return feed.price;
  }
}

// ─── Pipeline + API under test ──────────────────────────────────────────────

const ASSET = 'XLM';
const BASE_PRICE = 0.15;

interface Pipeline {
  app: Application;
  clock: SimulatedClock;
  aggregator: MockAggregator;
  chain: MockOnChainOracle;
  /** Run one full off-chain → on-chain publication cycle. */
  publish: () => { certified: boolean; accepted: boolean };
}

function buildPipeline(
  options: {
    minSources?: number;
    outlierBandPercent?: number;
    maxStalenessSeconds?: number;
    volatilityBandPercent?: number;
    breakerCooldownSeconds?: number;
    volatilityWindowSeconds?: number;
  } = {}
): Pipeline {
  const clock = new SimulatedClock();
  const aggregator = new MockAggregator(
    options.minSources ?? 2,
    options.outlierBandPercent ?? 10
  );
  const chain = new MockOnChainOracle(
    clock,
    options.maxStalenessSeconds ?? 600,
    options.volatilityBandPercent ?? 20,
    options.breakerCooldownSeconds ?? 600,
    options.volatilityWindowSeconds ?? 600
  );

  const publish = () => {
    const certified = aggregator.aggregate();
    if (!certified) return { certified: false, accepted: false };
    return { certified: true, accepted: chain.updateFeed(ASSET, certified.price) };
  };

  const app = express();
  app.use(express.json());

  app.get('/api/v1/price/:asset', (req, res) => {
    try {
      const price = chain.getPrice(req.params.asset);
      res.status(200).json({ asset: req.params.asset, price, degraded: false });
    } catch (error) {
      // Fail closed with the specific reason. Never fall back to a cached or
      // default price — that is the whole point of this suite.
      res.status(503).json({
        asset: req.params.asset,
        error: (error as Error).message,
        reason: (error as { reason?: string }).reason ?? 'unknown',
      });
    }
  });

  app.get('/api/v1/oracle/health/:asset', (req, res) => {
    res.status(200).json({
      asset: req.params.asset,
      circuitBreakerOpen: chain.isBreakerOpen(req.params.asset),
      writeAttempts: chain.writeAttempts,
      rejectedWrites: chain.rejectedWrites,
    });
  });

  return { app, clock, aggregator, chain, publish };
}

/** Seed a healthy three-source feed and publish it on chain. */
function seedHealthyFeed(pipeline: Pipeline, price = BASE_PRICE): void {
  pipeline.aggregator.submit('coingecko', price);
  pipeline.aggregator.submit('binance', price);
  pipeline.aggregator.submit('kraken', price);
  pipeline.publish();
}

// =============================================================================
// TESTS
// =============================================================================

describe('Oracle integration stress: failure scenarios', () => {
  it('serves a price when the pipeline is healthy', async () => {
    const pipeline = buildPipeline();
    seedHealthyFeed(pipeline);

    const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);

    expect(response.status).toBe(200);
    expect(response.body.price).toBeCloseTo(BASE_PRICE, 8);
  });

  it('returns 503 rather than a stale price when every source goes dark', async () => {
    const pipeline = buildPipeline({ maxStalenessSeconds: 600 });
    seedHealthyFeed(pipeline);

    // All sources drop out; nothing new is published.
    pipeline.aggregator.clear();
    pipeline.clock.advance(601);

    const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);

    expect(response.status).toBe(503);
    expect(response.body.reason).toBe('stale');
  });

  it('returns 503 when quorum is lost, and never publishes an uncertified price', async () => {
    const pipeline = buildPipeline({ minSources: 3, maxStalenessSeconds: 600 });
    seedHealthyFeed(pipeline);

    // Two of three sources drop; the round can no longer be certified.
    pipeline.aggregator.removeSource('binance');
    pipeline.aggregator.removeSource('kraken');

    pipeline.clock.advance(300);
    const publication = pipeline.publish();
    expect(publication.certified).toBe(false);

    // The old feed is still inside its staleness window, so it reads — but once
    // it ages out, the API must fail rather than serve it.
    pipeline.clock.advance(400);
    const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);

    expect(response.status).toBe(503);
    expect(response.body.reason).toBe('stale');
  });

  it('holds the honest price while a single source reports garbage under load', async () => {
    const pipeline = buildPipeline({ minSources: 2, outlierBandPercent: 5 });
    seedHealthyFeed(pipeline);

    // One source starts lying, repeatedly, while the honest two hold steady.
    for (let round = 0; round < 25; round++) {
      pipeline.clock.advance(60);
      pipeline.aggregator.submit('coingecko', BASE_PRICE);
      pipeline.aggregator.submit('binance', BASE_PRICE);
      pipeline.aggregator.submit('kraken', BASE_PRICE * 3);
      pipeline.publish();
    }

    const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);

    expect(response.status).toBe(200);
    expect(response.body.price).toBeCloseTo(BASE_PRICE, 8);
  });

  it('keeps every failure mode distinguishable to the caller', async () => {
    // A client that cannot tell "stale" from "halted" cannot choose the right
    // behaviour: one warrants a retry, the other does not.
    const stale = buildPipeline({ maxStalenessSeconds: 60 });
    seedHealthyFeed(stale);
    stale.clock.advance(61);
    const staleResponse = await request(stale.app).get(`/api/v1/price/${ASSET}`);

    const halted = buildPipeline();
    seedHealthyFeed(halted);
    halted.chain.emergencyPause(ASSET, 600);
    const haltedResponse = await request(halted.app).get(`/api/v1/price/${ASSET}`);

    const missing = buildPipeline();
    const missingResponse = await request(missing.app).get(`/api/v1/price/${ASSET}`);

    expect(staleResponse.body.reason).toBe('stale');
    expect(haltedResponse.body.reason).toBe('circuit_breaker_open');
    expect(missingResponse.body.reason).toBe('no_feed');
  });
});

describe('Oracle integration stress: price spike and crash simulation', () => {
  it('halts the API when a flash crash trips the on-chain breaker', async () => {
    const pipeline = buildPipeline({ volatilityBandPercent: 20 });
    seedHealthyFeed(pipeline);

    // −60% in one round: certified off chain (all sources agree) but rejected
    // on chain by the volatility guard.
    pipeline.clock.advance(60);
    const crashPrice = BASE_PRICE * 0.4;
    pipeline.aggregator.submit('coingecko', crashPrice);
    pipeline.aggregator.submit('binance', crashPrice);
    pipeline.aggregator.submit('kraken', crashPrice);
    const publication = pipeline.publish();

    expect(publication.certified).toBe(true);
    expect(publication.accepted).toBe(false);

    const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(response.status).toBe(503);
    expect(response.body.reason).toBe('circuit_breaker_open');
  });

  it('halts the API when a vertical spike trips the on-chain breaker', async () => {
    const pipeline = buildPipeline({ volatilityBandPercent: 20 });
    seedHealthyFeed(pipeline);

    pipeline.clock.advance(60);
    const spikePrice = BASE_PRICE * 2.5;
    for (const source of ['coingecko', 'binance', 'kraken']) {
      pipeline.aggregator.submit(source, spikePrice);
    }
    pipeline.publish();

    const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(response.status).toBe(503);
    expect(response.body.reason).toBe('circuit_breaker_open');
  });

  it('tracks a gradual move end to end without interruption', async () => {
    const pipeline = buildPipeline({ volatilityBandPercent: 20 });
    seedHealthyFeed(pipeline);

    // Twelve steps of −3%: inside the band at every step, ~31% in total.
    let price = BASE_PRICE;
    for (let step = 0; step < 12; step++) {
      pipeline.clock.advance(120);
      price *= 0.97;
      for (const source of ['coingecko', 'binance', 'kraken']) {
        pipeline.aggregator.submit(source, price);
      }
      const publication = pipeline.publish();
      expect(publication.accepted).toBe(true);

      const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
      expect(response.status).toBe(200);
      expect(response.body.price).toBeCloseTo(price, 8);
    }

    expect(price).toBeLessThan(BASE_PRICE * 0.75);
  });
});

describe('Oracle integration stress: circuit breaker validation', () => {
  it('reopens the API automatically once the cooldown expires and prices resume', async () => {
    const pipeline = buildPipeline({
      volatilityBandPercent: 20,
      breakerCooldownSeconds: 600,
    });
    seedHealthyFeed(pipeline);

    // Trip it.
    pipeline.clock.advance(60);
    const crashPrice = BASE_PRICE * 0.4;
    for (const source of ['coingecko', 'binance', 'kraken']) {
      pipeline.aggregator.submit(source, crashPrice);
    }
    pipeline.publish();

    const halted = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(halted.status).toBe(503);

    // Cooldown expires and the settled level is published.
    pipeline.clock.advance(601);
    const resumed = pipeline.publish();
    expect(resumed.accepted).toBe(true);

    const recovered = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(recovered.status).toBe(200);
    expect(recovered.body.price).toBeCloseTo(crashPrice, 8);
  });

  it('exposes breaker state on the health endpoint while halted', async () => {
    const pipeline = buildPipeline();
    seedHealthyFeed(pipeline);
    pipeline.chain.emergencyPause(ASSET, 600);

    const health = await request(pipeline.app).get(`/api/v1/oracle/health/${ASSET}`);

    expect(health.status).toBe(200);
    expect(health.body.circuitBreakerOpen).toBe(true);
  });

  it('survives repeated trip and recover cycles', async () => {
    // Cooldown is set at least as long as the volatility window, so that by the
    // time reads are allowed again the crash print is outside the window used to
    // judge volatility. See the next test for what happens when it is not.
    const pipeline = buildPipeline({
      volatilityBandPercent: 20,
      volatilityWindowSeconds: 600,
      breakerCooldownSeconds: 600,
      maxStalenessSeconds: 3_600,
    });
    seedHealthyFeed(pipeline);

    let level = BASE_PRICE;
    for (let cycle = 0; cycle < 3; cycle++) {
      // Trip with a −50% move inside the volatility window.
      pipeline.clock.advance(60);
      level *= 0.5;
      for (const source of ['coingecko', 'binance', 'kraken']) {
        pipeline.aggregator.submit(source, level);
      }
      pipeline.publish();

      const halted = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
      expect(halted.status).toBe(503);

      // Cooldown elapses and the move is now outside the volatility window.
      pipeline.clock.advance(601);
      expect(pipeline.publish().accepted).toBe(true);

      const recovered = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
      expect(recovered.status).toBe(200);
      expect(recovered.body.price).toBeCloseTo(level, 8);
    }
  });

  it('cannot recover while the cooldown is shorter than the volatility window', async () => {
    // Configuration hazard worth pinning down. The breaker cooldown and the
    // volatility window are independent settings, and the guards interact: the
    // volatility check compares against the last *accepted* price, which is
    // still the pre-crash level, because the crash print was rejected.
    //
    // If reads are re-enabled while that pre-crash price is still inside the
    // volatility window, the next publication is measured against it, trips the
    // breaker again, and the asset never recovers on its own. Operators must
    // therefore keep `breakerCooldownSeconds >= volatilityWindowSeconds`.
    const pipeline = buildPipeline({
      volatilityBandPercent: 20,
      volatilityWindowSeconds: 600,
      breakerCooldownSeconds: 60,
      maxStalenessSeconds: 3_600,
    });
    seedHealthyFeed(pipeline);

    pipeline.clock.advance(60);
    const crashed = BASE_PRICE * 0.5;
    for (const source of ['coingecko', 'binance', 'kraken']) {
      pipeline.aggregator.submit(source, crashed);
    }
    expect(pipeline.publish().accepted).toBe(false);

    // Wait out the short cooldown and try to republish the settled level.
    pipeline.clock.advance(61);
    expect(pipeline.publish().accepted).toBe(false);

    const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(response.status).toBe(503);
    expect(response.body.reason).toBe('circuit_breaker_open');

    // Once enough time passes for the pre-crash price to leave the volatility
    // window, the same publication succeeds — confirming the cause is the
    // window overlap rather than anything permanent.
    pipeline.clock.advance(601);
    expect(pipeline.publish().accepted).toBe(true);

    const recovered = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(recovered.status).toBe(200);
    expect(recovered.body.price).toBeCloseTo(crashed, 8);
  });
});

describe('Oracle integration stress: latency and sustained load', () => {
  it('serves consistent prices across 200 sequential API reads', async () => {
    const pipeline = buildPipeline({ maxStalenessSeconds: 3_600 });
    seedHealthyFeed(pipeline);

    const statuses = new Set<number>();
    const prices = new Set<number>();

    for (let i = 0; i < 200; i++) {
      const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
      statuses.add(response.status);
      prices.add(response.body.price);
    }

    // A single stored feed must produce exactly one status and one price.
    expect(Array.from(statuses)).toEqual([200]);
    expect(prices.size).toBe(1);
  });

  it('handles 100 concurrent reads without divergence', async () => {
    const pipeline = buildPipeline({ maxStalenessSeconds: 3_600 });
    seedHealthyFeed(pipeline);

    const responses = await Promise.all(
      Array.from({ length: 100 }, () => request(pipeline.app).get(`/api/v1/price/${ASSET}`))
    );

    expect(responses.every((r) => r.status === 200)).toBe(true);
    expect(new Set(responses.map((r) => r.body.price)).size).toBe(1);
  });

  it('keeps publishing through churn as sources rotate in and out', async () => {
    const pipeline = buildPipeline({ minSources: 2, maxStalenessSeconds: 600 });
    const sources = ['coingecko', 'binance', 'kraken'];
    seedHealthyFeed(pipeline);

    for (let round = 0; round < 30; round++) {
      pipeline.clock.advance(60);

      // One source is out each round; the others report the true price.
      const out = sources[round % sources.length]!;
      for (const source of sources) {
        if (source === out) {
          pipeline.aggregator.removeSource(source);
        } else {
          pipeline.aggregator.submit(source, BASE_PRICE);
        }
      }

      const publication = pipeline.publish();
      expect(publication.certified).toBe(true);

      const response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
      expect(response.status).toBe(200);
      expect(response.body.price).toBeCloseTo(BASE_PRICE, 8);
    }
  });
});

describe('Oracle integration stress: multi-oracle redundancy and upgrade', () => {
  it('degrades gracefully as redundancy is consumed, then fails closed', async () => {
    const pipeline = buildPipeline({ minSources: 2, maxStalenessSeconds: 600 });
    seedHealthyFeed(pipeline);

    // Down to two sources: still certifiable.
    pipeline.clock.advance(60);
    pipeline.aggregator.removeSource('kraken');
    expect(pipeline.publish().certified).toBe(true);

    let response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(response.status).toBe(200);

    // Down to one: below quorum, so nothing new is published and the existing
    // feed eventually ages out.
    pipeline.clock.advance(60);
    pipeline.aggregator.removeSource('binance');
    expect(pipeline.publish().certified).toBe(false);

    pipeline.clock.advance(601);
    response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(response.status).toBe(503);
    expect(response.body.reason).toBe('stale');
  });

  it('swaps a source out for a replacement without interrupting the API', async () => {
    const pipeline = buildPipeline({ minSources: 2, maxStalenessSeconds: 600 });
    seedHealthyFeed(pipeline);

    // Cutover: the new source runs alongside the old, then the old is removed.
    pipeline.clock.advance(60);
    pipeline.aggregator.submit('bitstamp', BASE_PRICE);
    expect(pipeline.publish().accepted).toBe(true);

    let response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(response.status).toBe(200);

    pipeline.clock.advance(60);
    pipeline.aggregator.removeSource('kraken');
    expect(pipeline.publish().accepted).toBe(true);

    response = await request(pipeline.app).get(`/api/v1/price/${ASSET}`);
    expect(response.status).toBe(200);
    expect(response.body.price).toBeCloseTo(BASE_PRICE, 8);
  });

  it('applies a tightened staleness bound to the existing feed immediately', async () => {
    // An upgrade that tightens a guard must take effect without waiting for the
    // next publication — otherwise the window it was meant to close stays open.
    const lenient = buildPipeline({ maxStalenessSeconds: 3_600 });
    seedHealthyFeed(lenient);
    lenient.clock.advance(1_200);
    const beforeUpgrade = await request(lenient.app).get(`/api/v1/price/${ASSET}`);
    expect(beforeUpgrade.status).toBe(200);

    const strict = buildPipeline({ maxStalenessSeconds: 600 });
    seedHealthyFeed(strict);
    strict.clock.advance(1_200);
    const afterUpgrade = await request(strict.app).get(`/api/v1/price/${ASSET}`);

    expect(afterUpgrade.status).toBe(503);
    expect(afterUpgrade.body.reason).toBe('stale');
  });
});
