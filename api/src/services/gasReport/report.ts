/**
 * Contract gas optimization report (issue #684).
 *
 * Pure function over benchmark measurements (Soroban CPU instructions and
 * memory bytes from `stellar-lend/benchmarks`), per-function and
 * per-operation-type budgets, the committed baseline, benchmark history and
 * journey reports. Produces per-function tracking, budget status, regression
 * detection, trends and rule-based optimization recommendations. Consumed by
 * the analytics API (`/api/analytics/gas/contract`), the CI generator script
 * and the frontend dashboard.
 */

export type OperationType =
  | 'read'
  | 'admin'
  | 'user_write'
  | 'liquidation'
  | 'flash_loan'
  | 'batch';

export const OPERATION_TYPES: readonly OperationType[] = [
  'read',
  'admin',
  'user_write',
  'liquidation',
  'flash_loan',
  'batch',
];

export interface GasMeasurement {
  contract: string;
  /** Function name without contract prefix, e.g. `deposit_collateral`. */
  fn: string;
  /** Scenario label, e.g. `write_cold`; empty when not provided. */
  scenario: string;
  cpuInstructions: number;
  memoryBytes: number;
  storageReads?: number;
  storageWrites?: number;
}

export interface HistoryPoint {
  timestamp: string;
  source?: string;
  totalBenchmarks: number;
  maxInstructions: number;
  avgInstructions: number;
}

export interface JourneyStep {
  step: string;
  budget_key: string;
  budget: number;
  cpu_instructions: number;
  memory_bytes: number;
  over_budget?: boolean;
}

export interface JourneyReport {
  journeys: Array<{ name: string; total_cpu_instructions: number; steps: JourneyStep[] }>;
  recommendations?: string[];
}

export interface GasReportInput {
  current: GasMeasurement[];
  /** Committed reference measurements for regression detection. */
  baseline?: GasMeasurement[];
  /** `contract::fn` → max CPU instructions. */
  functionBudgets: Record<string, number>;
  /** Fallback budget for any function of the given operation type. */
  typeBudgets: Partial<Record<OperationType, number>>;
  history?: HistoryPoint[];
  journeys?: JourneyReport;
  /** Regression threshold in percent (default 10). */
  regressionThresholdPct?: number;
  /** Utilization at which a function is flagged as near budget (default 80). */
  nearBudgetPct?: number;
  generatedAt?: string;
  source?: string;
}

export type BudgetStatus = 'ok' | 'near' | 'over' | 'unbudgeted';
export type Severity = 'critical' | 'high' | 'medium' | 'low';

export interface FunctionRow {
  key: string;
  contract: string;
  fn: string;
  scenario: string;
  operationType: OperationType;
  cpuInstructions: number;
  memoryBytes: number;
  budget: number | null;
  budgetSource: 'function' | 'type' | null;
  utilizationPct: number | null;
  status: BudgetStatus;
  baselineCpu: number | null;
  changePct: number | null;
}

export interface Recommendation {
  key: string;
  severity: Severity;
  rule: string;
  message: string;
}

export interface GasReport {
  generatedAt: string;
  source: string;
  thresholds: { regressionPct: number; nearBudgetPct: number };
  summary: {
    measurements: number;
    functions: number;
    overBudget: number;
    nearBudget: number;
    regressions: number;
    improvements: number;
    maxCpuInstructions: number;
    avgCpuInstructions: number;
  };
  functions: FunctionRow[];
  byContract: Record<
    string,
    { measurements: number; maxCpu: number; avgCpu: number; overBudget: number }
  >;
  byOperationType: Record<
    OperationType,
    {
      measurements: number;
      maxCpu: number;
      avgCpu: number;
      budget: number | null;
      overBudget: number;
      maxUtilizationPct: number | null;
    }
  >;
  regressions: Array<{ key: string; baseline: number; current: number; changePct: number }>;
  improvements: Array<{ key: string; baseline: number; current: number; changePct: number }>;
  recommendations: Recommendation[];
  trends: Array<HistoryPoint & { avgChangePct: number | null }>;
  journeys: Array<{ name: string; totalCpu: number; overBudgetSteps: string[] }>;
}

