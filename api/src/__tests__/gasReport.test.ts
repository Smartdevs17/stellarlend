import express from 'express';
import path from 'path';
import request from 'supertest';
import gasReportRoutes from '../routes/gasReport.routes';
import { errorHandler } from '../middleware/errorHandler';
import {
  buildGasReport,
  classifyOperation,
  clearGasReportCache,
  GasMeasurement,
  loadGasReport,
  parseHistory,
  parseMeasurements,
  renderGasReportMarkdown,
} from '../services/gasReport';

const STELLAR_LEND = path.resolve(__dirname, '../../../stellar-lend');

const m = (
  fn: string,
  cpu: number,
  scenario = '',
  extra: Partial<GasMeasurement> = {}
): GasMeasurement => ({
  contract: 'lending',
  fn,
  scenario,
  cpuInstructions: cpu,
  memoryBytes: 1_000,
  ...extra,
});

describe('classifyOperation', () => {
  it.each([
    ['get_user_position', 'read'],
    ['get_max_liquidatable_amount', 'read'],
    ['can_be_liquidated', 'read'],
    ['set_oracle', 'admin'],
    ['initialize', 'admin'],
    ['deposit_collateral', 'user_write'],
    ['borrow', 'user_write'],
    ['liquidate', 'liquidation'],
    ['flash_loan', 'flash_loan'],
    ['batch_liquidate', 'batch'],
  ])('%s → %s', (fn, type) => {
    expect(classifyOperation(fn)).toBe(type);
  });
});

