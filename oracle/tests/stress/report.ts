/**
 * Oracle Stress Test Reporting (#691)
 *
 * Scenarios call `recordScenario` as they run; the vitest global teardown
 * calls `writeStressReport` once at the end to emit machine-readable JSON and
 * a human-readable markdown summary.
 *
 * Vitest runs each test file in its own worker by default, so an in-memory
 * accumulator would only ever see one file's worth of results. Each worker
 * therefore appends a JSON-lines shard to `stress-report/shards/`, and the
 * teardown — which runs once in the main process — merges them.
 */

import { appendFileSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { LatencyStats } from './harness.js';

/** Root for all stress artifacts, overridable so CI can redirect them. */
export const REPORT_DIR =
  process.env.ORACLE_STRESS_REPORT_DIR ?? join(process.cwd(), 'stress-report');

const SHARD_DIR = join(REPORT_DIR, 'shards');

/** Which acceptance-criteria bucket a scenario belongs to. */
export type StressCategory =
  | 'failure-scenarios'
  | 'price-shock'
  | 'latency'
  | 'circuit-breaker'
  | 'redundancy'
  | 'upgrade';

export interface ScenarioRecord {
  category: StressCategory;
  /** Human-readable scenario name, unique within its category. */
  name: string;
  /** Did the protocol behave as the scenario requires? */
  passed: boolean;
  /** Calls issued during the scenario. */
  iterations?: number;
  successes?: number;
  failures?: number;
  latency?: LatencyStats;
  /** Free-form scenario detail surfaced in the markdown report. */
  notes?: Record<string, string | number | boolean>;
  recordedAt: string;
}

export type ScenarioInput = Omit<ScenarioRecord, 'recordedAt'>;

/**
 * Record one stress scenario's outcome.
 *
 * Never throws: a reporting failure must not fail the suite it is reporting on.
 */
export function recordScenario(record: ScenarioInput): void {
  try {
    mkdirSync(SHARD_DIR, { recursive: true });
    const entry: ScenarioRecord = { ...record, recordedAt: new Date().toISOString() };
    const shard = join(SHARD_DIR, `${process.pid}.jsonl`);
    appendFileSync(shard, `${JSON.stringify(entry)}\n`, 'utf8');
  } catch {
    // Reporting is best-effort.
  }
}

/** Remove artifacts from a previous run so a report is never a mix of two. */
export function resetStressReport(): void {
  try {
    rmSync(REPORT_DIR, { recursive: true, force: true });
  } catch {
    // Nothing to clean.
  }
}

function readShards(): ScenarioRecord[] {
  let files: string[];
  try {
    files = readdirSync(SHARD_DIR).filter((f) => f.endsWith('.jsonl'));
  } catch {
    return [];
  }

  const records: ScenarioRecord[] = [];
  for (const file of files) {
    const raw = readFileSync(join(SHARD_DIR, file), 'utf8');
    for (const line of raw.split('\n')) {
      if (line.trim() === '') continue;
      try {
        records.push(JSON.parse(line) as ScenarioRecord);
      } catch {
        // Skip a torn line rather than losing the whole shard.
      }
    }
  }
  return records;
}

const CATEGORY_TITLES: Record<StressCategory, string> = {
  'failure-scenarios': 'Oracle Failure Scenarios',
  'price-shock': 'Price Spike / Crash Simulation',
  latency: 'Oracle Latency',
  'circuit-breaker': 'Circuit Breaker Validation',
  redundancy: 'Multi-Oracle Redundancy',
  upgrade: 'Oracle Upgrade',
};

export interface StressSummary {
  generatedAt: string;
  totalScenarios: number;
  passed: number;
  failed: number;
  categories: Record<string, { total: number; passed: number; failed: number }>;
  scenarios: ScenarioRecord[];
}

function summarize(records: ScenarioRecord[]): StressSummary {
  const categories: StressSummary['categories'] = {};

  for (const record of records) {
    const bucket = (categories[record.category] ??= { total: 0, passed: 0, failed: 0 });
    bucket.total++;
    if (record.passed) bucket.passed++;
    else bucket.failed++;
  }

  return {
    generatedAt: new Date().toISOString(),
    totalScenarios: records.length,
    passed: records.filter((r) => r.passed).length,
    failed: records.filter((r) => !r.passed).length,
    categories,
    scenarios: records.sort(
      (a, b) => a.category.localeCompare(b.category) || a.name.localeCompare(b.name)
    ),
  };
}

function formatLatency(latency?: LatencyStats): string {
  if (!latency || latency.samples === 0) return '—';
  const r = (n: number) => Math.round(n);
  return `p50 ${r(latency.p50Ms)}ms · p95 ${r(latency.p95Ms)}ms · p99 ${r(latency.p99Ms)}ms · max ${r(latency.maxMs)}ms`;
}

function renderMarkdown(summary: StressSummary): string {
  const lines: string[] = [];

  lines.push('# Oracle Stress Test Report');
  lines.push('');
  lines.push(`Generated: ${summary.generatedAt}`);
  lines.push('');
  lines.push(
    `**${summary.passed}/${summary.totalScenarios} scenarios passed**` +
      (summary.failed > 0 ? ` — ${summary.failed} failed` : '')
  );
  lines.push('');

  lines.push('## Coverage by category');
  lines.push('');
  lines.push('| Category | Scenarios | Passed | Failed |');
  lines.push('| --- | ---: | ---: | ---: |');
  for (const [category, bucket] of Object.entries(summary.categories)) {
    const title = CATEGORY_TITLES[category as StressCategory] ?? category;
    lines.push(`| ${title} | ${bucket.total} | ${bucket.passed} | ${bucket.failed} |`);
  }
  lines.push('');

  for (const category of Object.keys(summary.categories)) {
    const title = CATEGORY_TITLES[category as StressCategory] ?? category;
    lines.push(`## ${title}`);
    lines.push('');
    lines.push('| Scenario | Result | Calls | Success | Fail | Latency |');
    lines.push('| --- | :---: | ---: | ---: | ---: | --- |');

    for (const scenario of summary.scenarios.filter((s) => s.category === category)) {
      lines.push(
        `| ${scenario.name} | ${scenario.passed ? '✅' : '❌'} ` +
          `| ${scenario.iterations ?? '—'} | ${scenario.successes ?? '—'} ` +
          `| ${scenario.failures ?? '—'} | ${formatLatency(scenario.latency)} |`
      );
    }
    lines.push('');

    const withNotes = summary.scenarios.filter(
      (s) => s.category === category && s.notes && Object.keys(s.notes).length > 0
    );
    if (withNotes.length > 0) {
      lines.push('<details><summary>Scenario detail</summary>');
      lines.push('');
      for (const scenario of withNotes) {
        lines.push(`**${scenario.name}**`);
        lines.push('');
        for (const [key, value] of Object.entries(scenario.notes ?? {})) {
          lines.push(`- \`${key}\`: ${value}`);
        }
        lines.push('');
      }
      lines.push('</details>');
      lines.push('');
    }
  }

  return lines.join('\n');
}

/**
 * Merge every worker shard into `stress-report/`.
 *
 * Returns the summary so a caller (CI, or a test of the reporter itself) can
 * assert on it without re-reading the files.
 */
export function writeStressReport(): StressSummary {
  const summary = summarize(readShards());

  mkdirSync(REPORT_DIR, { recursive: true });
  writeFileSync(
    join(REPORT_DIR, 'oracle-stress-report.json'),
    `${JSON.stringify(summary, null, 2)}\n`,
    'utf8'
  );
  writeFileSync(
    join(REPORT_DIR, 'oracle-stress-report.md'),
    `${renderMarkdown(summary)}\n`,
    'utf8'
  );

  try {
    rmSync(SHARD_DIR, { recursive: true, force: true });
  } catch {
    // Shards are disposable.
  }

  return summary;
}
