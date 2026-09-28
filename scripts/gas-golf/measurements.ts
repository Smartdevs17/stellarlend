/**
 * Measurement ingestion and integrity for the gas golf leaderboard (#1014).
 *
 * This is the part that decides whether a leaderboard is worth anything, and it
 * is where the repository's current state forces a hard design constraint.
 *
 * ## The problem
 *
 * `stellar-lend/benchmarks/baseline.json` has `"results": []`, and
 * `gas-baseline.json` holds 39 real measurements that are **all `hello-world`** —
 * there is not one `lending::*` row. Meanwhile:
 *
 * - `framework.rs:317-319` hardcodes `disk_read_entries` / `write_entries` to `0`,
 *   and every `BenchmarkResult::new(...)` declares `storage_reads`,
 *   `storage_writes` and `cold_storage` as **integer/boolean literals**.
 * - `RunConfig.iterations` is parsed and never read, so every published number is
 *   a single run of a single call.
 * - The coverage gate is set-membership only, so a row of `instructions: 0` for
 *   every operation passes it.
 *
 * A scorer that reads those files and prints a ranked table would be printing
 * fiction. So ingestion is split from scoring, and this module's job is to make
 * the *absence* of a usable measurement explicit rather than quietly score zero.
 *
 * ## What is and is not scoreable
 *
 * `instructions` and `memory_bytes` are genuinely measured by
 * `env.cost_estimate()`. Storage read/write counts are **declared, not measured**,
 * so they are carried through as `declared` provenance and never used as a
 * ranking input.
 */

import * as fs from "node:fs";
import * as path from "node:path";

import { splitId } from "./challenges.ts";

/** One ingested measurement row. */
export interface Measurement {
  /** Canonical id: `contract::fn`, optionally with ` [scenario]`. */
  id: string;
  contract: string;
  fn: string;
  scenario: string;
  /** Measured by `env.cost_estimate()`. */
  instructions: number;
  memoryBytes: number;
  /**
   * Hand-declared in every `BenchmarkResult::new(...)` call, not measured. Kept
   * for display only; never a ranking input.
   */
  declaredStorageReads: number;
  declaredStorageWrites: number;
  declaredColdStorage: boolean;
  /** ISO timestamp from the report, when the source carries one. */
  timestamp: string | null;
  /** The git commit the benchmark ran against, when the source records it. */
  commit: string | null;
}

/** Why a measurement or measurement set cannot be scored. */
export type IntegrityReason =
  | "empty"
  | "zero-instructions"
  | "no-budget"
  | "unknown-operation"
  | "not-required"
  | "stale-pairing"
  | "unreadable";

export interface IntegrityFinding {
  reason: IntegrityReason;
  id: string;
  detail: string;
}

export interface MeasurementSet {
  measurements: Measurement[];
  /** The report's own timestamp, if the source had one. */
  reportedAt: string | null;
  source: string;
  shape: "benchmark-report" | "gas-baseline" | "unknown";
}

/**
 * Normalise either committed measurement shape.
 *
 * `benchmark-results.json` is a `BenchmarkReport` (`results[]`, field
 * `instructions`); `gas-baseline.json` is `{ benchmarks: [{ cpu_insns }] }` under
 * a single top-level `contract`. Accepting both mirrors what
 * `gas_benchmark_report.py:56-63` already does, so a contributor is not forced
 * to convert a file by hand.
 */
