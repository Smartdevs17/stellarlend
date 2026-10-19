/**
 * Shared types for the lending pool gas cost estimator (#1011).
 *
 * The tool answers two questions for every lending pool entry point:
 *   1. "How much does this operation cost on Soroban?"      -> `estimator.ts`
 *   2. "Why does it cost that much, and what can change?"   -> `suggestions.ts`
 *
 * Everything here is a plain data shape so the three modules can be tested
 * independently and the whole report round-trips through JSON.
 */

/** Soroban storage tiers, cheapest and smallest first. */
export type StorageTier = "persistent" | "instance" | "temporary";

/**
 * The four operations a Soroban ledger entry can be subject to.
 * `write`/`remove` mutate rent-bearing state; `read`/`exists` only widen the
 * transaction footprint.
 */
export type StorageOpKind = "read" | "write" | "exists" | "remove";

/** A single `env.storage().<tier>().<op>(..)` call site found in the source. */
export interface StorageAccess {
  tier: StorageTier;
  kind: StorageOpKind;
  /** Normalised key expression, e.g. `HotStorageKey::DepositState`. */
  key: string;
  /** Owning key enum, e.g. `HotStorageKey`; empty for variable/literal keys. */
  keyEnum: string;
  /** True when the access sits inside a `for`/`while`/`loop` body. */
  perIteration: boolean;
  /**
   * Estimated serialized size of the written value in bytes, when the value
   * type could be resolved. `null` when the key is only ever read, or when the
   * value type is a local/parameter we could not trace.
   */
  entryBytes: number | null;
  file: string;
  line: number;
  /** Internal function that performed the access when reached transitively. */
  via?: string;
}

/** A call that leaves the current contract (token transfer, oracle read, ...). */
export interface CrossContractCall {
  /** `invoke_contract`, `token_client`, ... */
  kind: string;
  file: string;
  line: number;
  via?: string;
}

/** Resolved storage behaviour of a single contract entry point. */
export interface OperationFacts {
  /** Normalised operation name, e.g. `deposit`. */
  operation: string;
  /** On-contract name, e.g. `LendingContract::deposit`. */
  entryPoint: string;
  file: string;
  line: number;
  accesses: StorageAccess[];
  crossContractCalls: CrossContractCall[];
  /** Internal functions reached from the entry point. */
  callees: string[];
  /** Calls we could not resolve to a definition in the scanned tree. */
  unresolvedCalls: string[];
  /** True when the entry point body itself contains a loop. */
  hasLoop: boolean;
}

/** Aggregate per-tier/op storage counts derived from `OperationFacts`. */
export interface StorageSummary {
  reads: number;
  writes: number;
  exists: number;
  removes: number;
  persistent: number;
  instance: number;
  temporary: number;
  /**
   * Counts per kind for the rent-bearing tiers (persistent + instance). These
   * drive the storage fees; the per-tier totals above cannot be subtracted
   * reliably because they mix kinds.
   */
  ledger: { reads: number; writes: number; exists: number; removes: number };
  /**
   * Counts per kind for `temporary` (scratch) entries, which are priced at
   * `temporaryAccessStroops` and are never rent-bearing.
   */
  scratch: { reads: number; writes: number; exists: number; removes: number };
  /** Distinct ledger entries touched — this is the transaction footprint. */
  footprintEntries: number;
  /** Distinct entries written (the rent-bearing part of the footprint). */
  writtenEntries: number;
  /** Sum of `entryBytes` over resolvable writes; `null` if any write is unknown. */
  estimatedWriteBytes: number | null;
}

/** Priced cost of one operation, in stroops (1 XLM = 10,000,000 stroops). */
export interface CostBreakdown {
  baseFeeStroops: number;
  storageWriteStroops: number;
  storageReadStroops: number;
  storageExistsStroops: number;
  storageRemoveStroops: number;
  crossContractStroops: number;
  resourceStroops: number;
  totalStroops: number;
  totalXlm: number;
}

/** Estimator input knobs. All stroop values are overridable. */
export interface CostModel {
  /** Flat per-transaction inclusion fee. */
  baseFeeStroops: number;
  /** Charged for every persistent/instance `set`. */
  storageWriteStroops: number;
  /** Charged for every persistent/instance `get` (footprint read). */
  storageReadStroops: number;
  /** Charged for every `has` (existence probe, still a footprint read). */
  storageExistsStroops: number;
  /** Charged for every `remove` (rent is returned, the read is not free). */
  storageRemoveStroops: number;
  /** Charged for every `temporary` access — scratch entries, priced high. */
  temporaryAccessStroops: number;
  /** Charged for every call that leaves the contract. */
  crossContractStroops: number;
  /** Divisor turning a measured CPU instruction count into stroops. */
  cpuStroopDivisor: number;
  /** Divisor turning a measured memory byte count into stroops. */
  memoryStroopDivisor: number;
}

/** A measured CPU/memory baseline for an operation, when one is known. */
export interface ResourceBaseline {
  cpuInstructions: number | null;
  memoryBytes: number | null;
  source: string;
}

/** One priced operation: facts + summary + cost. */
export interface OperationEstimate {
  operation: string;
  entryPoint: string;
  facts: OperationFacts;
  storage: StorageSummary;
  cost: CostBreakdown;
  baseline: ResourceBaseline;
}

/** How a suggestion was derived; used for `--only` filtering. */
export type SuggestionCategory =
  | "packing"
  | "redundant-access"
  | "loop-amplification"
  | "instance-guard"
  | "migration"
  | "footprint"
  | "cross-contract"
  | "drift";

export type Severity = "info" | "low" | "medium" | "high" | "critical";

/** Ascending severity, used for sorting and for the `--fail-on` gate. */
export const SEVERITY_ORDER: Record<Severity, number> = {
  info: 0,
  low: 1,
  medium: 2,
  high: 3,
  critical: 4,
};

/** A storage-pattern-derived optimization recommendation. */
export interface Suggestion {
  id: string;
  category: SuggestionCategory;
  severity: Severity;
  operation: string;
  title: string;
  detail: string;
  /** Storage-pattern evidence that triggered the rule (keys, files, lines). */
  evidence: string[];
  /** Best-effort annualised/one-off saving in stroops, `null` when unknown. */
  estimatedSavingStroops: number | null;
}

/** Drift between the static truth and a hand-maintained complexity table. */
export interface DriftFinding {
  operation: string;
  tableWrites: number;
  indexedWrites: number;
  delta: number;
  tableCrossContractCalls: number;
  indexedCrossContractCalls: number;
}

/** The full machine-readable report. */
export interface EstimateReport {
  contract: string;
  sourceDir: string;
  model: CostModel;
  iterations: number;
  operations: OperationEstimate[];
  suggestions: Suggestion[];
  drift: DriftFinding[];
  /**
   * How to read these numbers. Static analysis counts call sites, so a few of
   * them are upper bounds; the caveats say which and why.
   */
  caveats: string[];
  totals: {
    operations: number;
    reads: number;
    writes: number;
    footprintEntries: number;
    totalStroops: number;
    suggestions: number;
    worstSeverity: Severity | null;
  };
}
