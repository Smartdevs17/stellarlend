/**
 * Budget planning for a lender's interaction pattern (#1012).
 *
 * `planBudget` takes what a lender expects to do and produces the gas budget
 * that implies: the per-call cost of each operation, the pattern's total per
 * period and annualised, how that sits against a budget, what happens as volume
 * grows, and which levers would reduce it.
 *
 * The rules are all derived from the plan's own numbers — concentration,
 * amortisation, the protocol's committed instruction budgets — rather than from
 * fixed advice, so a pattern with a different shape gets different suggestions.
 */

import {
  BATCHABLE_BY_DEFAULT,
  amortisedUnitStroops,
  costOfAllOperations,
  costOfOperation,
  type CostData,
} from "./cost-model.ts";
import type {
  BudgetCheck,
  GasBudgetPlan,
  InteractionPattern,
  OperationCost,
  PlanSuggestion,
  PlannedLine,
  Projection,
} from "./types.ts";
import { STROOPS_PER_XLM } from "./types.ts";

/** Utilisation at or above this fraction of the budget counts as tight. */
export const TIGHT_BUDGET_PCT = 85;

/** Share of the plan's cost above which one operation counts as concentrated. */
export const CONCENTRATION_PCT = 40;

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

function round4(value: number): number {
  return Math.round(value * 10_000) / 10_000;
}

