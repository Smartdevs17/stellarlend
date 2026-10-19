import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import {
  FALLBACK_CONSTANTS,
  REQUIRED_CONSTANT_NAMES,
  assertContractConstants,
  loadContractConstants,
  missingConstants,
} from "./contract.ts";
import {
  breakEvenThresholdBps,
  collateralValue,
  computeHealthFactor,
  debtValue,
  evaluatePosition,
  healthFactorAtThreshold,
  legValue,
  liquidationIncentiveAmount,
  maxLiquidatableAmount,
  toI128,
  totalDebtAmount,
  type Position,
} from "./health-factor.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");
const LENDING_SRC = path.join(REPO_ROOT, "stellar-lend/contracts/lending/src");

const I128_MAX = (1n << 127n) - 1n;

const VIEWS_SOURCE = `
const PRICE_SCALE: i128 = 100_000_000;

pub const HEALTH_FACTOR_SCALE: i128 = 10000;

pub const HEALTH_FACTOR_NO_DEBT: i128 = 100_000_000;
`;

const BORROW_SOURCE = `
const COLLATERAL_RATIO_MIN: i128 = 15000; // 150% in basis points

/// Returns liquidation threshold in basis points (e.g. 8000 = 80%). Default 8000 if not set.
pub fn get_liquidation_threshold_bps(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&BorrowDataKey::LiquidationThresholdBps)
        .unwrap_or(8000)
}

/// Returns close factor in basis points (e.g. 5000 = 50%). Default 5000 if not set.
pub fn get_close_factor_bps(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&BorrowDataKey::CloseFactorBps)
        .unwrap_or(5000)
}

/// Returns liquidation incentive in basis points (e.g. 1000 = 10%). Default 1000 if not set.
pub fn get_liquidation_incentive_bps(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&BorrowDataKey::LiquidationIncentiveBps)
        .unwrap_or(1000)
}
`;

const C = loadContractConstants({
  views: VIEWS_SOURCE,
  borrow: BORROW_SOURCE,
  paths: { views: "views.rs", borrow: "borrow.rs" },
});

/** A position priced by a flat oracle, matching `MockOracle` in views_test.rs. */
function flatOraclePosition(over: Partial<Position> = {}): Position {
  return {
    collateral: [{ asset: "XLM", amount: 1_000n, price: C.priceScale }],
    debt: { asset: "USDC", amount: 1_000n, price: C.priceScale },
    oraclePresent: true,
    ...over,
  };
}

// ── Constant loading ─────────────────────────────────────────────────────────

test("loadContractConstants reads every constant out of the Rust source", () => {
  assert.deepEqual(C, {
    priceScale: 100_000_000n,
    healthFactorScale: 10_000n,
    healthFactorNoDebt: 100_000_000n,
    defaultLiquidationThresholdBps: 8_000n,
    defaultCloseFactorBps: 5_000n,
    defaultLiquidationIncentiveBps: 1_000n,
    collateralRatioMinBps: 15_000n,
    sources: { views: "views.rs", borrow: "borrow.rs" },
  });
});

test("a getter default is scoped to its own function body", () => {
  const decoy = BORROW_SOURCE.replace(
    "pub fn get_close_factor_bps(env: &Env) -> i128 {\n    env.storage()",
    "pub fn unrelated(env: &Env) -> i128 {\n    env.storage().get(&Other).unwrap_or(9999);\n    0\n}\n\npub fn get_close_factor_bps(env: &Env) -> i128 {\n    env.storage()",
  );
  const parsed = loadContractConstants({
    views: VIEWS_SOURCE,
    borrow: decoy,
    paths: { views: "v", borrow: "b" },
  });
  assert.equal(parsed.defaultCloseFactorBps, 5_000n, "the decoy must not be picked up");
});

test("missingConstants names what could not be read", () => {
  assert.deepEqual(missingConstants({ views: VIEWS_SOURCE, borrow: BORROW_SOURCE }), []);
  const broken = missingConstants({ views: "const NOTHING: i128 = 1;", borrow: "fn nope() {}" });
  for (const name of [...REQUIRED_CONSTANT_NAMES, "get_liquidation_threshold_bps", "get_close_factor_bps", "get_liquidation_incentive_bps"]) {
    assert.ok(broken.includes(name), `${name} should be reported missing`);
  }
});

