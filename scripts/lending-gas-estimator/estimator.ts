/**
 * Gas cost model for the lending pool (#1011).
 *
 * The defaults mirror the constants the API already uses in
 * `api/src/services/gas/estimator.ts` (`BASE_FEE`, `STORAGE_WRITE_COST`,
 * `CROSS_CONTRACT_CALL_COST`) so that a number produced here can be compared
 * directly against `/api/gas/estimate`. Everything is overridable through
 * `CostModel` because these are *estimation* inputs, not protocol constants.
 *
 * Soroban actually charges for CPU instructions and memory bytes; this model
 * prices the storage footprint explicitly (which is what the issue asks for)
 * and folds a measured CPU/memory baseline in as the resource term.
 */

import type {
  CostBreakdown,
  CostModel,
  EstimateReport,
  OperationEstimate,
  OperationFacts,
  ResourceBaseline,
  StorageAccess,
  StorageSummary,
} from "./types.ts";
import { buildSuggestions, type SuggestionOptions } from "./suggestions.ts";
import { SEVERITY_ORDER, type Severity } from "./types.ts";

/**
 * 1 XLM = 10,000,000 stroops.
 */
export const STROOPS_PER_XLM = 10_000_000;

export const DEFAULT_COST_MODEL: CostModel = {
  baseFeeStroops: 100,
  storageWriteStroops: 10_000,
  storageReadStroops: 1_000,
  storageExistsStroops: 500,
  storageRemoveStroops: 1_000,
  temporaryAccessStroops: 20_000,
  crossContractStroops: 5_000,
  cpuStroopDivisor: 100,
  memoryStroopDivisor: 100,
};

export interface EstimateOptions {
  model?: Partial<CostModel>;
  /** Multiplier applied to accesses flagged `perIteration` (batch sizing). */
  iterations?: number;
  /** Measured CPU/memory baselines keyed by operation name. */
  baselines?: Record<string, ResourceBaseline>;
  /** Hand-maintained table to diff the static truth against. */
  complexityTable?: ComplexityTable | null;
  /** Per-operation filters passed through to the suggestion rules. */
  suggestions?: Partial<SuggestionOptions>;
  contract?: string;
  sourceDir?: string;
}

/** Counts per kind, keyed the same way for the ledger and scratch tiers. */
interface KindCounts {
  reads: number;
  writes: number;
  exists: number;
  removes: number;
}

const KIND_BUCKET: Record<StorageAccess["kind"], keyof KindCounts> = {
  read: "reads",
  write: "writes",
  exists: "exists",
  remove: "removes",
};

/** Collapse a set of per-iteration accesses into a footprint summary. */
export function summariseStorage(facts: OperationFacts, iterations = 1): StorageSummary {
  const entries = new Set<string>();
  const written = new Set<string>();
  let estimatedWriteBytes: number | null = 0;

  const ledger: KindCounts = { reads: 0, writes: 0, exists: 0, removes: 0 };
  const scratch: KindCounts = { reads: 0, writes: 0, exists: 0, removes: 0 };
  const tierTotals = { persistent: 0, instance: 0, temporary: 0 };
  const kindTotals: KindCounts = { reads: 0, writes: 0, exists: 0, removes: 0 };

  for (const access of facts.accesses) {
    const repeat = access.perIteration ? Math.max(1, iterations) : 1;
    const bucket = KIND_BUCKET[access.kind];

    (access.tier === "temporary" ? scratch : ledger)[bucket] += repeat;
    kindTotals[bucket] += repeat;
    tierTotals[access.tier] += repeat;

    if (access.kind === "write") {
      written.add(`${access.tier}::${access.key}`);
      if (access.entryBytes === null) estimatedWriteBytes = null;
      else if (estimatedWriteBytes !== null) estimatedWriteBytes += access.entryBytes * repeat;
    }
    entries.add(`${access.tier}::${access.key}`);
  }

  return {
    reads: kindTotals.reads,
    writes: kindTotals.writes,
    exists: kindTotals.exists,
    removes: kindTotals.removes,
    persistent: tierTotals.persistent,
    instance: tierTotals.instance,
    temporary: tierTotals.temporary,
    ledger,
    scratch,
    footprintEntries: entries.size,
    writtenEntries: written.size,
    estimatedWriteBytes,
  };
}

