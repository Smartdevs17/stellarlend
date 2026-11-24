/**
 * Issue #1759 — SDK-facing mirror of `contracts/pool_math`.
 *
 * Every helper is intentionally pure and uses `bigint`. Scales match the Rust
 * crate exactly (bps = 10_000, indexes = 1e12, year = 365 days), so SDK
 * previews cannot drift from on-chain results.
 */

export const BPS_DIVISOR = 10_000n;
export const SECONDS_PER_YEAR = 365n * 86_400n;
export const INDEX_SCALE = 1_000_000_000_000n;
export const NO_DEBT_HEALTH_FACTOR = BigInt('170141183460469231731687303715884105727'); // i128::MAX

export type MathErrorCode = 'DIVISION_BY_ZERO' | 'OVERFLOW' | 'INVALID_INPUT';

export class MathError extends Error {
  readonly code: MathErrorCode;

  constructor(code: MathErrorCode, message: string) {
    super(message);
    this.name = 'MathError';
    this.code = code;
  }
}

function invalid(message: string): never {
  throw new MathError('INVALID_INPUT', message);
}

/** Overflow-safe `a * b / d` with truncating division semantics. */
export function checkedMulDiv(a: bigint, b: bigint, d: bigint): bigint {
  if (d === 0n) {
    throw new MathError('DIVISION_BY_ZERO', 'division by zero');
  }
  return (a * b) / d;
}

/** Apply `bps` basis points to a non-negative amount. */
export function applyBps(amount: bigint, bps: bigint): bigint {
  if (amount < 0n || bps < 0n) {
    invalid('amount and bps must be non-negative');
  }
  return checkedMulDiv(amount, bps, BPS_DIVISOR);
}

/** Express `numerator / denominator` in basis points; zero denominator is 0. */
export function ratioBps(numerator: bigint, denominator: bigint): bigint {
  if (numerator < 0n || denominator < 0n) {
    invalid('ratio inputs must be non-negative');
  }
  if (denominator === 0n) {
    return 0n;
  }
  return checkedMulDiv(numerator, BPS_DIVISOR, denominator);
}

/** Pool utilisation in basis points, clamped to 10_000. */
export function utilizationBps(totalBorrows: bigint, totalSupply: bigint): bigint {
  if (totalBorrows < 0n || totalSupply < 0n) {
    invalid('utilisation inputs must be non-negative');
  }
  if (totalSupply === 0n) {
    return 0n;
  }
  const raw = ratioBps(totalBorrows, totalSupply);
  return raw > BPS_DIVISOR ? BPS_DIVISOR : raw;
}

/** Simple accrued interest over `elapsedSeconds` using a 365-day year. */
export function simpleInterest(principal: bigint, rateBps: bigint, elapsedSeconds: bigint): bigint {
  if (principal < 0n || rateBps < 0n || elapsedSeconds < 0n) {
    invalid('interest inputs must be non-negative');
  }
  return checkedMulDiv(
    checkedMulDiv(principal, rateBps, BPS_DIVISOR),
    elapsedSeconds,
    SECONDS_PER_YEAR,
  );
}

/** Grow a lending index by `rateBps` over `elapsedSeconds`. */
export function accrueIndex(
  currentIndex: bigint,
  rateBps: bigint,
  elapsedSeconds: bigint,
): bigint {
  if (currentIndex <= 0n || rateBps < 0n || elapsedSeconds < 0n) {
    invalid('index must be positive and time/rate non-negative');
  }
  const growth = checkedMulDiv(
    checkedMulDiv(currentIndex, rateBps, BPS_DIVISOR),
    elapsedSeconds,
    SECONDS_PER_YEAR,
  );
  return currentIndex + growth;
}

/** Health factor in bps; debt-free positions return `NO_DEBT_HEALTH_FACTOR`. */
export function healthFactorBps(
  collateralValue: bigint,
  debtValue: bigint,
  liquidationThresholdBps: bigint,
): bigint {
  if (collateralValue < 0n || debtValue < 0n) {
    invalid('position values must be non-negative');
  }
  if (liquidationThresholdBps <= 0n || liquidationThresholdBps > BPS_DIVISOR) {
    invalid('liquidation threshold must be in 1..=10_000 bps');
  }
  if (debtValue === 0n) {
    return NO_DEBT_HEALTH_FACTOR;
  }
  return applyBps(collateralValue, liquidationThresholdBps) / debtValue;
}

/** Maximum additional borrowing power after existing debt. */
export function maxBorrowAmount(
  collateralValue: bigint,
  ltvBps: bigint,
  currentDebt: bigint,
): bigint {
  if (
    collateralValue < 0n ||
    currentDebt < 0n ||
    ltvBps < 0n ||
    ltvBps > BPS_DIVISOR
  ) {
    invalid('invalid borrow inputs');
  }
  const capacity = applyBps(collateralValue, ltvBps);
  return capacity > currentDebt ? capacity - currentDebt : 0n;
}

/** Collateral seized for a repayment, including the liquidation incentive. */
export function seizeAmount(repaid: bigint, incentiveBps: bigint): bigint {
  if (repaid < 0n || incentiveBps < 0n) {
    invalid('repaid and incentive must be non-negative');
  }
  return applyBps(repaid, BPS_DIVISOR + incentiveBps);
}

/** Repayment bounded by both the close factor and the requested amount. */
export function maxRepayable(
  totalDebt: bigint,
  closeFactorBps: bigint,
  requested: bigint,
): bigint {
  if (
    totalDebt < 0n ||
    requested < 0n ||
    closeFactorBps < 0n ||
    closeFactorBps > BPS_DIVISOR
  ) {
    invalid('invalid repayment inputs');
  }
  const cap = applyBps(totalDebt, closeFactorBps);
  const limit = cap < requested ? cap : requested;
  return limit < totalDebt ? limit : totalDebt;
}
