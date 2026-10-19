/**
 * Cost model for the lending protocol gas budget planner (#1012).
 *
 * The numbers come from the repository, not from constants written here:
 *
 *   - `api/src/services/gas/estimator.ts` supplies the stroop prices
 *     (`BASE_FEE`, `STORAGE_WRITE_COST`, `CROSS_CONTRACT_CALL_COST`), the
 *     per-operation storage/CPU cost of each call (`OPERATION_COMPLEXITY`,
 *     `BASELINE_CPU_COSTS`) and the `cpu / 100` conversion the API's
 *     `calculateBaselineFee` uses. Reading them means a plan produced here
 *     matches what `POST /api/gas/estimate` returns for the same operation,
 *     and the two cannot drift apart silently.
 *   - `stellar-lend/benchmarks/baseline.json` supplies the protocol's committed
 *     CPU-instruction budget per call (`gas_budgets`), so a plan is validated
 *     against the budget CI enforces rather than only against a currency amount.
 *   - `stellar-lend/benchmarks/gas-baseline.json` optionally supplies *measured*
 *     CPU counts to use in place of the API's baselines.
 *
 * Every loader is pure — it takes source text and returns data — so the whole
 * model is testable without touching the filesystem.
 */

import type { OperationCost, PlannedOperation } from "./types.ts";
import { STROOPS_PER_XLM } from "./types.ts";

/** `alias -> `module::name`` table, in the repo's own naming conventions. */
export type ComplexityTable = Record<string, { storageWrites: number; crossContractCalls: number }>;

/** The stroop prices and CPU conversion the API uses. */
export interface CostConstants {
  baseFeeStroops: number;
  storageWriteStroops: number;
  crossContractCallStroops: number;
  /** The API divides the CPU baseline by 100 to get stroops. */
  cpuStroopDivisor: number;
}

/** Everything needed to price a call, loaded from the repository. */
export interface CostData {
  constants: CostConstants;
  complexity: ComplexityTable;
  baselineCpu: Record<string, number>;
  /** `contract::op` → committed CPU-instruction budget. */
  instructionBudgets: Record<string, number>;
  /** Where each input came from, so the report can cite its sources. */
  sources: {
    estimator: string;
    instructionBudgets: string;
    measuredBaselines: string | null;
  };
}

const IDENT = "[A-Za-z_][A-Za-z0-9_]*";

/**
 * Read the cost constants out of `api/src/services/gas/estimator.ts`.
 *
 * A renamed or deleted constant falls back to the value documented here rather
 * than throwing, so the planner keeps working and the report's `sources` block
 * shows where the number came from.
 */
export function parseCostConstants(source: string): CostConstants {
  const read = (name: string, fallback: number): number => {
    const named = new RegExp(`const\\s+${name}\\s*=\\s*['"](\\d+)['"]`).exec(source);
    return named ? Number(named[1]) : fallback;
  };
  return {
    baseFeeStroops: read("BASE_FEE", 100),
    storageWriteStroops: read("STORAGE_WRITE_COST", 10_000),
    crossContractCallStroops: read("CROSS_CONTRACT_CALL_COST", 5_000),
    // Matches the `BigInt(BASELINE_CPU_COSTS[operation]) / BigInt(100)` in
    // `calculateBaselineFee`.
    cpuStroopDivisor: 100,
  };
}

/** Parse `OPERATION_COMPLEXITY` — `{ op: { storageWrites, crossContractCalls } }`. */
export function parseComplexityTable(source: string): ComplexityTable {
  const block = /OPERATION_COMPLEXITY[^=]*=\s*\{([\s\S]*?)\n\}/.exec(source);
  if (!block) return {};
  const table: ComplexityTable = {};
  const rowRe = new RegExp(
    `(${IDENT})\\s*:\\s*\\{\\s*storageWrites\\s*:\\s*(\\d+)\\s*,\\s*crossContractCalls\\s*:\\s*(\\d+)\\s*\\}`,
    "g",
  );
  for (let m = rowRe.exec(block[1]); m; m = rowRe.exec(block[1])) {
    table[m[1]] = { storageWrites: Number(m[2]), crossContractCalls: Number(m[3]) };
  }
  return table;
}