/** Classify a contract function into an operation type for type budgets. */
export function classifyOperation(fn: string): OperationType {
  const name = fn.toLowerCase();
  if (name.startsWith('batch_')) return 'batch';
  // Views first: `get_max_liquidatable_amount` is a read, not a liquidation.
  if (/^(get_|can_|require_|compute_|list_|hello|error_)/.test(name)) return 'read';
  if (name.includes('flash')) return 'flash_loan';
  if (name.includes('liquidat')) return 'liquidation';
  if (
    /^(set_|initialize|init|gov_initialize|transfer_admin|register_|update_|configure_|add_amm_protocol|claim_reserves)/.test(
      name
    )
  )
    return 'admin';
  return 'user_write';
}

export const measurementKey = (m: Pick<GasMeasurement, 'contract' | 'fn' | 'scenario'>) =>
  `${m.contract}::${m.fn}${m.scenario ? ` [${m.scenario}]` : ''}`;

const pct = (part: number, whole: number) => (whole > 0 ? (part / whole) * 100 : 0);
const round1 = (n: number) => Math.round(n * 10) / 10;

export function buildGasReport(input: GasReportInput): GasReport {
  const regressionPct = input.regressionThresholdPct ?? 10;
  const nearBudgetPct = input.nearBudgetPct ?? 80;
  const baseline = new Map((input.baseline ?? []).map((m) => [measurementKey(m), m]));

  const functions: FunctionRow[] = input.current.map((m) => {
    const key = measurementKey(m);
    const operationType = classifyOperation(m.fn);
    const fnBudget = input.functionBudgets[`${m.contract}::${m.fn}`];
    const typeBudget = input.typeBudgets[operationType];
    const budget = fnBudget ?? typeBudget ?? null;
    const utilization = budget ? round1(pct(m.cpuInstructions, budget)) : null;
    const base = baseline.get(key);
    return {
      key,
      contract: m.contract,
      fn: m.fn,
      scenario: m.scenario,
      operationType,
      cpuInstructions: m.cpuInstructions,
      memoryBytes: m.memoryBytes,
      budget,
      budgetSource: fnBudget !== undefined ? 'function' : typeBudget !== undefined ? 'type' : null,
      utilizationPct: utilization,
      status:
        utilization === null
          ? 'unbudgeted'
          : utilization > 100
            ? 'over'
            : utilization >= nearBudgetPct
              ? 'near'
              : 'ok',
      baselineCpu: base ? base.cpuInstructions : null,
      changePct:
        base && base.cpuInstructions > 0
          ? round1(pct(m.cpuInstructions - base.cpuInstructions, base.cpuInstructions))
          : null,
    };
  });

  const regressions = functions
    .filter((f) => f.changePct !== null && f.changePct > regressionPct)
    .map((f) => ({
      key: f.key,
      baseline: f.baselineCpu!,
      current: f.cpuInstructions,
      changePct: f.changePct!,
    }))
    .sort((a, b) => b.changePct - a.changePct);
  const improvements = functions
    .filter((f) => f.changePct !== null && f.changePct < -1)
    .map((f) => ({
      key: f.key,
      baseline: f.baselineCpu!,
      current: f.cpuInstructions,
      changePct: f.changePct!,
    }))
    .sort((a, b) => a.changePct - b.changePct);

  const byContract: GasReport['byContract'] = {};
  for (const f of functions) {
    const c = (byContract[f.contract] ??= { measurements: 0, maxCpu: 0, avgCpu: 0, overBudget: 0 });
    c.measurements += 1;
    c.maxCpu = Math.max(c.maxCpu, f.cpuInstructions);
    c.avgCpu += f.cpuInstructions;
    if (f.status === 'over') c.overBudget += 1;
  }
  for (const c of Object.values(byContract)) c.avgCpu = Math.round(c.avgCpu / c.measurements);

  const byOperationType = {} as GasReport['byOperationType'];
  for (const type of OPERATION_TYPES) {
    const rows = functions.filter((f) => f.operationType === type);
    const budget = input.typeBudgets[type] ?? null;
    const maxCpu = rows.reduce((m, f) => Math.max(m, f.cpuInstructions), 0);
    byOperationType[type] = {
      measurements: rows.length,
      maxCpu,
      avgCpu: rows.length
        ? Math.round(rows.reduce((s, f) => s + f.cpuInstructions, 0) / rows.length)
        : 0,
      budget,
      // Type budgets are an upper bound for every function of that type.
      overBudget: budget ? rows.filter((f) => f.cpuInstructions > budget).length : 0,
      maxUtilizationPct: budget && rows.length ? round1(pct(maxCpu, budget)) : null,
    };
  }

  const history = input.history ?? [];
  const trends = history.map((h, i) => {
    const prev = history[i - 1];
    return {
      ...h,
      avgChangePct:
        prev && prev.avgInstructions > 0
          ? round1(pct(h.avgInstructions - prev.avgInstructions, prev.avgInstructions))
          : null,
    };
  });

  const journeys = (input.journeys?.journeys ?? []).map((j) => ({
    name: j.name,
    totalCpu: j.total_cpu_instructions,
    overBudgetSteps: j.steps.filter((s) => s.over_budget).map((s) => s.step),
  }));

  const cpus = functions.map((f) => f.cpuInstructions);
  return {
    generatedAt: input.generatedAt ?? new Date().toISOString(),
    source: input.source ?? 'unknown',
    thresholds: { regressionPct, nearBudgetPct },
    summary: {
      measurements: functions.length,
      functions: new Set(functions.map((f) => `${f.contract}::${f.fn}`)).size,
      overBudget: functions.filter((f) => f.status === 'over').length,
      nearBudget: functions.filter((f) => f.status === 'near').length,
      regressions: regressions.length,
      improvements: improvements.length,
      maxCpuInstructions: cpus.length ? Math.max(...cpus) : 0,
      avgCpuInstructions: cpus.length
        ? Math.round(cpus.reduce((s, n) => s + n, 0) / cpus.length)
        : 0,
    },
    functions,
    byContract,
    byOperationType,
    regressions,
    improvements,
    recommendations: recommend(functions, regressions, input, nearBudgetPct),
    trends,
    journeys,
  };
}