/** Price one operation's storage footprint. */
export function priceOperation(
  storage: StorageSummary,
  facts: OperationFacts,
  model: CostModel,
): CostBreakdown {
  const storageWriteStroops = storage.ledger.writes * model.storageWriteStroops;
  const storageReadStroops = storage.ledger.reads * model.storageReadStroops;
  const storageExistsStroops = storage.ledger.exists * model.storageExistsStroops;
  const storageRemoveStroops = storage.ledger.removes * model.storageRemoveStroops;

  const baseFeeStroops = model.baseFeeStroops;
  const crossContractStroops = facts.crossContractCalls.length * model.crossContractStroops;
  const scratchAccesses =
    storage.scratch.reads + storage.scratch.writes + storage.scratch.exists + storage.scratch.removes;
  const resourceStroops = 0; // filled in by `estimateOperation` from the baseline

  const totalStroops =
    baseFeeStroops +
    storageWriteStroops +
    storageReadStroops +
    storageExistsStroops +
    storageRemoveStroops +
    scratchAccesses * model.temporaryAccessStroops +
    crossContractStroops +
    resourceStroops;

  return {
    baseFeeStroops,
    storageWriteStroops,
    storageReadStroops,
    storageExistsStroops,
    storageRemoveStroops,
    crossContractStroops,
    resourceStroops,
    totalStroops,
    totalXlm: totalStroops / STROOPS_PER_XLM,
  };
}

const EMPTY_BASELINE: ResourceBaseline = {
  cpuInstructions: null,
  memoryBytes: null,
  source: "none",
};

/** Price a single operation end to end. */
export function estimateOperation(
  facts: OperationFacts,
  options: EstimateOptions = {},
): OperationEstimate {
  const model: CostModel = { ...DEFAULT_COST_MODEL, ...options.model };
  const iterations = Math.max(1, Math.trunc(options.iterations ?? 1));
  const storage = summariseStorage(facts, iterations);
  const baseline = (options.baselines ? baselineFor(options.baselines, facts.operation) : undefined) ?? EMPTY_BASELINE;

  const cost = priceOperation(storage, facts, model);
  const cpuStroops = baseline.cpuInstructions ? Math.floor(baseline.cpuInstructions / model.cpuStroopDivisor) : 0;
  const memStroops = baseline.memoryBytes ? Math.floor(baseline.memoryBytes / model.memoryStroopDivisor) : 0;
  cost.resourceStroops = cpuStroops + memStroops;
  cost.totalStroops += cost.resourceStroops;
  cost.totalXlm = cost.totalStroops / STROOPS_PER_XLM;

  return { operation: facts.operation, entryPoint: facts.entryPoint, facts, storage, cost, baseline };
}

/** Price every operation and assemble the full report. */
export function buildReport(
  factsList: OperationFacts[],
  options: EstimateOptions = {},
): EstimateReport {
  const model: CostModel = { ...DEFAULT_COST_MODEL, ...options.model };
  const iterations = Math.max(1, Math.trunc(options.iterations ?? 1));

  const operations = factsList
    .map((facts) => estimateOperation(facts, { ...options, model, iterations }))
    .sort((a, b) => b.cost.totalStroops - a.cost.totalStroops);

  const suggestions = buildSuggestions(operations, { model, iterations, ...options.suggestions });
  const drift = auditComplexityTable(operations, options.complexityTable);

  const severities = suggestions.map((s) => s.severity);
  const worstSeverity = severities.length
    ? severities.reduce<Severity>((worst, s) => (SEVERITY_ORDER[s] > SEVERITY_ORDER[worst] ? s : worst), "info")
    : null;

  return {
    contract: options.contract ?? "lending",
    sourceDir: options.sourceDir ?? "",
    model,
    iterations,
    operations,
    suggestions,
    drift,
    caveats: buildCaveats(operations, drift),
    totals: {
      operations: operations.length,
      reads: operations.reduce((n, o) => n + o.storage.reads, 0),
      writes: operations.reduce((n, o) => n + o.storage.writes, 0),
      footprintEntries: operations.reduce((n, o) => n + o.storage.footprintEntries, 0),
      totalStroops: operations.reduce((n, o) => n + o.cost.totalStroops, 0),
      suggestions: suggestions.length,
      worstSeverity,
    },
  };
}

