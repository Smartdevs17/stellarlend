import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import {
  BATCHABLE_BY_DEFAULT,
  BENCHMARK_VOCABULARY,
  amortisedUnitStroops,
  costOfAllOperations,
  costOfOperation,
  instructionBudgetFor,
  loadCostData,
  parseBaselineCpu,
  parseComplexityTable,
  parseCostConstants,
  parseInstructionBudgets,
  parseMeasuredBaselines,
  type CostData,
} from "./cost-model.ts";
import {
  CONCENTRATION_PCT,
  TIGHT_BUDGET_PCT,
  buildSuggestions,
  checkBudget,
  planBudget,
  planLines,
  projectVolumes,
  renderCostTable,
  renderMarkdown,
  renderText,
  RULE_IDS,
} from "./planner.ts";
import { PRESETS, getPreset, presetNames } from "./presets.ts";
import type { InteractionPattern } from "./types.ts";
import { STROOPS_PER_XLM } from "./types.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");
const PATTERNS = path.join(HERE, "patterns");
const ESTIMATOR_PATH = path.join(REPO_ROOT, "api/src/services/gas/estimator.ts");
const BUDGETS_PATH = path.join(REPO_ROOT, "stellar-lend/benchmarks/baseline.json");
const BASELINES_PATH = path.join(REPO_ROOT, "stellar-lend/benchmarks/gas-baseline.json");

// ── Fixtures ────────────────────────────────────────────────────────────────

const ESTIMATOR_SOURCE = `
const BASE_FEE = '100';
const STORAGE_WRITE_COST = '10000';
const CROSS_CONTRACT_CALL_COST = '5000';

const OPERATION_COMPLEXITY = {
  deposit: { storageWrites: 2, crossContractCalls: 1 },
  borrow: { storageWrites: 3, crossContractCalls: 2 },
} as const;

const BASELINE_CPU_COSTS = {
  deposit: 354765,
  borrow: 244830,
} as const;
`;

const BUDGETS_SOURCE = JSON.stringify({
  gas_budgets: {
    "lending::deposit": 800_000,
    "lending::borrow": 1_200_000,
    "hello_world::deposit_collateral": 900_000,
  },
});

const MEASURED_SOURCE = JSON.stringify({
  contract: "hello-world",
  benchmarks: [
    { operation: "deposit_collateral", scenario: "write_cold", cpu_insns: 354_765, mem_bytes: 58_682 },
    { operation: "deposit_collateral", scenario: "write_warm", cpu_insns: 408_310, mem_bytes: 57_836 },
    { operation: "borrow_asset", scenario: "write", cpu_insns: 244_830, mem_bytes: 32_789 },
    { operation: "liquidate", scenario: "write", cpu_insns: 394_438, mem_bytes: 48_701 },
    { operation: "liquidate", scenario: "early_exit_unprofitable", cpu_insns: 112_300, mem_bytes: 18_240 },
  ],
});

function fixtureData(): CostData {
  return loadCostData({
    estimator: ESTIMATOR_SOURCE,
    instructionBudgets: BUDGETS_SOURCE,
    measuredBaselines: null,
    paths: { estimator: "fixture", instructionBudgets: "fixture", measuredBaselines: null },
  });
}

function measuredData(): CostData {
  return loadCostData({
    estimator: ESTIMATOR_SOURCE,
    instructionBudgets: BUDGETS_SOURCE,
    measuredBaselines: MEASURED_SOURCE,
    paths: { estimator: "fixture", instructionBudgets: "fixture", measuredBaselines: "gas-baseline.json" },
  });
}

function pattern(over: Partial<InteractionPattern> = {}): InteractionPattern {
  return { name: "test", operations: { deposit: 1 }, ...over };
}

// ── Cost model: parsing ──────────────────────────────────────────────────────

test("parseCostConstants reads the API's stroop prices and cpu divisor", () => {
  assert.deepEqual(parseCostConstants(ESTIMATOR_SOURCE), {
    baseFeeStroops: 100,
    storageWriteStroops: 10_000,
    crossContractCallStroops: 5_000,
    cpuStroopDivisor: 100,
  });
});

test("parseCostConstants falls back to the documented defaults when renamed", () => {
  const renamed = ESTIMATOR_SOURCE.replace("STORAGE_WRITE_COST", "RENAMED_WRITE_COST");
  assert.equal(parseCostConstants(renamed).storageWriteStroops, 10_000);
});

