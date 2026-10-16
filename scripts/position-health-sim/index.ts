#!/usr/bin/env node
/**
 * Lending pool position health simulation CLI (#1013).
 *
 * Simulates market conditions against the lending pool's own health factor and
 * liquidation threshold math, so a change in prices can be checked before it
 * happens on-chain.
 *
 * Run (no install needed on Node >= 22):
 *   node --experimental-strip-types scripts/position-health-sim/index.ts
 *   node --experimental-strip-types scripts/position-health-sim/index.ts \
 *     --position leveraged-xlm-borrow --grid flash_crash
 *   node --experimental-strip-types scripts/position-health-sim/index.ts \
 *     --position healthy-xlm-borrow --format markdown --out health.md
 *   node --experimental-strip-types scripts/position-health-sim/index.ts \
 *     --fail-on liquidation --scenario luna-ust-collapse
 *   node --experimental-strip-types scripts/position-health-sim/index.ts --list-scenarios
 *
 * Exit codes: 0 ok, 1 gate failed, 2 usage error.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import {
  assertContractConstants,
  loadContractConstants,
  missingConstants,
  type ContractConstants,
} from "./contract.ts";
import { evaluatePosition, type Position } from "./health-factor.ts";
import { renderMarkdown, renderText, toJson } from "./report.ts";
import {
  findScenario,
  getShockGrid,
  loadScenarioDir,
  shockGridNames,
  type LoadedScenario,
} from "./scenarios.ts";
import {
  breakEven,
  defaultThresholdGrid,
  distanceToLiquidation,
  riskLevel,
  simulateScenario,
  simulateShocks,
  sweepThresholds,
  unmatchedAssets,
  type SimulationReport,
} from "./simulation.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");

const DEFAULT_CONTRACT_SRC = "stellar-lend/contracts/lending/src";
const DEFAULT_SCENARIO_DIR = "scenarios";
const DEFAULT_POSITION_DIR = "scripts/position-health-sim/positions";
const DEFAULT_POSITION = "leveraged-xlm-borrow";
const DEFAULT_GRID = "flash_crash";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

function flag(name: string): boolean {
  return process.argv.includes(`--${name}`);
}

const USAGE = `Lending pool position health simulation (#1013)

Usage:
  index.ts [options]

Options:
  --position <name|file>   Position to simulate (default: ${DEFAULT_POSITION})
  --contract-src <path>    Contract source to read the constants from
                           (default: ${DEFAULT_CONTRACT_SRC}). Point this at a
                           different checkout or revision to simulate against that.
  --positions-dir <path>   Where the named positions live
  --scenario <id>          Historical scenario from scenarios/ (repeatable)
  --grid <name>            Price shock grid (default: ${DEFAULT_GRID})
  --threshold-bps <n>      Override liquidation_threshold_bps for the position
  --close-factor-bps <n>   Override close_factor_bps
  --incentive-bps <n>      Override liquidation_incentive_bps
  --thresholds <list>      Threshold sweep values in bps (default: the governance grid)
  --no-scenarios           Skip the historical scenario table
  --format <fmt>           text | json | markdown (default: text)
  --out <file>             Write the report to a file instead of stdout
  --fail-on <level>        Fail when any price-grid step reaches this risk level
                          (liquidatable|critical|at-risk|moderate|unknown)
  --list-scenarios         Print the available scenarios and shock grids, then exit
  --help                   Print this help and exit

Exit codes:
  0 ok · 1 gate failed · 2 usage error
`;

const RISK_LEVELS = ["liquidatable", "critical", "at-risk", "moderate", "safe", "unknown"] as const;
type RiskLevelName = (typeof RISK_LEVELS)[number];

interface Cli {
  position: string;
  positionsDir: string;
  scenarioIds: string[];
  grid: string;
  thresholdBps?: bigint;
  closeFactorBps?: bigint;
  incentiveBps?: bigint;
  thresholds?: bigint[];
  includeScenarios: boolean;
  format: "text" | "json" | "markdown";
  out?: string;
  failOn?: RiskLevelName;
  scenarioDir: string;
  contractSrc: string;
}

/** The JSON shape of a position file. */
interface PositionFile {
  name?: string;
  description?: string;
  oraclePresent?: boolean;
  collateral: { asset: string; amount: string; price: string }[];
  debt: { asset: string; amount: string; price: string };
  liquidationThresholdBps?: string;
  closeFactorBps?: string;
  liquidationIncentiveBps?: string;
}

