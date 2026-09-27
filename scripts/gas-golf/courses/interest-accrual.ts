/**
 * Reference implementation: interest accrual (#1014).
 *
 * The benchmark suite already frames this contest — `lending::interest_full_recompute`
 * (600k budget) against `interest_incremental_update` (220k) against
 * `interest_same_block_cached` (80k). What was missing was a way to check that
 * an "optimised" accrual still produces the same answer, which is what this
 * course is for.
 *
 * The math is index-based, matching how an on-ledger accrual actually works:
 *
 * ```text
 * interest = principal * (index_now - index_last) / INDEX_SCALE
 * ```
 *
 * with `INDEX_SCALE = 1e9` and the index advanced by `rate_bps * elapsed / YEAR_BPS`
 * accumulated across updates. The trap for an optimiser is the *order* of the
 * integer divisions: rounding the per-update index increment and rounding the
 * final interest are different results for the same total elapsed time, so a
 * "cheaper" implementation that reassociates the arithmetic drifts.
 *
 * Amounts are strings, because they are `i128` and the comparison is exact.
 */

const INDEX_SCALE = 1_000_000_000n;
const BPS = 10_000n;
const SECONDS_PER_YEAR = 31_536_000n;
const I128_MAX = (1n << 127n) - 1n;

export const name = "reference: index-based interest accrual";

export interface AccrualInputs {
  /** Deposited principal, in the asset's base units. */
  principal: string;
  /** Annual rate in basis points, e.g. `800` for 8%. */
  rateBps: string;
  /** Elapsed seconds since the last update. */
  elapsedSeconds: string;
  /** Accumulated index at the last update, 1e9-scaled. */
  indexLast: string;
  /** Number of discrete updates the period was split into. */
  updates: string;
}

function toI128(value: bigint): bigint {
  return value > I128_MAX || value < -(1n << 127n) ? 0n : value;
}

export async function run(scenario: { inputs: AccrualInputs }): Promise<unknown> {
  const { principal, rateBps, elapsedSeconds, indexLast, updates } = scenario.inputs;
  const p = BigInt(principal);
  const rate = BigInt(rateBps);
  const elapsed = BigInt(elapsedSeconds);
  const base = BigInt(indexLast);
  const steps = BigInt(updates) > 0n ? BigInt(updates) : 1n;

  if (p <= 0n || rate <= 0n || elapsed <= 0n) {
    return { interest: "0", indexNow: base.toString(), total: p.toString() };
  }

  // The index advances once per update, each step covering `elapsed / steps`.
  // Truncating per step is what the contract does, so it is what is modelled.
  const indexNow = base + (rate * (elapsed / steps) * INDEX_SCALE) / (SECONDS_PER_YEAR * BPS);
  const delta = indexNow - base;
  const interest = toI128((p * delta) / INDEX_SCALE);

  return {
    interest: interest.toString(),
    indexNow: indexNow.toString(),
    total: (p + interest).toString(),
  };
}
