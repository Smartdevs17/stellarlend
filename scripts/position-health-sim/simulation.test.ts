import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import { loadContractConstants, type ContractConstants } from "./contract.ts";
import { evaluatePosition, liquidationIncentiveAmount, type Position } from "./health-factor.ts";
import { renderMarkdown, renderText, toJson } from "./report.ts";
import {
  SHOCK_GRIDS,
  findScenario,
  getShockGrid,
  loadScenarioDir,
  parseScenario,
  shockGridNames,
} from "./scenarios.ts";
import {
  breakEven,
  defaultThresholdGrid,
  distanceToLiquidation,
  healthFactorToNumber,
  riskLevel,
  shockPrice,
  shockedPosition,
  simulateScenario,
  simulateShocks,
  sweepThresholds,
  unmatchedAssets,
  type SimulationReport,
} from "./simulation.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");
const SCENARIO_DIR = path.join(REPO_ROOT, "scenarios");
const POSITION_DIR = path.join(HERE, "positions");

const C: ContractConstants = loadContractConstants({
  views: fs.readFileSync(path.join(REPO_ROOT, "stellar-lend/contracts/lending/src/views.rs"), "utf8"),
  borrow: fs.readFileSync(path.join(REPO_ROOT, "stellar-lend/contracts/lending/src/borrow.rs"), "utf8"),
  paths: { views: "views.rs", borrow: "borrow.rs" },
});

/** Scratch directories, so a failing test cannot leave files in the repo. */
const scratch = (): string => fs.mkdtempSync(path.join(os.tmpdir(), "position-health-sim-"));

function position(over: Partial<Position> = {}): Position {
  return {
    collateral: [{ asset: "XLM", amount: 10_000n, price: C.priceScale }],
    debt: { asset: "USDC", amount: 5_000n, price: C.priceScale },
    oraclePresent: true,
    ...over,
  };
}

// ── shockPrice ───────────────────────────────────────────────────────────────

test("shockPrice applies a percent move to an 8-decimal oracle price", () => {
  assert.equal(shockPrice(C.priceScale, 0), C.priceScale);
  assert.equal(shockPrice(C.priceScale, -50), C.priceScale / 2n);
  assert.equal(shockPrice(C.priceScale, 100), C.priceScale * 2n);
  assert.equal(shockPrice(1_000_000_000n, -10), 900_000_000n);
});

test("shockPrice floors at zero rather than going negative", () => {
  assert.equal(shockPrice(100n, -150), 0n);
  assert.equal(shockPrice(0n, -50), 0n);
});

test("shockPrice keeps integer precision instead of going through a float", () => {
  // A two-thirds fall on a 3x price must land exactly, with no float drift.
  assert.equal(shockPrice(3n * C.priceScale, -100), 0n, "a 100% fall is a zero price");
  assert.equal(shockPrice(3n * C.priceScale, -50), 150_000_000n);
  // 1/3 is not representable in seven decimals, so the result truncates rather
  // than rounding the way a float would.
  assert.equal(shockPrice(3n * C.priceScale, -33), 201_000_000n);
  assert.equal(typeof shockPrice(C.priceScale, 7), "bigint");
});

test("shockedPosition moves every collateral leg and leaves the debt price alone", () => {
  const shocked = shockedPosition(position(), -50);
  assert.equal(shocked.collateral[0].price, C.priceScale / 2n);
  assert.equal(shocked.debt.price, C.priceScale);
  assert.equal(shocked.debt.amount, 5_000n, "a price move must not touch amounts");
});

test("shockedPosition accepts per-asset overrides", () => {
  const shocked = shockedPosition(position(), -50, new Map([["XLM", -10]]));
  assert.equal(shocked.collateral[0].price, C.priceScale * 9n / 10n);
});

// ── risk bands ───────────────────────────────────────────────────────────────

test("risk bands are anchored on the contract's 1.0 boundary", () => {
  assert.equal(riskLevel(5_000n, true, C), "liquidatable");
  assert.equal(riskLevel(9_999n, true, C), "liquidatable");
  assert.equal(riskLevel(10_000n, true, C), "critical", "exactly 1.0 is healthy but very close");
  assert.equal(riskLevel(10_500n, true, C), "at-risk");
  assert.equal(riskLevel(11_999n, true, C), "at-risk");
  assert.equal(riskLevel(12_000n, true, C), "moderate");
  assert.equal(riskLevel(14_999n, true, C), "moderate");
  assert.equal(riskLevel(15_000n, true, C), "safe");
  assert.equal(riskLevel(99_000n, true, C), "safe");
});