/** Parse `BASELINE_CPU_COSTS` — `{ op: cpuInstructions }`. */
export function parseBaselineCpu(source: string): Record<string, number> {
  const block = /BASELINE_CPU_COSTS[^=]*=\s*\{([\s\S]*?)\n\}/.exec(source);
  if (!block) return {};
  const out: Record<string, number> = {};
  const rowRe = new RegExp(`(${IDENT})\\s*:\\s*(\\d+)`, "g");
  for (let m = rowRe.exec(block[1]); m; m = rowRe.exec(block[1])) {
    out[m[1]] = Number(m[2]);
  }
  return out;
}

/** Parse the `gas_budgets` block of `stellar-lend/benchmarks/baseline.json`. */
export function parseInstructionBudgets(source: string): Record<string, number> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source);
  } catch {
    return {};
  }
  const budgets = (parsed as { gas_budgets?: Record<string, unknown> })?.gas_budgets;
  if (!budgets || typeof budgets !== "object") return {};
  const out: Record<string, number> = {};
  for (const [key, value] of Object.entries(budgets)) {
    if (typeof value === "number") out[key] = value;
  }
  return out;
}

/**
 * Parse measured CPU counts from `stellar-lend/benchmarks/gas-baseline.json`
 * (`{ benchmarks: [{ operation, cpu_insns }] }`).
 *
 * The committed file was recorded against the `hello-world` contract, which
 * names the same operations differently, so the benchmark vocabulary is mapped
 * onto the API's.
 *
 * Where an operation has several measured scenarios the **most expensive** one is
 * used. The scenarios are not all variants of the same work — `liquidate` is
 * recorded as `write` (394,438) and `early_exit_unprofitable` (112,300), which
 * are different code paths, and averaging them understates the real cost by a
 * third. For a budget, under-estimating is the worse failure.
 */
export function parseMeasuredBaselines(
  source: string,
  alias: Record<string, string> = BENCHMARK_VOCABULARY,
): Record<string, number> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source);
  } catch {
    return {};
  }
  const rows = (parsed as { benchmarks?: { operation?: string; cpu_insns?: number }[] })?.benchmarks;
  if (!Array.isArray(rows)) return {};

  const byOperation = new Map<string, number[]>();
  for (const row of rows) {
    if (typeof row?.operation !== "string" || typeof row?.cpu_insns !== "number") continue;
    const list = byOperation.get(row.operation) ?? [];
    list.push(row.cpu_insns);
    byOperation.set(row.operation, list);
  }

  // Invert the alias table: benchmark name -> API operation name.
  const byApiOperation = new Map<string, string[]>();
  for (const [apiOperation, benchmarkName] of Object.entries(alias)) {
    const list = byApiOperation.get(benchmarkName) ?? [];
    list.push(apiOperation);
    byApiOperation.set(benchmarkName, list);
  }

  const out: Record<string, number> = {};
  for (const [benchmarkName, values] of byOperation) {
    const targets = byApiOperation.get(benchmarkName) ?? [benchmarkName];
    for (const target of targets) out[target] = Math.max(...values);
  }
  return out;
}

/**
 * Maps an API operation to the name the benchmark suite records it under.
 * The committed `gas-baseline.json` was measured against the `hello-world`
 * contract, where a deposit is `deposit_collateral` and a borrow is
 * `borrow_asset`.
 */
export const BENCHMARK_VOCABULARY: Record<string, string> = {
  deposit: "deposit_collateral",
  deposit_collateral: "deposit_collateral",
  withdraw: "withdraw_collateral",
  emergency_withdraw: "withdraw_collateral",
  borrow: "borrow_asset",
  repay: "repay_debt",
  liquidation: "liquidate",
  flash_loan: "execute_flash_loan",
};

/**
 * `contract::op` keys in `gas_budgets` for one API operation. The benchmark
 * baseline publishes the lending pool's budgets under `lending::`, so that
 * prefix is preferred over the `hello_world::` ones.
 */
export function instructionBudgetFor(
  budgets: Record<string, number>,
  operation: string,
): number | null {
  const candidates = [
    `lending::${operation}`,
    `lending::${BENCHMARK_VOCABULARY[operation] ?? operation}`,
  ];
  for (const key of candidates) {
    if (typeof budgets[key] === "number") return budgets[key];
  }
  return null;
}