describe('buildGasReport', () => {
  const functionBudgets = { 'lending::deposit': 1_000, 'lending::borrow': 1_000 };
  const typeBudgets = { read: 500, user_write: 2_000, batch: 10_000, liquidation: 3_000 };

  it('tracks per-function budget status with function budgets overriding type budgets', () => {
    const report = buildGasReport({
      current: [m('deposit', 1_100), m('borrow', 850), m('get_position', 100), m('repay', 1_500)],
      functionBudgets,
      typeBudgets,
    });
    const byKey = Object.fromEntries(report.functions.map((f) => [f.key, f]));

    expect(byKey['lending::deposit']).toMatchObject({
      status: 'over',
      budget: 1_000,
      budgetSource: 'function',
      utilizationPct: 110,
    });
    expect(byKey['lending::borrow']).toMatchObject({ status: 'near', utilizationPct: 85 });
    expect(byKey['lending::get_position']).toMatchObject({
      status: 'ok',
      budget: 500,
      budgetSource: 'type',
    });
    expect(byKey['lending::repay']).toMatchObject({
      status: 'ok',
      budget: 2_000,
      budgetSource: 'type',
    });
    expect(report.summary).toMatchObject({ overBudget: 1, nearBudget: 1, functions: 4 });
  });

  it('aggregates per operation type against the type budget', () => {
    const report = buildGasReport({
      current: [m('deposit', 1_100), m('repay', 2_500), m('get_position', 100)],
      functionBudgets,
      typeBudgets,
    });
    expect(report.byOperationType.user_write).toMatchObject({
      measurements: 2,
      maxCpu: 2_500,
      budget: 2_000,
      overBudget: 1,
      maxUtilizationPct: 125,
    });
    expect(report.byOperationType.admin).toMatchObject({ measurements: 0, budget: null });
  });

  it('detects regressions and improvements against the baseline', () => {
    const report = buildGasReport({
      current: [m('deposit', 1_200), m('borrow', 500), m('repay', 1_005)],
      baseline: [m('deposit', 1_000), m('borrow', 1_000), m('repay', 1_000)],
      functionBudgets: {},
      typeBudgets: {},
      regressionThresholdPct: 10,
    });
    expect(report.regressions).toEqual([
      { key: 'lending::deposit', baseline: 1_000, current: 1_200, changePct: 20 },
    ]);
    expect(report.improvements).toEqual([
      { key: 'lending::borrow', baseline: 1_000, current: 500, changePct: -50 },
    ]);
    expect(report.recommendations.find((r) => r.rule === 'regression')).toMatchObject({
      severity: 'high',
    });
  });

  it('recommends on warm > cold, batch scaling, expensive views and memory', () => {
    const report = buildGasReport({
      current: [
        m('deposit', 1_000, 'write_cold'),
        m('deposit', 1_200, 'write_warm'),
        m('liquidate', 1_000, 'write'),
        m('batch_liquidate', 1_900, 'write_2_positions'),
        m('batch_liquidate', 4_600, 'write_5_positions', { memoryBytes: 200_000 }),
        m('get_everything', 250_000, 'read_only'),
      ],
      functionBudgets: {},
      typeBudgets: {},
    });
    const rules = report.recommendations.map((r) => `${r.rule}:${r.key}`);
    expect(rules).toEqual(
      expect.arrayContaining([
        'warm-costlier-than-cold:lending::deposit',
        'batch-scaling:lending::batch_liquidate',
        'expensive-view:lending::get_everything [read_only]',
        'memory-heavy:lending::batch_liquidate [write_5_positions]',
      ])
    );
  });

  it('orders recommendations by severity', () => {
    const report = buildGasReport({
      current: [m('deposit', 2_000), m('borrow', 900), m('get_x', 300_000)],
      functionBudgets,
      typeBudgets: { read: 1_000_000 },
    });
    expect(report.recommendations.map((r) => r.severity)).toEqual(['critical', 'medium', 'low']);
  });

  it('surfaces journey steps over budget and history trends', () => {
    const report = buildGasReport({
      current: [m('deposit', 100)],
      functionBudgets: {},
      typeBudgets: {},
      history: [
        { timestamp: 't1', totalBenchmarks: 10, maxInstructions: 500, avgInstructions: 200 },
        { timestamp: 't2', totalBenchmarks: 10, maxInstructions: 550, avgInstructions: 220 },
      ],
      journeys: {
        journeys: [
          {
            name: 'full_cycle',
            total_cpu_instructions: 1_000,
            steps: [
              {
                step: 'check',
                budget_key: 'lending::get_user_position',
                budget: 400,
                cpu_instructions: 410,
                memory_bytes: 1,
                over_budget: true,
              },
            ],
          },
        ],
      },
    });
    expect(report.trends.map((t) => t.avgChangePct)).toEqual([null, 10]);
    expect(report.journeys).toEqual([
      { name: 'full_cycle', totalCpu: 1_000, overBudgetSteps: ['check'] },
    ]);
    expect(report.recommendations.some((r) => r.rule === 'journey-step-over-budget')).toBe(true);
  });

  it('renders a markdown report with type budgets and recommendations', () => {
    const md = renderGasReportMarkdown(
      buildGasReport({ current: [m('deposit', 1_100)], functionBudgets, typeBudgets })
    );
    expect(md).toContain('## Contract Gas Report');
    expect(md).toContain('| user_write | 1 |');
    expect(md).toContain('❌ | lending::deposit');
    expect(md).toContain('over-budget');
  });
});

