#!/usr/bin/env node
/**
 * Lending pool gas cost estimator CLI (#1011).
 *
 * Statically indexes the Soroban lending contract's storage accesses per entry
 * point, prices them, and suggests storage-pattern optimizations.
 *
 * Run (no install needed on Node >= 22):
 *   node --experimental-strip-types scripts/lending-gas-estimator/index.ts
 *   node --experimental-strip-types scripts/lending-gas-estimator/index.ts \
 *     --operations deposit,borrow,repay --format markdown --out report.md
 *   node --experimental-strip-types scripts/lending-gas-estimator/index.ts \
 *     --iterations 5 --fail-on high
 *   node --experimental-strip-types scripts/lending-gas-estimator/index.ts \
 *     --compare-source api/src/services/gas/estimator.ts
 *
 * Exit codes: 0 ok, 1 gate failed, 2 usage error.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import {
  buildReport,
  parseBaselines,
  parseComplexityTable,
  parseSharedCostConstants,
  renderMarkdown,
  renderText,
  type ComplexityTable,
} from "./estimator.ts";
import { indexContract, resolveAllOperations } from "./storage-indexer.ts";
import { RULE_IDS } from "./suggestions.ts";
import { SEVERITY_ORDER, type CostModel, type ResourceBaseline, type Severity } from "./types.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");
const DEFAULT_CONTRACT_DIR = "stellar-lend/contracts/lending";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

function flag(name: string): boolean {
  return process.argv.includes(`--${name}`);
}

const USAGE = `Lending pool gas cost estimator (#1011)

Usage:
  index.ts [options]

Options:
  --contract-dir <path>   Contract source root (default: ${DEFAULT_CONTRACT_DIR})
  --operations <list>     Comma-separated entry points (default: all)
  --iterations <n>        Multiplier for storage access inside a loop (default: 1)
  --format <fmt>          text | json | markdown (default: text)
  --out <file>            Write the report to a file instead of stdout
  --fail-on <severity>    Fail when a suggestion reaches this severity
                          (info|low|medium|high|critical, default: critical)
  --min-severity <sev>    Hide suggestions below this severity
  --disable <ids>         Comma-separated suggestion rule ids to skip
  --only <ids>            Comma-separated suggestion rule ids to keep
  --compare-source <path> Diff the static index against a hand-maintained
                          OPERATION_COMPLEXITY table and adopt its stroop
                          constants, so this tool and /api/gas/estimate share
                          one source of truth
                          (default: api/src/services/gas/estimator.ts when present)
  --baselines <path>      Measured CPU/memory per operation. Accepts a
                          { operation: { cpuInstructions, memoryBytes } } map or
                          the committed stellar-lend/benchmarks/gas-baseline.json
                          ({ benchmarks: [{ operation, cpu_insns, mem_bytes }] })
  --list-rules            Print the available suggestion rule ids and exit
  --help                  Print this help and exit

Exit codes:
  0 ok · 1 gate failed · 2 usage error

Suggestion rules:
  ${RULE_IDS.join("\n  ")}
`;

interface Cli {
  contractDir: string;
  operations?: string[];
  iterations: number;
  format: "text" | "json" | "markdown";
  out?: string;
  failOn: Severity;
  minSeverity?: Severity;
  disable?: string[];
  only?: string[];
  compareSource?: string;
  baselines: Record<string, ResourceBaseline>;
  complexityTable: ComplexityTable | null;
  model: Partial<CostModel>;
}

const SEVERITIES: Severity[] = ["info", "low", "medium", "high", "critical"];

function parseSeverity(value: string, label: string): Severity {
  if (!SEVERITIES.includes(value as Severity)) {
    throw new Error(`${label} must be one of ${SEVERITIES.join("|")}, got "${value}"`);
  }
  return value as Severity;
}

function parseArgs(): Cli {
  if (flag("list-rules")) {
    console.log(RULE_IDS.join("\n"));
    process.exit(0);
  }
  if (flag("help") || flag("h")) {
    console.log(USAGE);
    process.exit(0);
  }

  const format = (arg("format") ?? "text") as Cli["format"];
  if (format !== "text" && format !== "json" && format !== "markdown") {
    throw new Error(`--format must be text|json|markdown, got "${format}"`);
  }

  const iterationsRaw = arg("iterations");
  const iterations = iterationsRaw === undefined ? 1 : Number.parseInt(iterationsRaw, 10);
  if (!Number.isFinite(iterations) || iterations < 1) {
    throw new Error(`--iterations must be a positive integer, got "${iterationsRaw}"`);
  }

  const contractDirRaw = arg("contract-dir") ?? DEFAULT_CONTRACT_DIR;
  const contractDir = path.isAbsolute(contractDirRaw)
    ? contractDirRaw
    : path.resolve(REPO_ROOT, contractDirRaw);
  if (!fs.existsSync(contractDir)) {
    throw new Error(`--contract-dir not found: ${contractDirRaw}`);
  }

  let baselines: Record<string, ResourceBaseline> = {};
  const baselinesPath = arg("baselines");
  if (baselinesPath) {
    const resolved = path.isAbsolute(baselinesPath) ? baselinesPath : path.resolve(process.cwd(), baselinesPath);
    if (!fs.existsSync(resolved)) throw new Error(`--baselines not found: ${baselinesPath}`);
    baselines = parseBaselines(fs.readFileSync(resolved, "utf8"), resolved);
    if (Object.keys(baselines).length === 0) {
      console.error(`::warning::no CPU/memory measurements found in ${baselinesPath}`);
    } else {
      console.error(
        `::notice::loaded ${Object.keys(baselines).length} measured operation(s) from ${baselinesPath}`,
      );
    }
  }

  // Default to the API estimator so the constants are adopted without ceremony;
  // an explicit path overrides it, and a missing file just disables the check.
  const compareSource = arg("compare-source") ?? path.join(REPO_ROOT, "api/src/services/gas/estimator.ts");
  let complexityTable: ComplexityTable | null = null;
  let model: Partial<CostModel> = {};
  if (fs.existsSync(compareSource)) {
    const source = fs.readFileSync(compareSource, "utf8");
    complexityTable = parseComplexityTable(source);
    model = parseSharedCostConstants(source);
    if (!complexityTable) {
      console.error(`::warning::no OPERATION_COMPLEXITY table found in ${compareSource}; drift check skipped`);
    }
  } else if (arg("compare-source")) {
    throw new Error(`--compare-source not found: ${arg("compare-source")}`);
  }

  const operationsRaw = arg("operations");
  const disableRaw = arg("disable");
  const onlyRaw = arg("only");
  const minSeverityRaw = arg("min-severity");

  return {
    contractDir,
    operations: operationsRaw ? operationsRaw.split(",").map((s) => s.trim()).filter(Boolean) : undefined,
    iterations,
    format,
    out: arg("out"),
    failOn: parseSeverity(arg("fail-on") ?? "critical", "--fail-on"),
    minSeverity: minSeverityRaw ? parseSeverity(minSeverityRaw, "--min-severity") : undefined,
    disable: disableRaw ? disableRaw.split(",").map((s) => s.trim()).filter(Boolean) : undefined,
    only: onlyRaw ? onlyRaw.split(",").map((s) => s.trim()).filter(Boolean) : undefined,
    compareSource,
    baselines,
    complexityTable,
    model,
  };
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

  const index = indexContract(cli.contractDir);
  const known = new Set(index.entryPoints.map((fn) => fn.name));
  const unknown = (cli.operations ?? []).filter((op) => !known.has(op));
  if (unknown.length > 0) {
    console.error(
      `error: unknown operation(s): ${unknown.join(", ")}\n` +
        `available: ${[...known].sort().join(", ")}`,
    );
    process.exit(2);
  }

  const facts = resolveAllOperations(index, { operations: cli.operations });
  if (facts.length === 0) {
    console.error(`error: no #[contractimpl] entry points found under ${cli.contractDir}`);
    process.exit(2);
  }

  const disabled = [...(cli.disable ?? [])];
  const enabled = cli.only;
  if (enabled) {
    for (const id of enabled) {
      if (!RULE_IDS.includes(id)) {
        console.error(`error: unknown rule id "${id}". Use --list-rules.`);
        process.exit(2);
      }
    }
    for (const id of RULE_IDS) if (!enabled.includes(id)) disabled.push(id);
  }

  const report = buildReport(facts, {
    contract: index.contract,
    sourceDir: path.relative(REPO_ROOT, cli.contractDir) || cli.contractDir,
    iterations: cli.iterations,
    baselines: cli.baselines,
    complexityTable: cli.complexityTable,
    model: cli.model,
    suggestions: {
      minSeverity: cli.minSeverity,
      disabled,
    },
  });

  const body =
    cli.format === "json"
      ? JSON.stringify(report, null, 2)
      : cli.format === "markdown"
        ? renderMarkdown(report)
        : renderText(report);

  if (cli.out) {
    const outPath = path.isAbsolute(cli.out) ? cli.out : path.resolve(process.cwd(), cli.out);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, `${body}\n`);
    console.error(`Wrote ${cli.format} report to ${path.relative(REPO_ROOT, outPath)}`);
  } else {
    console.log(body);
  }

  const worst = report.totals.worstSeverity;
  if (worst && SEVERITY_ORDER[worst] >= SEVERITY_ORDER[cli.failOn]) {
    const offenders = report.suggestions.filter(
      (s) => SEVERITY_ORDER[s.severity] >= SEVERITY_ORDER[cli.failOn],
    );
    console.error(
      `\n::error::${offenders.length} storage-pattern finding(s) at or above "${cli.failOn}" severity`,
    );
    for (const s of offenders.slice(0, 10)) console.error(`  - [${s.severity}] ${s.id}: ${s.title}`);
    process.exit(1);
  }

  process.exit(0);
}

main();