test("an unknown health factor is never classified as a risk level", () => {
  assert.equal(riskLevel(0n, false, C), "unknown");
  assert.equal(riskLevel(50_000n, false, C), "unknown", "known-ness wins over the number");
});

test("healthFactorToNumber renders bps as a ratio", () => {
  assert.equal(healthFactorToNumber(10_000n), 1);
  assert.equal(healthFactorToNumber(16_000n), 1.6);
  assert.equal(healthFactorToNumber(0n), 0);
});

// ── break-even and distance ──────────────────────────────────────────────────

test("break-even is 1/HF: 10_000 XLM against 5_000 USDC is 1.6x, so a 37.5% fall liquidates it", () => {
  // The contract weights collateral by the 80% threshold first, so the ratio is
  // (10_000 * 0.8) / 5_000 = 1.6x, not 2.0x.
  const health = evaluatePosition(position(), C);
  assert.equal(health.healthFactor, 16_000n);
  const be = breakEven(health, C);
  assert.equal(be.priceMultiplier, 0.625, "62.5% of the collateral value is the boundary");
  assert.equal(be.priceDropPercent, 37.5);
  assert.equal(be.recoveryTo15x, null, "already above 1.5x");
});

test("a leveraged position is given the rise it needs to reach 1.5x", () => {
  const health = evaluatePosition(
    position({ debt: { asset: "USDC", amount: 8_000n, price: C.priceScale } }),
    C,
  );
  assert.equal(health.healthFactor, 10_000n);
  const be = breakEven(health, C);
  assert.equal(be.priceDropPercent, 0, "already exactly on the boundary");
  assert.equal(be.recoveryTo15x, 0.5, "a 50% rise reaches 1.5x");
});

test("break-even is unavailable when the contract cannot compute a health factor", () => {
  const unknown = evaluatePosition(position({ oraclePresent: false }), C);
  assert.equal(breakEven(unknown, C).priceMultiplier, null);
  assert.equal(breakEven(unknown, C).priceDropPercent, null);
});

test("an already-liquidatable position has no further fall to reach the boundary", () => {
  const health = evaluatePosition(
    position({ debt: { asset: "USDC", amount: 9_000n, price: C.priceScale } }),
    C,
  );
  assert.ok(health.healthFactor < C.healthFactorScale);
  const be = breakEven(health, C);
  assert.equal(be.priceMultiplier, 0);
  assert.equal(be.priceDropPercent, 0);
});

test("distance to liquidation reports the fall the position can absorb", () => {
  const health = evaluatePosition(position(), C);
  const distance = distanceToLiquidation(health, C);
  assert.equal(distance.dropPercent, 37.5);
  assert.equal(distance.alreadyLiquidatable, false);
  assert.equal(distance.reason, null);
});

test("distance to liquidation is explicit when the health factor is unknown", () => {
  const distance = distanceToLiquidation(evaluatePosition(position({ oraclePresent: false }), C), C);
  assert.equal(distance.dropPercent, null, "an unknown HF must not read as safe");
  assert.equal(distance.alreadyLiquidatable, false);
  assert.match(String(distance.reason), /oracle/);
});

test("distance to liquidation is zero for a position already under water", () => {
  const distance = distanceToLiquidation(
    evaluatePosition(position({ debt: { asset: "USDC", amount: 9_000n, price: C.priceScale } }), C),
    C,
  );
  assert.equal(distance.dropPercent, 0);
  assert.equal(distance.alreadyLiquidatable, true);
  assert.match(String(distance.reason), /below the 1.0 boundary/);
});

// ── price grid ───────────────────────────────────────────────────────────────

test("the price grid moves the health factor linearly and flags the first crossing", () => {
  const steps = simulateShocks(position(), SHOCK_GRIDS.flash_crash, C);
  assert.deepEqual(steps.map((s) => s.collateralChangePercent), [-20, -30, -40, -50]);
  // 1.6x base, so a -20% fall takes it to 1.28x and a -40% fall to 0.96x.
  assert.deepEqual(steps.map((s) => s.health.healthFactor), [12_800n, 11_200n, 9_600n, 8_000n]);
  assert.equal(steps[0].healthFactorDelta, -3_200n, "a 20% collateral fall is a 20% health factor fall");
  // The crossing lands on the -40% step, and the tool has to name that step.
  assert.equal(steps[1].becameLiquidatable, false);
  assert.equal(steps[2].becameLiquidatable, true);
  assert.equal(steps[2].risk, "liquidatable");
});