/** `plural(2, "entry", "entries")` -> `"2 entries"`. */
function plural(count: number, singular: string, pluralForm = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : pluralForm}`;
}

/** Order operations by cost so the report reads hottest-first. */
function byCostDescending(costs: OperationCost[]): OperationCost[] {
  return [...costs].sort((a, b) => b.totalStroops - a.totalStroops);
}

/**
 * Turn a pattern's call counts into priced lines.
 *
 * Operations the cost table does not know about are reported as unknown rather
 * than priced at zero, so a typo in a plan file cannot silently understate the
 * budget.
 */
export function planLines(
  pattern: InteractionPattern,
  data: CostData,
  xlmPriceUsd?: number | null,
): { lines: PlannedLine[]; unknownOperations: string[] } {
  const batchSize = pattern.batchSize ?? 1;
  const lines: PlannedLine[] = [];
  const unknownOperations: string[] = [];

  const subtotals: number[] = [];
  for (const [operation, callsRaw] of Object.entries(pattern.operations)) {
    const calls = Math.max(0, Math.trunc(callsRaw));
    if (calls === 0) continue;
    const cost = costOfOperation(data, operation, xlmPriceUsd);
    if (!cost) {
      unknownOperations.push(operation);
      continue;
    }

    // Only operations the contract actually exposes a batch entry point for are
    // amortised, plus anything the plan explicitly opted in. Without a
    // `batchSize` every call costs a full operation.
    const sharedFraction = pattern.amortisation?.[operation] ?? BATCHABLE_BY_DEFAULT[operation];
    const unit =
      sharedFraction === undefined
        ? cost.totalStroops
        : amortisedUnitStroops(cost.totalStroops, batchSize, sharedFraction);
    const subtotalStroops = Math.round(unit * calls);
    subtotals.push(subtotalStroops);
    lines.push({
      operation,
      calls,
      unitStroops: cost.totalStroops,
      amortisedUnitStroops: round4(unit),
      subtotalStroops,
      sharePct: 0,
    });
  }

  const grandTotal = subtotals.reduce((sum, value) => sum + value, 0);
  for (const line of lines) {
    line.sharePct = grandTotal > 0 ? round2((line.subtotalStroops / grandTotal) * 100) : 0;
  }

  lines.sort((a, b) => b.subtotalStroops - a.subtotalStroops);
  return { lines, unknownOperations };
}

/** Compare the plan's total against a budget. */
export function checkBudget(totalStroops: number, budgetStroops?: number | null): BudgetCheck {
  if (budgetStroops === undefined || budgetStroops === null || budgetStroops <= 0) {
    return {
      budgetStroops: 0,
      totalStroops,
      utilisationPct: null,
      headroomStroops: null,
      status: "unset",
    };
  }
  const utilisationPct = round2((totalStroops / budgetStroops) * 100);
  return {
    budgetStroops,
    totalStroops,
    utilisationPct,
    headroomStroops: budgetStroops - totalStroops,
    status: totalStroops > budgetStroops ? "over" : utilisationPct >= TIGHT_BUDGET_PCT ? "tight" : "within",
  };
}

/** Project the plan at higher volumes, so a lender can see where the budget breaks. */
export function projectVolumes(
  totalStroops: number,
  budgetStroops: number | null,
  multipliers: number[],
): Projection[] {
  return multipliers
    .filter((m) => Number.isFinite(m) && m > 0)
    .sort((a, b) => a - b)
    .map((multiplier) => {
      const scaled = Math.round(totalStroops * multiplier);
      const check = checkBudget(scaled, budgetStroops);
      return {
        multiplier,
        totalStroops: scaled,
        totalXlm: scaled / STROOPS_PER_XLM,
        utilisationPct: check.utilisationPct,
        status: check.status,
      };
    });
}

// ── Rules ───────────────────────────────────────────────────────────────────

/** What a rule may need beyond the plan it is judging. */
export interface RuleContext {
  data: CostData;
  pattern: InteractionPattern;
}


const SEVERITY_ORDER = { info: 0, low: 1, medium: 2, high: 3 } as const;

/**
 * Batching: a pattern with many deposits pays shared work repeatedly. Price the
 * same deposits through `deposit_batch` and report the difference.
 */
function ruleBatching(plan: GasBudgetPlan, ctx: RuleContext): PlanSuggestion[] {
  const batchSize = ctx.pattern.batchSize ?? 1;
  if (batchSize <= 1) return [];

  const depositLine = plan.lines.find((line) => line.operation === "deposit");
  if (!depositLine) return [];

  // The line is already priced as if batched, so the saving is measured against
  // the same calls priced one-by-one.
  const unbatched = depositLine.unitStroops * depositLine.calls;
  const saving = unbatched - depositLine.subtotalStroops;
  if (saving <= 0) return [];

  const totalStroops = plan.totals.totalStroops;
  const sharePct = totalStroops > 0 ? round2((saving / totalStroops) * 100) : 0;

  return [
    {
      id: "batch-deposits",
      category: "batching",
      severity: sharePct >= 25 ? "high" : sharePct >= 10 ? "medium" : "low",
      title: `This plan already batches ${depositLine.calls} deposit(s), saving ~${saving.toLocaleString("en-US")} stroops`,
      detail:
        `At a batch size of ${batchSize} each deposit costs ` +
        `${round4(depositLine.amortisedUnitStroops)} stroops instead of ` +
        `${depositLine.unitStroops.toLocaleString("en-US")}. ` +
        `${depositLine.calls} unbatched deposits would cost ` +
        `${unbatched.toLocaleString("en-US")} stroops against ` +
        `${depositLine.subtotalStroops.toLocaleString("en-US")} batched — ` +
        `${sharePct}% of this plan. The contract pays one authorization, one reentrancy guard, ` +
        `one pause lookup, one packed deposit-state read/write and one user-position read/write ` +
        `for the whole batch (stellar-lend/contracts/lending/src/deposit_batch.rs), capped at ` +
        `MAX_BATCH_DEPOSITS. Raising the batch size cuts the per-deposit cost further; dropping ` +
        `batchSize from the plan prices them one at a time.`,
      estimatedDeltaStroops: -saving,
    },
  ];
}

/**
 * Concentration: one operation dominating the budget is the first thing to
 * attack, and the report says which one and what makes it expensive.
 */
function ruleConcentration(plan: GasBudgetPlan, ctx: RuleContext): PlanSuggestion[] {
  const top = plan.lines[0];
  if (!top || top.sharePct < CONCENTRATION_PCT) return [];

  const cost = costOfOperation(ctx.data, top.operation);
  if (!cost) return [];

  const writesShare =
    cost.totalStroops > 0
      ? round2((cost.storageWriteStroops / cost.totalStroops) * 100)
      : 0;

  return [
    {
      id: "concentrated-operation",
      category: "concentration",
      severity: top.sharePct >= 60 ? "high" : "medium",
      title: `${round2(top.sharePct)}% of this plan's cost is \`${top.operation}\``,
      detail:
        `One operation carries most of the plan, so optimising anything else will not move the ` +
        `number. Each \`${top.operation}\` call writes ${plural(cost.storageWrites, "persistent entry", "persistent entries")} ` +
        `and makes ${plural(cost.crossContractCalls, "cross-contract call")}; storage writes ` +
        `are ${writesShare}% of its ${cost.totalStroops.toLocaleString("en-US")} stroops. ` +
        `Packing those writes into a single \`#[contracttype]\` struct, or re-checking how often ` +
        `the call is genuinely needed, is where the saving is.`,
      estimatedDeltaStroops: 0,
    },
  ];
}