test("assertContractConstants refuses to guess when a constant is gone", () => {
  assert.throws(() => assertContractConstants(["HEALTH_FACTOR_SCALE"]), /HEALTH_FACTOR_SCALE/);
  assert.doesNotThrow(() => assertContractConstants([]));
});

test("fallbacks match the contract on main", () => {
  // The fallbacks are a floor for a clear error, not a silent substitute; if the
  // contract changes, this test is what notices the docs are stale.
  assert.equal(FALLBACK_CONSTANTS.healthFactorScale, 10_000n);
  assert.equal(FALLBACK_CONSTANTS.defaultLiquidationThresholdBps, 8_000n);
  assert.equal(FALLBACK_CONSTANTS.defaultCloseFactorBps, 5_000n);
  assert.equal(FALLBACK_CONSTANTS.defaultLiquidationIncentiveBps, 1_000n);
});

// ── The contract's own test vectors ──────────────────────────────────────────
// Every vector below is transcribed from the contract's tests, with the file and
// test name in the title, so a reviewer can check the port against the source.

test("views_test.rs::test_get_health_factor_no_debt_returns_sentinel", () => {
  // A position with no debt returns the sentinel, not infinity.
  assert.equal(
    computeHealthFactor(1_000n, 0n, false, C.defaultLiquidationThresholdBps, true, C),
    C.healthFactorNoDebt,
  );
  assert.equal(C.healthFactorNoDebt, 100_000_000n);
});

test("views_test.rs::test_get_health_factor_zero_when_oracle_not_set", () => {
  // Debt but no oracle: 0, not infinity and not 1.0.
  assert.equal(computeHealthFactor(0n, 0n, true, C.defaultLiquidationThresholdBps, false, C), 0n);
  assert.equal(computeHealthFactor(20_000n, 10_000n, true, C.defaultLiquidationThresholdBps, false, C), 0n);
});

test("views_test.rs::test_get_health_factor_healthy_above_threshold", () => {
  // Collateral 20_000, debt 10_000 at the default 80%: weighted = 16_000,
  // HF = 16_000 * 10000 / 10_000 = 16_000.
  assert.equal(computeHealthFactor(20_000n, 10_000n, true, 8_000n, true, C), 16_000n);
});

test("views_test.rs::test_get_health_factor_liquidatable_below_threshold", () => {
  // Threshold 40%, collateral 30_000, debt 15_000: weighted = 12_000,
  // HF = 12_000 * 10000 / 15_000 = 8_000.
  assert.equal(computeHealthFactor(30_000n, 15_000n, true, 4_000n, true, C), 8_000n);
});

test("views_test.rs::test_get_health_factor_boundary_at_one", () => {
  // Threshold 6667, collateral 1500, debt 1000: weighted = 1000, HF = 10_000
  // exactly — healthy, because the contract tests `hf >= HEALTH_FACTOR_SCALE`.
  assert.equal(computeHealthFactor(1_500n, 1_000n, true, 6_667n, true, C), 10_000n);
  assert.equal(computeHealthFactor(1_500n, 1_000n, true, 6_667n, true, C) >= C.healthFactorScale, true);
});

test("views_test.rs::test_set_liquidation_threshold_bps_valid", () => {
  // Threshold 75%, collateral 20_000, debt 10_000: weighted = 15_000, HF = 15_000.
  assert.equal(computeHealthFactor(20_000n, 10_000n, true, 7_500n, true, C), 15_000n);
});

test("math_safety_test.rs::test_views_math_safety — overflow falls back to 0", () => {
  // cv = i128::MAX / 2, dv = 1: the final value does not fit in i128, so the
  // contract's `to_i128().unwrap_or(0)` yields 0.
  const cv = I128_MAX / 2n;
  assert.equal(computeHealthFactor(cv, 1n, true, C.defaultLiquidationThresholdBps, true, C), 0n);
});

test("math_safety_test.rs::test_views_math_safety — zero debt uses the sentinel", () => {
  assert.equal(computeHealthFactor(1_000n, 0n, false, C.defaultLiquidationThresholdBps, true, C), C.healthFactorNoDebt);
});

test("math_safety_test.rs::test_views_math_safety — value is 0 without an oracle", () => {
  const leg = { asset: "XLM", amount: I128_MAX, price: C.priceScale };
  assert.equal(legValue(leg, C, false), 0n, "no oracle short-circuits before any scaling");
});

// ── Fidelity details a naive port gets wrong ────────────────────────────────