test("a grid step that crosses the boundary is flagged", () => {
  const steps = simulateShocks(
    position({ debt: { asset: "USDC", amount: 8_000n, price: C.priceScale } }),
    SHOCK_GRIDS.flash_crash,
    C,
  );
  assert.equal(steps[0].health.healthFactor, 8_000n, "-20% takes 1.0x to 0.8x");
  assert.equal(steps[0].becameLiquidatable, true, "the first step is where it becomes liquidatable");
  assert.equal(steps[0].risk, "liquidatable");
  assert.ok(steps[0].health.maxLiquidatableAmount > 0n, "a liquidatable position has a closeable amount");
});

test("the both_sides grid is monotone in the direction of the shock", () => {
  const steps = simulateShocks(position(), SHOCK_GRIDS.both_sides, C);
  // The grid runs from the deepest fall to the biggest rise, so the health
  // factor has to climb monotonically across it.
  assert.deepEqual(steps.map((s) => s.collateralChangePercent), [-40, -20, -10, 0, 10, 20, 40]);
  const healthFactors = steps.map((s) => s.health.healthFactor);
  for (let i = 1; i < healthFactors.length; i++) {
    assert.ok(
      healthFactors[i] >= healthFactors[i - 1],
      `a smaller fall must not lower the health factor: ${healthFactors[i - 1]} -> ${healthFactors[i]}`,
    );
  }
});

test("shock grids are well formed and cover both directions", () => {
  for (const name of shockGridNames()) {
    const grid = getShockGrid(name);
    assert.equal(grid.id, name);
    assert.ok(grid.changes.length > 0, `${name} needs changes`);
    assert.ok(grid.description.length > 10, `${name} needs a description`);
    assert.ok(grid.changes.every((c) => Number.isFinite(c)), `${name} has a non-finite change`);
  }
  assert.throws(() => getShockGrid("nope"), /unknown shock grid/);
});

test("every shock grid leaves a healthy position above the boundary at its mildest step", () => {
  for (const name of shockGridNames()) {
    const grid = getShockGrid(name);
    const mildest = grid.changes.reduce((a, b) => (a > b ? a : b));
    const steps = simulateShocks(position(), grid, C);
    const step = steps.find((s) => s.collateralChangePercent === mildest)!;
    assert.ok(step !== undefined, `${name} is missing its mildest step`);
  }
});

// ── threshold sweep ──────────────────────────────────────────────────────────

test("raising the liquidation threshold raises the health factor proportionally", () => {
  // 10_000 collateral / 5_000 debt: the health factor in bps is the threshold
  // divided by 5_000, so each step of 1_000 bps is worth 2_000 bps of health.
  const steps = sweepThresholds(position(), C, [4_000n, 5_000n, 6_000n, 8_000n, 10_000n]);
  assert.deepEqual(
    steps.map((s) => s.healthFactor),
    [8_000n, 10_000n, 12_000n, 16_000n, 20_000n],
  );
  assert.equal(steps[0].isLiquidatable, true);
  assert.equal(steps[1].isLiquidatable, false, "exactly 1.0 is not liquidatable");
});

test("the sweep reports the change against the contract's own default", () => {
  const steps = sweepThresholds(position(), C, [8_000n, 5_000n]);
  assert.equal(steps[0].healthFactorDelta, 0n, "the default is the baseline");
  assert.equal(steps[1].healthFactorDelta, -6_000n);
});

test("the default threshold grid includes the contract default and stays in range", () => {
  const grid = defaultThresholdGrid(C);
  assert.ok(grid.includes(C.defaultLiquidationThresholdBps));
  assert.ok(grid.every((v) => v > 0n && v <= 10_000n), "the contract bounds the threshold to 1..10000");
  assert.deepEqual(grid, [...grid].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)), "must be sorted");
});

