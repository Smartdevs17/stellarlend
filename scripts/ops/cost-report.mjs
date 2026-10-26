#!/usr/bin/env node

import { appendFile, readFile } from 'node:fs/promises';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

function readMoney(value, label) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new Error(`${label} must be a non-negative number.`);
  }
  const cents = Math.round(value * 100);
  if (!Number.isSafeInteger(cents)) {
    throw new Error(`${label} exceeds the supported monetary range.`);
  }
  if (Math.abs(value * 100 - cents) > 1e-7) {
    throw new Error(`${label} must have no more than two decimal places.`);
  }
  return cents;
}

function readCurrency(value, label) {
  if (typeof value !== 'string' || !/^[A-Z]{3}$/.test(value)) {
    throw new Error(`${label} must be a three-letter uppercase currency code.`);
  }
  return value;
}

function dollars(cents, currency) {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
  }).format(cents / 100);
}

function parseInputs(args) {
  const values = new Map();
  for (let index = 0; index < args.length; index += 1) {
    const option = args[index];
    if (option === '--github-summary') {
      values.set(option, true);
      continue;
    }
    if (!['--budget', '--snapshot'].includes(option) || !args[index + 1]) {
      throw new Error(
        'Usage: cost-report.mjs --budget <file> --snapshot <file> [--github-summary]',
      );
    }
    values.set(option, args[index + 1]);
    index += 1;
  }
  if (!values.has('--budget') || !values.has('--snapshot')) {
    throw new Error(
      'Usage: cost-report.mjs --budget <file> --snapshot <file> [--github-summary]',
    );
  }
  return values;
}

