/**
 * E2E: complete user journeys with gas, timing, failure recovery and
 * optimization analysis — Issue #693
 *
 * Builds on the in-memory harness (`harness.ts`) and profiles every step with
 * `JourneyProfiler`: gas is priced from measured Soroban costs, latency is
 * recorded per step, and a JSON report (tests/e2e/reports/) is written for CI.
 * Contract-level gas for the same journey is measured for real in
 * `stellar-lend/contracts/lending/tests/user_journeys.rs`.
 */

import request from 'supertest';
import { Application } from 'express';
import { assignRole, buildLendingApp, reset, setPrice } from './harness';
import {
  JourneyProfiler,
  JourneySummary,
  loadGasModel,
  percentile,
  timingAnalysis,
  writeJourneyReport,
} from './journey-profiler';

const gas = loadGasModel();
const user = (n: number | string) => `GJOURNEY${String(n).padStart(48, '0')}`;
const collected: JourneySummary[] = [];

/** deposit → borrow → check → repay → withdraw, asserting each step. */
async function completeJourney(app: Application, name: string, who: string, scale = 1) {
  const p = new JourneyProfiler(app, name, who, gas);
  const collateral = 100_000 * scale;
  const debt = 5_000 * scale;
  expect((await p.deposit('XLM', collateral)).status).toBe(200);
  expect((await p.borrow('USDC', debt)).status).toBe(200);
  const pos = await p.position();
  expect(pos.body.healthFactor).toBeGreaterThan(1);
  expect((await p.repay('USDC', debt)).status).toBe(200);
  const done = await p.withdraw('XLM', collateral);
  expect(done.status).toBe(200);
  expect(done.body.position.debtValueUsd).toBe(0);
  expect(done.body.position.collateralValueUsd).toBe(0);
  return p;
}