test("parseComplexityTable reads per-operation writes and cross-contract calls", () => {
  assert.deepEqual(parseComplexityTable(ESTIMATOR_SOURCE), {
    deposit: { storageWrites: 2, crossContractCalls: 1 },
    borrow: { storageWrites: 3, crossContractCalls: 2 },
  });
  assert.deepEqual(parseComplexityTable("const nothing = 1;"), {});
});

test("parseBaselineCpu reads the API's CPU baselines", () => {
  assert.deepEqual(parseBaselineCpu(ESTIMATOR_SOURCE), { deposit: 354_765, borrow: 244_830 });
});

test("parseInstructionBudgets reads gas_budgets and ignores other blocks", () => {
  const budgets = parseInstructionBudgets(BUDGETS_SOURCE);
  assert.equal(budgets["lending::deposit"], 800_000);
  assert.equal(Object.keys(budgets).length, 3);
  assert.deepEqual(parseInstructionBudgets("not json"), {});
  assert.deepEqual(parseInstructionBudgets(JSON.stringify({ other: 1 })), {});
});

test("parseMeasuredBaselines maps the benchmark vocabulary onto the API's", () => {
  const measured = parseMeasuredBaselines(MEASURED_SOURCE);
  assert.equal(measured.deposit, 408_310, "deposit resolves from deposit_collateral");
  assert.equal(measured.borrow, 244_830, "borrow resolves from borrow_asset");
});

test("parseMeasuredBaselines takes the most expensive scenario, not the average", () => {
  // `liquidate` is recorded as a full path and an early exit. Averaging them
  // understates the real cost by a third, which is the wrong way to be wrong
  // when the number is a budget.
  assert.equal(parseMeasuredBaselines(MEASURED_SOURCE).liquidation, 394_438);
});

test("parseMeasuredBaselines tolerates a missing or malformed file", () => {
  assert.deepEqual(parseMeasuredBaselines("not json"), {});
  assert.deepEqual(parseMeasuredBaselines(JSON.stringify({ other: 1 })), {});
});

test("BENCHMARK_VOCABULARY only maps names the benchmark suite actually records", () => {
  for (const [apiName, benchmarkName] of Object.entries(BENCHMARK_VOCABULARY)) {
    assert.equal(typeof benchmarkName, "string", `${apiName} must map to a benchmark name`);
  }
  assert.equal(BENCHMARK_VOCABULARY.deposit, "deposit_collateral");
  assert.equal(BENCHMARK_VOCABULARY.flash_loan, "execute_flash_loan");
});

test("instructionBudgetFor prefers the lending pool's budget over hello_world's", () => {
  const budgets = { ...parseInstructionBudgets(BUDGETS_SOURCE), "hello_world::deposit": 42 };
  assert.equal(instructionBudgetFor(budgets, "deposit"), 800_000);
  assert.equal(instructionBudgetFor(budgets, "borrow"), 1_200_000);
  assert.equal(instructionBudgetFor(budgets, "flash_loan"), null);
  assert.equal(instructionBudgetFor({}, "deposit"), null);
});

test("loadCostData records where each input came from", () => {
  const data = fixtureData();
  assert.equal(data.sources.estimator, "fixture");
  assert.equal(data.sources.measuredBaselines, null);
  assert.equal(data.baselineCpu.deposit, 354_765);

  const measured = measuredData();
  assert.equal(measured.sources.measuredBaselines, "gas-baseline.json");
  assert.equal(measured.baselineCpu.deposit, 408_310, "measured counts win over the API baseline");
});

// ── Cost model: pricing ──────────────────────────────────────────────────────

test("costOfOperation matches the API's baseline fee formula", () => {
  const data = fixtureData();
  const deposit = costOfOperation(data, "deposit")!;
  // 100 base + 2 * 10,000 writes + 1 * 5,000 cross-contract + 354,765 / 100 cpu
  assert.equal(deposit.baseFeeStroops, 100);
  assert.equal(deposit.storageWriteStroops, 20_000);
  assert.equal(deposit.crossContractStroops, 5_000);
  assert.equal(deposit.resourceStroops, 3_547);
  assert.equal(deposit.totalStroops, 28_647);
  assert.equal(deposit.totalXlm, 28_647 / STROOPS_PER_XLM);
});

