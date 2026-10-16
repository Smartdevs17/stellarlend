/**
 * Reference implementation of interest accrual (#1067).
 *
 * Continuous compounding: amount = principal * e^(rate * years).
 * This stands in for the "trusted" reference implementation (e.g. the
 * on-chain math or the original contract logic).
 */

export const name = "interest-v1 (continuous)";

export async function run(scenario: { inputs: { principal: number; rate: number; years: number } }): Promise<unknown> {
  const { principal, rate, years } = scenario.inputs;
  const grown = principal * Math.exp(rate * years);
  return {
    accrued: grown - principal,
    total: grown,
    count: 1,
  };
}