/**
 * Storage writes: an operation whose cost is mostly writes is a packing
 * candidate, which is a contract change rather than a lender behaviour change.
 *
 * Every operation in this cost model is partly write-dominated, so the rule only
 * reports the ones whose write cost is **material to this plan** — otherwise it
 * would fire on every line and say nothing. All figures are derived from the
 * line's *amortised* unit cost, so a batched operation is not credited with
 * writes it does not perform.
 */
const STORAGE_WRITE_IMPACT_PCT = 15;

function ruleStorageWrites(plan: GasBudgetPlan): PlanSuggestion[] {
  const totalStroops = plan.totals.totalStroops;
  if (totalStroops <= 0) return [];

  return plan.lines
    .map((line) => {
      const cost = plan.costs.find((c) => c.operation === line.operation);
      if (!cost || cost.totalStroops === 0 || cost.storageWrites < 2) return null;

      // The write share of a call is fixed; applied to the amortised unit cost
      // it gives the write cost this plan actually pays.
      const writesShare = cost.storageWriteStroops / cost.totalStroops;
      const writeCostPerCall = line.amortisedUnitStroops * writesShare;
      const writeCost = writeCostPerCall * line.calls;
      // Packing N writes into one entry removes N-1 of them.
      const saving = writeCost * ((cost.storageWrites - 1) / cost.storageWrites);

      return {
        line,
        cost,
        writeCost,
        saving: Math.round(saving),
        impactPct: round2((writeCost / totalStroops) * 100),
        writesSharePct: round2(writesShare * 100),
      };
    })
    .filter((entry): entry is NonNullable<typeof entry> => entry !== null)
    .filter((entry) => entry.impactPct >= STORAGE_WRITE_IMPACT_PCT && entry.saving > 0)
    .sort((a, b) => b.writeCost - a.writeCost)
    .map(({ line, cost, writeCost, saving, impactPct, writesSharePct }) => ({
      id: "storage-write-dominated",
      category: "storage-writes" as const,
      severity: (impactPct >= 40 ? "high" : "medium") as PlanSuggestion["severity"],
      title: `\`${line.operation}\` spends ${impactPct}% of this plan on storage writes`,
      detail:
        `Storage writes are ${writesSharePct}% of each \`${line.operation}\` call, ` +
        `${writeCost.toLocaleString("en-US")} stroops of this plan's ` +
        `${totalStroops.toLocaleString("en-US")}. Every persistent entry is a separate ledger ` +
        `key with its own write fee and footprint slot, so packing the ` +
        `${plural(cost.storageWrites, "write")} into one entry would save up to ` +
        `${saving.toLocaleString("en-US")} stroops per period. This is a contract change, not a ` +
        `lender behaviour change.`,
      estimatedDeltaStroops: -saving,
    }));
}

/**
 * Growth: a plan that fits today may not fit at volume, and the lender should
 * know the multiplier at which it breaks.
 */
