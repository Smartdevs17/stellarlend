/**
 * Contract-sourced constants for the position health simulator (#1013).
 *
 * The health factor, the close factor and the liquidation incentive all depend
 * on constants that live in the Soroban source: `HEALTH_FACTOR_SCALE`,
 * `PRICE_SCALE`, `HEALTH_FACTOR_NO_DEBT`, and the admin-settable defaults that
 * `borrow.rs` falls back to. Hard-coding copies of those numbers in a simulator
 * is how a tool ends up disagreeing with the contract while looking correct, so
 * they are read out of the source instead.
 *
 * Every loader is pure: it takes file contents and returns data, so the whole
 * module is testable without touching the filesystem.
 */

/** The numbers the simulation needs, all in the contract's own terms. */
export interface ContractConstants {
  /** `PRICE_SCALE` — oracle prices carry 8 decimals, so 1.0 is 100_000_000. */
  priceScale: bigint;
  /** `HEALTH_FACTOR_SCALE` — 10_000 means a ratio of exactly 1.0. */
  healthFactorScale: bigint;
  /** `HEALTH_FACTOR_NO_DEBT` — the sentinel for a debt-free position. */
  healthFactorNoDebt: bigint;
  /** `get_liquidation_threshold_bps().unwrap_or(8000)`. */
  defaultLiquidationThresholdBps: bigint;
  /** `get_close_factor_bps().unwrap_or(5000)`. */
  defaultCloseFactorBps: bigint;
  /** `get_liquidation_incentive_bps().unwrap_or(1000)`. */
  defaultLiquidationIncentiveBps: bigint;
  /** `COLLATERAL_RATIO_MIN` — the borrow-time 150% raw-amount floor. */
  collateralRatioMinBps: bigint;
  /** Where each value was read, so the report can cite it. */
  sources: {
    views: string;
    borrow: string;
  };
}

/**
 * Values used when a constant cannot be found.
 *
 * These are the values the contract has on `main` today. They are a floor, not
 * a substitute: `assertContractConstants` fails the run when a constant is
 * missing, so a rename surfaces as a clear error rather than a silent guess.
 */
export const FALLBACK_CONSTANTS: Omit<ContractConstants, "sources"> = {
  priceScale: 100_000_000n,
  healthFactorScale: 10_000n,
  healthFactorNoDebt: 100_000_000n,
  defaultLiquidationThresholdBps: 8_000n,
  defaultCloseFactorBps: 5_000n,
  defaultLiquidationIncentiveBps: 1_000n,
  collateralRatioMinBps: 15_000n,
};

/** Names that must be recoverable, so a rename cannot pass silently. */
export const REQUIRED_CONSTANT_NAMES = [
  "PRICE_SCALE",
  "HEALTH_FACTOR_SCALE",
  "HEALTH_FACTOR_NO_DEBT",
  "COLLATERAL_RATIO_MIN",
] as const;

/** Rust writes large literals with `_` separators; BigInt does not accept them. */
function literal(text: string): bigint {
  return BigInt(text.replace(/_/g, ""));
}

/** `const NAME: i128 = 1_000;` in any file. */
function parseConst(source: string, name: string): bigint | null {
  const match = new RegExp(
    `\\bconst\\s+${name}\\s*:\\s*i128\\s*=\\s*(-?\\d[\\d_]*)`,
  ).exec(source);
  return match ? literal(match[1]) : null;
}

/**
 * The `.unwrap_or(N)` default of a getter, e.g. `get_close_factor_bps`.
 *
 * Scoped to the function body so a different `unwrap_or` elsewhere in the file
 * cannot be mistaken for the parameter default.
 */
function parseGetterDefault(source: string, fnName: string): bigint | null {
  const start = source.indexOf(`fn ${fnName}`);
  if (start === -1) return null;
  const body = source.slice(start, source.indexOf("\n}", start) + 2);
  const match = /\.unwrap_or\(\s*(-?\d[\d_]*)\s*\)/.exec(body);
  return match ? literal(match[1]) : null;
}

/** Read the constants out of `views.rs` and `borrow.rs`. */
export function loadContractConstants(sources: {
  views: string;
  borrow: string;
  paths: { views: string; borrow: string };
}): ContractConstants {
  const pick = (found: bigint | null, fallback: bigint): bigint =>
    found === null ? fallback : found;

  return {
    priceScale: pick(parseConst(sources.views, "PRICE_SCALE"), FALLBACK_CONSTANTS.priceScale),
    healthFactorScale: pick(
      parseConst(sources.views, "HEALTH_FACTOR_SCALE"),
      FALLBACK_CONSTANTS.healthFactorScale,
    ),
    healthFactorNoDebt: pick(
      parseConst(sources.views, "HEALTH_FACTOR_NO_DEBT"),
      FALLBACK_CONSTANTS.healthFactorNoDebt,
    ),
    collateralRatioMinBps: pick(
      parseConst(sources.borrow, "COLLATERAL_RATIO_MIN"),
      FALLBACK_CONSTANTS.collateralRatioMinBps,
    ),
    defaultLiquidationThresholdBps: pick(
      parseGetterDefault(sources.borrow, "get_liquidation_threshold_bps"),
      FALLBACK_CONSTANTS.defaultLiquidationThresholdBps,
    ),
    defaultCloseFactorBps: pick(
      parseGetterDefault(sources.borrow, "get_close_factor_bps"),
      FALLBACK_CONSTANTS.defaultCloseFactorBps,
    ),
    defaultLiquidationIncentiveBps: pick(
      parseGetterDefault(sources.borrow, "get_liquidation_incentive_bps"),
      FALLBACK_CONSTANTS.defaultLiquidationIncentiveBps,
    ),
    sources: sources.paths,
  };
}

/** Constants that could not be read from the contract source. */
export function missingConstants(
  sources: { views: string; borrow: string },
): string[] {
  const missing: string[] = [];
  for (const name of REQUIRED_CONSTANT_NAMES) {
    const found =
      name === "COLLATERAL_RATIO_MIN"
        ? parseConst(sources.borrow, name)
        : parseConst(sources.views, name);
    if (found === null) missing.push(name);
  }
  for (const [fn, where_] of [
    ["get_liquidation_threshold_bps", "defaultLiquidationThresholdBps"],
    ["get_close_factor_bps", "defaultCloseFactorBps"],
    ["get_liquidation_incentive_bps", "defaultLiquidationIncentiveBps"],
  ] as const) {
    void where_;
    if (parseGetterDefault(sources.borrow, fn) === null) missing.push(fn);
  }
  return missing;
}

/**
 * Fail loudly when a constant is missing, rather than simulating against a
 * fallback that may no longer match the contract.
 */
export function assertContractConstants(missing: string[]): void {
  if (missing.length > 0) {
    throw new Error(
      `could not read ${missing.join(", ")} from the contract source; ` +
        "the simulator would be guessing at the contract's parameters",
    );
  }
}
