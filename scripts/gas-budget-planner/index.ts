#!/usr/bin/env node
/**
 * Lending protocol gas budget planner CLI (#1012).
 *
 * Turns a lender's interaction pattern into a gas budget: what each call costs,
 * what the pattern costs per period, how that sits against a budget, and which
 * levers would reduce it.
 *
 * Run (no install needed on Node >= 22):
 *   node --experimental-strip-types scripts/gas-budget-planner/index.ts --preset steady_lender
 *   node --experimental-strip-types scripts/gas-budget-planner/index.ts \
 *     --plan patterns/steady-lender.json --budget-xlm 12 --xlm-price 0.11
 *   node --experimental-strip-types scripts/gas-budget-planner/index.ts --operation-costs
 *   node --experimental-strip-types scripts/gas-budget-planner/index.ts \
 *     --preset active_rebalancer --format markdown --out plan.md
 *   node --experimental-strip-types scripts/gas-budget-planner/index.ts \
 *     --preset borrow_repay_cycle --fail-over-budget
 *
 * Exit codes: 0 ok, 1 over budget, 2 usage error.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import { costOfAllOperations, loadCostData, type CostData } from "./cost-model.ts";
import { planBudget, renderCostTable, renderMarkdown, renderText, RULE_IDS } from "./planner.ts";
import { getPreset, presetNames } from "./presets.ts";
import type { InteractionPattern } from "./types.ts";
import { STROOPS_PER_XLM } from "./types.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");

const DEFAULT_ESTIMATOR = "api/src/services/gas/estimator.ts";
const DEFAULT_BUDGETS = "stellar-lend/benchmarks/baseline.json";
const DEFAULT_BASELINES = "stellar-lend/benchmarks/gas-baseline.json";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

function flag(name: string): boolean {
  return process.argv.includes(`--${name}`);
}

const USAGE = `Lending protocol gas budget planner (#1012)

Usage:
  index.ts (--plan <file> | --preset <name>) [options]
  index.ts --operation-costs [options]

Options:
  --plan <file>            Plan file: { name, periodDays, operations: { op: count } }
  --preset <name>          Built-in pattern; see --list-presets
  --budget-xlm <n>         Gas budget in XLM
  --budget-stroops <n>     Gas budget in stroops (wins over --budget-xlm)
  --xlm-price <usd>        USD price of XLM, enables the USD column
  --scale <n>              Multiply every operation count, for what-if runs
  --project <list>         Volume multipliers to project (default: 2,5,10)
  --format <fmt>           text | json | markdown (default: text)
  --out <file>             Write the report to a file instead of stdout
  --fail-over-budget       Exit 1 when the plan exceeds the budget
  --operation-costs        Print the per-call cost table and exit
  --baselines <path>       Measured CPU counts to use instead of the API baselines
  --estimator <path>       Cost constants and per-call costs
  --budgets <path>         Committed CPU-instruction budgets
  --list-presets           Print the built-in pattern names and exit
  --list-rules             Print the suggestion rule ids and exit
  --help                   Print this help and exit

Exit codes:
  0 ok · 1 over budget · 2 usage error

Suggestion rules:
  ${RULE_IDS.join("\n  ")}
`;

interface Cli {
  estimator: string;
  budgets: string;
  baselines: string | null;
  plan?: string;
  preset?: string;
  budgetStroops: number | null;
  xlmPriceUsd: number | null;
  scale: number;
  project: number[];
  format: "text" | "json" | "markdown";
  out?: string;
  failOverBudget: boolean;
  operationCosts: boolean;
}

function resolveFromRepo(relative: string): string {
  return path.isAbsolute(relative) ? relative : path.resolve(REPO_ROOT, relative);
}

function readNumber(label: string, raw: string | undefined): number {
  if (raw === undefined) throw new Error(`${label} requires a value`);
  const value = Number(raw);
  if (!Number.isFinite(value)) throw new Error(`${label} must be a number, got "${raw}"`);
  return value;
}

function parseArgs(): Cli {
  if (flag("help") || flag("h")) {
    console.log(USAGE);
    process.exit(0);
  }
  if (flag("list-presets")) {
    console.log(presetNames().join("\n"));
    process.exit(0);
  }
  if (flag("list-rules")) {
    console.log(RULE_IDS.join("\n"));
    process.exit(0);
  }

  const format = (arg("format") ?? "text") as Cli["format"];
  if (format !== "text" && format !== "json" && format !== "markdown") {
    throw new Error(`--format must be text|json|markdown, got "${format}"`);
  }

  const estimator = arg("estimator") ?? DEFAULT_ESTIMATOR;
  const budgets = arg("budgets") ?? DEFAULT_BUDGETS;
  for (const [label, value] of [
    ["--estimator", estimator],
    ["--budgets", budgets],
  ] as const) {
    if (!fs.existsSync(resolveFromRepo(value))) throw new Error(`${label} not found: ${value}`);
  }

  const baselinesRaw = arg("baselines") ?? DEFAULT_BASELINES;
  const baselinesPath = resolveFromRepo(baselinesRaw);
  if (!fs.existsSync(baselinesPath)) {
    if (arg("baselines")) throw new Error(`--baselines not found: ${baselinesRaw}`);
    // The measured file is optional; the API's own baselines are the fallback.
  }

  const scale = readNumber("--scale", arg("scale") ?? "1");
  if (scale <= 0) throw new Error(`--scale must be positive, got ${scale}`);

  const projectRaw = arg("project") ?? "2,5,10";
  const project = projectRaw
    .split(",")
    .map((part) => Number(part.trim()))
    .filter((value) => Number.isFinite(value) && value > 0);
  if (project.length === 0) throw new Error(`--project must list positive multipliers, got "${projectRaw}"`);

  const budgetXlm = arg("budget-xlm");
  const budgetStroopsRaw = arg("budget-stroops");
  const budgetStroops = budgetStroopsRaw
    ? readNumber("--budget-stroops", budgetStroopsRaw)
    : budgetXlm !== undefined
      ? Math.round(readNumber("--budget-xlm", budgetXlm) * STROOPS_PER_XLM)
      : null;

  const xlmPriceRaw = arg("xlm-price");
  const xlmPriceUsd = xlmPriceRaw ? readNumber("--xlm-price", xlmPriceRaw) : null;

  const operationCosts = flag("operation-costs");
  const plan = arg("plan");
  const preset = arg("preset");
  if (!operationCosts && !plan && !preset) {
    throw new Error("one of --plan, --preset or --operation-costs is required");
  }
  if (plan && preset) {
    throw new Error("--plan and --preset are mutually exclusive");
  }
  if (plan && !fs.existsSync(path.resolve(process.cwd(), plan))) {
    throw new Error(`--plan not found: ${plan}`);
  }
  if (preset && !getPreset(preset)) {
    throw new Error(`unknown preset "${preset}". Use --list-presets.`);
  }

  return {
    estimator,
    budgets,
    baselines: fs.existsSync(baselinesPath) ? baselinesRaw : null,
    plan,
    preset,
    budgetStroops,
    xlmPriceUsd,
    scale,
    project,
    format,
    out: arg("out"),
    failOverBudget: flag("fail-over-budget"),
    operationCosts,
  };
}

/** Read a plan file, applying the `--scale` multiplier and CLI overrides. */
function loadPattern(cli: Cli): InteractionPattern {
  let pattern: InteractionPattern;
  if (cli.plan) {
    const raw = JSON.parse(fs.readFileSync(path.resolve(process.cwd(), cli.plan), "utf8")) as InteractionPattern;
    if (!raw || typeof raw !== "object" || typeof raw.operations !== "object" || raw.operations === null) {
      throw new Error(`${cli.plan} must be an object with an "operations" map`);
    }
    pattern = { ...raw, name: raw.name ?? path.basename(cli.plan, ".json") };
  } else {
    pattern = getPreset(cli.preset as string) as InteractionPattern;
  }

  const operations: Record<string, number> = {};
  for (const [operation, count] of Object.entries(pattern.operations)) {
    if (typeof count !== "number" || !Number.isFinite(count) || count < 0) {
      throw new Error(`operation "${operation}" must be a non-negative number, got ${JSON.stringify(count)}`);
    }
    operations[operation] = Math.round(count * cli.scale);
  }

  return {
    ...pattern,
    operations,
    budgetStroops: cli.budgetStroops ?? pattern.budgetStroops,
    xlmPriceUsd: cli.xlmPriceUsd ?? pattern.xlmPriceUsd,
  };
}