function ruleGrowth(plan: GasBudgetPlan): PlanSuggestion[] {
  const budget = plan.budget;
  if (budget.status === "unset" || budget.budgetStroops <= 0) return [];
  if (plan.totals.totalStroops <= 0) return [];

  // The multiplier at which the budget is exactly exhausted, and the first
  // tested multiplier that actually exceeds it.
  const exhaustion = round2(budget.budgetStroops / plan.totals.totalStroops);
  const nextProjection = plan.projections.find((p) => p.status === "over");
  if (budget.status !== "over" && nextProjection === undefined && exhaustion > 2) return [];

  return [
    {
      id: "budget-growth-limit",
      category: "growth",
      severity: budget.status === "over" ? "high" : "medium",
      title:
        budget.status === "over"
          ? `This plan is already ${budget.utilisationPct}% of the budget`
          : `The budget is exhausted at ~${exhaustion}x the planned volume`,
      detail:
        budget.status === "over"
          ? `The plan costs ${plan.totals.totalStroops.toLocaleString("en-US")} stroops against a ` +
            `budget of ${budget.budgetStroops.toLocaleString("en-US")}. Either raise the budget or ` +
            `move volume into batched operations.`
          : `At ${plan.totals.totalStroops.toLocaleString("en-US")} stroops per period, the budget of ` +
            `${budget.budgetStroops.toLocaleString("en-US")} stroops runs out at roughly ` +
            `${exhaustion}x the planned volume` +
            (nextProjection
              ? `, and the plan is over budget by the ${nextProjection.multiplier}x projection.`
              : "."),
      estimatedDeltaStroops: 0,
    },
  ];
}

/**
 * Headroom: when a plan uses a small slice of its budget, say so — it is useful
 * information, not a warning to manufacture.
 */
function ruleHeadroom(plan: GasBudgetPlan): PlanSuggestion[] {
  const budget = plan.budget;
  if (budget.status !== "within" || budget.utilisationPct === null) return [];
  if (budget.utilisationPct > 50) return [];

  return [
    {
      id: "ample-headroom",
      category: "headroom",
      severity: "info",
      title: `The plan uses ${budget.utilisationPct}% of the budget`,
      detail:
        `${budget.headroomStroops?.toLocaleString("en-US")} stroops of headroom remain per period. ` +
        `Activity can grow to about ${Math.floor(100 / budget.utilisationPct)}x the planned volume ` +
        `before the budget is a constraint.`,
      estimatedDeltaStroops: 0,
    },
  ];
}

/**
 * Instruction budget: a call close to the protocol's committed CPU-instruction
 * ceiling is a risk even when the stroop total looks comfortable, because the
 * two limits are enforced differently.
 */
function ruleInstructionBudget(plan: GasBudgetPlan): PlanSuggestion[] {
  const close = plan.lines
    .map((line) => {
      const cost = plan.costs.find((c) => c.operation === line.operation);
      return cost && cost.budgetUtilisationPct !== null
        ? { line, cost, pct: cost.budgetUtilisationPct }
        : null;
    })
    .filter((entry): entry is { line: PlannedLine; cost: OperationCost; pct: number } => entry !== null)
    .filter((entry) => entry.pct >= 50)
    .sort((a, b) => b.pct - a.pct);

  if (close.length === 0) return [];

  return close.map(({ line, cost, pct }) => ({
    id: "instruction-budget-pressure",
    category: "storage-writes" as const,
    severity: (pct >= 90 ? "high" : "medium") as PlanSuggestion["severity"],
    title: `\`${line.operation}\` uses ${round2(pct)}% of its instruction budget`,
    detail:
      `The plan's CPU baseline for \`${line.operation}\` is ` +
      `${cost.cpuInstructions?.toLocaleString("en-US")} instructions against a committed budget of ` +
      `${cost.instructionBudget?.toLocaleString("en-US")} ` +
      `(stellar-lend/benchmarks/baseline.json). A measured run that comes in higher would trip the ` +
      `gas budget gate even though the stroop total is fine.`,
    estimatedDeltaStroops: 0,
  }));
}