/** Hand-maintained `{ operation: { storageWrites, crossContractCalls } }` table. */
export type ComplexityTable = Record<string, { storageWrites: number; crossContractCalls: number }>;

/**
 * Parse the `OPERATION_COMPLEXITY` constant out of
 * `api/src/services/gas/estimator.ts` so the static truth can be diffed against
 * the values the API estimates with. Returns `null` when the shape is not found
 * (the drift check is then simply skipped).
 */
export function parseComplexityTable(source: string): ComplexityTable | null {
  const blockMatch = /OPERATION_COMPLEXITY[^=]*=\s*\{([\s\S]*?)\n\}/.exec(source);
  if (!blockMatch) return null;
  const table: ComplexityTable = {};
  const rowRe = /([A-Za-z_][A-Za-z0-9_]*)\s*:\s*\{\s*storageWrites\s*:\s*(\d+)\s*,\s*crossContractCalls\s*:\s*(\d+)\s*\}/g;
  for (let m = rowRe.exec(blockMatch[1]); m; m = rowRe.exec(blockMatch[1])) {
    table[m[1]] = { storageWrites: Number(m[2]), crossContractCalls: Number(m[3]) };
  }
  return Object.keys(table).length > 0 ? table : null;
}

/**
 * Read the stroop constants out of `api/src/services/gas/estimator.ts` so this
 * tool shares a single source of truth with `/api/gas/estimate` instead of
 * hard-coding a second copy that can drift.
 *
 * Only the three constants the API actually defines are adopted; the read /
 * exists / remove / temporary prices stay as the model's own defaults.
 */
export function parseSharedCostConstants(source: string): Partial<CostModel> {
  const read = (name: string): number | undefined => {
    const m = new RegExp(`const\\s+${name}\\s*=\\s*['"](\\d+)['"]`).exec(source);
    return m ? Number(m[1]) : undefined;
  };
  const baseFee = read("BASE_FEE");
  const write = read("STORAGE_WRITE_COST");
  const crossContract = read("CROSS_CONTRACT_CALL_COST");
  return {
    ...(baseFee === undefined ? {} : { baseFeeStroops: baseFee }),
    ...(write === undefined ? {} : { storageWriteStroops: write }),
    ...(crossContract === undefined ? {} : { crossContractStroops: crossContract }),
  };
}

/** Compare indexed write counts against a hand-maintained complexity table. */
export function auditComplexityTable(
  operations: OperationEstimate[],
  table: ComplexityTable | null | undefined,
): EstimateReport["drift"] {
  if (!table) return [];
  const findings: EstimateReport["drift"] = [];
  for (const op of operations) {
    const expected = table[op.operation];
    if (!expected) continue;
    findings.push({
      operation: op.operation,
      tableWrites: expected.storageWrites,
      indexedWrites: op.storage.writes,
      delta: op.storage.writes - expected.storageWrites,
      tableCrossContractCalls: expected.crossContractCalls,
      indexedCrossContractCalls: op.facts.crossContractCalls.length,
    });
  }
  return findings.filter((f) => f.delta !== 0 || f.tableCrossContractCalls !== f.indexedCrossContractCalls);
}