test("the two truncating divisions are preserved, not collapsed into one", () => {
  // 1501 * 6667 = 10_007_167. Collapsed: /1000 = 10_007. As the contract does
  // it: (10_007_167 / 10_000) * 10_000 / 1000 = 1_000 * 10_000 / 1000 = 10_000.
  // A port that multiplies through gets a different, wrong answer.
  const hf = computeHealthFactor(1_501n, 1_000n, true, 6_667n, true, C);
  assert.equal(hf, 10_000n, "must match the contract's two-step truncation");
  assert.notEqual(hf, 10_007n, "the collapsed formula would give this");
});

test("toI128 narrows like I256::to_i128 and can be told to saturate", () => {
  assert.equal(toI128(5n), 5n);
  assert.equal(toI128(I128_MAX), I128_MAX);
  assert.equal(toI128(I128_MAX + 1n), 0n, "the default fallback is 0");
  assert.equal(toI128(I128_MAX + 1n, I128_MAX), I128_MAX, "saturating fallback");
  assert.equal(toI128(-(1n << 127n) - 1n), 0n);
});

test("liquidationBoundary: the incentive saturates where the health factor falls back to 0", () => {
  // `get_liquidation_incentive_amount` uses `to_i128().unwrap_or(i128::MAX)`, so it
  // must not share the health factor's 0 fallback.
  assert.equal(liquidationIncentiveAmount(I128_MAX, 1_000n), I128_MAX);
  assert.equal(computeHealthFactor(I128_MAX, 1n, true, 10_000n, true, C), 0n);
});

test("legValue returns 0 for a non-positive amount or price, before scaling", () => {
  assert.equal(legValue({ asset: "XLM", amount: 0n, price: C.priceScale }, C, true), 0n);
  assert.equal(legValue({ asset: "XLM", amount: -5n, price: C.priceScale }, C, true), 0n);
  assert.equal(legValue({ asset: "XLM", amount: 100n, price: 0n }, C, true), 0n);
  assert.equal(legValue({ asset: "XLM", amount: 100n, price: -1n }, C, true), 0n);
  // A flat oracle price of 1.0 makes value equal to amount.
  assert.equal(legValue({ asset: "XLM", amount: 123n, price: C.priceScale }, C, true), 123n);
});

test("a non-zero price with 8 decimals scales the amount", () => {
  // 2 XLM at $0.50 = 1 USD-ish unit.
  assert.equal(legValue({ asset: "XLM", amount: 2n, price: C.priceScale / 2n }, C, true), 1n);
});

// ── Close factor and incentive ───────────────────────────────────────────────

test("liquidation_boundary_test.rs — the default close factor closes half the debt", () => {
  // `test_max_liquidatable_default_close_factor`: close factor 5000.
  const hf = 8_000n; // below the boundary
  assert.equal(maxLiquidatableAmount(hf, 10_000n, C.defaultCloseFactorBps, C), 5_000n);
});

test("liquidation_boundary_test.rs — a full close factor closes all the debt", () => {
  assert.equal(maxLiquidatableAmount(8_000n, 10_000n, 10_000n, C), 10_000n);
});

test("liquidation_boundary_test.rs — a 1 bps close factor rounds to a single unit", () => {
  // `test_max_liquidatable_minimum_close_factor_1_bps` asserts exactly 1.
  assert.equal(maxLiquidatableAmount(8_000n, 10_000n, 1n, C), 1n);
});

test("liquidation_boundary_test.rs — a healthy position cannot be liquidated", () => {
  assert.equal(maxLiquidatableAmount(10_000n, 10_000n, C.defaultCloseFactorBps, C), 0n);
  assert.equal(maxLiquidatableAmount(16_000n, 10_000n, 10_000n, C), 0n);
});

test("liquidation_boundary_test.rs — an unknown health factor cannot be liquidated", () => {
  // hf == 0 means the oracle is missing; the contract returns 0 so a liquidator
  // cannot act without price data.
  assert.equal(maxLiquidatableAmount(0n, 10_000n, 10_000n, C), 0n);
  assert.equal(maxLiquidatableAmount(0n, 0n, 10_000n, C), 0n, "no debt either");
});

test("liquidation_boundary_test.rs — the default incentive pays 10% on top", () => {
  assert.equal(liquidationIncentiveAmount(10_000n, C.defaultLiquidationIncentiveBps), 11_000n);
});