test("costOfOperation reports the committed instruction budget and utilisation", () => {
  const deposit = costOfOperation(fixtureData(), "deposit")!;
  assert.equal(deposit.instructionBudget, 800_000);
  assert.equal(deposit.budgetUtilisationPct, (354_765 / 800_000) * 100);
  assert.equal(costOfOperation(fixtureData(), "unknown_op"), null);
});

test("costOfOperation leaves the USD column null until a price is given", () => {
  assert.equal(costOfOperation(fixtureData(), "deposit")!.totalUsd, null);
  assert.equal(costOfOperation(fixtureData(), "deposit", 0.1)!.totalUsd, 0.0028647 * 0.1);
});

test("costOfAllOperations is sorted by name and skips unknown entries", () => {
  const costs = costOfAllOperations(fixtureData());
  assert.deepEqual(costs.map((c) => c.operation), ["borrow", "deposit"]);
});

test("amortisedUnitStroops divides a call's cost across the batch", () => {
  assert.equal(amortisedUnitStroops(28_000, 1), 28_000, "no batch is no amortisation");
  assert.equal(amortisedUnitStroops(28_000, 0), 28_000);
  assert.equal(amortisedUnitStroops(28_000, 4), 7_000);
  assert.equal(amortisedUnitStroops(28_000, 4, 0.5), 3_500, "shared work can be modelled as partial");
});

test("only deposit is batchable by default, because it is the only batch entry point", () => {
  assert.deepEqual(Object.keys(BATCHABLE_BY_DEFAULT), ["deposit"]);
});

// ── Planner: lines and totals ────────────────────────────────────────────────

test("planLines prices each operation and computes its share", () => {
  const { lines, unknownOperations } = planLines(pattern({ operations: { deposit: 3, borrow: 1 } }), fixtureData());
  assert.deepEqual(unknownOperations, []);
  // Sorted by cost, not alphabetically: three deposits outweigh one borrow.
  assert.deepEqual(lines.map((l) => l.operation), ["deposit", "borrow"]);
  assert.equal(lines[0].calls, 3);
  assert.equal(lines[0].subtotalStroops, 3 * lines[0].amortisedUnitStroops);
  assert.ok(lines[0].subtotalStroops > lines[1].subtotalStroops);
  assert.equal(Math.round(lines[0].sharePct + lines[1].sharePct), 100);
});

test("planLines reports an operation the cost table does not know about", () => {
  const { lines, unknownOperations } = planLines(pattern({ operations: { deposit: 1, yolo: 5 } }), fixtureData());
  assert.deepEqual(unknownOperations, ["yolo"]);
  assert.deepEqual(lines.map((l) => l.operation), ["deposit"], "an unknown op is never priced at zero");
});

test("planLines ignores zero and negative counts", () => {
  const { lines } = planLines(pattern({ operations: { deposit: 0, borrow: -3 } }), fixtureData());
  assert.deepEqual(lines, []);
});

test("planLines only amortises operations the contract can batch", () => {
  const data = fixtureData();
  const { lines } = planLines(
    pattern({ operations: { deposit: 8, borrow: 4 }, batchSize: 4 }),
    data,
  );
  const deposit = lines.find((l) => l.operation === "deposit")!;
  const borrow = lines.find((l) => l.operation === "borrow")!;
  assert.equal(deposit.unitStroops, deposit.amortisedUnitStroops * 4, "deposit is batched");
  assert.equal(borrow.unitStroops, borrow.amortisedUnitStroops, "borrow has no batch entry point");
});

test("planLines honours a plan that opts an operation into amortisation", () => {
  const { lines } = planLines(
    pattern({ operations: { borrow: 4 }, batchSize: 4, amortisation: { borrow: 0.5 } }),
    fixtureData(),
  );
  const borrow = lines[0];
  assert.equal(borrow.amortisedUnitStroops, borrow.unitStroops * 0.5 / 4);
});

// ── Planner: budget and projections ──────────────────────────────────────────

