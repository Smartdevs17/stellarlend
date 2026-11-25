/**
 * A faithful reference implementation of the lending pool's position health
 * math (#1013).
 *
 * This is a port of
 * `stellar-lend/contracts/lending/src/views.rs` (`collateral_value`,
 * `debt_value`, `compute_health_factor`, `get_max_liquidatable_amount`,
 * `get_liquidation_incentive_amount`) and the parameter getters in `borrow.rs`.
 * It exists so market conditions can be simulated against the *contract's*
 * definition of position health rather than an approximation of it.
 *
 * Fidelity details that a naive port gets wrong, and which the tests pin:
 *
 * 1. **Two truncating divisions.** The contract computes
 *    `weighted = cv * bps / 10000` and then `hf = weighted * 10000 / dv`. That
 *    is *not* equal to `cv * bps / dv` in integer arithmetic; the intermediate
 *    truncation is real and the port has to keep it.
 * 2. **`no oracle` yields `0`, not infinity and not 1.0.** A position with debt
 *    and no price feed reports a health factor of `0`, and
 *    `get_max_liquidatable_amount` treats that as "cannot act".
 * 3. **Overflow yields `0`, except the incentive which saturates.** The contract
 *    works in `I256` and then does `to_i128().unwrap_or(0)`; the liquidation
 *    incentive does `to_i128().unwrap_or(i128::MAX)` instead.
 * 4. **A debt-free position returns a 100_000_000 sentinel**, not infinity.
 * 5. `collateral_value` / `debt_value` return `0` for a non-positive amount or a
 *    non-positive price, before any scaling happens.
 *
 * All arithmetic is `BigInt`, so no intermediate value can overflow here; the
 * `toI128` helper reproduces the contract's narrowing step.
 */

import type { ContractConstants } from "./contract.ts";

const I128_MAX = (1n << 127n) - 1n;
const I128_MIN = -(1n << 127n);

/** A collateral or debt leg of a position. */
export interface AssetLeg {
  asset: string;
  /** Amount in the asset's own smallest unit. */
  amount: bigint;
  /** Oracle price with 8 decimals, i.e. the contract's `price(asset)` value. */
  price: bigint;
}

/** A borrower's position as the views module would see it. */
export interface Position {
  /** One leg per deposited collateral asset. */
  collateral: AssetLeg[];
  /** Principal plus accrued interest, in the asset's smallest unit. */
  debt: AssetLeg;
  /** Admin-settable parameters; omitted means the contract's own default. */
  liquidationThresholdBps?: bigint;
  closeFactorBps?: bigint;
  liquidationIncentiveBps?: bigint;
  /** Whether an oracle is configured. The contract returns 0 without one. */
  oraclePresent?: boolean;
}

/** Everything the views module would report for a position. */
export interface PositionHealth {
  /** Sum of `collateral_value` over every leg. */
  collateralValue: bigint;
  /** `debt_value` of the borrow. */
  debtValue: bigint;
  /** Principal + accrued interest, in the debt asset's smallest unit. */
  totalDebtAmount: bigint;
  /** `HEALTH_FACTOR_SCALE` (10_000) means a ratio of exactly 1.0. */
  healthFactor: bigint;
  /** `true` when the contract would treat the position as liquidatable. */
  isLiquidatable: boolean;
  /** `get_max_liquidatable_amount` — 0 when healthy, unknown or debt-free. */
  maxLiquidatableAmount: bigint;
  /** The effective threshold used, after defaults. */
  liquidationThresholdBps: bigint;
  closeFactorBps: bigint;
  liquidationIncentiveBps: bigint;
  /** `false` when the health factor could not be computed. */
  healthFactorKnown: boolean;
  /** Why the health factor is unknown, when it is. */
  unknownReason: string | null;
}

/** Narrow to `i128` the way `I256::to_i128` does, with the contract's fallback. */
export function toI128(value: bigint, onOverflow: bigint = 0n): bigint {
  if (value > I128_MAX || value < I128_MIN) return onOverflow;
  return value;
}

