/**
 * Market-condition simulation for a lending position (#1013).
 *
 * Given a position and a market condition, this answers the three questions a
 * lender or a risk reviewer actually has:
 *
 *   1. **Where is the health factor now, and after this shock?**
 *   2. **How far is the position from liquidation?** The price move on the
 *      collateral that takes it to exactly 1.0 — the break-even, and the single
 *      most useful number in a risk review.
 *   3. **Where would the liquidation boundary move if the admin changed the
 *      threshold?** A threshold sweep, because the threshold is admin-settable
 *      and its effect on the boundary is not obvious.
 *
 * Every health factor goes through `health-factor.ts`, which is a port of the
 * contract's own `views.rs`, so the answer is the contract's answer.
 */

import type { ContractConstants } from "./contract.ts";
import {
  evaluatePosition,
  healthFactorAtThreshold,
  type Position,
  type PositionHealth,
} from "./health-factor.ts";
import type { LoadedScenario, ShockGrid } from "./scenarios.ts";

/** Risk bands, anchored on the contract's own scale. */
export type RiskLevel = "safe" | "moderate" | "at-risk" | "critical" | "liquidatable" | "unknown";

/**
 * Risk bands. The edges above 1.0 are the repository's existing convention from
 * `hello-world/src/analytics.rs::calculate_user_risk_level`
 * (15_000 / 12_000 / 11_000 / 10_500). The 1.0 edge is not a convention — it is
 * `HEALTH_FACTOR_SCALE`, the exact point at which the contract stops treating
 * the position as healthy.
 */
const RISK_BANDS: { minHealthFactor: bigint; level: RiskLevel }[] = [
  { minHealthFactor: 15_000n, level: "safe" },
  { minHealthFactor: 12_000n, level: "moderate" },
  { minHealthFactor: 10_500n, level: "at-risk" },
];

/** Classify a health factor into a risk band. */
export function riskLevel(
  healthFactor: bigint,
  known: boolean,
  constants: ContractConstants,
): RiskLevel {
  if (!known) return "unknown";
  if (healthFactor < constants.healthFactorScale) return "liquidatable";
  for (const band of RISK_BANDS) {
    if (healthFactor >= band.minHealthFactor) return band.level;
  }
  // Between 1.0 and 1.05 the position is still healthy but very close.
  return "critical";
}

/** One evaluated market condition. */
export interface SimulationStep {
  label: string;
  /** Percent move applied to every collateral asset. */
  collateralChangePercent: number;
  health: PositionHealth;
  risk: RiskLevel;
  /** Change in health factor against the unshocked position, in the same scale. */
  healthFactorDelta: bigint;
  /** Did this step make the position liquidatable? */
  becameLiquidatable: boolean;
}

/** The break-even price: where the health factor is exactly 1.0. */
export interface BreakEven {
  /**
   * The multiplier the collateral price can fall to before liquidation, e.g.
   * `0.75` means a 25% fall liquidates the position. `null` when the position
   * cannot be priced or is already liquidatable.
   */
  priceMultiplier: number | null;
  /** `(1 - priceMultiplier) * 100`, the fall in percent that triggers it. */
  priceDropPercent: number | null;
  /** Price *rise* that would lift the position to a healthy 1.5. */
  recoveryTo15x: number | null;
}

/** How close a position is to its liquidation boundary. */
export interface DistanceToLiquidation {
  /** Percent of the current collateral value that can be lost first. */
  dropPercent: number | null;
  /** `true` when the position is already liquidatable or unpriceable. */
  alreadyLiquidatable: boolean;
  reason: string | null;
}

const SCALE_BPS = 10_000n;
const HEALTH_FACTOR_15X = 15_000n;

/** Convert an integer health factor to a float, for display only. */
export function healthFactorToNumber(healthFactor: bigint): number {
  return Number(healthFactor) / Number(SCALE_BPS);
}