const SEVERITY_ORDER: Record<Severity, number> = { critical: 0, high: 1, medium: 2, low: 3 };

function recommend(
  functions: FunctionRow[],
  regressions: GasReport['regressions'],
  input: GasReportInput,
  nearBudgetPct: number
): Recommendation[] {
  const out: Recommendation[] = [];
  const fmt = (n: number) => n.toLocaleString('en-US');

  for (const f of functions) {
    if (f.status === 'over' && f.budget) {
      out.push({
        key: f.key,
        severity: f.cpuInstructions > f.budget * 1.25 ? 'critical' : 'high',
        rule: 'over-budget',
        message: `Uses ${fmt(f.cpuInstructions)} CPU instructions, ${f.utilizationPct}% of its ${f.budgetSource} budget (${fmt(f.budget)}). Profile storage access and cross-contract calls first.`,
      });
    } else if (f.status === 'near') {
      out.push({
        key: f.key,
        severity: 'medium',
        rule: 'near-budget',
        message: `At ${f.utilizationPct}% of budget (threshold ${nearBudgetPct}%). Little headroom for new logic; optimize before extending.`,
      });
    }
    if (f.operationType === 'read' && f.cpuInstructions > 200_000) {
      out.push({
        key: f.key,
        severity: 'low',
        rule: 'expensive-view',
        message: `Read-only call costs ${fmt(f.cpuInstructions)} instructions. Clients should call it via RPC simulation; consider caching the aggregate on-chain if contracts call it.`,
      });
    }
    if (f.memoryBytes > 100_000) {
      out.push({
        key: f.key,
        severity: 'low',
        rule: 'memory-heavy',
        message: `Allocates ${fmt(f.memoryBytes)} bytes. Avoid materializing whole Vec/Map values; iterate or page instead.`,
      });
    }
  }

  for (const r of regressions) {
    out.push({
      key: r.key,
      severity: r.changePct > 25 ? 'critical' : 'high',
      rule: 'regression',
      message: `Regressed ${r.changePct}% vs baseline (${fmt(r.baseline)} → ${fmt(r.current)}).`,
    });
  }

  // Cold vs warm: a warm (cached-storage) call should never cost more.
  const byFn = new Map<string, FunctionRow[]>();
  for (const f of functions) {
    const k = `${f.contract}::${f.fn}`;
    byFn.set(k, [...(byFn.get(k) ?? []), f]);
  }
  for (const [fnKey, rows] of byFn) {
    const cold = rows.find((r) => /cold/.test(r.scenario));
    const warm = rows.find((r) => /warm/.test(r.scenario));
    if (cold && warm && warm.cpuInstructions > cold.cpuInstructions) {
      out.push({
        key: fnKey,
        severity: 'medium',
        rule: 'warm-costlier-than-cold',
        message: `Warm path (${fmt(warm.cpuInstructions)}) costs more than cold (${fmt(cold.cpuInstructions)}): the repeat call does extra work, e.g. rewriting unchanged entries or re-reading config. Skip writes when values are unchanged.`,
      });
    }

    // Batch scaling: per-item marginal cost vs. the single-item operation.
    const sized = rows
      .map((r) => ({ r, n: Number(/(\d+)_positions?/.exec(r.scenario)?.[1] ?? NaN) }))
      .filter((x) => Number.isFinite(x.n))
      .sort((a, b) => a.n - b.n);
    const single = fnKey.replace('::batch_', '::');
    const singleRow = functions.find(
      (f) => `${f.contract}::${f.fn}` === single && /^(write)?$/.test(f.scenario)
    );
    const first = sized[0];
    const last = sized[sized.length - 1];
    if (first && last && last.n > first.n && singleRow) {
      const marginal = (last.r.cpuInstructions - first.r.cpuInstructions) / (last.n - first.n);
      const saving = pct(singleRow.cpuInstructions - marginal, singleRow.cpuInstructions);
      if (saving < 25) {
        out.push({
          key: fnKey,
          severity: 'low',
          rule: 'batch-scaling',
          message: `Each extra item costs ~${fmt(Math.round(marginal))} instructions, only ${round1(saving)}% less than a standalone call (${fmt(singleRow.cpuInstructions)}). Hoist shared reads (oracle prices, config) out of the per-item loop.`,
        });
      }
    }
  }

  for (const j of input.journeys?.journeys ?? []) {
    for (const s of j.steps.filter((step) => step.over_budget)) {
      out.push({
        key: `journey:${j.name}:${s.step}`,
        severity: 'medium',
        rule: 'journey-step-over-budget',
        message: `Journey step '${s.step}' (${s.budget_key}) used ${fmt(s.cpu_instructions)} of ${fmt(s.budget)} budgeted instructions.`,
      });
    }
  }

  return out.sort(
    (a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity] || a.key.localeCompare(b.key)
  );
}