/**
 * `collateral_value` / `debt_value` for a single leg.
 *
 * Returns 0 for a non-positive amount or price, and 0 when no oracle is set —
 * before any scaling, exactly as the contract does.
 */
export function legValue(
  leg: AssetLeg,
  constants: ContractConstants,
  oraclePresent: boolean,
): bigint {
  if (leg.amount <= 0n) return 0n;
  if (!oraclePresent) return 0n;
  if (leg.price <= 0n) return 0n;
  return toI128((leg.amount * leg.price) / constants.priceScale);
}

/** Sum of `collateral_value` over every leg. */
export function collateralValue(
  position: Position,
  constants: ContractConstants,
): bigint {
  const oraclePresent = position.oraclePresent !== false;
  return position.collateral.reduce(
    (sum, leg) => sum + legValue(leg, constants, oraclePresent),
    0n,
  );
}

/** `debt_value` — `total_debt * price / PRICE_SCALE`, 0 when debt or price is 0. */
export function debtValue(position: Position, constants: ContractConstants): bigint {
  return legValue(
    { ...position.debt, amount: totalDebtAmount(position) },
    constants,
    position.oraclePresent !== false,
  );
}

/** Principal plus accrued interest. The contract `unwrap_or(0)`s the sum. */
export function totalDebtAmount(position: Position): bigint {
  const sum = position.debt.amount;
  return sum > 0n ? sum : 0n;
}

/**
 * `compute_health_factor`.
 *
 * Ported to keep the contract's two truncating divisions:
 * `weighted = cv * bps / 10000`, then `hf = weighted * 10000 / dv`.
 */
export function computeHealthFactor(
  collateralValueAmount: bigint,
  debtValueAmount: bigint,
  hasDebt: boolean,
  liquidationThresholdBps: bigint,
  oraclePresent: boolean,
  constants: ContractConstants,
): bigint {
  if (debtValueAmount <= 0n) {
    if (hasDebt) return 0n; // Oracle absent, or value collapsed to zero.
    return constants.healthFactorNoDebt;
  }
  if (!oraclePresent) return 0n;

  const weighted = (collateralValueAmount * liquidationThresholdBps) / 10_000n;
  const hf = (weighted * constants.healthFactorScale) / debtValueAmount;
  return toI128(hf);
}

/** `get_max_liquidatable_amount` — `total_debt * close_factor / 10000`, 0 when healthy. */
export function maxLiquidatableAmount(
  healthFactor: bigint,
  totalDebt: bigint,
  closeFactorBps: bigint,
  constants: ContractConstants,
): bigint {
  if (totalDebt <= 0n) return 0n;
  if (healthFactor === 0n) return 0n; // unknown: refuse to act without price data
  if (healthFactor >= constants.healthFactorScale) return 0n;
  return toI128((totalDebt * closeFactorBps) / 10_000n);
}

/**
 * `get_liquidation_incentive_amount` — `repay * (10000 + incentive) / 10000`.
 *
 * Saturates to `i128::MAX` on overflow rather than falling back to 0, matching
 * the contract's `to_i128().unwrap_or(i128::MAX)`.
 */
export function liquidationIncentiveAmount(repayAmount: bigint, incentiveBps: bigint): bigint {
  if (repayAmount <= 0n) return 0n;
  return toI128((repayAmount * (10_000n + incentiveBps)) / 10_000n, I128_MAX);
}