export function parseMeasurements(source: string, fallbackContract = "unknown"): MeasurementSet {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source);
  } catch (err) {
    throw new Error(`not valid JSON: ${(err as Error).message}`);
  }
  if (!parsed || typeof parsed !== "object") {
    throw new Error("expected a JSON object");
  }
  const object = parsed as Record<string, unknown>;

  const build = (
    row: Record<string, unknown>,
    contract: string,
    scenario: string,
    timestamp: string | null,
  ): Measurement => {
    // `lending::get_health_factor` carries its own contract, so the row's
    // `contract` field is absent and the top-level fallback is only right for
    // files like `gas-baseline.json` that put the contract at the top level.
    // `splitId` owns that rule, so parse it here rather than restating it.
    const operation = String(row.operation ?? row.name ?? "");
    const parsed = splitId(operation);
    const explicit = String(row.contract ?? "");
    const contractKey = explicit || (parsed.contract === "" ? contract : parsed.contract);
    const fn = parsed.contract === "" ? operation : parsed.fn;
    // Normalise the contract once, so the canonical id and the field agree:
    // `hello-world` is `hello_world` in every op name in this repository.
    const contractName = contractKey.replace(/-/g, "_");
    const id = scenario ? `${contractName}::${fn} [${scenario}]` : `${contractName}::${fn}`;
    const instructions = Number(row.instructions ?? row.cpu_insns ?? row.cpu ?? 0);
    const memoryBytes = Number(row.memory_bytes ?? row.mem_bytes ?? row.memory ?? 0);
    return {
      id,
      contract: contractName,
      fn,
      scenario,
      instructions: Number.isFinite(instructions) ? instructions : 0,
      memoryBytes: Number.isFinite(memoryBytes) ? memoryBytes : 0,
      declaredStorageReads: Number(row.storage_reads ?? 0),
      declaredStorageWrites: Number(row.storage_writes ?? 0),
      declaredColdStorage: row.cold_storage === true,
      timestamp: typeof row.timestamp === "string" ? row.timestamp : timestamp,
      commit: typeof row.git_commit === "string" ? row.git_commit : null,
    };
  };

  const reportedAt = typeof object.timestamp === "string" ? object.timestamp : null;

  if (Array.isArray(object.results)) {
    return {
      measurements: (object.results as Record<string, unknown>[]).map((row) =>
        build(row, fallbackContract, String(row.scenario ?? ""), reportedAt),
      ),
      reportedAt,
      source: "",
      shape: "benchmark-report",
    };
  }

  if (Array.isArray(object.benchmarks)) {
    const contract = String(object.contract ?? fallbackContract);
    return {
      measurements: (object.benchmarks as Record<string, unknown>[]).map((row) =>
        build(row, contract, String(row.scenario ?? ""), reportedAt),
      ),
      reportedAt,
      source: "",
      shape: "gas-baseline",
    };
  }

  return { measurements: [], reportedAt, source: "", shape: "unknown" };
}

/** Read and normalise a measurement file. */
export function loadMeasurements(file: string): MeasurementSet {
  const text = fs.readFileSync(file, "utf8");
  const set = parseMeasurements(text);
  return { ...set, source: path.basename(file) };
}

/** Index measurements by canonical id. */
export function indexById(set: MeasurementSet): Map<string, Measurement> {
  const out = new Map<string, Measurement>();
  for (const measurement of set.measurements) {
    if (!out.has(measurement.id)) out.set(measurement.id, measurement);
  }
  return out;
}

/**
 * Integrity findings for a measurement set against a course.
 *
 * These are the checks that stop a leaderboard from rendering fiction. Each one
 * closes a specific hole the repository's current data state leaves open.
 */
export function checkIntegrity(
  set: MeasurementSet,
  required: { id: string }[],
  options: { requireFresh?: boolean } = {},
): IntegrityFinding[] {
  const findings: IntegrityFinding[] = [];

  if (set.measurements.length === 0) {
    findings.push({
      reason: set.shape === "unknown" ? "unreadable" : "empty",
      id: "",
      detail:
        set.shape === "unknown"
          ? `${set.source} has neither a "results" nor a "benchmarks" array; nothing to score.`
          : `${set.source} contains no measurements.`,
    });
    return findings;
  }

  // A `0` instruction count is not a fast submission, it is an unmeasured one.
  // `coverage_failures` in `gas_benchmark_report.py` is set-membership only, so
  // a file of zeroes passes every existing gate in the repository.
  for (const measurement of set.measurements) {
    if (measurement.instructions === 0) {
      findings.push({
        reason: "zero-instructions",
        id: measurement.id,
        detail: `${measurement.id} reports 0 instructions, which means unmeasured rather than free.`,
      });
    }
  }

  const requiredIds = new Set(required.map((r) => r.id));
  for (const measurement of set.measurements) {
    if (!requiredIds.has(measurement.id)) {
      findings.push({
        reason: "not-required",
        id: measurement.id,
        detail: `${measurement.id} is not in public-functions.json, so it is not an open target.`,
      });
    }
  }

  if (options.requireFresh) {
    const commits = new Set(set.measurements.map((m) => m.commit).filter((c): c is string => Boolean(c)));
    if (commits.size > 1) {
      findings.push({
        reason: "stale-pairing",
        id: "",
        detail:
          `measurements come from ${commits.size} different commits (${[...commits].sort().join(", ")}); ` +
          "a score is only meaningful within a single session.",
      });
    }
  }

  return findings;
}

/** Findings that mean "this set cannot be scored at all". */
export const BLOCKING_REASONS: readonly IntegrityReason[] = [
  "empty",
  "unreadable",
  "zero-instructions",
  "stale-pairing",
];

/** True when at least one finding blocks scoring. */
export function hasBlockingFinding(findings: IntegrityFinding[]): boolean {
  return findings.some((f) => BLOCKING_REASONS.includes(f.reason));
}