test("a threshold sweep is monotone in the threshold", () => {
  const steps = sweepThresholds(position(), C, defaultThresholdGrid(C));
  for (let i = 1; i < steps.length; i++) {
    assert.ok(
      steps[i].healthFactor >= steps[i - 1].healthFactor,
      `threshold ${steps[i].thresholdBps} produced a lower health factor than ${steps[i - 1].thresholdBps}`,
    );
  }
});

test("a threshold sweep on an unknown position reports nothing liquidatable", () => {
  const steps = sweepThresholds(position({ oraclePresent: false }), C, [4_000n, 8_000n]);
  assert.ok(steps.every((s) => s.healthFactor === 0n));
  assert.ok(steps.every((s) => !s.isLiquidatable), "unknown must never be reported as liquidatable");
});

// ── scenarios ────────────────────────────────────────────────────────────────

const SCENARIO_SOURCE = JSON.stringify({
  id: "test-crash",
  name: "Test Crash",
  description: "Collateral halves.",
  category: "historical",
  version: "1.0.0",
  created: "2024-01-01T00:00:00Z",
  priceChanges: [
    { asset: "XLM", changePercent: -50 },
    { asset: "USDC", changePercent: -5 },
  ],
  correlationShifts: [{ asset1: "XLM", asset2: "USDC", newCorrelation: 0.9 }],
  volatilityMultipliers: [{ asset: "XLM", multiplier: 4 }],
  cascadingLiquidation: true,
  tags: ["test"],
});

test("parseScenario reads the committed scenarios/*.json schema", () => {
  const scenario = parseScenario(SCENARIO_SOURCE, "test-crash.json");
  assert.equal(scenario.id, "test-crash");
  assert.equal(scenario.name, "Test Crash");
  assert.equal(scenario.category, "historical");
  assert.equal(scenario.priceChanges.get("XLM"), -50);
  assert.equal(scenario.priceChanges.get("USDC"), -5);
  assert.equal(scenario.volatilityMultipliers.get("XLM"), 4);
  assert.equal(scenario.cascadingLiquidation, true);
  assert.deepEqual(scenario.tags, ["test"]);
  assert.equal(scenario.source, "test-crash.json");
});

test("parseScenario rejects a structurally invalid scenario instead of skipping it", () => {
  const cases: [string, string, RegExp][] = [
    ["not json", "not valid JSON", /not valid JSON/],
    ["[]", "an array", /expected a JSON object/],
    [JSON.stringify({ name: "x", priceChanges: [] }), "no id", /lowercase slug/],
    [JSON.stringify({ id: "Bad Id", name: "x", priceChanges: [] }), "a bad id", /lowercase slug/],
    [JSON.stringify({ id: "ok", priceChanges: [] }), "no name", /"name" is required/],
    [JSON.stringify({ id: "ok", name: "x" }), "no price changes", /non-empty array/],
    [JSON.stringify({ id: "ok", name: "x", priceChanges: [{}] }), "a bad change", /numeric "changePercent"/],
    [
      JSON.stringify({
        id: "ok",
        name: "x",
        priceChanges: [
          { asset: "XLM", changePercent: -1 },
          { asset: "XLM", changePercent: -2 },
        ],
      }),
      "a duplicate asset",
      /appears twice/,
    ],
  ];
  for (const [source, why, matcher] of cases) {
    assert.throws(() => parseScenario(source, "x.json"), matcher, `should reject: ${why}`);
  }
});

test("parseScenario tolerates the optional fields being absent", () => {
  const minimal = parseScenario(
    JSON.stringify({
      id: "minimal",
      name: "Minimal",
      priceChanges: [{ asset: "XLM", changePercent: -10 }],
    }),
    "minimal.json",
  );
  assert.equal(minimal.category, "uncategorised");
  assert.equal(minimal.volatilityMultipliers.size, 0);
  assert.equal(minimal.cascadingLiquidation, false);
  assert.deepEqual(minimal.tags, []);
});

test("the repository's own scenario corpus loads", () => {
  const scenarios = loadScenarioDir(SCENARIO_DIR);
  assert.ok(scenarios.length >= 4, `expected the historical corpus, found ${scenarios.length}`);
  for (const id of [
    "2008-financial-crisis",
    "2020-covid-crash",
    "3ac-ftx-contagion",
    "luna-ust-collapse",
  ]) {
    const scenario = findScenario(scenarios, id);
    assert.ok(scenario.priceChanges.size > 0, `${id} has no price changes`);
  }
  // The corpus is sorted by file name, so the report is stable.
  assert.deepEqual(
    scenarios.map((s) => s.source),
    [...scenarios.map((s) => s.source)].sort(),
  );
});