export function createCostReport(budget, snapshot) {
  if (!budget || typeof budget !== 'object' || Array.isArray(budget)) {
    throw new Error('Budget must be a JSON object.');
  }
  if (!snapshot || typeof snapshot !== 'object' || Array.isArray(snapshot)) {
    throw new Error('Snapshot must be a JSON object.');
  }

  const currency = readCurrency(budget.currency, 'Budget currency');
  if (readCurrency(snapshot.currency, 'Snapshot currency') !== currency) {
    throw new Error('Budget and snapshot currencies must match.');
  }
  if (typeof snapshot.month !== 'string' || !/^\d{4}-(0[1-9]|1[0-2])$/.test(snapshot.month)) {
    throw new Error('Snapshot month must use YYYY-MM format.');
  }
  if (typeof snapshot.updatedAt !== 'string' || !Number.isFinite(Date.parse(snapshot.updatedAt))) {
    throw new Error('Snapshot updatedAt must be a valid timestamp.');
  }
  if (
    typeof budget.warningPercent !== 'number' ||
    !Number.isFinite(budget.warningPercent) ||
    budget.warningPercent <= 0 ||
    budget.warningPercent >= 100
  ) {
    throw new Error('Budget warningPercent must be greater than 0 and less than 100.');
  }
  if (!budget.categories || typeof budget.categories !== 'object' || Array.isArray(budget.categories)) {
    throw new Error('Budget categories must be a non-empty object.');
  }

  const categoryBudgets = new Map(
    Object.entries(budget.categories).map(([category, amount]) => [
      category,
      readMoney(amount, `Budget for ${category || '<empty category>'}`),
    ]),
  );
  if ([...categoryBudgets.keys()].some((category) => category.trim() === '')) {
    throw new Error('Budget category names must not be empty.');
  }
  if ([...categoryBudgets.keys()].some((category) => !/^[a-z0-9][a-z0-9._-]*$/.test(category))) {
    throw new Error('Budget categories must use lowercase letters, numbers, dots, underscores, or hyphens.');
  }
  if (categoryBudgets.size === 0) {
    throw new Error('Budget must define at least one category.');
  }
  const totalBudget = readMoney(budget.monthlyTotal, 'Monthly total budget');
  const actuals = new Map([...categoryBudgets.keys()].map((category) => [category, 0]));

  if (!Array.isArray(snapshot.actuals)) {
    throw new Error('Snapshot actuals must be an array.');
  }
  for (const [index, entry] of snapshot.actuals.entries()) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      throw new Error(`Snapshot actuals[${index}] must be an object.`);
    }
    if (typeof entry.category !== 'string' || !categoryBudgets.has(entry.category)) {
      throw new Error(`Snapshot actuals[${index}] has an unbudgeted category.`);
    }
    if (typeof entry.provider !== 'string' || entry.provider.trim() === '') {
      throw new Error(`Snapshot actuals[${index}] must include a provider name.`);
    }
    actuals.set(
      entry.category,
      actuals.get(entry.category) + readMoney(entry.amount, `Actual for ${entry.category}`),
    );
  }

  const seenCategories = new Set(snapshot.actuals.map(({ category }) => category));
  const missingCategories = [...categoryBudgets.keys()].filter(
    (category) => !seenCategories.has(category),
  );
  if (missingCategories.length > 0) {
    throw new Error(
      `Snapshot is missing cost data for: ${missingCategories.join(', ')}. Record zero explicitly when there is no spend.`,
    );
  }

  const totalActual = [...actuals.values()].reduce((sum, amount) => sum + amount, 0);
  const rows = [
    {
      category: 'Total',
      actual: totalActual,
      budget: totalBudget,
    },
    ...[...categoryBudgets].map(([category, amount]) => ({
      category,
      actual: actuals.get(category),
      budget: amount,
    })),
  ].map((row) => ({
    ...row,
    percent: row.budget === 0 ? (row.actual === 0 ? 0 : Infinity) : (row.actual / row.budget) * 100,
    status:
      row.actual >= row.budget
        ? 'OVER BUDGET'
        : row.actual >= (row.budget * budget.warningPercent) / 100
          ? 'WARNING'
          : 'OK',
  }));

  const markdown = [
    `## Cost dashboard — ${snapshot.month}`,
    '',
    `Snapshot updated: ${snapshot.updatedAt} · Currency: ${currency}`,
    '',
    '| Category | Actual | Monthly budget | Used | Status |',
    '| --- | ---: | ---: | ---: | --- |',
    ...rows.map(
      (row) =>
        `| ${row.category} | ${dollars(row.actual, currency)} | ${dollars(row.budget, currency)} | ${Number.isFinite(row.percent) ? `${row.percent.toFixed(1)}%` : 'unlimited'} | ${row.status} |`,
    ),
    '',
    `Warning threshold: ${budget.warningPercent}% of each monthly budget.`,
    '',
  ].join('\n');

  return {
    rows,
    markdown,
    overBudget: rows.some((row) => row.status === 'OVER BUDGET'),
    warnings: rows.filter((row) => row.status === 'WARNING'),
  };
}

async function main() {
  const inputs = parseInputs(process.argv.slice(2));
  const [budgetText, snapshotText] = await Promise.all([
    readFile(inputs.get('--budget'), 'utf8'),
    readFile(inputs.get('--snapshot'), 'utf8'),
  ]);
  const report = createCostReport(JSON.parse(budgetText), JSON.parse(snapshotText));

  process.stdout.write(report.markdown);
  for (const row of report.warnings) {
    process.stderr.write(`::warning::${row.category} has reached its cost warning threshold.\n`);
  }
  if (report.overBudget) {
    process.stderr.write('::error::At least one monthly cost budget has been reached.\n');
  }
  if (inputs.has('--github-summary')) {
    if (!process.env.GITHUB_STEP_SUMMARY) {
      throw new Error('--github-summary requires the GITHUB_STEP_SUMMARY environment variable.');
    }
    await appendFile(process.env.GITHUB_STEP_SUMMARY, report.markdown);
  }
  if (report.overBudget) {
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    process.stderr.write(`Cost report failed: ${error.message}\n`);
    process.exitCode = 1;
  });
}
