/**
 * The gas golf course: which operations are open targets, and under what budget.
 *
 * The target list is **maintainer-owned** and is read from
 * `stellar-lend/benchmarks/public-functions.json`. A submission therefore cannot
 * introduce its own target, and — more importantly — cannot introduce an
 * operation with no budget, because `framework.rs`'s `get_budget()` returns `0`
 * for an unknown op and `budget == 0` means *unbudgeted*, which would silently
 * disable the gate.
 *
 * Budgets come from the same committed `gas_budgets` / `operation_type_budgets`
 * blocks the rest of the gas tooling reads, with `classifyOperation`'s lexical
 * rules for the per-type fallback. Nothing here is hand-written per operation.
 */

import * as fs from "node:fs";
import * as path from "node:path";

/** The six operation types `api/src/services/gasReport/report.ts` classifies into. */
export type OperationType = "read" | "admin" | "user_write" | "liquidation" | "flash_loan" | "batch";

export const OPERATION_TYPES: readonly OperationType[] = [
  "read",
  "admin",
  "user_write",
  "liquidation",
  "flash_loan",
  "batch",
];

/**
 * `classifyOperation`, ported from `api/src/services/gasReport/report.ts:146-160`.
 *
 * Purely lexical, exactly as the API does it, so a challenge and the gas report
 * agree on what kind of operation they are looking at.
 */
export function classifyOperation(fn: string): OperationType {
  const name = fn.toLowerCase();
  if (name.startsWith("batch_")) return "batch";
  // Views first: `get_max_liquidatable_amount` is a read, not a liquidation.
  if (/^(get_|can_|require_|compute_|list_|hello|error_)/.test(name)) return "read";
  if (name.includes("flash")) return "flash_loan";
  if (name.includes("liquidat")) return "liquidation";
  if (
    /^(set_|initialize|init|gov_initialize|transfer_admin|register_|update_|configure_|add_amm_protocol|claim_reserves)/.test(
      name,
    )
  ) {
    return "admin";
  }
  return "user_write";
}

/** One open target. */
export interface Challenge {
  /** Canonical identity: `contract::fn`, matching `measurementKey`'s base form. */
  id: string;
  contract: string;
  fn: string;
  operationType: OperationType;
  /**
   * Instruction budget. Always present: either the operation's own entry or the
   * operation type's fallback. A `null` here means the op is not scoreable and
   * the reason is reported.
   */
  budget: number | null;
  /** Where the budget came from, for the board to cite. */
  budgetSource: "operation" | "operation-type" | "none";
  /** `false` when there is no budget to score against. */
  scoreable: boolean;
}

/** The course definition, loaded from the repository. */
export interface Course {
  challenges: Challenge[];
  /** Operations in the required list with no budget at all. */
  unscored: string[];
  sources: {
    publicFunctions: string;
    budgets: string;
  };
}

/** Read the required-operation list. */
export function parsePublicFunctions(source: string): string[] {
  const parsed = JSON.parse(source) as { required_operations?: unknown };
  if (!parsed || !Array.isArray(parsed.required_operations)) {
    throw new Error("public-functions.json must have a required_operations array");
  }
  const out: string[] = [];
  for (const entry of parsed.required_operations) {
    if (typeof entry !== "string" || !entry.includes("::")) {
      throw new Error(`required_operations entry must be "contract::op", got ${JSON.stringify(entry)}`);
    }
    if (out.includes(entry)) continue; // de-dupe, keep first
    out.push(entry);
  }
  return out;
}

/** Read the two committed budget blocks. */
export function parseBudgets(source: string): {
  perOperation: Record<string, number>;
  perType: Record<string, number>;
} {
  const parsed = JSON.parse(source) as {
    gas_budgets?: unknown;
    operation_type_budgets?: unknown;
  };
  const numbers = (block: unknown): Record<string, number> => {
    const out: Record<string, number> = {};
    if (block && typeof block === "object") {
      for (const [key, value] of Object.entries(block as Record<string, unknown>)) {
        if (typeof value === "number" && value > 0) out[key] = value;
      }
    }
    return out;
  };
  return {
    perOperation: numbers(parsed.gas_budgets),
    perType: numbers(parsed.operation_type_budgets),
  };
}

