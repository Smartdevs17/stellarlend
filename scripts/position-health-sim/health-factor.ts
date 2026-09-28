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