describe('E2E: complete journey analysis', () => {
  let app: Application;

  beforeEach(() => {
    reset();
    app = buildLendingApp();
    setPrice('XLM', 0.12);
    setPrice('USDC', 1.0);
  });

  afterAll(() => {
    const report = writeJourneyReport('api-journeys.json', collected);
    // Surface the headline numbers in CI logs.
    console.log(
      `journey report: ${report.journeys.length} journeys, p95 ${report.timing.journeyP95Ms}ms`
    );
  });

  describe('complete journey', () => {
    it('charges measured gas for each write, nothing for reads, within budgets', async () => {
      const p = await completeJourney(app, 'single_user_complete', user(1));
      const s = p.summary();
      collected.push(s);

      expect(s.steps.map((x) => x.op)).toEqual(['deposit', 'borrow', 'position', 'repay', 'withdraw']);
      expect(s.failedAttempts).toBe(0);
      const expectedGas =
        gas.cost('deposit', false) + gas.cost('borrow', false) + gas.cost('repay', false) + gas.cost('withdraw', false);
      expect(s.totalGas).toBe(expectedGas);
      expect(s.steps.find((x) => x.op === 'position')!.gas).toBe(0);
      for (const step of s.steps.filter((x) => x.gas > 0)) {
        expect(step.gas).toBeLessThanOrEqual(step.budget);
      }
    });

    it('prices repeat deposits at the warm-storage cost', async () => {
      const p = new JourneyProfiler(app, 'split_deposit', user(2), gas);
      await p.deposit('XLM', 50_000);
      await p.deposit('XLM', 50_000);
      const [first, second] = p.summary().steps;
      expect(first!.gas).toBe(gas.cost('deposit', false));
      expect(second!.gas).toBe(gas.cost('deposit', true));
    });
  });

  describe('multi-user scenarios', () => {
    it('runs 10 concurrent journeys with isolated state and identical gas per user', async () => {
      const profilers = await Promise.all(
        Array.from({ length: 10 }, (_, i) => completeJourney(app, `concurrent_${i}`, user(100 + i)))
      );
      const summaries = profilers.map((p) => p.summary());
      collected.push(...summaries);

      expect(new Set(summaries.map((s) => s.totalGas)).size).toBe(1);
      expect(summaries.every((s) => s.failedAttempts === 0)).toBe(true);
    });

    it('liquidates one borrower without affecting another user mid-journey', async () => {
      const liquidator = user('LIQ');
      assignRole(liquidator, 'liquidator');
      const risky = new JourneyProfiler(app, 'risky_borrower', user(200), gas);
      const safe = new JourneyProfiler(app, 'safe_borrower', user(201), gas);

      await risky.deposit('XLM', 100_000); // $12,000
      await risky.borrow('USDC', 8_500); // close to the 75% LTV cap
      await safe.deposit('XLM', 100_000);
      await safe.borrow('USDC', 1_000);

      setPrice('XLM', 0.09); // risky HF = 9000*0.85/8500 < 1; safe stays healthy
      const liq = new JourneyProfiler(app, 'liquidator', liquidator, gas);
      const res = await liq.liquidate(risky.user, 'USDC', 'XLM', 4_000);
      expect(res.status).toBe(200);

      // The safe user's journey completes untouched.
      await safe.repay('USDC', 1_000);
      expect((await safe.withdraw('XLM', 100_000)).status).toBe(200);
      collected.push(risky.summary(), safe.summary(), liq.summary());
      expect(liq.summary().totalGas).toBe(gas.cost('liquidate', false));
    });
  });

  describe('failure recovery', () => {
    it('recovers from a protocol pause mid-journey', async () => {
      const admin = user('ADMIN');
      assignRole(admin, 'admin');
      const p = new JourneyProfiler(app, 'pause_recovery', user(300), gas);

      await p.deposit('XLM', 100_000);
      await p.borrow('USDC', 5_000);
      await request(app).post('/api/protocol/pause').send({ callerAddress: admin });

      expect((await p.repay('USDC', 5_000)).status).toBe(503);
      const during = await p.position();
      expect(during.body.debt.USDC).toBe(5_000); // nothing half-applied

      await request(app).post('/api/protocol/resume').send({ callerAddress: admin });
      expect((await p.repay('USDC', 5_000)).status).toBe(200);
      expect((await p.withdraw('XLM', 100_000)).status).toBe(200);

      const s = p.summary();
      collected.push(s);
      expect(s.failedAttempts).toBe(1);
      expect(s.steps.filter((x) => !x.ok).every((x) => x.gas === 0)).toBe(true);
    });

    it('recovers from a rejected borrow by adding collateral and retrying', async () => {
      const p = new JourneyProfiler(app, 'borrow_retry', user(301), gas);
      await p.deposit('XLM', 10_000); // $1,200 → max borrow $900
      expect((await p.borrow('USDC', 1_000)).status).toBe(400);
      await p.deposit('XLM', 10_000);
      expect((await p.borrow('USDC', 1_000)).status).toBe(200);
      await p.repay('USDC', 1_000);
      expect((await p.withdraw('XLM', 20_000)).status).toBe(200);
      collected.push(p.summary());
    });

    it('recovers from a rejected withdrawal by repaying first', async () => {
      const p = new JourneyProfiler(app, 'withdraw_retry', user(302), gas);
      await p.deposit('XLM', 100_000);
      await p.borrow('USDC', 5_000);
      expect((await p.withdraw('XLM', 99_000)).status).toBe(400);
      await p.repay('USDC', 5_000);
      expect((await p.withdraw('XLM', 100_000)).status).toBe(200);
      collected.push(p.summary());
    });

    it('recovers when an oracle price drop makes the position liquidatable', async () => {
      const p = new JourneyProfiler(app, 'price_shock_recovery', user(303), gas);
      await p.deposit('XLM', 100_000);
      await p.borrow('USDC', 8_000);
      setPrice('XLM', 0.09);
      expect((await p.position()).body.liquidatable).toBe(true);

      // User tops up collateral to restore health, then exits normally.
      await p.deposit('XLM', 50_000);
      expect((await p.position()).body.liquidatable).toBe(false);
      await p.repay('USDC', 8_000);
      expect((await p.withdraw('XLM', 150_000)).status).toBe(200);
      collected.push(p.summary());
    });
  });

  describe('optimization recommendations', () => {
    it('flags split deposits, redundant reads, partial repays and rejected calls', async () => {
      const p = new JourneyProfiler(app, 'inefficient', user(400), gas);
      await p.deposit('XLM', 50_000);
      await p.deposit('XLM', 50_000);
      await p.position();
      await p.borrow('USDC', 100_000); // rejected
      await p.borrow('USDC', 5_000);
      await p.position();
      await p.repay('USDC', 2_500);
      await p.repay('USDC', 2_500);
      await p.withdraw('XLM', 100_000);

      const recs = p.summary().recommendations.join('\n');
      expect(recs).toMatch(/Combine 2 deposits/);
      expect(recs).toMatch(/Position was read 2 times/);
      expect(recs).toMatch(/rejected call/);
      expect(recs).toMatch(/repaid in 2 transactions/);
      collected.push(p.summary());
    });

    it('has no recommendations for an optimal journey', async () => {
      const p = await completeJourney(app, 'optimal', user(401));
      expect(p.summary().recommendations).toEqual([]);
    });
  });

  describe('performance benchmarks', () => {
    const RUNS = Number(process.env.JOURNEY_BENCH_RUNS ?? 50);
    // Generous ceiling for the in-memory API path; catches pathological slowdowns.
    const P95_BUDGET_MS = Number(process.env.JOURNEY_P95_BUDGET_MS ?? 250);

    it(`completes ${RUNS} sequential journeys with p95 under ${P95_BUDGET_MS}ms`, async () => {
      const runs: JourneySummary[] = [];
      const started = Date.now();
      for (let i = 0; i < RUNS; i += 1) {
        runs.push((await completeJourney(app, `bench_${i}`, user(1000 + i))).summary());
      }
      const elapsed = Date.now() - started;
      const timing = timingAnalysis(runs);

      expect(timing.journeyP95Ms).toBeLessThan(P95_BUDGET_MS);
      expect(percentile(runs.map((r) => r.totalGas), 0.5)).toBe(runs[0]!.totalGas);

      writeJourneyReport('api-journey-benchmark.json', [], {
        benchmark: {
          runs: RUNS,
          elapsedMs: elapsed,
          journeysPerSecond: +(RUNS / (elapsed / 1000)).toFixed(1),
          p95BudgetMs: P95_BUDGET_MS,
          timing,
          gasPerJourney: runs[0]!.totalGas,
        },
      });
    });
  });
});
