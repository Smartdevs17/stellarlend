import assert from 'node:assert/strict';
import test from 'node:test';

import { createCostReport } from './cost-report.mjs';

const budget = {
  currency: 'USD',
  warningPercent: 80,
  monthlyTotal: 1000,
  categories: { api: 600, rpc: 400 },
};

const snapshot = {
  month: '2026-10',
  currency: 'USD',
  updatedAt: '2026-10-18T00:00:00Z',
  actuals: [
    { category: 'api', provider: 'Example', amount: 480 },
    { category: 'rpc', provider: 'Example', amount: 200 },
  ],
};

test('creates a categorized dashboard and warns at the configured threshold', () => {
  const report = createCostReport(budget, snapshot);

  assert.equal(report.overBudget, false);
  assert.deepEqual(
    report.warnings.map(({ category }) => category),
    ['api'],
  );
  assert.match(report.markdown, /Cost dashboard — 2026-10/);
  assert.match(report.markdown, /\| api \| \$480\.00 \| \$600\.00 \| 80\.0% \| WARNING \|/);
});

test('fails the budget when total or a category reaches its limit', () => {
  const report = createCostReport(budget, {
    ...snapshot,
    actuals: [
      { category: 'api', provider: 'Example', amount: 600 },
      { category: 'rpc', provider: 'Example', amount: 400 },
    ],
  });

  assert.equal(report.overBudget, true);
  assert.ok(report.rows.every(({ status }) => status === 'OVER BUDGET'));
});

test('rejects mixed currencies, unknown categories, and invalid costs', () => {
  assert.throws(
    () => createCostReport(budget, { ...snapshot, currency: 'EUR' }),
    /currencies must match/,
  );
  assert.throws(
    () =>
      createCostReport(budget, {
        ...snapshot,
        actuals: [{ category: 'hosting', provider: 'Example', amount: 10 }],
      }),
    /unbudgeted category/,
  );
  assert.throws(
    () =>
      createCostReport(budget, {
        ...snapshot,
        actuals: [{ category: 'api', provider: 'Example', amount: -1 }],
      }),
    /non-negative number/,
  );
});

test('requires all cost categories and timestamps to be well-formed', () => {
  assert.throws(
    () => createCostReport({ ...budget, warningPercent: 100 }, snapshot),
    /warningPercent/,
  );
  assert.throws(
    () => createCostReport(budget, { ...snapshot, month: 'October 2026' }),
    /YYYY-MM/,
  );
  assert.throws(
    () => createCostReport(budget, { ...snapshot, updatedAt: 'invalid' }),
    /valid timestamp/,
  );
  assert.throws(
    () => createCostReport(budget, { ...snapshot, actuals: snapshot.actuals.slice(0, 1) }),
    /missing cost data for: rpc/,
  );
});