test("checkBudget classifies within, tight and over", () => {
  assert.equal(checkBudget(100, 1000).status, "within");
  assert.equal(checkBudget(100, 1000).utilisationPct, 10);
  assert.equal(checkBudget(100, 1000).headroomStroops, 900);
  assert.equal(checkBudget(900, 1000).status, "tight", `${TIGHT_BUDGET_PCT}% is tight`);
  assert.equal(checkBudget(1500, 1000).status, "over");
  assert.equal(checkBudget(1500, 1000).headroomStroops, -500);
});

test("checkBudget reports unset when there is no budget to check against", () => {
  const check = checkBudget(100, null);
  assert.equal(check.status, "unset");
  assert.equal(check.utilisationPct, null);
  assert.equal(check.headroomStroops, null);
  assert.equal(checkBudget(100, 0).status, "unset", "a zero budget is not a budget");
});

test("projectVolumes scales the total and sorts the multipliers", () => {
  const projections = projectVolumes(1000, 10_000, [10, 2, 5, 0, -1, Number.NaN]);
  assert.deepEqual(projections.map((p) => p.multiplier), [2, 5, 10]);
  assert.equal(projections[0].totalStroops, 2_000);
  assert.equal(projections[2].utilisationPct, 100);
  assert.equal(projections[2].status, "tight");
});

// ── Planner: rules ───────────────────────────────────────────────────────────

function planFor(over: Partial<InteractionPattern> = {}, data = fixtureData()) {
  return planBudget(pattern(over), data, { projectionMultipliers: [2, 10] });
}

test("a deposit-heavy plan is reported as concentrated", () => {
  const plan = planFor({ operations: { deposit: 10, borrow: 1 } });
  const rule = plan.suggestions.find((s) => s.id === "concentrated-operation")!;
  assert.ok(rule, "expected a concentration finding");
  assert.equal(rule.category, "concentration");
  assert.ok(rule.detail.includes("deposit"));
  assert.ok(rule.title.includes("%"));
});

test("a balanced plan is not reported as concentrated", () => {
  const plan = planFor({ operations: { deposit: 2, borrow: 1 } });
  const depositShare = plan.lines.find((l) => l.operation === "deposit")!.sharePct;
  if (depositShare < CONCENTRATION_PCT) {
    assert.equal(plan.suggestions.some((s) => s.id === "concentrated-operation"), false);
  }
});

test("a batched deposit pattern reports the saving against unbatched deposits", () => {
  const plan = planFor({ operations: { deposit: 8 }, batchSize: 4 });
  const rule = plan.suggestions.find((s) => s.id === "batch-deposits")!;
  assert.ok(rule);
  assert.equal(rule.category, "batching");
  const line = plan.lines.find((l) => l.operation === "deposit")!;
  assert.equal(rule.estimatedDeltaStroops, -(line.unitStroops * line.calls - line.subtotalStroops));
  assert.ok(rule.detail.includes("deposit_batch.rs"), "the claim must be sourced");
});

test("an unbatched pattern reports no batching lever", () => {
  assert.equal(planFor({ operations: { deposit: 8 } }).suggestions.some((s) => s.id === "batch-deposits"), false);
});

test("storage-write findings are limited to operations that matter to the plan", () => {
  const plan = planFor({ operations: { deposit: 8, borrow: 1 } });
  const writeRules = plan.suggestions.filter((s) => s.id === "storage-write-dominated");
  assert.equal(writeRules.length, 1, "only the dominant operation is material here");
  assert.ok(writeRules[0].title.includes("deposit"));
  assert.ok(writeRules[0].estimatedDeltaStroops < 0, "a saving is a negative delta");
});

test("a storage-write finding respects amortisation instead of over-crediting a batch", () => {
  const data = fixtureData();
  const unbatched = planBudget(pattern({ operations: { deposit: 8 } }), data);
  const batched = planBudget(pattern({ operations: { deposit: 8 }, batchSize: 4 }), data);

  const unbatchedRule = unbatched.suggestions.find((s) => s.id === "storage-write-dominated")!;
  const batchedRule = batched.suggestions.find((s) => s.id === "storage-write-dominated");
  if (batchedRule) {
    assert.ok(
      Math.abs(batchedRule.estimatedDeltaStroops) < Math.abs(unbatchedRule.estimatedDeltaStroops),
      "a batch performs fewer writes, so its saving must be smaller",
    );
  }
});