function loadCostDataFrom(cli: Cli): CostData {
  return loadCostData({
    estimator: fs.readFileSync(resolveFromRepo(cli.estimator), "utf8"),
    instructionBudgets: fs.readFileSync(resolveFromRepo(cli.budgets), "utf8"),
    measuredBaselines: cli.baselines ? fs.readFileSync(resolveFromRepo(cli.baselines), "utf8") : null,
    paths: {
      estimator: cli.estimator,
      instructionBudgets: cli.budgets,
      measuredBaselines: cli.baselines,
    },
  });
}

function main(): void {
  let cli: Cli;
  try {
    cli = parseArgs();
  } catch (err) {
    console.error(`error: ${(err as Error).message}\n`);
    console.error(USAGE);
    process.exit(2);
  }

  const data = loadCostDataFrom(cli);
  if (Object.keys(data.complexity).length === 0) {
    console.error(
      `error: no per-operation cost table found in ${cli.estimator}; cannot price a plan.`,
    );
    process.exit(2);
  }

  if (cli.operationCosts) {
    const costs = costOfAllOperations(data, cli.xlmPriceUsd);
    const body =
      cli.format === "json"
        ? JSON.stringify(costs, null, 2)
        : renderCostTable(costs, data.sources.measuredBaselines ?? data.sources.estimator).join("\n");
    emit(body, cli);
    process.exit(0);
  }

  const pattern = loadPattern(cli);
  const plan = planBudget(pattern, data, {
    xlmPriceUsd: cli.xlmPriceUsd,
    budgetStroops: cli.budgetStroops,
    projectionMultipliers: cli.project,
  });

  if (plan.lines.length === 0) {
    console.error(
      `error: the pattern "${pattern.name}" prices to zero; set at least one operation count above 0.`,
    );
    process.exit(2);
  }

  const body =
    cli.format === "json"
      ? JSON.stringify(plan, null, 2)
      : cli.format === "markdown"
        ? renderMarkdown(plan, { estimator: data.sources.estimator, instructionBudgets: data.sources.instructionBudgets })
        : renderText(plan);
  emit(body, cli);

  if (cli.failOverBudget && plan.budget.status === "over") {
    console.error(
      `\n::error::${plan.pattern.name} costs ${plan.totals.totalStroops.toLocaleString("en-US")} stroops ` +
        `against a budget of ${plan.budget.budgetStroops.toLocaleString("en-US")} ` +
        `(${plan.budget.utilisationPct}% of budget).`,
    );
    process.exit(1);
  }
  if (cli.failOverBudget && plan.budget.status === "unset") {
    console.error("error: --fail-over-budget needs --budget-xlm or --budget-stroops");
    process.exit(2);
  }

  process.exit(0);
}

function emit(body: string, cli: Cli): void {
  if (cli.out) {
    const outPath = path.isAbsolute(cli.out) ? cli.out : path.resolve(process.cwd(), cli.out);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, `${body}\n`);
    console.error(`Wrote ${cli.format} plan to ${path.relative(REPO_ROOT, outPath)}`);
    return;
  }
  console.log(body);
}

main();
