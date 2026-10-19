/**
 * Reimplementation of interest accrual (#1067).
 *
 * Discrete daily compounding: amount = principal * (1 + rate/365)^(365*years).
 * Shares rounding with on-ledger integer-ish math; within the differential
 * tolerance it must agree with the continuous reference (interest-v1).
 */

export const name = "interest-v2 (daily discrete)";

export async function run(scenario: { inputs: { principal: number; rate: number; years: number } }): Promise<unknown> {
  const { principal, rate, years } = scenario.inputs;
  const grown = principal * Math.pow(1 + rate / 365, 365 * years);
  return {
    accrued: grown - principal,
    total: grown,
    count: 1,
  };
}