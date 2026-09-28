/**
 * Submission: cache the threshold lookup, keep both divisions (#1014).
 *
 * The realistic optimisation for `get_health_factor` is hoisting the threshold
 * read out of the per-call path, not touching the arithmetic. `compute_health_factor`
 * in `views.rs` calls `get_liquidation_threshold_bps(env)` on every invocation;
 * a packed or memoised read of that one key is a real saving and leaves the
 * result identical.
 *
 * The point of this submission is that it is *correct* — the gate must pass it,
 * which is what makes the rejected submissions next to it meaningful.
 */

const HEALTH_FACTOR_SCALE = 10_000n;
const HEALTH_FACTOR_NO_DEBT = 100_000_000n;
const BPS = 10_000n;
const PRICE_SCALE = 100_000_000n;
const I128_MAX = (1n << 127n) - 1n;

export const name = "submission: cached-threshold health factor";

/** The one hoisted read. A real contract would cache the key per ledger close. */
const thresholdCache = new Map<string, bigint>();

function thresholdFor(period: string, fallback: bigint): bigint {
  const cached = thresholdCache.get(period);
  if (cached !== undefined) return cached;
  thresholdCache.set(period, fallback);
  return fallback;
}

function toI128(value: bigint): bigint {
  return value > I128_MAX ? 0n : value;
}

function value(amount: bigint, price: bigint, oraclePresent: boolean): bigint {
  if (amount <= 0n) return 0n;
  if (!oraclePresent) return 0n;
  if (price <= 0n) return 0n;
  return toI128((amount * price) / PRICE_SCALE);
}

export async function run(scenario: {
  inputs: {
    collateralAmount: string;
    collateralPrice: string;
    debtAmount: string;
    debtPrice: string;
    liquidationThresholdBps?: string;
    oraclePresent?: boolean;
  };
}): Promise<unknown> {
  const { inputs } = scenario;
  const oraclePresent = inputs.oraclePresent !== false;
  const threshold = thresholdFor(
    inputs.liquidationThresholdBps ?? "default",
    BigInt(inputs.liquidationThresholdBps ?? "8000"),
  );

  const collateralValue = value(BigInt(inputs.collateralAmount), BigInt(inputs.collateralPrice), oraclePresent);
  const debtValue = value(BigInt(inputs.debtAmount), BigInt(inputs.debtPrice), oraclePresent);
  const hasDebt = BigInt(inputs.debtAmount) > 0n;

  let healthFactor: bigint;
  if (debtValue <= 0n) {
    healthFactor = hasDebt ? 0n : HEALTH_FACTOR_NO_DEBT;
  } else if (!oraclePresent) {
    healthFactor = 0n;
  } else {
    // Both divisions kept: the intermediate truncation is observable.
    const weighted = (collateralValue * threshold) / BPS;
    healthFactor = toI128((weighted * HEALTH_FACTOR_SCALE) / debtValue);
  }

  return {
    collateralValue: collateralValue.toString(),
    debtValue: debtValue.toString(),
    healthFactor: healthFactor.toString(),
    liquidatable: healthFactor > 0n && healthFactor < HEALTH_FACTOR_SCALE,
  };
}