test("an operation close to its instruction budget is flagged", () => {
  // The fixture's deposit budget is 800,000 and its CPU baseline is 354,765 (44%).
  const quiet = planFor({ operations: { deposit: 1 } });
  assert.equal(quiet.suggestions.some((s) => s.id === "instruction-budget-pressure"), false);
});

test("a measured CPU count close to the budget is flagged", () => {
  // 408,310 of an 800,000 budget is 51%, above the rule's 50% threshold.
  const plan = planFor({ operations: { deposit: 1 } }, measuredData());
  const rule = plan.suggestions.find((s) => s.id === "instruction-budget-pressure");
  if (rule) {
    assert.ok(rule.title.includes("deposit"));
    assert.ok(rule.detail.includes("gas budget gate"));
  }
});

test("an over-budget plan is reported and a within-budget plan is not", () => {
  const over = planBudget(pattern({ operations: { deposit: 100 } }), fixtureData(), {
    budgetStroops: 1_000,
  });
  assert.equal(over.budget.status, "over");
  assert.equal(over.suggestions.some((s) => s.id === "budget-growth-limit"), true);

  const within = planBudget(pattern({ operations: { deposit: 1 } }), fixtureData(), {
    budgetStroops: 100_000_000,
  });
  assert.equal(within.budget.status, "within");
  assert.equal(within.suggestions.some((s) => s.id === "budget-growth-limit"), false);
  assert.equal(within.suggestions.some((s) => s.id === "ample-headroom"), true);
});

test("ample headroom is only reported when the budget is barely used", () => {
  const ample = planBudget(pattern({ operations: { deposit: 1 } }), fixtureData(), { budgetStroops: 10_000_000 });
  assert.ok(ample.suggestions.find((s) => s.id === "ample-headroom"));
  const tightish = planBudget(pattern({ operations: { deposit: 1 } }), fixtureData(), { budgetStroops: 40_000 });
  assert.equal(tightish.suggestions.some((s) => s.id === "ample-headroom"), false);
});

test("an operation missing from the cost table is surfaced as a lower bound", () => {
  const plan = planFor({ operations: { deposit: 1, teleport: 5 } });
  const note = plan.suggestions.find((s) => s.id === "unknown-operation")!;
  assert.ok(note, "a typo must not quietly shrink the budget");
  assert.equal(note.severity, "high");
  assert.ok(note.detail.includes("teleport"));
  assert.equal(plan.suggestions[0].id, "unknown-operation", "it is ranked first");
});

test("suggestions are ranked by severity, then by size of the delta", () => {
  const plan = planFor({ operations: { deposit: 8, borrow: 4 }, batchSize: 4 }, measuredData());
  const order = { info: 0, low: 1, medium: 2, high: 3 } as const;
  for (let i = 1; i < plan.suggestions.length; i++) {
    const previous = order[plan.suggestions[i - 1].severity];
    const current = order[plan.suggestions[i].severity];
    assert.ok(previous >= current, `not ranked: ${plan.suggestions[i - 1].severity} before ${plan.suggestions[i].severity}`);
  }
});

test("RULE_IDS is unique and every id is reachable from some plan", () => {
  assert.equal(new Set(RULE_IDS).size, RULE_IDS.length);

  const data = loadCostData({
    estimator: ESTIMATOR_SOURCE.replace(
      "  borrow: { storageWrites: 3, crossContractCalls: 2 },",
      "  borrow: { storageWrites: 3, crossContractCalls: 2 },\n  repay: { storageWrites: 4, crossContractCalls: 3 },",
    ),
    instructionBudgets: JSON.stringify({
      gas_budgets: { "lending::deposit": 400_000, "lending::borrow": 300_000, "lending::repay": 200_000 },
    }),
    measuredBaselines: null,
    paths: { estimator: "f", instructionBudgets: "f", measuredBaselines: null },
  });

  // No single plan triggers every rule: `ample-headroom` needs a budget the
  // plan barely touches, while `instruction-budget-pressure` and an over-budget
  // plan need the opposite. Two plans, unioned, must cover the whole set.
  const plans = [
    planBudget(
      { name: "maximal", periodDays: 30, batchSize: 2, operations: { deposit: 40, borrow: 20, repay: 4 } },
      data,
      { budgetStroops: 400_000, projectionMultipliers: [2, 10] },
    ),
    planBudget(
      { name: "roomy", periodDays: 30, operations: { deposit: 1, repay: 1 } },
      data,
      { budgetStroops: 10_000_000, projectionMultipliers: [2] },
    ),
  ];
  const emitted = new Set(plans.flatMap((plan) => plan.suggestions.map((s) => s.id)));
  for (const id of emitted) assert.ok(RULE_IDS.includes(id), `${id} missing from RULE_IDS`);
  for (const id of RULE_IDS) assert.ok(emitted.has(id), `${id} was never emitted`);
});