/**
 * Split a `contract::op` id, tolerating the ` [scenario]` suffix that
 * `measurementKey` appends.
 *
 * The suffix is stripped *first*: a non-greedy match here would put the whole
 * `lending::deposit` into `contract` and leave `warm` as the function, which
 * would mis-classify every scenario-suffixed key.
 */
export function splitId(id: string): { contract: string; fn: string; scenario: string | null } {
  const match = /^(.*?)\s*\[(.*)\]$/.exec(id);
  const base = match ? match[1] : id;
  const scenario = match ? match[2] : null;
  const parts = base.split("::");
  // No separator means no contract: the bare name is the function and the
  // caller supplies the contract (which is what `gas-baseline.json` does by
  // putting it at the top level). Returning the name as the contract with an
  // empty function would invent an id nobody asked for.
  if (parts.length === 1) return { contract: "", fn: base, scenario };
  const [contract, ...rest] = parts;
  return { contract, fn: rest.join("::"), scenario };
}

/** Build the course from the two repository sources. */
export function loadCourse(sources: {
  publicFunctions: string;
  budgets: string;
  paths: { publicFunctions: string; budgets: string };
}): Course {
  const required = parsePublicFunctions(sources.publicFunctions);
  const { perOperation, perType } = parseBudgets(sources.budgets);

  const challenges: Challenge[] = required.map((id) => {
    const { contract, fn } = splitId(id);
    const operationType = classifyOperation(fn);

    // `framework.rs` keys a `_warm` benchmark under its base op's budget, and
    // so does `gas_benchmark_report.py`. Match that: a warm twin shares the
    // budget of the operation it warms.
    const base = { contract, fn: fn.replace(/_(warm|cold)$/, "") };
    const ownKey = `${contract}::${fn}`;
    const baseKey = `${base.contract}::${base.fn}`;

    let budget: number | null = null;
    let budgetSource: Challenge["budgetSource"] = "none";
    if (perOperation[ownKey] !== undefined) {
      budget = perOperation[ownKey];
      budgetSource = "operation";
    } else if (perOperation[baseKey] !== undefined) {
      budget = perOperation[baseKey];
      budgetSource = "operation";
    } else if (perType[operationType] !== undefined) {
      budget = perType[operationType];
      budgetSource = "operation-type";
    }

    return {
      id,
      contract,
      fn,
      operationType,
      budget,
      budgetSource,
      scoreable: budget !== null,
    };
  });

  return {
    challenges,
    unscored: challenges.filter((c) => !c.scoreable).map((c) => c.id),
    sources: sources.paths,
  };
}

/** Look a challenge up by id. */
export function findChallenge(course: Course, id: string): Challenge {
  const found = course.challenges.find((c) => c.id === id);
  if (!found) {
    const available = course.challenges
      .filter((c) => c.contract === "lending")
      .map((c) => c.id)
      .sort();
    throw new Error(`unknown challenge "${id}". Lending challenges: ${available.join(", ")}`);
  }
  return found;
}

/** Load the course from disk. */
export function loadCourseFromDisk(repoRoot: string, paths?: { publicFunctions?: string; budgets?: string }): Course {
  const publicFunctions = paths?.publicFunctions ?? "stellar-lend/benchmarks/public-functions.json";
  const budgets = paths?.budgets ?? "stellar-lend/benchmarks/baseline.json";
  const read = (relative: string): string =>
    fs.readFileSync(path.isAbsolute(relative) ? relative : path.join(repoRoot, relative), "utf8");
  return loadCourse({
    publicFunctions: read(publicFunctions),
    budgets: read(budgets),
    paths: { publicFunctions, budgets },
  });
}