/**
 * Every rule takes the built plan plus the inputs it was built from. Rules that
 * need neither extra value simply take the plan, which TypeScript accepts as an
 * implementation of this signature.
 */
const RULES: ((plan: GasBudgetPlan, ctx: RuleContext) => PlanSuggestion[])[] = [
  ruleBatching,
  ruleConcentration,
  ruleStorageWrites,
  ruleGrowth,
  ruleInstructionBudget,
  ruleHeadroom,
];

/** Run every rule over a built plan. */
export function buildSuggestions(
  plan: GasBudgetPlan,
  data: CostData,
  pattern: InteractionPattern,
): PlanSuggestion[] {
  const suggestions = RULES.flatMap((rule) => rule(plan, { data, pattern }));
  return suggestions.sort(
    (a, b) =>
      SEVERITY_ORDER[b.severity] - SEVERITY_ORDER[a.severity] ||
      Math.abs(b.estimatedDeltaStroops) - Math.abs(a.estimatedDeltaStroops) ||
      a.id.localeCompare(b.id),
  );
}

/** The rule ids this module can emit, for `--list-rules` and test assertions. */
export const RULE_IDS: string[] = [
  "batch-deposits",
  "concentrated-operation",
  "storage-write-dominated",
  "budget-growth-limit",
  "instruction-budget-pressure",
  "ample-headroom",
];

/** Build the full plan for an interaction pattern. */
export function planBudget(
  pattern: InteractionPattern,
  data: CostData,
  options: {
    xlmPriceUsd?: number | null;
    budgetStroops?: number | null;
    projectionMultipliers?: number[];
  } = {},
): GasBudgetPlan {
  const xlmPriceUsd = options.xlmPriceUsd ?? pattern.xlmPriceUsd ?? null;
  const periodDays = Math.max(1, Math.trunc(pattern.periodDays ?? 30));
  const { lines, unknownOperations } = planLines(pattern, data, xlmPriceUsd);

  const totalStroops = lines.reduce((sum, line) => sum + line.subtotalStroops, 0);
  const calls = lines.reduce((sum, line) => sum + line.calls, 0);
  const budgetStroops = options.budgetStroops ?? pattern.budgetStroops ?? null;
  const budget = checkBudget(totalStroops, budgetStroops);
  const projections = projectVolumes(
    totalStroops,
    budgetStroops,
    options.projectionMultipliers ?? [2, 5, 10],
  );

  const plan: GasBudgetPlan = {
    pattern: {
      name: pattern.name,
      description: pattern.description ?? "",
      periodDays,
      operations: { ...pattern.operations },
      batchSize: pattern.batchSize ?? 1,
      xlmPriceUsd,
    },
    costs: byCostDescending(costOfAllOperations(data, xlmPriceUsd)),
    lines,
    totals: {
      periodDays,
      calls,
      totalStroops,
      totalXlm: totalStroops / STROOPS_PER_XLM,
      totalUsd: xlmPriceUsd === null ? null : (totalStroops / STROOPS_PER_XLM) * xlmPriceUsd,
      annualisedStroops: Math.round(totalStroops * (365 / periodDays)),
      costliestCallStroops: lines[0]?.unitStroops ?? 0,
    },
    budget,
    projections,
    suggestions: [],
    knownOperations: Object.keys(data.complexity).sort(),
  };

  plan.suggestions = buildSuggestions(plan, data, pattern);

  if (unknownOperations.length > 0) {
    plan.suggestions = [
      {
        id: "unknown-operation",
        category: "concentration",
        severity: "high",
        title: `${unknownOperations.length} operation(s) in this pattern are not in the cost table`,
        detail:
          `\`${unknownOperations.join("`, `")}\` could not be priced, so this budget is a lower ` +
          `bound. The cost table covers ` +
          `${Object.keys(data.complexity).sort().join(", ")}.`,
        estimatedDeltaStroops: 0,
      },
      ...plan.suggestions,
    ];
  }

  return plan;
}