/** Assemble the cost data from the three repository sources. */
export function loadCostData(sources: {
  estimator: string;
  instructionBudgets: string;
  measuredBaselines?: string | null;
  paths: { estimator: string; instructionBudgets: string; measuredBaselines: string | null };
}): CostData {
  const measured = sources.measuredBaselines
    ? parseMeasuredBaselines(sources.measuredBaselines)
    : null;

  return {
    constants: parseCostConstants(sources.estimator),
    complexity: parseComplexityTable(sources.estimator),
    baselineCpu: measured ?? parseBaselineCpu(sources.estimator),
    instructionBudgets: parseInstructionBudgets(sources.instructionBudgets),
    sources: {
      estimator: sources.paths.estimator,
      instructionBudgets: sources.paths.instructionBudgets,
      measuredBaselines: measured && Object.keys(measured).length > 0 ? sources.paths.measuredBaselines : null,
    },
  };
}

/** Price a single call of `operation`. */
export function costOfOperation(
  data: CostData,
  operation: string,
  xlmPriceUsd?: number | null,
): OperationCost | null {
  const complexity = data.complexity[operation];
  if (!complexity) return null;

  const baseFeeStroops = data.constants.baseFeeStroops;
  const storageWriteStroops = complexity.storageWrites * data.constants.storageWriteStroops;
  const crossContractStroops =
    complexity.crossContractCalls * data.constants.crossContractCallStroops;
  const cpuInstructions = data.baselineCpu[operation] ?? null;
  const resourceStroops = cpuInstructions === null ? 0 : Math.floor(cpuInstructions / data.constants.cpuStroopDivisor);
  const totalStroops = baseFeeStroops + storageWriteStroops + crossContractStroops + resourceStroops;

  const instructionBudget = instructionBudgetFor(data.instructionBudgets, operation);

  return {
    operation,
    baseFeeStroops,
    storageWriteStroops,
    crossContractStroops,
    resourceStroops,
    totalStroops,
    totalXlm: totalStroops / STROOPS_PER_XLM,
    totalUsd: xlmPriceUsd === undefined || xlmPriceUsd === null ? null : (totalStroops / STROOPS_PER_XLM) * xlmPriceUsd,
    storageWrites: complexity.storageWrites,
    crossContractCalls: complexity.crossContractCalls,
    cpuInstructions,
    cpuSource: data.sources.measuredBaselines ?? data.sources.estimator,
    instructionBudget,
    budgetUtilisationPct:
      cpuInstructions !== null && instructionBudget !== null && instructionBudget > 0
        ? (cpuInstructions / instructionBudget) * 100
        : null,
  };
}

/** Price every operation the loaded cost table knows about. */
export function costOfAllOperations(data: CostData, xlmPriceUsd?: number | null): OperationCost[] {
  return Object.keys(data.complexity)
    .sort()
    .map((operation) => costOfOperation(data, operation, xlmPriceUsd))
    .filter((cost): cost is OperationCost => cost !== null);
}

/**
 * How many stroops a call costs once its per-item work is amortised across a
 * batch.
 *
 * `deposit_batch` is the case this exists for. The contract's own table
 * (`stellar-lend/contracts/lending/src/deposit_batch.rs`) shows that a batch of
 * N deposits pays one authorization, one reentrancy guard, one pause lookup,
 * one packed deposit-state read/write and one user-position read/write — the
 * same storage footprint as a single `deposit`, whatever N is. So the batch
 * costs one call's worth, and the marginal cost per item is that divided by N.
 *
 * `sharedFraction` is what fraction of a plain call the shared work represents.
 * It is `1` by default; a lender modelling a contract that also does per-item
 * work inside the batch can lower it, and a plan file can set it per operation.
 */
export function amortisedUnitStroops(
  unitStroops: number,
  batchSize: number,
  sharedFraction = 1,
): number {
  if (!batchSize || batchSize <= 1) return unitStroops;
  return (unitStroops * sharedFraction) / batchSize;
}

/**
 * Operations the contract actually exposes a batch entry point for, with the
 * fraction of a plain call that a batch still pays.
 *
 * Only `deposit` is listed: `deposit_batch` is the one batching entry point in
 * the lending pool. The planner does not assume batching exists for the others;
 * a plan file can add an operation to its own `amortisation` map to model a
 * different contract revision.
 */
export const BATCHABLE_BY_DEFAULT: Record<string, number> = {
  deposit: 1,
};

/** The operations a lender's interaction pattern is expected to use. */
export const PLANNABLE_OPERATIONS: PlannedOperation[] = [
  "deposit",
  "withdraw",
  "borrow",
  "repay",
  "liquidation",
  "flash_loan",
  "emergency_withdraw",
];