/** Apply a percentage move to an oracle price (8 decimals). */
export function shockPrice(price: bigint, changePercent: number): bigint {
  // Integer arithmetic: newPrice = price * (100 + change) / 100, never negative.
  const numerator = price * BigInt(Math.round((100 + changePercent) * 1_000_000));
  const next = numerator / BigInt(100_000_000);
  return next < 0n ? 0n : next;
}

/** Apply a percentage move to every collateral leg, leaving debt alone. */
export function shockedPosition(
  position: Position,
  changePercent: number,
  assetOverrides: Map<string, number> = new Map(),
): Position {
  return {
    ...position,
    collateral: position.collateral.map((leg) => ({
      ...leg,
      price: shockPrice(leg.price, assetOverrides.get(leg.asset) ?? changePercent),
    })),
  };
}

/**
 * The break-even collateral price multiplier.
 *
 * The health factor is linear in collateral value, so the price that puts it at
 * exactly 1.0 is `1 / HF` of the current price — derived from the contract's own
 * output rather than re-deriving the formula, so it stays correct even if the
 * threshold is unusual.
 */
export function breakEven(health: PositionHealth, constants: ContractConstants): BreakEven {
  if (!health.healthFactorKnown || health.healthFactor <= 0n) {
    return { priceMultiplier: null, priceDropPercent: null, recoveryTo15x: null };
  }
  if (health.healthFactor < constants.healthFactorScale) {
    // Already liquidatable: there is no further fall to reach the boundary.
    return { priceMultiplier: 0, priceDropPercent: 0, recoveryTo15x: null };
  }
  // HF is linear in collateral value, so the price that lands it on exactly 1.0
  // is `1 / (hf / SCALE_BPS)`. Derived from the contract's own output so it
  // stays right for any threshold, including ones where the truncation bites.
  const priceMultiplier = Number(SCALE_BPS) / Number(health.healthFactor);
  return {
    priceMultiplier,
    priceDropPercent: (1 - priceMultiplier) * 100,
    recoveryTo15x:
      health.healthFactor < HEALTH_FACTOR_15X
        ? Number(HEALTH_FACTOR_15X - health.healthFactor) / Number(health.healthFactor)
        : null,
  };
}

/** How far the position is from its liquidation boundary. */
export function distanceToLiquidation(
  health: PositionHealth,
  constants: ContractConstants,
): DistanceToLiquidation {
  if (!health.healthFactorKnown) {
    return { dropPercent: null, alreadyLiquidatable: false, reason: health.unknownReason };
  }
  if (health.healthFactor === 0n) {
    return { dropPercent: null, alreadyLiquidatable: true, reason: health.unknownReason };
  }
  if (health.healthFactor < constants.healthFactorScale) {
    return { dropPercent: 0, alreadyLiquidatable: true, reason: "already below the 1.0 boundary" };
  }
  const be = breakEven(health, constants);
  return { dropPercent: be.priceDropPercent, alreadyLiquidatable: false, reason: null };
}

/** Evaluate a position and every shock in a grid. */
export function simulateShocks(
  position: Position,
  grid: ShockGrid,
  constants: ContractConstants,
  assetOverrides: Map<string, number> = new Map(),
): SimulationStep[] {
  const base = evaluatePosition(position, constants);
  return grid.changes.map((changePercent) => {
    const health = evaluatePosition(shockedPosition(position, changePercent, assetOverrides), constants);
    return {
      label: `${changePercent > 0 ? "+" : ""}${changePercent}%`,
      collateralChangePercent: changePercent,
      health,
      risk: riskLevel(health.healthFactor, health.healthFactorKnown, constants),
      healthFactorDelta: health.healthFactor - base.healthFactor,
      becameLiquidatable: health.isLiquidatable && !base.isLiquidatable,
    };
  });
}