function parseBigIntField(label: string, value: unknown): bigint {
  if (typeof value === "number" && Number.isInteger(value)) return BigInt(value);
  if (typeof value === "string" && /^-?\d+$/.test(value.trim())) return BigInt(value.trim());
  throw new Error(`${label} must be an integer or an integer string, got ${JSON.stringify(value)}`);
}

/** Read a position file, validating the shape rather than trusting it. */
export function loadPosition(source: string, origin: string): { position: Position; name: string; description: string } {
  let raw: unknown;
  try {
    raw = JSON.parse(source);
  } catch (err) {
    throw new Error(`${origin}: not valid JSON (${(err as Error).message})`);
  }
  if (!raw || typeof raw !== "object") throw new Error(`${origin}: expected a JSON object`);
  const file = raw as PositionFile;

  if (!Array.isArray(file.collateral) || file.collateral.length === 0) {
    throw new Error(`${origin}: "collateral" must be a non-empty array`);
  }
  if (!file.debt || typeof file.debt !== "object") {
    throw new Error(`${origin}: "debt" is required`);
  }

  const leg = (entry: { asset: string; amount: unknown; price: unknown }, label: string) => {
    if (typeof entry.asset !== "string" || entry.asset.length === 0) {
      throw new Error(`${origin}: ${label} needs an "asset"`);
    }
    return {
      asset: entry.asset,
      amount: parseBigIntField(`${origin}: ${label}.amount`, entry.amount),
      price: parseBigIntField(`${origin}: ${label}.price`, entry.price),
    };
  };

  const assets = new Set<string>();
  const collateral = file.collateral.map((entry, i) => {
    const parsed = leg(entry, `collateral[${i}]`);
    if (assets.has(parsed.asset)) {
      throw new Error(`${origin}: asset "${parsed.asset}" appears twice; the contract keys collateral by user, not per asset`);
    }
    assets.add(parsed.asset);
    return parsed;
  });

  const debt = leg(file.debt, "debt");
  if (assets.has(debt.asset)) {
    throw new Error(
      `${origin}: the debt asset "${debt.asset}" is also listed as collateral; ` +
        "the contract prices collateral and debt separately",
    );
  }

  const position: Position = {
    collateral,
    debt,
    oraclePresent: file.oraclePresent !== false,
  };
  if (file.liquidationThresholdBps !== undefined) {
    position.liquidationThresholdBps = parseBigIntField(`${origin}: liquidationThresholdBps`, file.liquidationThresholdBps);
  }
  if (file.closeFactorBps !== undefined) {
    position.closeFactorBps = parseBigIntField(`${origin}: closeFactorBps`, file.closeFactorBps);
  }
  if (file.liquidationIncentiveBps !== undefined) {
    position.liquidationIncentiveBps = parseBigIntField(`${origin}: liquidationIncentiveBps`, file.liquidationIncentiveBps);
  }

  return {
    position,
    name: file.name ?? path.basename(origin, ".json"),
    description: file.description ?? "",
  };
}

/** All `--flag` occurrences, so `--scenario` can be repeated. */
function allArgs(name: string): string[] {
  const out: string[] = [];
  process.argv.forEach((value, index) => {
    if (value === `--${name}` && process.argv[index + 1]) out.push(process.argv[index + 1]);
  });
  return out;
}

