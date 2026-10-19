/**
 * Built-in interaction patterns for the gas budget planner (#1012).
 *
 * Each preset is a plausible lender strategy expressed as calls per period. They
 * exist so `index.ts --preset <name>` gives a lender a starting point to edit,
 * and so the tests have a stable set of patterns spanning the four operations
 * the issue names — deposit, borrow, repay and withdraw.
 *
 * The counts are deliberately round and are *not* derived from any user data:
 * they are a starting hypothesis for someone to overwrite in a plan file.
 */

import type { InteractionPattern } from "./types.ts";

export const PRESETS: Record<string, InteractionPattern> = {
  steady_lender: {
    name: "steady-lender",
    description:
      "Monthly top-ups into one or two assets, periodic exits, and interest paid back on the borrow it funds.",
    periodDays: 30,
    operations: { deposit: 4, borrow: 1, repay: 1, withdraw: 1 },
  },

  borrow_repay_cycle: {
    name: "borrow-repay-cycle",
    description:
      "A leveraged position re-levered monthly: each cycle opens a borrow, pays it down, and re-opens.",
    periodDays: 30,
    operations: { deposit: 1, borrow: 4, repay: 4, withdraw: 1 },
  },

  active_rebalancer: {
    name: "active-rebalancer",
    description:
      "Many small deposits and withdrawals across several assets, which is the shape batching is for.",
    periodDays: 30,
    operations: { deposit: 24, borrow: 2, repay: 2, withdraw: 12 },
    batchSize: 4,
  },

  liquidator: {
    name: "liquidator",
    description:
      "Watching unhealthy positions and closing them. Liquidations are the most expensive call in the table.",
    periodDays: 30,
    operations: { deposit: 1, borrow: 1, repay: 1, liquidation: 4 },
  },

  long_horizon_compounder: {
    name: "long-horizon-compounder",
    description:
      "Quarterly review over a year: one deposit, a few rebalances, and an exit at the end.",
    periodDays: 365,
    operations: { deposit: 1, borrow: 2, repay: 2, withdraw: 1 },
  },
};

/** Preset names, sorted, for `--list-presets`. */
export function presetNames(): string[] {
  return Object.keys(PRESETS).sort();
}

/** Look up a preset by name. */
export function getPreset(name: string): InteractionPattern | null {
  return PRESETS[name] ?? null;
}