/** Evaluate a position under a historical market scenario. */
export function simulateScenario(
  position: Position,
  scenario: LoadedScenario,
  constants: ContractConstants,
  baselineHealthFactor = 0n,
): SimulationStep {
  // A scenario names assets; map them onto the position's legs. Assets the
  // position does not hold are reported by `unmatchedAssets` so a scenario that
  // does not apply is visible rather than silent. A scenario that moves the debt
  // asset moves the debt value too, which is why the debt leg is overridden here.
  const overrides = new Map<string, number>();
  for (const leg of position.collateral) {
    const change = scenario.priceChanges.get(leg.asset);
    if (change !== undefined) overrides.set(leg.asset, change);
  }
  {
    const change = scenario.priceChanges.get(position.debt.asset);
    if (change !== undefined) overrides.set(position.debt.asset, change);
  }

  const shocked: Position = {
    ...position,
    collateral: position.collateral.map((leg) => ({
      ...leg,
      price: shockPrice(leg.price, overrides.get(leg.asset) ?? 0),
    })),
    debt: { ...position.debt, price: shockPrice(position.debt.price, overrides.get(position.debt.asset) ?? 0) },
  };

  const health = evaluatePosition(shocked, constants);
  return {
    label: scenario.name,
    collateralChangePercent: 0,
    health,
    risk: riskLevel(health.healthFactor, health.healthFactorKnown, constants),
    healthFactorDelta: health.healthFactor - baselineHealthFactor,
    becameLiquidatable: health.isLiquidatable,
  };
}

/** Assets a scenario names that the position does not hold. */
export function unmatchedAssets(position: Position, scenario: LoadedScenario): string[] {
  const held = new Set([...position.collateral.map((l) => l.asset), position.debt.asset]);
  return [...scenario.priceChanges.keys()].filter((asset) => !held.has(asset)).sort();
}

/** One row of a liquidation-threshold sweep. */
export interface ThresholdStep {
  thresholdBps: bigint;
  healthFactor: bigint;
  risk: RiskLevel;
  isLiquidatable: boolean;
  /** Change against the contract's default threshold. */
  healthFactorDelta: bigint;
}

/**
 * Sweep `liquidation_threshold_bps` and report what it does to the boundary.
 *
 * The contract lets an admin move this between 1 and 10_000 bps, so the effect
 * of a change is a governance question, and this is the table that answers it.
 */
export function sweepThresholds(
  position: Position,
  constants: ContractConstants,
  thresholds: bigint[],
): ThresholdStep[] {
  const base = evaluatePosition(position, constants);
  const oraclePresent = position.oraclePresent !== false;
  const hasDebt = base.totalDebtAmount > 0n;

  return thresholds.map((thresholdBps) => {
    const healthFactor = healthFactorAtThreshold(
      base.collateralValue,
      base.debtValue,
      hasDebt,
      thresholdBps,
      oraclePresent,
      constants,
    );
    const known = hasDebt ? oraclePresent && base.debtValue > 0n : true;
    return {
      thresholdBps,
      healthFactor,
      risk: riskLevel(healthFactor, known, constants),
      isLiquidatable: known && healthFactor > 0n && healthFactor < constants.healthFactorScale,
      healthFactorDelta: healthFactor - base.healthFactor,
    };
  });
}

/** Default threshold sweep: the contract's own default plus the usual governance grid. */
export function defaultThresholdGrid(constants: ContractConstants): bigint[] {
  const values = new Set<bigint>([
    constants.defaultLiquidationThresholdBps,
    4_000n,
    5_000n,
    6_000n,
    6_500n,
    7_000n,
    7_500n,
    8_500n,
    9_000n,
    10_000n,
  ]);
  return [...values].filter((v) => v > 0n && v <= 10_000n).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

/** The full report for a position. */
export interface SimulationReport {
  positionName: string;
  constants: ContractConstants;
  baseline: PositionHealth;
  baselineRisk: RiskLevel;
  breakEven: BreakEven;
  distance: DistanceToLiquidation;
  scenarios: { scenario: LoadedScenario; step: SimulationStep; unmatched: string[] }[];
  grid: { grid: ShockGrid; steps: SimulationStep[] };
  thresholds: { defaultBps: bigint; steps: ThresholdStep[] };
  /** The liquidation boundary as a function of threshold, for a one-line summary. */
  thresholdAtLiquidation: bigint | null;
}