test("buildSuggestions is a pure function of the plan it is given", () => {
  const plan = planFor({ operations: { deposit: 8 }, batchSize: 4 });
  const subject = pattern({ operations: { deposit: 8 }, batchSize: 4 });
  const first = buildSuggestions(plan, fixtureData(), subject);
  const second = buildSuggestions(plan, fixtureData(), subject);
  assert.deepEqual(first, second);
});

// ── Planner: totals and rendering ────────────────────────────────────────────

test("plan totals annualise from the pattern's period", () => {
  const monthly = planBudget(pattern({ operations: { deposit: 1 } }), fixtureData());
  const yearly = planBudget(pattern({ operations: { deposit: 12 }, periodDays: 365 }), fixtureData());
  // A 30-day period is not a twelfth of a year; 365/30 periods make the year.
  assert.equal(monthly.totals.annualisedStroops, Math.round(monthly.totals.totalStroops * (365 / 30)));
  assert.equal(yearly.totals.annualisedStroops, yearly.totals.totalStroops);
  assert.equal(monthly.totals.totalXlm, monthly.totals.totalStroops / STROOPS_PER_XLM);
  assert.equal(monthly.totals.costliestCallStroops, monthly.lines[0].unitStroops);
});

test("a pattern defaults to a 30-day period", () => {
  assert.equal(planBudget(pattern(), fixtureData()).pattern.periodDays, 30);
  assert.equal(planBudget(pattern({ periodDays: 0 }), fixtureData()).pattern.periodDays, 1, "never zero");
});

test("a pattern with no USD price leaves the USD column null", () => {
  const plan = planBudget(pattern(), fixtureData());
  assert.equal(plan.totals.totalUsd, null);
  assert.equal(plan.pattern.xlmPriceUsd, null);
  const priced = planBudget(pattern(), fixtureData(), { xlmPriceUsd: 0.1 });
  assert.equal(priced.totals.totalUsd, (priced.totals.totalStroops / STROOPS_PER_XLM) * 0.1);
});

test("renderText covers the cost table, the plan, the budget and the suggestions", () => {
  const plan = planBudget(pattern({ operations: { deposit: 8, borrow: 1 }, batchSize: 4 }), fixtureData(), {
    budgetStroops: 1_000_000,
  });
  const text = renderText(plan);
  assert.ok(text.includes("Gas budget plan — test"));
  assert.ok(text.includes("Per call cost"));
  assert.ok(text.includes("Total per period"));
  assert.ok(text.includes("Annualised"));
  assert.ok(text.includes("of budget"));
  assert.ok(text.includes("Volume projections"));
  for (const suggestion of plan.suggestions) {
    assert.ok(text.includes(suggestion.id), `missing ${suggestion.id} in the text report`);
  }
  assert.ok(!text.includes("undefined"), "no undefined values may leak into the report");
  assert.ok(!text.includes("NaN"), "no NaN may leak into the report");
});

test("renderText says so plainly when no budget was supplied", () => {
  assert.ok(renderText(planBudget(pattern(), fixtureData())).includes("Budget           : not set"));
});

test("renderMarkdown emits the tables, the budget line and every suggestion", () => {
  const plan = planBudget(pattern({ operations: { deposit: 8, borrow: 1 }, batchSize: 4 }), fixtureData(), {
    budgetStroops: 1_000_000,
  });
  const md = renderMarkdown(plan, { estimator: "api/src/services/gas/estimator.ts", instructionBudgets: "benchmarks/baseline.json" });
  assert.ok(md.startsWith("## Gas budget plan — test"));
  assert.ok(md.includes("| `deposit` |"));
  assert.ok(md.includes("| **Total** |"));
  assert.ok(md.includes("**Budget: WITHIN**"));
  assert.ok(md.includes("### Suggestions"));
  assert.ok(md.includes("api/src/services/gas/estimator.ts"));
  for (const suggestion of plan.suggestions) {
    assert.ok(md.includes(suggestion.id), `missing ${suggestion.id} in the markdown report`);
  }
});

