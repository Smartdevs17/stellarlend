/**
 * Reference implementation: liquidation sizing (#1014).
 *
 * Two single truncating divisions from
 * `stellar-lend/contracts/lending/src/views.rs`, with the edge cases that an
 * "optimised" rewrite tends to drop:
 *
 * ```text
 * get_max_liquidatable_amount     = total_debt * close_factor_bps / 10_000
 * get_liquidation_incentive_amount = repay_amount * (10_000 + incentive_bps) / 10_000
 * ```
 *
 * - `get_max_liquidatable_amount` returns `0` when the position is healthy
 *   (`hf >= HEALTH_FACTOR_SCALE`), when the health factor is `0` (no oracle, so
 *   it cannot act), and when there is no debt. Dropping any of those three turns
 *   a healthy position into a liquidatable one.
 * - `get_liquidation_incentive_amount` returns `0` for a zero or negative
 *   repayment, and **saturates** to `i128::MAX` on overflow where the health
 *   factor falls back to `0`. Two different overflow behaviours in one contract,
 *   which is exactly the kind of detail a rewrite loses.
 *
 * Amounts are strings, because they are `i128` and the comparison is exact.
 */

const BPS = 10_000n;
const HEALTH_FACTOR_SCALE = 10_000n;
const I128_MAX = (1n << 127n) - 1n;

export const name = "reference: views.rs liquidation sizing";

export interface SizingInputs {
  /** Principal + accrued interest, in the debt asset's base units. */
  totalDebt: string;
  /** The position's health factor, 1e4-scaled. */
  healthFactor: string;
  /** `close_factor_bps`; defaults to the contract's 5000. */
  closeFactorBps?: string;
  /** `liquidation_incentive_bps`; defaults to the contract's 1000. */
  liquidationIncentiveBps?: string;
  /** The repayment being sized; defaults to the maximum. */
  repayAmount?: string;
}

export async function run(scenario: { inputs: SizingInputs }): Promise<unknown> {
  const { totalDebt, healthFactor } = scenario.inputs;
  const closeFactor = BigInt(scenario.inputs.closeFactorBps ?? "5000");
  const incentive = BigInt(scenario.inputs.liquidationIncentiveBps ?? "1000");

  const debt = BigInt(totalDebt);
  const hf = BigInt(healthFactor);

  let maxRepay = 0n;
  if (debt > 0n && hf !== 0n && hf < HEALTH_FACTOR_SCALE) {
    maxRepay = debt > I128_MAX / BPS ? I128_MAX : (debt * closeFactor) / BPS;
  }

  const repay = scenario.inputs.repayAmount === undefined ? maxRepay : BigInt(scenario.inputs.repayAmount);
  const bonus = repay <= 0n ? 0n : (repay * (BPS + incentive)) / BPS;

  return {
    maxRepayable: maxRepay.toString(),
    maxBonus: bonus.toString(),
  };
}
