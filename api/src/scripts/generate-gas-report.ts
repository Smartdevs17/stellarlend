/**
 * Gas report generator CLI (issue #684).
 *
 *   npx ts-node --transpile-only api/src/scripts/generate-gas-report.ts \
 *     [--stellar-lend-dir stellar-lend] [--results stellar-lend/benchmark-results.json] \
 *     [--journeys <lending-journeys.json>] [--out-json gas-report.json] [--out-md gas-report.md] \
 *     [--regression-threshold 10] [--fail-on-over-budget] [--fail-on-regression]
 *
 * Also appends the Markdown report to $GITHUB_STEP_SUMMARY when set.
 * Exit codes: 0 ok · 1 gate failed (over budget / regression) · 2 bad input.
 */

import { appendFileSync, writeFileSync } from 'fs';
import path from 'path';
import { loadGasReport } from '../services/gasReport/sources';
import { renderGasReportMarkdown } from '../services/gasReport/report';

function parseArgs(argv: string[]): Map<string, string | true> {
  const args = new Map<string, string | true>();
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i] ?? '';
    if (!arg.startsWith('--')) continue;
    const next = argv[i + 1];
    if (next !== undefined && !next.startsWith('--')) {
      args.set(arg.slice(2), next);
      i += 1;
    } else {
      args.set(arg.slice(2), true);
    }
  }
  return args;
}

function main(): number {
  const args = parseArgs(process.argv.slice(2));
  const str = (key: string) => {
    const v = args.get(key);
    return typeof v === 'string' ? path.resolve(v) : undefined;
  };
  const threshold = args.get('regression-threshold');

  const report = loadGasReport({
    stellarLendDir: str('stellar-lend-dir') ?? path.resolve(__dirname, '../../../stellar-lend'),
    resultsPath: str('results'),
    journeyReportPath: str('journeys'),
    regressionThresholdPct: typeof threshold === 'string' ? Number(threshold) : undefined,
  });
  if (report.summary.measurements === 0) {
    console.error('gas-report: no benchmark measurements found');
    return 2;
  }

  const markdown = renderGasReportMarkdown(report);
  const outJson = str('out-json');
  const outMd = str('out-md');
  if (outJson) writeFileSync(outJson, JSON.stringify(report, null, 2) + '\n');
  if (outMd) writeFileSync(outMd, markdown + '\n');
  if (!outJson && !outMd) process.stdout.write(markdown + '\n');
  if (process.env.GITHUB_STEP_SUMMARY)
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown + '\n');

  const { overBudget, regressions } = report.summary;
  console.error(
    `gas-report: ${report.summary.measurements} scenarios, ${overBudget} over budget, ${regressions} regressions`
  );
  if (args.has('fail-on-over-budget') && overBudget > 0) return 1;
  if (args.has('fail-on-regression') && regressions > 0) return 1;
  return 0;
}

process.exit(main());