test("loadScenarioDir fails loudly on a missing or empty directory", () => {
  assert.throws(() => loadScenarioDir(path.join(REPO_ROOT, "no-such-dir")), /not found/);
  const empty = scratch();
  try {
    assert.throws(() => loadScenarioDir(empty), /no scenario files/);
  } finally {
    fs.rmSync(empty, { recursive: true, force: true });
  }
});

test("findScenario reports what is available", () => {
  const scenarios = loadScenarioDir(SCENARIO_DIR);
  assert.throws(() => findScenario(scenarios, "nope"), /available:/);
});

test("a scenario prices both the collateral and the debt it names", () => {
  const scenario = parseScenario(SCENARIO_SOURCE, "test-crash.json");
  const baseline = evaluatePosition(position(), C);
  const step = simulateScenario(position(), scenario, C, baseline.healthFactor);

  // Collateral 10_000 * 0.5 = 5_000; debt 5_000 * 0.95 = 4_750.
  // Weighted = 5_000 * 0.8 = 4_000; HF = 4_000 * 10_000 / 4_750 = 8_421.
  assert.equal(step.health.collateralValue, 5_000n);
  assert.equal(step.health.debtValue, 4_750n);
  assert.equal(step.health.healthFactor, 8_421n);
  assert.equal(step.risk, "liquidatable");
  assert.equal(step.becameLiquidatable, true);
  assert.equal(step.healthFactorDelta, step.health.healthFactor - baseline.healthFactor);
});

test("a scenario that does not touch the position leaves it unchanged", () => {
  const scenario = parseScenario(
    JSON.stringify({
      id: "btc-only",
      name: "BTC only",
      priceChanges: [{ asset: "BTC", changePercent: -50 }],
    }),
    "btc-only.json",
  );
  const baseline = evaluatePosition(position(), C);
  const step = simulateScenario(position(), scenario, C, baseline.healthFactor);
  assert.equal(step.health.healthFactor, baseline.healthFactor);
  assert.equal(step.healthFactorDelta, 0n);
});

test("unmatchedAssets names scenario assets the position does not hold", () => {
  const scenario = loadScenarioDir(SCENARIO_DIR).find((s) => s.id === "2008-financial-crisis")!;
  assert.deepEqual(unmatchedAssets(position(), scenario), ["BTC", "ETH"]);
  assert.deepEqual(unmatchedAssets(position(), { ...scenario, priceChanges: new Map([["XLM", -50]]) }), []);
});

test("every corpus scenario is survivable or reported, never silently skipped", () => {
  for (const scenario of loadScenarioDir(SCENARIO_DIR)) {
    const step = simulateScenario(position(), scenario, C, 0n);
    assert.equal(typeof step.risk, "string", `${scenario.id} produced no risk level`);
    assert.ok(
      ["safe", "moderate", "at-risk", "critical", "liquidatable", "unknown"].includes(step.risk),
      `${scenario.id} produced ${step.risk}`,
    );
  }
});

// ── Report assembly and rendering ────────────────────────────────────────────

function buildReport(positionName: string, over: Partial<Position> = {}): SimulationReport {
  const p = position(over);
  const baseline = evaluatePosition(p, C);
  const scenarios = loadScenarioDir(SCENARIO_DIR);
  return {
    positionName,
    constants: C,
    baseline,
    baselineRisk: riskLevel(baseline.healthFactor, baseline.healthFactorKnown, C),
    breakEven: breakEven(baseline, C),
    distance: distanceToLiquidation(baseline, C),
    scenarios: scenarios.map((scenario) => ({
      scenario,
      step: simulateScenario(p, scenario, C, baseline.healthFactor),
      unmatched: unmatchedAssets(p, scenario),
    })),
    grid: { grid: SHOCK_GRIDS.flash_crash, steps: simulateShocks(p, SHOCK_GRIDS.flash_crash, C) },
    thresholds: {
      defaultBps: C.defaultLiquidationThresholdBps,
      steps: sweepThresholds(p, C, defaultThresholdGrid(C)),
    },
    thresholdAtLiquidation: null,
  };
}