/** Markdown rendering for CI comments and `?format=markdown`. */
export function renderGasReportMarkdown(report: GasReport): string {
  const fmt = (n: number | null) => (n === null ? '—' : n.toLocaleString('en-US'));
  const icon: Record<BudgetStatus, string> = { ok: '✅', near: '⚠️', over: '❌', unbudgeted: '·' };
  const bar = (p: number | null) => {
    if (p === null) return '';
    const filled = Math.min(10, Math.round(p / 10));
    return `\`${'█'.repeat(filled)}${'░'.repeat(10 - filled)}\``;
  };
  const s = report.summary;
  const lines = [
    '## Contract Gas Report',
    '',
    `Source: \`${report.source}\` · generated ${report.generatedAt}`,
    '',
    `**${s.functions}** functions / **${s.measurements}** scenarios · ❌ over budget **${s.overBudget}** · ⚠️ near budget **${s.nearBudget}** · regressions **${s.regressions}** (>${report.thresholds.regressionPct}%) · improvements **${s.improvements}**`,
    '',
    '### Budget by operation type',
    '',
    '| Type | Scenarios | Max CPU | Avg CPU | Type budget | Max utilization | Over |',
    '| --- | ---: | ---: | ---: | ---: | --- | ---: |',
    ...OPERATION_TYPES.filter((t) => report.byOperationType[t].measurements > 0).map((t) => {
      const o = report.byOperationType[t];
      return `| ${t} | ${o.measurements} | ${fmt(o.maxCpu)} | ${fmt(o.avgCpu)} | ${fmt(o.budget)} | ${bar(o.maxUtilizationPct)} ${o.maxUtilizationPct ?? '—'}% | ${o.overBudget} |`;
    }),
    '',
  ];

  if (report.regressions.length) {
    lines.push(
      '### Regressions',
      '',
      '| Function | Baseline | Current | Change |',
      '| --- | ---: | ---: | ---: |'
    );
    for (const r of report.regressions) {
      lines.push(`| ${r.key} | ${fmt(r.baseline)} | ${fmt(r.current)} | +${r.changePct}% |`);
    }
    lines.push('');
  }

  lines.push(
    '### Functions',
    '',
    '| | Function | Type | CPU | Budget | Utilization | Δ vs baseline |',
    '| --- | --- | --- | ---: | ---: | --- | ---: |',
    ...[...report.functions]
      .sort((a, b) => (b.utilizationPct ?? -1) - (a.utilizationPct ?? -1))
      .map(
        (f) =>
          `| ${icon[f.status]} | ${f.key} | ${f.operationType} | ${fmt(f.cpuInstructions)} | ${fmt(f.budget)} | ${bar(f.utilizationPct)} ${f.utilizationPct ?? '—'}% | ${f.changePct === null ? '—' : `${f.changePct > 0 ? '+' : ''}${f.changePct}%`} |`
      ),
    ''
  );

  if (report.recommendations.length) {
    lines.push('### Optimization recommendations', '');
    for (const r of report.recommendations) {
      lines.push(`- **${r.severity}** \`${r.key}\` (${r.rule}): ${r.message}`);
    }
    lines.push('');
  }

  if (report.journeys.length) {
    lines.push(
      '### User journeys',
      '',
      '| Journey | Total CPU | Steps over budget |',
      '| --- | ---: | --- |'
    );
    for (const j of report.journeys) {
      lines.push(`| ${j.name} | ${fmt(j.totalCpu)} | ${j.overBudgetSteps.join(', ') || '—'} |`);
    }
    lines.push('');
  }

  if (report.trends.length > 1) {
    lines.push(
      '### Historical trend',
      '',
      '| Recorded | Benchmarks | Avg CPU | Max CPU | Δ avg |',
      '| --- | ---: | ---: | ---: | ---: |'
    );
    for (const t of report.trends.slice(-10)) {
      lines.push(
        `| ${t.timestamp} | ${t.totalBenchmarks} | ${fmt(t.avgInstructions)} | ${fmt(t.maxInstructions)} | ${t.avgChangePct === null ? '—' : `${t.avgChangePct}%`} |`
      );
    }
    lines.push('');
  }

  return lines.join('\n');
}