function bar(value: number, max: number, width = 20): string {
  if (max <= 0) return "";
  const filled = Math.max(1, Math.round((value / max) * width));
  return "█".repeat(filled);
}

function formatStroops(value: number): string {
  return value.toLocaleString("en-US");
}

/**
 * The benchmark suite and the lending contract use different names for the same
 * operations (`benchmarks/gas-baseline.json` was recorded against the
 * `hello-world` contract). This maps a contract entry point to the benchmark
 * operation it is measured as, so `--baselines` can combine measured CPU/memory
 * with the storage cost this tool computes.
 */
export const BASELINE_NAME_ALIASES: Record<string, string> = {
  deposit: "deposit_collateral",
  deposit_collateral: "deposit_collateral",
  borrow: "borrow_asset",
  borrow_with_rate: "borrow_asset",
  repay: "repay_debt",
  sweep_debt_dust: "repay_debt",
  withdraw: "withdraw_collateral",
  emergency_withdraw: "withdraw_collateral",
  sweep_deposit_dust: "withdraw_collateral",
  flash_loan: "execute_flash_loan",
  liquidate: "liquidate",
};

/**
 * Resolve the baseline for an operation, falling back to the benchmark
 * vocabulary. Exact matches always win over aliases.
 */
export function baselineFor(
  baselines: Record<string, ResourceBaseline>,
  operation: string,
): ResourceBaseline | undefined {
  if (baselines[operation]) return baselines[operation];
  const alias = BASELINE_NAME_ALIASES[operation];
  return alias ? baselines[alias] : undefined;
}

/**
 * Read measured CPU/memory baselines.
 *
 * Accepts either a plain `{ operation: { cpuInstructions, memoryBytes } }` map or
 * the shape committed at `stellar-lend/benchmarks/gas-baseline.json`
 * (`{ benchmarks: [{ operation, cpu_insns, mem_bytes }] }`), so the measured
 * numbers already in the repo can be fed in without a conversion step.
 *
 * When several rows share an operation name, the median is used — the benchmark
 * suite records cold and warm scenarios for the same call.
 */
export function parseBaselines(
  source: string,
  origin: string,
): Record<string, ResourceBaseline> {
  const raw = JSON.parse(source) as unknown;
  const rows: { operation: string; cpu: number | null; memory: number | null }[] = [];

  const takeRow = (operation: unknown, cpu: unknown, memory: unknown): void => {
    if (typeof operation !== "string") return;
    rows.push({
      operation,
      cpu: typeof cpu === "number" ? cpu : null,
      memory: typeof memory === "number" ? memory : null,
    });
  };

  if (Array.isArray(raw)) {
    for (const entry of raw as Record<string, unknown>[]) {
      takeRow(entry?.operation, entry?.cpuInstructions ?? entry?.cpu_insns, entry?.memoryBytes ?? entry?.mem_bytes);
    }
  } else if (raw && typeof raw === "object") {
    const object = raw as Record<string, unknown>;
    if (Array.isArray(object.benchmarks)) {
      for (const entry of object.benchmarks as Record<string, unknown>[]) {
        takeRow(entry?.operation, entry?.cpu_insns, entry?.mem_bytes);
      }
    } else {
      for (const [operation, value] of Object.entries(object)) {
        if (!value || typeof value !== "object") continue;
        const entry = value as Record<string, unknown>;
        takeRow(operation, entry.cpuInstructions ?? entry.cpu_insns, entry.memoryBytes ?? entry.mem_bytes);
      }
    }
  }

  const byOperation = new Map<string, typeof rows>();
  for (const row of rows) {
    const list = byOperation.get(row.operation) ?? [];
    list.push(row);
    byOperation.set(row.operation, list);
  }

  const median = (values: number[]): number | null => {
    if (values.length === 0) return null;
    const sorted = [...values].sort((a, b) => a - b);
    const middle = Math.floor(sorted.length / 2);
    return sorted.length % 2 === 1 ? sorted[middle] : Math.floor((sorted[middle - 1] + sorted[middle]) / 2);
  };

  const out: Record<string, ResourceBaseline> = {};
  for (const [operation, list] of byOperation) {
    out[operation] = {
      cpuInstructions: median(list.map((r) => r.cpu).filter((v): v is number => v !== null)),
      memoryBytes: median(list.map((r) => r.memory).filter((v): v is number => v !== null)),
      source: origin,
    };
  }
  return out;
}