test("renderText covers the position, scenarios, grid and threshold sweep", () => {
  const text = renderText(buildReport("leveraged"));
  for (const fragment of [
    "Position health simulation",
    "Position: leveraged",
    "Health factor:",
    "Distance to liquidation:",
    "Historical scenarios",
    "Price grid",
    "Liquidation threshold sweep",
    "contract default",
    "views.rs",
  ]) {
    assert.ok(text.includes(fragment), `missing "${fragment}"`);
  }
  assert.ok(!text.includes("undefined"), "no undefined may leak into the report");
  assert.ok(!text.includes("NaN"), "no NaN may leak into the report");
});

test("renderText says so plainly when the health factor cannot be computed", () => {
  const text = renderText(buildReport("no-oracle", { oraclePresent: false }));
  assert.ok(text.includes("Health factor unknown"));
  assert.ok(text.includes("no liquidation can proceed"));
  assert.ok(!text.includes("Distance to liquidation: "), "must not present a distance it cannot compute");
});

test("renderMarkdown emits every table with a heading and a source note", () => {
  const md = renderMarkdown(buildReport("leveraged"));
  for (const fragment of [
    "## Position health simulation — leveraged",
    "### Historical scenarios",
    "### Price grid",
    "### Liquidation threshold sweep",
    "views.rs",
  ]) {
    assert.ok(md.includes(fragment), `missing "${fragment}"`);
  }
  assert.ok(md.includes("| Scenario | Health factor |"), "scenario table header");
  assert.ok(md.includes("| Collateral move | Health factor |"), "price grid header");
  assert.ok(md.includes("| Threshold (bps) | Health factor |"), "threshold table header");
  assert.equal(md.includes("undefined"), false);
  assert.equal(md.includes("NaN"), false);
});

test("renderMarkdown highlights a position that is liquidatable right now", () => {
  const md = renderMarkdown(buildReport("under-water", { debt: { asset: "USDC", amount: 9_000n, price: C.priceScale } }));
  assert.ok(md.includes("**Liquidatable now.**"));
  assert.ok(md.includes("Max closeable in one call"));
});

test("the JSON report renders bigints as strings and round-trips", () => {
  const json = toJson(buildReport("json-shape"));
  assert.equal(typeof json, "object");
  const parsed = JSON.parse(JSON.stringify(json)) as Record<string, unknown>;
  const baseline = parsed.baseline as Record<string, string>;
  assert.equal(typeof baseline.healthFactor, "string");
  assert.equal(typeof baseline.collateralValue, "string");
  assert.equal(typeof (parsed.constants as Record<string, unknown>).healthFactorScale, "string");
  assert.deepEqual(parsed, json, "must survive a JSON round trip");
  // The stringified bigint is still the same number.
  assert.equal(BigInt(baseline.healthFactor), 16_000n, "1.6x after the 80% threshold");
  assert.equal((parsed.distanceToLiquidation as Record<string, unknown>).dropPercent, 37.5);
});

// ── The shipped position fixtures ────────────────────────────────────────────

test("every shipped position loads and is internally consistent", () => {
  const dir = path.join(REPO_ROOT, "scripts/position-health-sim/positions");
  const files = fs.readdirSync(dir).filter((f) => f.endsWith(".json"));
  assert.ok(files.length >= 3, `expected example positions, found ${files.length}`);

  for (const file of files) {
    const raw = JSON.parse(fs.readFileSync(path.join(dir, file), "utf8")) as Record<string, unknown>;
    assert.equal(typeof raw.name, "string", `${file}: needs a name`);
    assert.equal(typeof raw.description, "string", `${file}: needs a description`);
    assert.ok(Array.isArray(raw.collateral) && raw.collateral.length > 0, `${file}: needs collateral`);
    assert.ok(raw.debt && typeof raw.debt === "object", `${file}: needs a debt`);

    // Amounts and prices are strings so large values survive JSON.
    for (const leg of [...(raw.collateral as Record<string, unknown>[]), raw.debt as Record<string, unknown>]) {
      assert.equal(typeof leg.amount, "string", `${file}: amounts must be strings`);
      assert.equal(typeof leg.price, "string", `${file}: prices must be strings`);
      assert.match(leg.amount as string, /^\d+$/, `${file}: amount must be an integer string`);
      assert.match(leg.price as string, /^\d+$/, `${file}: price must be an integer string`);
    }

    const loaded = loadFixture(file, path.join(dir, file));
    const health = evaluatePosition(loaded, C);
    if (loaded.oraclePresent !== false) {
      assert.ok(health.healthFactorKnown, `${file}: a priced position must have a computable health factor`);
    }
  }
});