test("liquidation_boundary_test.rs — a 100% incentive pays double", () => {
  assert.equal(liquidationIncentiveAmount(10_000n, 10_000n), 20_000n);
});

test("liquidation_boundary_test.rs — a zero incentive pays exactly the repayment", () => {
  assert.equal(liquidationIncentiveAmount(10_000n, 0n), 10_000n);
});

test("liquidation_boundary_test.rs — a zero or negative repayment has no incentive", () => {
  assert.equal(liquidationIncentiveAmount(0n, 1_000n), 0n);
  assert.equal(liquidationIncentiveAmount(-1_000n, 1_000n), 0n);
});

test("liquidation_boundary_test.rs — a single unit still pays its incentive", () => {
  assert.equal(liquidationIncentiveAmount(1n, 1_000n), 1n);
});

// ── spec/health_factor.rs lemmas ─────────────────────────────────────────────

test("H-01: zero debt returns the no-debt sentinel", () => {
  assert.equal(
    computeHealthFactor(1_000_000n, 0n, false, 8_000n, true, C),
    C.healthFactorNoDebt,
  );
});

test("H-02: no oracle yields 0 when the user has debt", () => {
  assert.equal(computeHealthFactor(1_000_000n, 500_000n, true, 8_000n, false, C), 0n);
});

test("H-03: half the collateral at the default threshold is liquidatable", () => {
  const hf = computeHealthFactor(500_000n, 1_000_000n, true, 8_000n, true, C);
  assert.ok(hf < C.healthFactorScale, `H-03: half collateral should be liquidatable, got ${hf}`);
});

test("H-04: the health factor is monotonically non-decreasing in collateral value", () => {
  const debt = 500_000n;
  const collateralValues = [0n, 100_000n, 500_000n, 800_000n, 1_000_000n, 5_000_000n];
  let previous = 0n;
  for (const cv of collateralValues) {
    const hf = computeHealthFactor(cv, debt, true, 8_000n, true, C);
    assert.ok(hf >= previous, `H-04: not monotone at cv=${cv}: ${hf} < ${previous}`);
    previous = hf;
  }
});

test("H-05: the health factor is monotonically non-increasing in debt value", () => {
  const collateral = 1_000_000n;
  const debts = [1n, 500n, 100_000n, 500_000n, 1_000_000n];
  let previous = I128_MAX;
  for (const dv of debts) {
    const hf = computeHealthFactor(collateral, dv, true, 8_000n, true, C);
    assert.ok(hf <= previous, `H-05: not monotone at dv=${dv}: ${hf} > ${previous}`);
    previous = hf;
  }
});

test("H-06: large protocol values do not overflow", () => {
  // Typical pool scale: a $1B position valued in 1e8-scaled units.
  const cv = 100_000_000_000n;
  const dv = 50_000_000_000n;
  const hf = computeHealthFactor(cv, dv, true, 8_000n, true, C);
  assert.equal(hf, 16_000n, "1.6x at the default threshold");
});

test("H-07: an exact break-even threshold yields HEALTH_FACTOR_SCALE", () => {
  // The contract's two truncations mean the boundary is only *landed on* when
  // the arithmetic is exact. These pairs are the ones that are.
  for (const [cv, dv, bps] of [
    [1_500n, 1_000n, 6_667n],
    [20_000n, 10_000n, 5_000n],
    [1_000n, 800n, 8_000n],
  ] as const) {
    assert.equal(
      computeHealthFactor(cv, dv, true, bps, true, C),
      C.healthFactorScale,
      `${cv}/${dv} at ${bps} bps should sit exactly on the boundary`,
    );
  }
  // And the computed break-even agrees with the contract's own boundary case.
  assert.equal(breakEvenThresholdBps(1_500n, 1_000n), 6_666n);
  assert.equal(breakEvenThresholdBps(20_000n, 10_000n), 5_000n);
});

test("H-07: raising the threshold moves the health factor across the boundary", () => {
  // Where truncation prevents an exact landing, the health factor still has to
  // cross HEALTH_FACTOR_SCALE as the threshold rises — it cannot skip the
  // boundary from one side to the other.
  for (const [cv, dv] of [
    [1_500n, 1_000n],
    [20_000n, 10_000n],
    [1_234_567n, 890_123n],
  ] as const) {
    let crossed = false;
    let previousLiquidatable: boolean | null = null;
    for (let bps = 1n; bps <= 10_000n; bps += 1n) {
      const hf = computeHealthFactor(cv, dv, true, bps, true, C);
      const liquidatable = hf < C.healthFactorScale;
      if (previousLiquidatable !== null && liquidatable !== previousLiquidatable) crossed = true;
      previousLiquidatable = liquidatable;
    }
    assert.ok(crossed, `${cv}/${dv}: the health factor never crossed the boundary`);
  }
  assert.equal(breakEvenThresholdBps(0n, 1_000n), null, "no collateral has no break-even");
  assert.equal(breakEvenThresholdBps(1_000n, 0n), null, "no debt has no break-even");
});