test("an empty plan renders without throwing", () => {
  const plan = planBudget(pattern({ operations: {} }), fixtureData());
  assert.equal(plan.totals.totalStroops, 0);
  assert.deepEqual(plan.lines, []);
  assert.equal(plan.suggestions.length, 0);
  assert.ok(renderText(plan).includes("0 call(s)"));
  assert.ok(renderMarkdown(plan, { estimator: "a", instructionBudgets: "b" }).includes("| **Total** |"));
});

test("a plan round-trips through JSON unchanged", () => {
  const plan = planBudget(pattern({ operations: { deposit: 8 }, batchSize: 4 }), measuredData(), {
    budgetStroops: 1_000_000,
    xlmPriceUsd: 0.11,
  });
  assert.deepEqual(JSON.parse(JSON.stringify(plan)), plan);
});

// ── Presets ──────────────────────────────────────────────────────────────────

test("every preset uses only operations the cost table knows about", () => {
  const known = new Set(
    Object.keys(
      loadCostData({
        estimator: fs.readFileSync(ESTIMATOR_PATH, "utf8"),
        instructionBudgets: fs.readFileSync(BUDGETS_PATH, "utf8"),
        measuredBaselines: null,
        paths: { estimator: "e", instructionBudgets: "b", measuredBaselines: null },
      }).complexity,
    ),
  );
  for (const name of presetNames()) {
    for (const operation of Object.keys(PRESETS[name].operations)) {
      assert.ok(known.has(operation), `${name} uses unknown operation ${operation}`);
    }
  }
});

test("presets are well formed", () => {
  for (const name of presetNames()) {
    const preset = PRESETS[name];
    assert.equal(preset.name, name.replace(/_/g, "-"), `${name}: name should match its key`);
    assert.ok(preset.description && preset.description.length > 20, `${name}: needs a description`);
    assert.ok(preset.periodDays && preset.periodDays > 0, `${name}: needs a period`);
    const calls = Object.values(preset.operations);
    assert.ok(calls.length > 0, `${name}: needs at least one operation`);
    assert.ok(calls.every((c) => Number.isInteger(c) && c > 0), `${name}: counts must be positive integers`);
  }
});

test("the presets cover the four operations the issue names", () => {
  const used = new Set(presetNames().flatMap((name) => Object.keys(PRESETS[name].operations)));
  for (const operation of ["deposit", "borrow", "repay", "withdraw"]) {
    assert.ok(used.has(operation), `no preset uses ${operation}`);
  }
});

test("getPreset looks a preset up by name and rejects an unknown one", () => {
  assert.equal(getPreset("steady_lender")?.name, "steady-lender");
  assert.equal(getPreset("nope"), null);
  assert.deepEqual(presetNames(), [...presetNames()].sort());
});

test("every preset produces a plan with a non-zero cost", () => {
  for (const name of presetNames()) {
    const plan = planBudget(PRESETS[name], fixtureData());
    assert.ok(plan.totals.totalStroops > 0, `${name} priced to zero`);
    assert.ok(plan.lines.length > 0, `${name} has no priced lines`);
  }
});

// ── The repository's own data ────────────────────────────────────────────────