/**
 * How to read the numbers in this report.
 *
 * Static analysis counts call sites, so a few counts are upper bounds rather
 * than exact. Saying so is cheaper than a reader assuming precision that is not
 * there, and it keeps the report honest about what "estimate" means here.
 */
export function buildCaveats(operations: OperationEstimate[], drift: EstimateReport["drift"]): string[] {
  const caveats: string[] = [
    "Counts are static call sites. Accesses in mutually exclusive branches are both counted, so read and write counts are upper bounds; the distinct-entry footprint is exact.",
    "A helper called from two places is priced twice, matching what the runtime does. Recursive call paths are cut to keep the walk finite.",
    "Storage accessed through a variable key (e.g. `&key`) is reported as `key` with no namespace, so it is never proposed as a packing candidate.",
  ];

  const unresolved = operations.filter((o) => o.facts.unresolvedCalls.length > 0);
  if (unresolved.length > 0) {
    caveats.push(
      `${unresolved.length} operation(s) call helpers defined outside the scanned tree ` +
        "(token transfers, the reentrancy guard, other crates); their storage cost is not included.",
    );
  }

  const looped = operations.filter((o) => o.facts.hasLoop);
  if (looped.length > 0) {
    caveats.push(
      `${looped.length} operation(s) contain a loop. Accesses inside a loop body are flagged ` +
        "`perIteration` and only multiplied by `--iterations`, so per-iteration savings are what the loop-amplification rule reports.",
    );
  }

  if (drift.length > 0) {
    caveats.push(
      "The API's hand-maintained OPERATION_COMPLEXITY table disagrees with the source; the " +
        "drift table lists the differences so /api/gas/estimate can be corrected.",
    );
  }

  return caveats;
}

/** Human-readable report for a terminal. */
export function renderText(report: EstimateReport): string {
  const lines: string[] = [];
  lines.push(`Lending pool gas estimate — ${report.contract} (${report.totals.operations} operations)`);
  lines.push(`Source: ${report.sourceDir || "n/a"}`);
  lines.push("");
  lines.push("Operation        Reads Writes Rem Ex Foot  X-cross  Cost (stroops)   Cost (XLM)");
  const maxCost = Math.max(1, ...report.operations.map((o) => o.cost.totalStroops));
  for (const op of report.operations) {
    lines.push(
      [
        op.operation.padEnd(16),
        String(op.storage.reads).padStart(5),
        String(op.storage.writes).padStart(6),
        String(op.storage.removes).padStart(3),
        String(op.storage.exists).padStart(2),
        String(op.storage.footprintEntries).padStart(4),
        String(op.facts.crossContractCalls.length).padStart(8),
        formatStroops(op.cost.totalStroops).padStart(15),
        op.cost.totalXlm.toFixed(7).padStart(12),
        `  ${bar(op.cost.totalStroops, maxCost, 12)}`,
      ].join(" "),
    );
  }

  lines.push("");
  lines.push(`Suggestions: ${report.suggestions.length}`);
  for (const suggestion of report.suggestions) {
    lines.push(`  [${suggestion.severity.toUpperCase()}] ${suggestion.id} — ${suggestion.title}`);
    lines.push(`      ${suggestion.operation}: ${suggestion.detail}`);
    for (const item of suggestion.evidence.slice(0, 4)) lines.push(`      - ${item}`);
  }

  lines.push("");
  lines.push("Caveats:");
  for (const caveat of report.caveats) lines.push(`  - ${caveat}`);

  if (report.drift.length > 0) {
    lines.push("");
    lines.push("Complexity table drift (static index vs hand-maintained table):");
    for (const finding of report.drift) {
      lines.push(
        `  ${finding.operation}: writes ${finding.tableWrites} -> ${finding.indexedWrites} ` +
          `(${finding.delta > 0 ? "+" : ""}${finding.delta}), ` +
          `cross-contract ${finding.tableCrossContractCalls} -> ${finding.indexedCrossContractCalls}`,
      );
    }
  }

  return lines.join("\n");
}