// ── evaluatePosition ─────────────────────────────────────────────────────────

test("evaluatePosition prices a flat-oracle position the way the views module does", () => {
  const health = evaluatePosition(
    flatOraclePosition({
      collateral: [{ asset: "XLM", amount: 20_000n, price: C.priceScale }],
      debt: { asset: "USDC", amount: 10_000n, price: C.priceScale },
    }),
    C,
  );
  assert.equal(health.collateralValue, 20_000n);
  assert.equal(health.debtValue, 10_000n);
  assert.equal(health.totalDebtAmount, 10_000n);
  assert.equal(health.healthFactor, 16_000n);
  assert.equal(health.isLiquidatable, false);
  assert.equal(health.healthFactorKnown, true);
  assert.equal(health.liquidationThresholdBps, 8_000n, "falls back to the contract default");
  assert.equal(health.closeFactorBps, 5_000n);
  assert.equal(health.liquidationIncentiveBps, 1_000n);
});

test("evaluatePosition reports a debt-free position as healthy, not liquidatable", () => {
  const health = evaluatePosition(
    flatOraclePosition({ debt: { asset: "USDC", amount: 0n, price: C.priceScale } }),
    C,
  );
  assert.equal(health.healthFactor, C.healthFactorNoDebt);
  assert.equal(health.isLiquidatable, false);
  assert.equal(health.healthFactorKnown, true);
  assert.equal(health.maxLiquidatableAmount, 0n);
});

test("evaluatePosition reports the no-oracle case as unknown, with a reason", () => {
  const health = evaluatePosition(flatOraclePosition({ oraclePresent: false }), C);
  assert.equal(health.healthFactor, 0n, "the contract reports 0 without an oracle");
  assert.equal(health.healthFactorKnown, false);
  assert.equal(health.isLiquidatable, false);
  assert.equal(health.maxLiquidatableAmount, 0n, "no liquidation without price data");
  assert.match(String(health.unknownReason), /oracle/);
});

test("evaluatePosition reports a zero-priced debt as unknown rather than liquidatable", () => {
  const health = evaluatePosition(
    flatOraclePosition({ debt: { asset: "USDC", amount: 10_000n, price: 0n } }),
    C,
  );
  assert.equal(health.healthFactor, 0n);
  assert.equal(health.healthFactorKnown, false);
  assert.equal(health.isLiquidatable, false, "a 0 is 'cannot compute', not 'collateralised'");
  assert.match(String(health.unknownReason), /debt value/);
});

test("evaluatePosition sums every collateral leg", () => {
  const health = evaluatePosition(
    flatOraclePosition({
      collateral: [
        { asset: "XLM", amount: 10_000n, price: C.priceScale },
        { asset: "BTC", amount: 5_000n, price: C.priceScale },
      ],
      debt: { asset: "USDC", amount: 10_000n, price: C.priceScale },
    }),
    C,
  );
  assert.equal(health.collateralValue, 15_000n);
  assert.equal(health.healthFactor, 12_000n);
});

test("collateralValue and debtValue agree with evaluatePosition", () => {
  const position = flatOraclePosition({
    collateral: [{ asset: "XLM", amount: 20_000n, price: C.priceScale }],
    debt: { asset: "USDC", amount: 10_000n, price: C.priceScale },
  });
  const health = evaluatePosition(position, C);
  assert.equal(collateralValue(position, C), health.collateralValue);
  assert.equal(debtValue(position, C), health.debtValue);
  assert.equal(totalDebtAmount(position), health.totalDebtAmount);
});

test("a position's own threshold overrides the contract default", () => {
  const health = evaluatePosition(flatOraclePosition({ liquidationThresholdBps: 4_000n }), C);
  assert.equal(health.liquidationThresholdBps, 4_000n);
  assert.equal(health.healthFactor, 4_000n, "weighted 1000 * 0.4 = 400, HF = 400 * 10000 / 1000");
  assert.equal(health.isLiquidatable, true);
  assert.equal(health.maxLiquidatableAmount, 500n, "debt 1000 * 5000/10000");
});