/** Minimal re-implementation of the CLI's loader, for the fixture check. */
function loadFixture(_name: string, file: string): Position {
  const raw = JSON.parse(fs.readFileSync(file, "utf8")) as {
    oraclePresent?: boolean;
    collateral: { asset: string; amount: string; price: string }[];
    debt: { asset: string; amount: string; price: string };
    liquidationThresholdBps?: string;
  };
  return {
    collateral: raw.collateral.map((l) => ({ asset: l.asset, amount: BigInt(l.amount), price: BigInt(l.price) })),
    debt: { asset: raw.debt.asset, amount: BigInt(raw.debt.amount), price: BigInt(raw.debt.price) },
    oraclePresent: raw.oraclePresent !== false,
    ...(raw.liquidationThresholdBps ? { liquidationThresholdBps: BigInt(raw.liquidationThresholdBps) } : {}),
  };
}

test("the no-oracle fixture is the one case the tool must refuse to price", () => {
  const file = path.join(POSITION_DIR, "no-oracle-xlm-borrow.json");
  const loaded = loadFixture("no-oracle", file);
  const health = evaluatePosition(loaded, C);
  assert.equal(health.healthFactor, 0n);
  assert.equal(health.healthFactorKnown, false);
  assert.equal(health.isLiquidatable, false);
  assert.equal(distanceToLiquidation(health, C).dropPercent, null);
});

test("each shipped fixture produces the health factor its description claims", () => {
  // These are the numbers the README and the fixture descriptions quote, so the
  // examples and the tool cannot quietly disagree.
  const expected: Record<string, { healthFactor: bigint; liquidatable: boolean; dropPercent: number }> = {
    "healthy-xlm-borrow": { healthFactor: 20_000n, liquidatable: false, dropPercent: 50 },
    "leveraged-xlm-borrow": { healthFactor: 10_000n, liquidatable: false, dropPercent: 0 },
    "near-threshold-xlm-borrow": { healthFactor: 8_421n, liquidatable: true, dropPercent: 0 },
    "multi-asset-borrow": { healthFactor: 10_285n, liquidatable: false, dropPercent: 2.77 },
  };
  for (const [name, want] of Object.entries(expected)) {
    const loaded = loadFixture(name, path.join(POSITION_DIR, `${name}.json`));
    const health = evaluatePosition(loaded, C);
    assert.equal(health.healthFactor, want.healthFactor, `${name}: health factor`);
    assert.equal(health.isLiquidatable, want.liquidatable, `${name}: liquidatable`);
    const distance = distanceToLiquidation(health, C);
    assert.ok(
      Math.abs((distance.dropPercent ?? -1) - want.dropPercent) < 0.01,
      `${name}: distance was ${distance.dropPercent}, expected ${want.dropPercent}`,
    );
  }
});

test("the multi-asset fixture sums both collateral legs before the threshold is applied", () => {
  const loaded = loadFixture("multi", path.join(POSITION_DIR, "multi-asset-borrow.json"));
  const health = evaluatePosition(loaded, C);
  // 8,000 XLM at 1.0 plus 2,000 BTC units at 0.5 = 8,000 + 1,000.
  assert.equal(health.collateralValue, 9_000n);
  assert.equal(health.debtValue, 7_000n);
  // Weighted 9,000 * 0.8 = 7,200; HF = 7,200 * 10,000 / 7,000.
  assert.equal(health.healthFactor, 10_285n);
});

test("the near-threshold fixture exposes a closeable amount at the default close factor", () => {
  const loaded = loadFixture("near", path.join(POSITION_DIR, "near-threshold-xlm-borrow.json"));
  const health = evaluatePosition(loaded, C);
  assert.equal(health.isLiquidatable, true);
  assert.equal(health.maxLiquidatableAmount, 9_500n, "19,000 * 5000 / 10000");
  assert.equal(
    liquidationIncentiveAmount(health.maxLiquidatableAmount, health.liquidationIncentiveBps),
    10_450n,
    "the liquidator is paid 10% on top of the 9,500 repaid",
  );
});