function formatStroops(value: number): string {
  return Math.round(value).toLocaleString("en-US");
}

function bar(value: number, max: number, width = 24): string {
  if (max <= 0) return "";
  return "█".repeat(Math.max(1, Math.round((value / max) * width)));
}

/** The per-call price table, hottest first. */
export function renderCostTable(costs: OperationCost[], source: string): string[] {
  const lines: string[] = [];
  lines.push(`Per call cost (from ${source}):`);
  lines.push("Operation              Writes X-call  Stroops/call    XLM        CPU insns  Budget");
  for (const cost of costs) {
    lines.push(
      [
        cost.operation.padEnd(22),
        String(cost.storageWrites).padStart(5),
        String(cost.crossContractCalls).padStart(6),
        formatStroops(cost.totalStroops).padStart(12),
        cost.totalXlm.toFixed(7).padStart(11),
        (cost.cpuInstructions?.toLocaleString("en-US") ?? "—").padStart(11),
        (cost.budgetUtilisationPct === null ? "—" : `${round2(cost.budgetUtilisationPct)}%`).padStart(7),
      ].join(" "),
    );
  }
  return lines;
}

/** Human-readable plan for a terminal. */
export function renderText(plan: GasBudgetPlan): string {
  const lines: string[] = [];
  lines.push(`Gas budget plan — ${plan.pattern.name}`);
  if (plan.pattern.description) lines.push(`  ${plan.pattern.description}`);
  lines.push("");
  lines.push(...renderCostTable(plan.costs, plan.costs[0]?.cpuSource ?? "the cost table"));
  lines.push("");
  lines.push(`Plan — ${plan.totals.calls} call(s) per ${plan.pattern.periodDays} day(s):`);
  lines.push("Operation              Calls  Unit stroops  Subtotal        Share");
  const maxSubtotal = Math.max(1, ...plan.lines.map((line) => line.subtotalStroops));
  for (const line of plan.lines) {
    lines.push(
      [
        line.operation.padEnd(22),
        String(line.calls).padStart(5),
        formatStroops(line.amortisedUnitStroops).padStart(12),
        formatStroops(line.subtotalStroops).padStart(9),
        `${formatStroops(round2(line.sharePct))}%`.padStart(8),
        `  ${bar(line.subtotalStroops, maxSubtotal, 14)}`,
      ].join(" "),
    );
  }

  lines.push("");
  lines.push(`Total per period : ${formatStroops(plan.totals.totalStroops)} stroops (${plan.totals.totalXlm.toFixed(7)} XLM${plan.totals.totalUsd === null ? "" : `, $${plan.totals.totalUsd.toFixed(4)}`})`);
  lines.push(`Annualised       : ${formatStroops(plan.totals.annualisedStroops)} stroops (${(plan.totals.annualisedStroops / STROOPS_PER_XLM).toFixed(7)} XLM)`);
  if (plan.budget.status === "unset") {
    lines.push("Budget           : not set (pass --budget-xlm or --budget-stroops)");
  } else {
    lines.push(
      `Budget           : ${formatStroops(plan.budget.budgetStroops)} stroops · ` +
        `${plan.budget.utilisationPct}% used · ${formatStroops(plan.budget.headroomStroops ?? 0)} headroom · ${plan.budget.status}`,
    );
  }

  if (plan.projections.length > 0) {
    lines.push("");
    lines.push("Volume projections:");
    for (const projection of plan.projections) {
      lines.push(
        `  ${projection.multiplier}x  ${formatStroops(projection.totalStroops)} stroops` +
          `  (${projection.totalXlm.toFixed(7)} XLM)` +
          `  ${projection.utilisationPct === null ? "—" : `${projection.utilisationPct}% of budget`}` +
          `  ${projection.status === "unset" ? "" : projection.status}`,
      );
    }
  }

  if (plan.suggestions.length > 0) {
    lines.push("");
    lines.push(`Suggestions: ${plan.suggestions.length}`);
    for (const suggestion of plan.suggestions) {
      const delta =
        suggestion.estimatedDeltaStroops === 0
          ? ""
          : ` (${suggestion.estimatedDeltaStroops < 0 ? "saves" : "adds"} ${formatStroops(Math.abs(suggestion.estimatedDeltaStroops))} stroops)`;
      lines.push(`  [${suggestion.severity.toUpperCase()}] ${suggestion.id}${delta} — ${suggestion.title}`);
      lines.push(`      ${suggestion.detail}`);
    }
  }

  return lines.join("\n");
}

