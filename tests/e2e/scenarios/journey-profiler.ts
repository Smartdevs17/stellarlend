/**
 * Journey profiler — Issue #693
 *
 * Wraps the harness's HTTP steps to record, per user journey:
 *   - wall-clock latency of every step (timing analysis),
 *   - on-chain gas for every state-changing step, priced from the *measured*
 *     Soroban costs in `stellar-lend/benchmarks/gas-baseline.json` and checked
 *     against the budgets in `stellar-lend/benchmarks/baseline.json`,
 *   - failed attempts and retries (failure recovery cost),
 * and derives rule-based optimization recommendations. `writeJourneyReport`
 * emits JSON for CI artifacts alongside the contract-level report from
 * `stellar-lend/contracts/lending/tests/user_journeys.rs`.
 */

import fs from 'fs';
import path from 'path';
import request from 'supertest';
import { Application } from 'express';

const BENCHMARKS = path.resolve(__dirname, '../../../stellar-lend/benchmarks');

export type JourneyOp = 'deposit' | 'borrow' | 'repay' | 'withdraw' | 'position' | 'liquidate';

/** API operation → the contract entry point it submits (hello-world). */
const CONTRACT_FN: Record<JourneyOp, { fn: string; scenario: string[] }> = {
  deposit: { fn: 'deposit_collateral', scenario: ['write_cold', 'write_warm'] },
  borrow: { fn: 'borrow_asset', scenario: ['write'] },
  repay: { fn: 'repay_debt', scenario: ['write'] },
  withdraw: { fn: 'withdraw_collateral', scenario: ['write'] },
  // Reads go through RPC simulation; tracked for recommendations, not fees.
  position: { fn: 'get_user_position', scenario: ['read_cold', 'read_warm'] },
  liquidate: { fn: 'liquidate', scenario: ['write'] },
};

export interface GasModel {
  cost(op: JourneyOp, warm: boolean): number;
  budget(op: JourneyOp): number;
}

export function loadGasModel(): GasModel {
  const measured = JSON.parse(fs.readFileSync(path.join(BENCHMARKS, 'gas-baseline.json'), 'utf8')) as {
    benchmarks: Array<{ operation: string; scenario: string; cpu_insns: number }>;
  };
  const budgets = (
    JSON.parse(fs.readFileSync(path.join(BENCHMARKS, 'baseline.json'), 'utf8')) as {
      gas_budgets: Record<string, number>;
    }
  ).gas_budgets;

  const lookup = (op: JourneyOp, warm: boolean): number => {
    const { fn, scenario } = CONTRACT_FN[op];
    const wanted = scenario.length > 1 ? scenario[warm ? 1 : 0] : scenario[0];
    const hit =
      measured.benchmarks.find((b) => b.operation === fn && b.scenario === wanted) ??
      measured.benchmarks.find((b) => b.operation === fn);
    if (!hit) throw new Error(`no measured gas for ${fn}`);
    return hit.cpu_insns;
  };

  return {
    cost: lookup,
    budget: (op) => budgets[`hello_world::${CONTRACT_FN[op].fn}`] ?? 0,
  };
}

export interface StepRecord {
  op: JourneyOp;
  status: number;
  ok: boolean;
  latencyMs: number;
  /** CPU instructions charged on-chain (0 for reads and rejected calls). */
  gas: number;
  budget: number;
}

export interface JourneySummary {
  name: string;
  user: string;
  steps: StepRecord[];
  totalGas: number;
  totalLatencyMs: number;
  failedAttempts: number;
  recommendations: string[];
}

export class JourneyProfiler {
  readonly steps: StepRecord[] = [];
  private readonly touched = new Set<string>();

  constructor(
    private readonly app: Application,
    readonly name: string,
    readonly user: string,
    private readonly gas: GasModel = loadGasModel()
  ) {}

  private async run(op: JourneyOp, send: () => request.Test): Promise<request.Response> {
    const started = process.hrtime.bigint();
    const res = await send();
    const latencyMs = Number(process.hrtime.bigint() - started) / 1e6;
    const ok = res.status >= 200 && res.status < 300;
    const isRead = op === 'position';
    // Warm = this user already touched this op's storage in the journey.
    const warm = this.touched.has(op);
    if (ok) this.touched.add(op);
    this.steps.push({
      op,
      status: res.status,
      ok,
      latencyMs,
      gas: ok && !isRead ? this.gas.cost(op, warm) : 0,
      budget: this.gas.budget(op),
    });
    return res;
  }

  deposit(asset: string, amount: number) {
    return this.run('deposit', () =>
      request(this.app).post('/api/lending/deposit').send({ userAddress: this.user, asset, amount })
    );
  }

  borrow(asset: string, amount: number) {
    return this.run('borrow', () =>
      request(this.app).post('/api/lending/borrow').send({ userAddress: this.user, asset, amount })
    );
  }