/** Evaluate a position the way `get_user_position` would. */
export function evaluatePosition(
  position: Position,
  constants: ContractConstants,
): PositionHealth {
  const oraclePresent = position.oraclePresent !== false;
  const liquidationThresholdBps =
    position.liquidationThresholdBps ?? constants.defaultLiquidationThresholdBps;
  const closeFactorBps = position.closeFactorBps ?? constants.defaultCloseFactorBps;
  const liquidationIncentiveBps =
    position.liquidationIncentiveBps ?? constants.defaultLiquidationIncentiveBps;

  const totalDebt = totalDebtAmount(position);
  const cv = collateralValue(position, constants);
  const dv = debtValue(position, constants);
  const healthFactor = computeHealthFactor(
    cv,
    dv,
    totalDebt > 0n,
    liquidationThresholdBps,
    oraclePresent,
    constants,
  );

  // The contract returns 0 for a debt-free position's sentinel too, and a
  // sentinel of 100_000_000 is >= HEALTH_FACTOR_SCALE, so both cases read as
  // "not liquidatable" here. `healthFactorKnown` is the honest signal instead.
  const isDebtFree = totalDebt <= 0n;
  const healthFactorKnown = isDebtFree || (oraclePresent && dv > 0n);

  let unknownReason: string | null = null;
  if (!oraclePresent) unknownReason = "no oracle is configured, so the contract reports 0";
  else if (!isDebtFree && dv <= 0n) {
    unknownReason =
      dv === 0n && totalDebt > 0n
        ? "debt value priced at 0 (missing or non-positive oracle price), so the contract reports 0"
        : "debt value priced at 0, so the contract reports 0";
  }

  return {
    collateralValue: cv,
    debtValue: dv,
    totalDebtAmount: totalDebt,
    healthFactor,
    isLiquidatable:
      healthFactorKnown && healthFactor > 0n && healthFactor < constants.healthFactorScale,
    maxLiquidatableAmount: maxLiquidatableAmount(
      healthFactor,
      totalDebt,
      closeFactorBps,
      constants,
    ),
    liquidationThresholdBps,
    closeFactorBps,
    liquidationIncentiveBps,
    healthFactorKnown,
    unknownReason,
  };
}

/** The `liquidation_threshold_bps` at which this position sits exactly at 1.0. */
export function breakEvenThresholdBps(
  collateralValueAmount: bigint,
  debtValueAmount: bigint,
): bigint | null {
  if (debtValueAmount <= 0n || collateralValueAmount <= 0n) return null;
  return (debtValueAmount * 10_000n) / collateralValueAmount;
}

/**
 * The health factor at any `liquidation_threshold_bps`, without re-deriving the
 * values. This is what makes a *threshold* sweep possible: the contract lets an
 * admin move the threshold, and the simulation has to show what that does.
 */
export function healthFactorAtThreshold(
  collateralValueAmount: bigint,
  debtValueAmount: bigint,
  hasDebt: boolean,
  thresholdBps: bigint,
  oraclePresent: boolean,
  constants: ContractConstants,
): bigint {
  // A thin alias kept so the threshold sweep reads as a sweep, not as a
  // re-derivation of the whole health factor call.
  return computeHealthFactor(
    collateralValueAmount,
    debtValueAmount,
    hasDebt,
    thresholdBps,
    oraclePresent,
    constants,
  );
}

/** One axis of a shock grid: a label plus the price multiplier it applies. */
export interface ShockStep {
  /** Human-readable label, e.g. `-25%`. */
  label: string;
  /** Multiplier in basis points of the original price (10_000 = unchanged). */
  multiplierBps: bigint;
}

/** A single cell of the shock grid. */
export interface ShockCell {
  /** The shock applied to the shocked asset. */
  shock: ShockStep;
  /** The shocked asset's price after the shock, in the oracle's 8 decimals. */
  shockedPrice: bigint;
  /** Health factor under this shock; `0` when the contract cannot compute one. */
  healthFactor: bigint;
  /** `true` when the contract would treat the position as liquidatable. */
  isLiquidatable: boolean;
  /** `false` when the health factor could not be computed. */
  healthFactorKnown: boolean;
  /** Why the health factor is unknown, when it is. */
  unknownReason: string | null;
  /** `get_max_liquidatable_amount` under this shock. */
  maxLiquidatableAmount: bigint;
}