/** Markdown report for a PR comment or job summary. */
export function renderMarkdown(report: EstimateReport): string {
  const lines: string[] = [];
  lines.push(`## Lending pool gas estimate — ${report.contract}`);
  lines.push("");
  lines.push(
    `${report.totals.operations} operations · ${report.totals.footprintEntries} footprint entries · ` +
      `${report.totals.suggestions} suggestions` +
      (report.totals.worstSeverity ? ` (worst: ${report.totals.worstSeverity})` : ""),
  );
  lines.push("");
  lines.push("| Operation | Reads | Writes | Removes | Exists | Footprint | Cross-contract | Stroops | XLM |");
  lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
  for (const op of report.operations) {
    lines.push(
      `| \`${op.operation}\` | ${op.storage.reads} | ${op.storage.writes} | ${op.storage.removes} | ` +
        `${op.storage.exists} | ${op.storage.footprintEntries} | ${op.facts.crossContractCalls.length} | ` +
        `${formatStroops(op.cost.totalStroops)} | ${op.cost.totalXlm.toFixed(7)} |`,
    );
  }

  if (report.suggestions.length > 0) {
    lines.push("");
    lines.push("### Storage-pattern optimization suggestions");
    lines.push("");
    for (const suggestion of report.suggestions) {
      lines.push(`#### ${suggestion.title} (${suggestion.severity})`);
      lines.push("");
      lines.push(`- Rule: \`${suggestion.id}\` · category \`${suggestion.category}\``);
      lines.push(`- Operation: \`${suggestion.operation}\``);
      lines.push(`- ${suggestion.detail}`);
      if (suggestion.estimatedSavingStroops !== null) {
        lines.push(`- Estimated saving: ~${formatStroops(suggestion.estimatedSavingStroops)} stroops per call`);
      }
      if (suggestion.evidence.length > 0) {
        lines.push("- Evidence:");
        for (const item of suggestion.evidence) lines.push(`  - ${item}`);
      }
      lines.push("");
    }
  }

  lines.push("");
  lines.push("### Caveats");
  lines.push("");
  for (const caveat of report.caveats) lines.push(`- ${caveat}`);

  if (report.drift.length > 0) {
    lines.push("");
    lines.push("### Complexity table drift");
    lines.push("");
    lines.push("The API's hand-maintained `OPERATION_COMPLEXITY` table no longer matches the contract source.");
    lines.push("");
    lines.push("| Operation | Table writes | Indexed writes | Table cross-contract | Indexed cross-contract |");
    lines.push("| --- | ---: | ---: | ---: | ---: |");
    for (const finding of report.drift) {
      lines.push(
        `| \`${finding.operation}\` | ${finding.tableWrites} | ${finding.indexedWrites} | ` +
          `${finding.tableCrossContractCalls} | ${finding.indexedCrossContractCalls} |`,
      );
    }
  }

  lines.push("");
  lines.push(
    `<sub>Generated by scripts/lending-gas-estimator from ${report.sourceDir || "the contract source"} · ` +
      `cost model: base ${report.model.baseFeeStroops}, write ${report.model.storageWriteStroops}, ` +
      `read ${report.model.storageReadStroops}, cross-contract ${report.model.crossContractStroops} stroops.</sub>`,
  );
  return lines.join("\n");
}