test("healthFactorAtThreshold is computeHealthFactor with a different threshold", () => {
  for (const bps of [1n, 4_000n, 8_000n, 10_000n]) {
    assert.equal(
      healthFactorAtThreshold(20_000n, 10_000n, true, bps, true, C),
      computeHealthFactor(20_000n, 10_000n, true, bps, true, C),
    );
  }
});

test("a plan round-trips through JSON with bigints rendered as strings", () => {
  // The CLI serialises bigints as strings; this guards the shape the renderer uses.
  const health = evaluatePosition(flatOraclePosition(), C);
  assert.equal(typeof JSON.parse(JSON.stringify({ hf: health.healthFactor.toString() })).hf, "string");
  assert.equal(Number(JSON.parse(JSON.stringify({ hf: health.healthFactor.toString() })).hf), 8_000);
});

// ── The repository's own source ──────────────────────────────────────────────

test("the constants the port depends on are the ones on main today", () => {
  const views = fs.readFileSync(path.join(LENDING_SRC, "views.rs"), "utf8");
  const borrow = fs.readFileSync(path.join(LENDING_SRC, "borrow.rs"), "utf8");
  const real = loadContractConstants({
    views,
    borrow,
    paths: { views: "views.rs", borrow: "borrow.rs" },
  });
  assert.deepEqual(missingConstants({ views, borrow }), [], "every constant must be readable");
  assert.equal(real.healthFactorScale, 10_000n);
  assert.equal(real.priceScale, 100_000_000n);
  assert.equal(real.healthFactorNoDebt, 100_000_000n);
  assert.equal(real.defaultLiquidationThresholdBps, 8_000n);
  assert.equal(real.defaultCloseFactorBps, 5_000n);
  assert.equal(real.defaultLiquidationIncentiveBps, 1_000n);
  assert.equal(real.collateralRatioMinBps, 15_000n);
});

test("the views.rs math this tool ports is still the math that is there", () => {
  // If views.rs is restructured, the port has to follow. These are the lines the
  // port is a copy of; their absence means the port is now guessing.
  const views = fs.readFileSync(path.join(LENDING_SRC, "views.rs"), "utf8");
  for (const fragment of [
    "fn compute_health_factor",
    "fn collateral_value",
    "fn debt_value",
    "fn get_max_liquidatable_amount",
    "fn get_liquidation_incentive_amount",
    "HEALTH_FACTOR_SCALE",
    "PRICE_SCALE",
  ]) {
    assert.ok(views.includes(fragment), `views.rs no longer contains ${fragment}`);
  }
  // The two truncating divisions are the whole point of the port.
  assert.ok(
    views.includes("collat_256.mul(&bps_256).div(&I256::from_i128(env, 10000))"),
    "the weighted-collateral truncation moved",
  );
  assert.ok(
    views.includes("hf_256 = weighted_collateral.mul(&hf_scale_256).div(&debt_256)"),
    "the health-factor truncation moved",
  );
});

test("a contract source with a renamed constant is rejected, not silently defaulted", () => {
  // This is what the CLI's negative gate exercises: point it at a mutated copy
  // and the tool must fail rather than fall back to FALLBACK_CONSTANTS.
  const renamed = missingConstants({
    views: VIEWS_SOURCE.replace("HEALTH_FACTOR_SCALE", "HEALTH_FACTOR_SCALE_RENAMED"),
    borrow: BORROW_SOURCE,
  });
  assert.ok(renamed.includes("HEALTH_FACTOR_SCALE"), "the rename must be detected");
  assert.throws(() => assertContractConstants(renamed), /HEALTH_FACTOR_SCALE/);
});

test("a valid contract source passes the gate and yields the contract's values", () => {
  assert.doesNotThrow(() => assertContractConstants(missingConstants({ views: VIEWS_SOURCE, borrow: BORROW_SOURCE })));
  const parsed = loadContractConstants({
    views: VIEWS_SOURCE,
    borrow: BORROW_SOURCE,
    paths: { views: "alt/views.rs", borrow: "alt/borrow.rs" },
  });
  assert.equal(parsed.healthFactorScale, 10_000n);
  assert.deepEqual(parsed.sources, { views: "alt/views.rs", borrow: "alt/borrow.rs" });
});
