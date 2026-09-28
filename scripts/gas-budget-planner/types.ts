/**
 * Shared types for the lending protocol gas budget planner (#1012).
 *
 * A lender describes an *interaction pattern* — how many deposits, borrows,
 * repays and withdrawals they expect per period — and this tool turns it into a
 * gas budget: what each operation costs, what the pattern costs in total, how
 * that compares against a budget, and which levers would reduce it.
 *
 * Everything is a plain data shape so the whole plan round-trips through JSON.
 */

/** 1 XLM = 10,000,000 stroops. */
export const STROOPS_PER_XLM = 10_000_000;

/** A built-in operation name understood by the API's cost table. */
export type PlannedOperation =
  | "deposit"
  | "withdraw"
  | "borrow"
  | "repay"
  | "liquidation"
  | "flash_loan"
  | "emergency_withdraw";

/** What a lender expects to do, per period. */
export interface InteractionPattern {
  name: string;
  description?: string;
  /** Period the counts below apply to, in days. Default 30 (one month). */
  periodDays?: number;
  /** Expected number of calls per period, keyed by operation name. */
  operations: Record<string, number>;
  /**
   * Average number of items per `deposit_batch` call, when the pattern uses
   * batching. Lets the planner price batched deposits as the contract does —
   * one shared write instead of one per item.
   */
  batchSize?: number;
  /** Gas budget in stroops. Combined with the CLI `--budget-*` flags. */
  budgetStroops?: number;
  /** USD price of XLM, used for the USD column. */
  xlmPriceUsd?: number;
  /**
   * Override the amortisation of an operation, as a per-item multiplier.
   * `1` means every call costs a full operation; `0` means the cost is fully
   * shared. A plan file can set this to model a different contract revision.
   */
  amortisation?: Record<string, number>;
}

/** Per-operation storage/CPU cost as published by the repo. */
export interface OperationCost {
  operation: string;
  baseFeeStroops: number;
  storageWriteStroops: number;
  crossContractStroops: number;
  resourceStroops: number;
  totalStroops: number;
  totalXlm: number;
  totalUsd: number | null;
  /** Persistent entries this call writes — the dominant term in most cases. */
  storageWrites: number;
  crossContractCalls: number;
  /** CPU instructions, from the API baseline or a measured benchmark. */
  cpuInstructions: number | null;
  /** Where `cpuInstructions` came from, for the report to cite. */
  cpuSource: string;
  /** The protocol's committed CPU-instruction budget for this call. */
  instructionBudget: number | null;
  /** `cpuInstructions / instructionBudget` as a percentage, when both are known. */
  budgetUtilisationPct: number | null;
}

/** One line of the plan: an operation, how many times, and what it costs. */
export interface PlannedLine {
  operation: string;
  calls: number;
  unitStroops: number;
  subtotalStroops: number;
  /** Share of the plan's total cost, as a percentage. */
  sharePct: number;
  /** The unit cost each call actually pays after amortisation. */
  amortisedUnitStroops: number;
}

/** Outcome of comparing a plan against a budget. */
export interface BudgetCheck {
  budgetStroops: number;
  totalStroops: number;
  /** `total / budget * 100`, or `null` when no budget was supplied. */
  utilisationPct: number | null;
  /** `budget - total`; negative when over budget. */
  headroomStroops: number | null;
  status: "unset" | "within" | "tight" | "over";
}

export type PlanSeverity = "info" | "low" | "medium" | "high";

export type PlanSuggestionCategory =
  | "batching"
  | "concentration"
  | "growth"
  | "headroom"
  | "storage-writes";

/** A lever a lender can pull, derived from the plan's own shape. */
export interface PlanSuggestion {
  id: string;
  category: PlanSuggestionCategory;
  severity: PlanSeverity;
  title: string;
  detail: string;
  /** Stroops saved (or, for projections, added) if the lever is taken. */
  estimatedDeltaStroops: number;
}

/** A projection of the plan at a different volume. */
export interface Projection {
  /** Volume multiplier, e.g. `2` for twice the planned activity. */
  multiplier: number;
  totalStroops: number;
  totalXlm: number;
  utilisationPct: number | null;
  status: BudgetCheck["status"];
}

/** The full machine-readable plan. */
export interface GasBudgetPlan {
  pattern: {
    name: string;
    description: string;
    periodDays: number;
    operations: Record<string, number>;
    batchSize: number;
    xlmPriceUsd: number | null;
  };
  costs: OperationCost[];
  lines: PlannedLine[];
  totals: {
    periodDays: number;
    calls: number;
    totalStroops: number;
    totalXlm: number;
    totalUsd: number | null;
    annualisedStroops: number;
    /** Most expensive single call in the pattern, for the "hot spot" line. */
    costliestCallStroops: number;
  };
  budget: BudgetCheck;
  projections: Projection[];
  suggestions: PlanSuggestion[];
  /** Every operation the loaded cost table knows about, for `--operation-costs`. */
  knownOperations: string[];
}
