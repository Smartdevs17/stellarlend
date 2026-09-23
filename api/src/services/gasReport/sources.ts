/**
 * Loads gas report inputs from the `stellar-lend/benchmarks` directory:
 *
 * - `benchmark-results.json` (latest `run_benchmarks` output), when present;
 *   otherwise `benchmarks/gas-baseline.json` (committed measured costs)
 * - `benchmarks/baseline.json` → `gas_budgets` and `operation_type_budgets`
 * - `benchmarks/history.jsonl` → historical trend
 * - `contracts/lending/target/journey-reports/lending-journeys.json` → journey
 *   gas from `contracts/lending/tests/user_journeys.rs`, when present
 */

import { existsSync, readFileSync } from 'fs';
import path from 'path';
import {
  buildGasReport,
  GasMeasurement,
  GasReport,
  HistoryPoint,
  JourneyReport,
  OperationType,
} from './report';

export interface GasReportSourceOptions {
  /** Root of the Rust workspace (default: `STELLAR_LEND_DIR` or `../stellar-lend`). */
  stellarLendDir?: string;
  /** Explicit benchmark results file (BenchmarkReport JSON). */
  resultsPath?: string;
  journeyReportPath?: string;
  regressionThresholdPct?: number;
}

function readJson<T>(file: string): T | null {
  if (!existsSync(file)) return null;
  return JSON.parse(readFileSync(file, 'utf8')) as T;
}

const normalizeContract = (c: string) => c.replace(/-/g, '_');

/** `execute_swap_warm` → (`execute_swap`, `warm`) so budgets keyed by fn match. */
function splitScenario(fn: string, scenario: string): [string, string] {
  const m = /^(.*)_(warm|cold)$/.exec(fn);
  if (m && m[1]) return [m[1], scenario || m[2]!];
  return [fn, scenario];
}

/** Parse either a `run_benchmarks` BenchmarkReport or `gas-baseline.json`. */
export function parseMeasurements(payload: unknown): GasMeasurement[] {
  const p = (payload ?? {}) as Record<string, unknown>;
  if (Array.isArray(p.results)) {
    return (p.results as Array<Record<string, unknown>>).map((r) => {
      const op = String(r.operation ?? '');
      const [contractPart, fnPart] = op.includes('::')
        ? op.split('::', 2)
        : [String(r.contract ?? ''), op];
      const [fn, scenario] = splitScenario(
        fnPart ?? '',
        r.cold_storage === true ? 'cold' : String(r.scenario ?? '')
      );
      return {
        contract: normalizeContract(contractPart || String(r.contract ?? '')),
        fn,
        scenario,
        cpuInstructions: Number(r.instructions ?? 0),
        memoryBytes: Number(r.memory_bytes ?? 0),
        storageReads: Number(r.storage_reads ?? 0),
        storageWrites: Number(r.storage_writes ?? 0),
      };
    });
  }
  if (Array.isArray(p.benchmarks)) {
    const contract = normalizeContract(String(p.contract ?? 'unknown'));
    return (p.benchmarks as Array<Record<string, unknown>>).map((b) => ({
      contract,
      fn: String(b.operation ?? ''),
      scenario: String(b.scenario ?? ''),
      cpuInstructions: Number(b.cpu_insns ?? 0),
      memoryBytes: Number(b.mem_bytes ?? 0),
    }));
  }
  return [];
}

export function parseHistory(jsonl: string): HistoryPoint[] {
  return (
    jsonl
      .split('\n')
      .filter((line) => line.trim())
      .map((line) => JSON.parse(line) as Record<string, unknown>)
      .map((h) => ({
        timestamp: String(h.timestamp ?? ''),
        source: h.source ? String(h.source) : undefined,
        totalBenchmarks: Number(h.total_benchmarks ?? 0),
        maxInstructions: Number(h.max_instructions ?? 0),
        avgInstructions: Number(h.avg_instructions ?? 0),
      }))
      // An empty bootstrap entry carries no signal for trends.
      .filter((h) => h.totalBenchmarks > 0)
  );
}

export function defaultStellarLendDir(): string {
  return process.env.STELLAR_LEND_DIR ?? path.resolve(process.cwd(), '..', 'stellar-lend');
}

export function loadGasReport(options: GasReportSourceOptions = {}): GasReport {
  const root = options.stellarLendDir ?? defaultStellarLendDir();
  const benchDir = path.join(root, 'benchmarks');

  const resultsPath = options.resultsPath ?? path.join(root, 'benchmark-results.json');
  const results = readJson<unknown>(resultsPath);
  const measuredBaseline = readJson<unknown>(path.join(benchDir, 'gas-baseline.json'));
  const committed = readJson<{
    gas_budgets?: Record<string, number>;
    operation_type_budgets?: Partial<Record<OperationType, number>>;
  }>(path.join(benchDir, 'baseline.json'));

  const current = results ? parseMeasurements(results) : [];
  const useResults = current.length > 0;

  const historyFile = path.join(benchDir, 'history.jsonl');
  const journeyPath =
    options.journeyReportPath ??
    path.join(root, 'contracts/lending/target/journey-reports/lending-journeys.json');

  return buildGasReport({
    current: useResults ? current : parseMeasurements(measuredBaseline),
    baseline: useResults ? parseMeasurements(measuredBaseline) : undefined,
    functionBudgets: committed?.gas_budgets ?? {},
    typeBudgets: committed?.operation_type_budgets ?? {},
    history: existsSync(historyFile) ? parseHistory(readFileSync(historyFile, 'utf8')) : [],
    journeys: readJson<JourneyReport>(journeyPath) ?? undefined,
    regressionThresholdPct: options.regressionThresholdPct,
    source: useResults ? path.relative(root, resultsPath) : 'benchmarks/gas-baseline.json',
  });
}