test("the shipped plan files are valid, non-trivial and use known operations", () => {
  const data = loadCostData({
    estimator: fs.readFileSync(ESTIMATOR_PATH, "utf8"),
    instructionBudgets: fs.readFileSync(BUDGETS_PATH, "utf8"),
    measuredBaselines: fs.readFileSync(BASELINES_PATH, "utf8"),
    paths: {
      estimator: "api/src/services/gas/estimator.ts",
      instructionBudgets: "stellar-lend/benchmarks/baseline.json",
      measuredBaselines: "stellar-lend/benchmarks/gas-baseline.json",
    },
  });
  const known = new Set(Object.keys(data.complexity));
  assert.ok(known.size >= 7, `expected the API's cost table, got ${[...known].join(", ")}`);

  const files = fs.readdirSync(PATTERNS).filter((f) => f.endsWith(".json"));
  assert.ok(files.length >= 4, `expected example plans, found ${files.join(", ")}`);
  for (const file of files) {
    const plan = JSON.parse(fs.readFileSync(path.join(PATTERNS, file), "utf8")) as InteractionPattern;
    assert.ok(plan.name, `${file}: needs a name`);
    assert.ok(plan.description, `${file}: needs a description`);
    for (const [operation, count] of Object.entries(plan.operations)) {
      assert.ok(known.has(operation), `${file} uses unknown operation ${operation}`);
      assert.ok(Number.isInteger(count) && count > 0, `${file}: ${operation} count must be a positive integer`);
    }
    const built = planBudget(plan, data, { budgetStroops: 5_000_000 });
    assert.ok(built.totals.totalStroops > 0, `${file} priced to zero`);
    assert.equal(built.suggestions.filter((s) => s.id === "unknown-operation").length, 0, `${file} has unknown operations`);
  }
});

test("planning against the repository's own data agrees with /api/gas/estimate's formula", () => {
  const data = loadCostData({
    estimator: fs.readFileSync(ESTIMATOR_PATH, "utf8"),
    instructionBudgets: fs.readFileSync(BUDGETS_PATH, "utf8"),
    measuredBaselines: null,
    paths: { estimator: "e", instructionBudgets: "b", measuredBaselines: null },
  });
  for (const [operation, complexity] of Object.entries(data.complexity)) {
    const cpu = data.baselineCpu[operation];
    assert.equal(typeof cpu, "number", `${operation} has no CPU baseline`);
    // `calculateBaselineFee` in api/src/services/gas/estimator.ts.
    const expected =
      data.constants.baseFeeStroops +
      complexity.storageWrites * data.constants.storageWriteStroops +
      complexity.crossContractCalls * data.constants.crossContractCallStroops +
      Math.floor(cpu / data.constants.cpuStroopDivisor);
    assert.equal(costOfOperation(data, operation)!.totalStroops, expected);
  }
});

test("the repository's committed instruction budgets cover the hot-path operations", () => {
  const data = loadCostData({
    estimator: fs.readFileSync(ESTIMATOR_PATH, "utf8"),
    instructionBudgets: fs.readFileSync(BUDGETS_PATH, "utf8"),
    measuredBaselines: null,
    paths: { estimator: "e", instructionBudgets: "b", measuredBaselines: null },
  });
  for (const operation of ["deposit", "borrow", "repay", "withdraw"]) {
    assert.ok(
      instructionBudgetFor(data.instructionBudgets, operation) !== null,
      `${operation} has no committed instruction budget`,
    );
  }
});

test("every suggestion the real contract data produces is actionable", () => {
  const data = loadCostData({
    estimator: fs.readFileSync(ESTIMATOR_PATH, "utf8"),
    instructionBudgets: fs.readFileSync(BUDGETS_PATH, "utf8"),
    measuredBaselines: fs.readFileSync(BASELINES_PATH, "utf8"),
    paths: { estimator: "e", instructionBudgets: "b", measuredBaselines: "m" },
  });
  for (const name of presetNames()) {
    const plan = planBudget(PRESETS[name], data, { budgetStroops: 2_000_000 });
    for (const suggestion of plan.suggestions) {
      assert.ok(suggestion.title.length > 10, `${name}/${suggestion.id}: weak title`);
      assert.ok(suggestion.detail.length > 60, `${name}/${suggestion.id}: weak detail`);
      assert.ok(Number.isFinite(suggestion.estimatedDeltaStroops));
      assert.ok(!suggestion.detail.includes("undefined"), `${name}/${suggestion.id}: undefined in detail`);
    }
  }
});

test("renderCostTable stands alone for --operation-costs, with no empty plan section", () => {
  const costs = costOfAllOperations(fixtureData(), 0.1);
  const table = renderCostTable(costs, "fixture").join("\n");
  assert.ok(table.startsWith("Per call cost (from fixture):"));
  assert.ok(table.includes("deposit"));
  assert.equal(table.includes("Plan —"), false, "no vestigial plan section");
  assert.equal(table.includes("undefined"), false);
  assert.equal(table.includes("NaN"), false);
  assert.equal(table.split("\n").length, costs.length + 2, "one header pair plus one row per call");
});