/** Markdown plan for a PR comment or job summary. */
export function renderMarkdown(plan: GasBudgetPlan, sources: { estimator: string; instructionBudgets: string }): string {
  const out: string[] = [];
  out.push(`## Gas budget plan — ${plan.pattern.name}`);
  if (plan.pattern.description) out.push(`> ${plan.pattern.description}`);
  out.push("");
  out.push(
    `${plan.totals.calls} call(s) per ${plan.pattern.periodDays} day(s) · ` +
      `${formatStroops(plan.totals.totalStroops)} stroops per period ` +
      `(${plan.totals.totalXlm.toFixed(7)} XLM${plan.totals.totalUsd === null ? "" : `, $${plan.totals.totalUsd.toFixed(4)}`}) · ` +
      `${formatStroops(plan.totals.annualisedStroops)} stroops annualised`,
  );
  out.push("");

  out.push("| Operation | Calls | Unit (stroops) | Subtotal | Share |");
  out.push("| --- | ---: | ---: | ---: | ---: |");
  for (const line of plan.lines) {
    out.push(
      `| \`${line.operation}\` | ${line.calls} | ${formatStroops(line.amortisedUnitStroops)} | ` +
        `${formatStroops(line.subtotalStroops)} | ${round2(line.sharePct)}% |`,
    );
  }
  out.push(`| **Total** | **${plan.totals.calls}** | | **${formatStroops(plan.totals.totalStroops)}** | 100% |`);

  if (plan.budget.status !== "unset") {
    out.push("");
    const label = plan.budget.status.toUpperCase();
    out.push(
      `**Budget: ${label}** — ${plan.budget.utilisationPct}% of ` +
        `${formatStroops(plan.budget.budgetStroops)} stroops used, ` +
        `${formatStroops(plan.budget.headroomStroops ?? 0)} stroops headroom.`,
    );
  }

  if (plan.projections.length > 0) {
    out.push("");
    out.push("| Volume | Stroops | XLM | Of budget | |");
    out.push("| ---: | ---: | ---: | ---: | --- |");
    for (const projection of plan.projections) {
      out.push(
        `| ${projection.multiplier}x | ${formatStroops(projection.totalStroops)} | ` +
          `${projection.totalXlm.toFixed(7)} | ` +
          `${projection.utilisationPct === null ? "—" : `${projection.utilisationPct}%`} | ${projection.status} |`,
      );
    }
  }

  if (plan.suggestions.length > 0) {
    out.push("");
    out.push("### Suggestions");
    out.push("");
    for (const suggestion of plan.suggestions) {
      out.push(`- **[${suggestion.severity}] \`${suggestion.id}\`** — ${suggestion.title}`);
      if (suggestion.estimatedDeltaStroops !== 0) {
        const verb = suggestion.estimatedDeltaStroops < 0 ? "saves" : "adds";
        out.push(
          `  - ${verb} ~${formatStroops(Math.abs(suggestion.estimatedDeltaStroops))} stroops per period`,
        );
      }
      out.push(`  - ${suggestion.detail}`);
    }
  }

  out.push("");
  out.push(
    `<sub>Per-call costs read from ${sources.estimator} (the same constants ` +
      `\`POST /api/gas/estimate\` uses) and instruction budgets from ${sources.instructionBudgets}.</sub>`,
  );
  return out.join("\n");
}