describe('gas report sources', () => {
  it('parses run_benchmarks output, splitting warm/cold suffixes into scenarios', () => {
    const parsed = parseMeasurements({
      results: [
        {
          operation: 'amm::execute_swap_warm',
          contract: 'amm',
          instructions: 10,
          memory_bytes: 2,
          cold_storage: false,
        },
        {
          operation: 'lending::deposit',
          contract: 'lending',
          instructions: 5,
          memory_bytes: 1,
          cold_storage: true,
        },
      ],
    });
    expect(parsed).toEqual([
      expect.objectContaining({
        contract: 'amm',
        fn: 'execute_swap',
        scenario: 'warm',
        cpuInstructions: 10,
      }),
      expect.objectContaining({
        contract: 'lending',
        fn: 'deposit',
        scenario: 'cold',
        cpuInstructions: 5,
      }),
    ]);
  });

  it('parses gas-baseline.json and normalizes contract names', () => {
    const parsed = parseMeasurements({
      contract: 'hello-world',
      benchmarks: [{ operation: 'hello', scenario: 'read_only', cpu_insns: 13, mem_bytes: 2 }],
    });
    expect(parsed).toEqual([
      {
        contract: 'hello_world',
        fn: 'hello',
        scenario: 'read_only',
        cpuInstructions: 13,
        memoryBytes: 2,
      },
    ]);
  });

  it('skips empty bootstrap history entries', () => {
    const history = parseHistory(
      '{"timestamp":"a","total_benchmarks":0}\n{"timestamp":"b","total_benchmarks":3,"max_instructions":9,"avg_instructions":4}\n'
    );
    expect(history).toEqual([
      {
        timestamp: 'b',
        source: undefined,
        totalBenchmarks: 3,
        maxInstructions: 9,
        avgInstructions: 4,
      },
    ]);
  });

  it('builds a report from the committed benchmark data', () => {
    const report = loadGasReport({
      stellarLendDir: STELLAR_LEND,
      resultsPath: '/nonexistent',
      journeyReportPath: '/nonexistent',
    });
    expect(report.source).toBe('benchmarks/gas-baseline.json');
    expect(report.summary.measurements).toBeGreaterThan(30);
    // Every committed measurement must be within its budget.
    expect(report.functions.filter((f) => f.status === 'over')).toEqual([]);
    expect(report.byOperationType.user_write.budget).toBe(1_200_000);
  });
});

describe('gas report routes (/api/analytics/gas/contract)', () => {
  const app = express();
  app.use('/api/analytics/gas/contract', gasReportRoutes);
  app.use(errorHandler);

  beforeAll(() => {
    process.env.STELLAR_LEND_DIR = STELLAR_LEND;
    clearGasReportCache();
  });
  afterAll(() => {
    delete process.env.STELLAR_LEND_DIR;
    clearGasReportCache();
  });

  it('GET / returns the JSON report', async () => {
    const res = await request(app).get('/api/analytics/gas/contract');
    expect(res.status).toBe(200);
    expect(res.body.summary.measurements).toBeGreaterThan(0);
    expect(res.body.byOperationType).toHaveProperty('batch');
  });

  it('GET /?format=markdown returns markdown', async () => {
    const res = await request(app).get('/api/analytics/gas/contract').query({ format: 'markdown' });
    expect(res.status).toBe(200);
    expect(res.headers['content-type']).toContain('text/markdown');
    expect(res.text).toContain('## Contract Gas Report');
  });

  it('GET /budgets and /budgets/:type expose budget utilization', async () => {
    const all = await request(app).get('/api/analytics/gas/contract/budgets');
    expect(all.body.functions[0]).toHaveProperty('utilizationPct');

    const reads = await request(app).get('/api/analytics/gas/contract/budgets/read');
    expect(reads.status).toBe(200);
    expect(
      reads.body.functions.every((f: { operationType: string }) => f.operationType === 'read')
    ).toBe(true);

    expect((await request(app).get('/api/analytics/gas/contract/budgets/bogus')).status).toBe(404);
  });

  it('GET /regressions, /recommendations and /trends', async () => {
    const regressions = await request(app).get('/api/analytics/gas/contract/regressions');
    expect(regressions.body).toHaveProperty('regressions');

    const recs = await request(app)
      .get('/api/analytics/gas/contract/recommendations')
      .query({ severity: 'low' });
    expect(recs.body.every((r: { severity: string }) => r.severity === 'low')).toBe(true);

    const trends = await request(app).get('/api/analytics/gas/contract/trends');
    expect(trends.body).toHaveProperty('trends');
    expect(trends.body).toHaveProperty('journeys');
  });
});