function parseArgs(): Cli {
  if (flag("help") || flag("h")) {
    console.log(USAGE);
    process.exit(0);
  }

  const format = (arg("format") ?? "text") as Cli["format"];
  if (format !== "text" && format !== "json" && format !== "markdown") {
    throw new Error(`--format must be text|json|markdown, got "${format}"`);
  }

  const failOnRaw = arg("fail-on");
  if (failOnRaw && !RISK_LEVELS.includes(failOnRaw as RiskLevelName)) {
    throw new Error(`--fail-on must be one of ${RISK_LEVELS.join("|")}, got "${failOnRaw}"`);
  }

  const thresholdsRaw = arg("thresholds");
  let thresholds: bigint[] | undefined;
  if (thresholdsRaw !== undefined) {
    thresholds = thresholdsRaw.split(",").map((part) => {
      const trimmed = part.trim();
      if (!/^\d+$/.test(trimmed)) throw new Error(`--thresholds must be a comma-separated list of bps, got "${trimmed}"`);
      return BigInt(trimmed);
    });
    if (thresholds.length === 0) throw new Error("--thresholds must list at least one value");
    for (const value of thresholds) {
      if (value <= 0n || value > 10_000n) {
        throw new Error(`--thresholds values must be in 1..10000 bps, got ${value}`);
      }
    }
  }

  const optionalBps = (name: string): bigint | undefined => {
    const raw = arg(name);
    if (raw === undefined) return undefined;
    if (!/^\d+$/.test(raw.trim())) throw new Error(`--${name} must be an integer, got "${raw}"`);
    return BigInt(raw.trim());
  };

  return {
    position: arg("position") ?? DEFAULT_POSITION,
    positionsDir: arg("positions-dir") ?? DEFAULT_POSITION_DIR,
    scenarioIds: allArgs("scenario"),
    grid: arg("grid") ?? DEFAULT_GRID,
    thresholdBps: optionalBps("threshold-bps"),
    closeFactorBps: optionalBps("close-factor-bps"),
    incentiveBps: optionalBps("incentive-bps"),
    thresholds,
    includeScenarios: !flag("no-scenarios"),
    format,
    out: arg("out"),
    failOn: failOnRaw as RiskLevelName | undefined,
    scenarioDir: arg("scenarios-dir") ?? DEFAULT_SCENARIO_DIR,
    contractSrc: arg("contract-src") ?? DEFAULT_CONTRACT_SRC,
  };
}

function resolveFromRepo(relative: string): string {
  return path.isAbsolute(relative) ? relative : path.resolve(REPO_ROOT, relative);
}

function loadConstants(contractSrc: string): ContractConstants {
  const viewsPath = resolveFromRepo(`${contractSrc}/views.rs`);
  const borrowPath = resolveFromRepo(`${contractSrc}/borrow.rs`);
  for (const file of [viewsPath, borrowPath]) {
    if (!fs.existsSync(file)) throw new Error(`contract source not found: ${file}`);
  }
  const views = fs.readFileSync(viewsPath, "utf8");
  const borrow = fs.readFileSync(borrowPath, "utf8");

  const missing = missingConstants({ views, borrow });
  assertContractConstants(missing);

  return loadContractConstants({
    views,
    borrow,
    paths: {
      views: `${contractSrc}/views.rs`,
      borrow: `${contractSrc}/borrow.rs`,
    },
  });
}