/** Health factors for one shocked asset across every shock step. */
export interface ShockRow {
  /** The asset whose price is being shocked. */
  asset: string;
  /** `true` when the shocked asset is the debt leg rather than a collateral leg. */
  isDebt: boolean;
  /** One cell per shock step, in the order the steps were supplied. */
  cells: ShockCell[];
}

/** The full grid: one row per shocked asset, one cell per shock step. */
export interface ShockGrid {
  /** The shock axis, in the order supplied. */
  shocks: ShockStep[];
  /** One row per asset that appears in the position. */
  rows: ShockRow[];
  /** The unshocked evaluation, for reference against the grid. */
  baseline: PositionHealth;
}

/**
 * Apply a basis-point multiplier to an 8-decimal oracle price, truncating the
 * way integer arithmetic does. A multiplier of `10_000` returns the price
 * unchanged, so the grid's "no shock" column matches the baseline exactly.
 */
export function applyPriceShock(price: bigint, multiplierBps: bigint): bigint {
  if (price <= 0n) return 0n;
  if (multiplierBps <= 0n) return 0n;
  return (price * multiplierBps) / 10_000n;
}

/**
 * Compute the position's health factor across a grid of price shocks.
 *
 * Each shock is applied to exactly one asset at a time — the debt leg or one
 * collateral leg — while every other price stays at its original value. That
 * is the shape a heatmap needs: rows are assets, columns are shocks, and each
 * cell is the health factor the contract would report under that scenario.
 *
 * The unshocked column (`multiplierBps === 10_000`) reproduces `baseline`
 * exactly, because `applyPriceShock` is the identity at that multiplier and the
 * health factor math is unchanged.
 */
export function computeShockGrid(
  position: Position,
  shocks: ShockStep[],
  constants: ContractConstants,
): ShockGrid {
  const baseline = evaluatePosition(position, constants);

  const rows: ShockRow[] = [];

  const debtRow = buildShockRow(
    position,
    position.debt.asset,
    true,
    shocks,
    constants,
  );
  if (debtRow) rows.push(debtRow);

  for (const leg of position.collateral) {
    const row = buildShockRow(position, leg.asset, false, shocks, constants);
    if (row) rows.push(row);
  }

  return { shocks, rows, baseline };
}

/**
 * Build one row of the grid by re-pricing a single asset at each shock step.
 *
 * Returns `null` when the asset does not appear in the position, so callers
 * cannot accidentally emit an empty row.
 */
function buildShockRow(
  position: Position,
  asset: string,
  isDebt: boolean,
  shocks: ShockStep[],
  constants: ContractConstants,
): ShockRow | null {
  const appears = isDebt
    ? position.debt.asset === asset && position.debt.amount > 0n
    : position.collateral.some((leg) => leg.asset === asset);
  if (!appears) return null;

  const cells: ShockCell[] = shocks.map((shock) => {
    const shocked = shockPositionPrice(position, asset, isDebt, shock.multiplierBps);
    const health = evaluatePosition(shocked, constants);
    const shockedPrice = isDebt
      ? shocked.debt.price
      : (shocked.collateral.find((leg) => leg.asset === asset)?.price ?? 0n);
    return {
      shock,
      shockedPrice,
      healthFactor: health.healthFactor,
      isLiquidatable: health.isLiquidatable,
      healthFactorKnown: health.healthFactorKnown,
      unknownReason: health.unknownReason,
      maxLiquidatableAmount: health.maxLiquidatableAmount,
    };
  });

  return { asset, isDebt, cells };
}

/** Return a copy of `position` with one asset's price scaled by `multiplierBps`. */
function shockPositionPrice(
  position: Position,
  asset: string,
  isDebt: boolean,
  multiplierBps: bigint,
): Position {
  if (isDebt) {
    return {
      ...position,
      debt: {
        ...position.debt,
        price: applyPriceShock(position.debt.price, multiplierBps),
      },
    };
  }
  return {
    ...position,
    collateral: position.collateral.map((leg) =>
      leg.asset === asset
        ? { ...leg, price: applyPriceShock(leg.price, multiplierBps) }
        : leg,
    ),
  };
}
