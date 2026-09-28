/**
 * Submission: correct but a deliberately poor strategy (#1014).
 *
 * Recomputes the accrual from scratch on every update instead of advancing a
 * stored index. The benchmark suite already frames this as
 * `interest_full_recompute` (600k budget) versus `interest_incremental_update`
 * (220k). The answer is identical, which is the point: correctness and speed are
 * independent axes, and the gate only judges the first.
 *
 * Kept as the counterpart to the rejected submissions, so the board has a
 * correct-but-slow entry and the scoring can be shown not to be inverted.
 */

const INDEX_SCALE = 1_000_000_000n;
const BPS = 10_000n;
const SECONDS_PER_YEAR = 31_536_000n;
const I128_MAX = (1n << 127n) - 1n;

export const name = "submission: full recompute per update";

function toI128(value: bigint): bigint {
  return value > I128_MAX || value < -(1n << 127n) ? 0n : value;
}

export async function run(scenario: {
  inputs: {
    principal: string;
    rateBps: string;
    elapsedSeconds: string;
    indexLast: string;
    updates: string;
  };
}): Promise<unknown> {
  const { principal, rateBps, elapsedSeconds, indexLast, updates } = scenario.inputs;
  const p = BigInt(principal);
  const rate = BigInt(rateBps);
  const elapsed = BigInt(elapsedSeconds);
  const base = BigInt(indexLast);
  const steps = BigInt(updates) > 0n ? BigInt(updates) : 1n;

  if (p <= 0n || rate <= 0n || elapsed <= 0n) {
    return { interest: "0", indexNow: base.toString(), total: p.toString() };
  }

  // Recomputed from the base index each step instead of carried forward. The
  // same truncating division, applied the same number of times, so the total
  // index matches the incremental version exactly.
  let indexNow = base;
  for (let step = 0n; step < steps; step++) {
    indexNow = base + (rate * (elapsed / steps) * INDEX_SCALE) / (SECONDS_PER_YEAR * BPS);
  }

  const interest = toI128((p * (indexNow - base)) / INDEX_SCALE);
  return {
    interest: interest.toString(),
    indexNow: indexNow.toString(),
    total: (p + interest).toString(),
  };
}
