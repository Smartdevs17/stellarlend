/**
 * Reference implementation: the lending pool's health factor (#1014).
 *
 * This is the contract's arithmetic, not a JS approximation of it. From
 * `stellar-lend/contracts/lending/src/views.rs`:
 *
 * ```text
 * weighted_collateral = collateral_value * liquidation_threshold_bps / 10_000
 * health_factor       = weighted_collateral * HEALTH_FACTOR_SCALE / debt_value
 * ```
 *
 * Two details make it a useful golf target rather than a toy:
 *
 * 1. Both divisions **truncate** toward zero, and the intermediate truncation is
 *    observable: `(1501 * 6667) / 10000` is `1000`, so the health factor comes
 *    out at `10000` — exactly on the boundary. Computing it as
 *    `collateral_value * threshold_bps / debt_value` in one step gives `10007`.
 *    An "optimisation" that collapses the two divisions therefore looks free and
 *    is wrong, which is exactly the exploit the gate exists to catch.
 * 2. The zero-debt case returns a sentinel rather than a ratio, and a position
 *    with debt but no price feed returns `0` rather than a ratio.
 *
 * Values are returned as **strings** because they are `i128`; the comparison is
 * exact, so a submission must return integers rather than floats.
 */

const PRICE_SCALE = 100_000_000n; // views.rs:20
const HEALTH_FACTOR_SCALE = 10_000n; // views.rs:23
const HEALTH_FACTOR_NO_DEBT = 100_000_000n; // views.rs:26
const BPS = 10_000n;

const I128_MAX = (1n << 127n) - 1n;

export const name = "reference: views.rs compute_health_factor";

export interface HealthFactorInputs {
  /** Collateral amount in the asset's base units. */
  collateralAmount: string;
  /** Oracle price, 8 decimals. */
  collateralPrice: string;
  /** Principal + accrued interest, in the debt asset's base units. */
  debtAmount: string;
  /** Oracle price of the debt asset, 8 decimals. */
  debtPrice: string;
  /** `liquidation_threshold_bps`; defaults to the contract's 8000. */
  liquidationThresholdBps?: string;
  /** Whether the pool has an oracle configured. */
  oraclePresent?: boolean;
}

function toI128(value: bigint): bigint {
  return value > I128_MAX ? 0n : value;
}

/** `collateral_value` / `debt_value`: 0 unless amount and price are both positive. */
function value(amount: bigint, price: bigint, oraclePresent: boolean): bigint {
  if (amount <= 0n) return 0n;
  if (!oraclePresent) return 0n;
  if (price <= 0n) return 0n;
  return toI128((amount * price) / PRICE_SCALE);
}

export async function run(scenario: { inputs: HealthFactorInputs }): Promise<unknown> {
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
    const weighted = (collateralValue * threshold) / BPS;
    healthFactor = toI128((weighted * HEALTH_FACTOR_SCALE) / debtValue);
  }

  return {
    collateralValue: collateralValue.toString(),
    debtValue: debtValue.toString(),
    // The exact-value flag is what makes the truncation observable to the gate.
    healthFactor: healthFactor.toString(),
    liquidatable: healthFactor > 0n && healthFactor < HEALTH_FACTOR_SCALE,
  };
}
