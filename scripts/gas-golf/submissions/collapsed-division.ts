/**
 * Submission: REJECTED — collapse the two divisions into one (#1014).
 *
 * This is the exploit the correctness gate exists to catch, and it is worth
 * keeping in the tree as the worked example.
 *
 * `compute_health_factor` computes
 * `(collateral_value * threshold_bps / 10_000) * 10_000 / debt_value`, and a
 * reviewer skimming it would reasonably read that as
 * `collateral_value * threshold_bps / debt_value` — one division instead of two.
 * It is not the same function. Where the first division truncates, the results
 * diverge, and the divergence is exactly at the values that decide whether a
 * position is liquidatable:
 *
 *   collateral 1501, threshold 6667, debt 1000
 *     contract:  (1501 * 6667 / 10000) * 10000 / 1000 = 1000 * 10000 / 1000 = 10000
 *     collapsed: 1501 * 6667 / 1000                                                      = 10007
 *
 * The two answers straddle `HEALTH_FACTOR_SCALE`. The collapsed version says a
 * position is comfortably healthy; the contract says it is exactly on the
 * boundary. On a different collateral it would put a liquidatable position above
 * the line.
 */

const HEALTH_FACTOR_SCALE = 10_000n;
const HEALTH_FACTOR_NO_DEBT = 100_000_000n;
const PRICE_SCALE = 100_000_000n;
const I128_MAX = (1n << 127n) - 1n;

export const name = "submission (invalid): collapsed division";

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
  const threshold = BigInt(inputs.liquidationThresholdBps ?? "8000");

  const collateralValue = value(BigInt(inputs.collateralAmount), BigInt(inputs.collateralPrice), oraclePresent);
  const debtValue = value(BigInt(inputs.debtAmount), BigInt(inputs.debtPrice), oraclePresent);
  const hasDebt = BigInt(inputs.debtAmount) > 0n;

  let healthFactor: bigint;
  if (debtValue <= 0n) {
    healthFactor = hasDebt ? 0n : HEALTH_FACTOR_NO_DEBT;
  } else if (!oraclePresent) {
    healthFactor = 0n;
  } else {
    // WRONG: drops the intermediate truncation the contract performs.
    healthFactor = toI128((collateralValue * threshold) / debtValue);
  }

  return {
    collateralValue: collateralValue.toString(),
    debtValue: debtValue.toString(),
    healthFactor: healthFactor.toString(),
    liquidatable: healthFactor > 0n && healthFactor < HEALTH_FACTOR_SCALE,
  };
}