  repay(asset: string, amount: number) {
    return this.run('repay', () =>
      request(this.app).post('/api/lending/repay').send({ userAddress: this.user, asset, amount })
    );
  }

  withdraw(asset: string, amount: number) {
    return this.run('withdraw', () =>
      request(this.app).post('/api/lending/withdraw').send({ userAddress: this.user, asset, amount })
    );
  }

  position() {
    return this.run('position', () => request(this.app).get(`/api/positions/${this.user}`));
  }

  /** Acts as liquidator (`this.user` must hold the liquidator role). */
  liquidate(targetUser: string, debtAsset: string, collateralAsset: string, repayAmount: number) {
    return this.run('liquidate', () =>
      request(this.app)
        .post('/api/liquidations/liquidate')
        .send({ callerAddress: this.user, targetUser, debtAsset, collateralAsset, repayAmount })
    );
  }

  summary(): JourneySummary {
    return {
      name: this.name,
      user: this.user,
      steps: [...this.steps],
      totalGas: this.steps.reduce((s, x) => s + x.gas, 0),
      totalLatencyMs: this.steps.reduce((s, x) => s + x.latencyMs, 0),
      failedAttempts: this.steps.filter((x) => !x.ok).length,
      recommendations: recommend(this.steps, this.gas),
    };
  }
}

/** Rule-based recommendations for a single journey. */
export function recommend(steps: StepRecord[], gas: GasModel): string[] {
  const out: string[] = [];
  const count = (op: JourneyOp, ok = true) => steps.filter((s) => s.op === op && s.ok === ok).length;

  const deposits = count('deposit');
  if (deposits > 1) {
    const saved = (deposits - 1) * gas.cost('deposit', true);
    out.push(
      `Combine ${deposits} deposits into one: saves ~${saved.toLocaleString('en-US')} CPU instructions (${deposits - 1} extra transaction fees).`
    );
  }

  const reads = count('position');
  if (reads > 1) {
    out.push(
      `Position was read ${reads} times; read it once after the last write and reuse it — every write response already returns the updated position.`
    );
  }

  const failed = steps.filter((s) => !s.ok);
  if (failed.length) {
    const ops = [...new Set(failed.map((s) => s.op))].join(', ');
    out.push(
      `${failed.length} rejected call(s) (${ops}); simulate before submitting so users never pay for or wait on a transaction that will fail.`
    );
  }

  const repays = count('repay');
  if (repays > 1) {
    out.push(`Debt repaid in ${repays} transactions; a single full repay saves ${(repays - 1) * gas.cost('repay', true)} instructions.`);
  }

  for (const s of steps) {
    if (s.gas > 0 && s.budget > 0 && s.gas > s.budget * 0.8) {
      out.push(`${s.op} used ${Math.round((s.gas / s.budget) * 100)}% of its gas budget.`);
    }
  }
  return [...new Set(out)];
}

export function percentile(values: number[], p: number): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))]!;
}

/** Per-op latency distribution across journeys (timing analysis). */
export function timingAnalysis(journeys: JourneySummary[]) {
  const byOp = new Map<JourneyOp, number[]>();
  for (const j of journeys) {
    for (const s of j.steps) byOp.set(s.op, [...(byOp.get(s.op) ?? []), s.latencyMs]);
  }
  const perOp = Object.fromEntries(
    [...byOp].map(([op, xs]) => [
      op,
      {
        samples: xs.length,
        p50Ms: +percentile(xs, 0.5).toFixed(3),
        p95Ms: +percentile(xs, 0.95).toFixed(3),
        maxMs: +Math.max(...xs).toFixed(3),
      },
    ])
  );
  const totals = journeys.map((j) => j.totalLatencyMs);
  return {
    journeys: journeys.length,
    journeyP50Ms: +percentile(totals, 0.5).toFixed(3),
    journeyP95Ms: +percentile(totals, 0.95).toFixed(3),
    perOp,
  };
}

export function writeJourneyReport(file: string, journeys: JourneySummary[], extra: Record<string, unknown> = {}) {
  const dir = process.env.JOURNEY_REPORT_DIR ?? path.resolve(__dirname, '../reports');
  fs.mkdirSync(dir, { recursive: true });
  const report = {
    generatedAt: new Date().toISOString(),
    gasSource: 'stellar-lend/benchmarks/gas-baseline.json (measured Soroban CPU instructions)',
    timing: timingAnalysis(journeys),
    journeys: journeys.map((j) => ({
      ...j,
      totalLatencyMs: +j.totalLatencyMs.toFixed(3),
      steps: j.steps.map((s) => ({ ...s, latencyMs: +s.latencyMs.toFixed(3) })),
    })),
    ...extra,
  };
  fs.writeFileSync(path.join(dir, file), JSON.stringify(report, null, 2));
  return report;
}