/** Resolve `--position` to a file, accepting a bare name or a path. */
function resolvePositionFile(name: string, dir: string): string {
  const direct = path.isAbsolute(name) ? name : path.resolve(process.cwd(), name);
  if (fs.existsSync(direct) && fs.statSync(direct).isFile()) return direct;

  const base = resolveFromRepo(dir);
  if (fs.existsSync(base) && fs.statSync(base).isFile()) return base;

  const named = path.join(base, name.endsWith(".json") ? name : `${name}.json`);
  if (fs.existsSync(named)) return named;

  const available = fs.existsSync(base)
    ? fs.readdirSync(base).filter((f) => f.endsWith(".json")).map((f) => f.replace(/\.json$/, "")).sort()
    : [];
  throw new Error(
    `unknown position "${name}". Available: ${available.join(", ") || "(none)"}`,
  );
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

  let constants: ContractConstants;
  let scenarios: LoadedScenario[] = [];
  let positionFile: Position;
  let positionName: string;

  try {
    constants = loadConstants(cli.contractSrc);
    if (flag("list-scenarios")) {
      scenarios = loadScenarioDir(resolveFromRepo(cli.scenarioDir));
      console.log("Scenarios (scenarios/*.json):");
      for (const scenario of scenarios) {
        console.log(`  ${scenario.id.padEnd(24)} ${scenario.name}`);
      }
      console.log("\nShock grids:");
      for (const name of shockGridNames()) console.log(`  ${name}`);
      process.exit(0);
    }

    const file = resolvePositionFile(cli.position, cli.positionsDir);
    const loaded = loadPosition(fs.readFileSync(file, "utf8"), path.basename(file));
    positionFile = {
      ...loaded.position,
      liquidationThresholdBps: cli.thresholdBps ?? loaded.position.liquidationThresholdBps,
      closeFactorBps: cli.closeFactorBps ?? loaded.position.closeFactorBps,
      liquidationIncentiveBps: cli.incentiveBps ?? loaded.position.liquidationIncentiveBps,
    };
    positionName = loaded.name;
    if (cli.includeScenarios) scenarios = loadScenarioDir(resolveFromRepo(cli.scenarioDir));
  } catch (err) {
    console.error(`error: ${(err as Error).message}`);
    process.exit(2);
  }

  const baseline = evaluatePosition(positionFile, constants);
  const report: SimulationReport = {
    positionName,
    constants,
    baseline,
    baselineRisk: riskLevel(baseline.healthFactor, baseline.healthFactorKnown, constants),
    breakEven: breakEven(baseline, constants),
    distance: distanceToLiquidation(baseline, constants),
    scenarios:
      cli.includeScenarios && scenarios.length > 0
        ? cli.scenarioIds.length > 0
          ? cli.scenarioIds.map((id) => {
              const scenario = findScenario(scenarios, id);
              return {
                scenario,
                step: simulateScenario(positionFile, scenario, constants, baseline.healthFactor),
                unmatched: unmatchedAssets(positionFile, scenario),
              };
            })
          : scenarios.map((scenario) => ({
              scenario,
              step: simulateScenario(positionFile, scenario, constants, baseline.healthFactor),
              unmatched: unmatchedAssets(positionFile, scenario),
            }))
        : [],
    grid: { grid: getShockGrid(cli.grid), steps: simulateShocks(positionFile, getShockGrid(cli.grid), constants) },
    thresholds: {
      defaultBps: constants.defaultLiquidationThresholdBps,
      steps: sweepThresholds(
        positionFile,
        constants,
        cli.thresholds ?? defaultThresholdGrid(constants),
      ),
    },
    thresholdAtLiquidation: findThresholdAtLiquidation(positionFile, constants, cli.thresholds ?? defaultThresholdGrid(constants)),
  };

  const body =
    cli.format === "json"
      ? JSON.stringify(toJson(report), null, 2)
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

  if (cli.failOn) {
    const order = RISK_LEVELS;
    const threshold = order.indexOf(cli.failOn);
    const offenders = [
      ...report.grid.steps.map((s) => ({ where: s.label, risk: s.risk })),
      ...report.scenarios.map((s) => ({ where: s.scenario.name, risk: s.step.risk })),
    ].filter((entry) => order.indexOf(entry.risk as RiskLevelName) <= threshold);
    if (offenders.length > 0) {
      console.error(
        `\n::error::${offenders.length} market condition(s) reach "${cli.failOn}" or worse: ` +
          offenders.map((o) => `${o.where} (${o.risk})`).join(", "),
      );
      process.exit(1);
    }
  }

  process.exit(0);
}

/**
 * The lowest threshold at which the position becomes liquidatable — the
 * governance question a threshold change actually turns on.
 */
function findThresholdAtLiquidation(
  position: Position,
  constants: ContractConstants,
  thresholds: bigint[],
): bigint | null {
  const liquidatable = sweepThresholds(position, constants, thresholds).filter((s) => s.isLiquidatable);
  if (liquidatable.length === 0) return null;
  return liquidatable
    .map((step) => step.thresholdBps)
    .reduce((lowest, bps) => (bps < lowest ? bps : lowest));
}

main();
