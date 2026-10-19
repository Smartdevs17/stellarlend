/**
 * Storage-pattern optimization suggestions for the lending pool (#1011).
 *
 * Every rule here is derived from a *storage pattern* the indexer recovered
 * from the contract source — a cluster of keys in the same namespace, an
 * existence probe that is immediately followed by a read or a delete, writes
 * amplified by a loop, instance-tier writes on a hot path, legacy fallback
 * reads, and so on. Nothing is derived from timing, batching or gas-price
 * heuristics, which is what separates this from the existing
 * `api/src/services/gas/estimator.ts` suggestions.
 *
 * Each rule is pure: `OperationFacts` in, `Suggestion[]` out.
 */

import type {
  CostModel,
  OperationEstimate,
  Severity,
  StorageAccess,
  Suggestion,
} from "./types.ts";
import { SEVERITY_ORDER } from "./types.ts";

export interface SuggestionOptions {
  /** The cost model the rules price their savings with. */
  model: CostModel;
  iterations: number;
  /** Only emit suggestions at or above this severity. */
  minSeverity?: Severity;
  /** Restrict the rules to these operation names. */
  operations?: string[];
  /** Disable rules by id. */
  disabled?: string[];
}

function shortFile(file: string): string {
  const parts = file.split("/");
  return parts.slice(-2).join("/");
}

function describe(access: StorageAccess): string {
  const loc = `${shortFile(access.file)}:${access.line}`;
  const via = access.via ? ` via \`${access.via}\`` : "";
  return `\`${access.tier}.${access.kind}\` on \`${access.key}\`${via} (${loc})`;
}

/** Distinct ledger-entry identifiers touched by an operation, cited by source. */
function footprintEvidence(estimate: OperationEstimate): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const access of estimate.facts.accesses) {
    const key = `${access.tier}::${access.key}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(describe(access));
  }
  return out;
}

/**
 * Keys belonging to the same `#[contracttype] enum` namespace, grouped.
 * Packing candidates come from groups of two or more distinct variants.
 */
function namespaceGroups(accesses: StorageAccess[]): Map<string, StorageAccess[]> {
  const groups = new Map<string, StorageAccess[]>();
  for (const access of accesses) {
    if (!access.keyEnum) continue;
    const list = groups.get(access.keyEnum) ?? [];
    list.push(access);
    groups.set(access.keyEnum, list);
  }
  return groups;
}

// ── Rules ───────────────────────────────────────────────────────────────────

/**
 * Two or more entries of the same key enum are written in a single call, so the
 * pool pays rent and a footprint slot per entry. Packing them into one
 * `#[contracttype]` struct halves that.
 */
function rulePackableNamespace(estimate: OperationEstimate, options: SuggestionOptions): Suggestion[] {
  const { model } = options;
  const groups = namespaceGroups(estimate.facts.accesses);
  const out: Suggestion[] = [];

  for (const [keyEnum, accesses] of groups) {
    const mutating = accesses.filter((a) => a.kind === "write" || a.kind === "remove");
    const distinct = new Set(mutating.map((a) => `${a.tier}::${a.key}`));
    if (distinct.size < 2) continue;

    // A cluster that is only ever read together is already packed, or is a
    // read-only config set — only flag it once it is on a write path.
    const reducible = distinct.size - 1;
    out.push({
      id: "packable-key-namespace",
      category: "packing",
      severity: distinct.size >= 4 ? "high" : "medium",
      operation: estimate.operation,
      title: `Pack ${distinct.size} \`${keyEnum}\` entries into one ledger entry`,
      detail:
        `A single \`${estimate.operation}\` call mutates ${distinct.size} entries of the ` +
        `\`${keyEnum}\` namespace. Each persistent entry is a separate ledger key with its own ` +
        `write fee and footprint slot, so packing them into one \`#[contracttype]\` struct ` +
        `removes ${reducible} write(s) and ${reducible} footprint slot(s) per call.`,
      evidence: mutating.map((a) => describe(a)),
      estimatedSavingStroops: reducible * model.storageWriteStroops,
    });
  }

  return out;
}

/**
 * The same key is read more than once in one call. The second read is a
 * redundant footprint hit; caching it in a local (or a loaded slot) is free.
 */
function ruleRepeatedKeyReads(estimate: OperationEstimate, options: SuggestionOptions): Suggestion[] {
  const { model } = options;
  const counts = new Map<string, StorageAccess[]>();
  for (const access of estimate.facts.accesses) {
    if (access.kind !== "read") continue;
    const key = `${access.tier}::${access.key}`;
    const list = counts.get(key) ?? [];
    list.push(access);
    counts.set(key, list);
  }

  const out: Suggestion[] = [];
  for (const [key, accesses] of counts) {
    if (accesses.length < 2) continue;
    const redundant = accesses.length - 1;
    out.push({
      id: "repeated-key-read",
      category: "redundant-access",
      severity: redundant >= 2 ? "medium" : "low",
      operation: estimate.operation,
      title: `Read \`${key}\` ${accesses.length}× in one \`${estimate.operation}\` call`,
      detail:
        `The same ledger entry is fetched ${accesses.length} times within a single invocation. ` +
        `Load it once into a local (or a packed slot) and reuse the value, saving ` +
        `${redundant} redundant read(s).`,
      evidence: accesses.map((a) => describe(a)),
      estimatedSavingStroops: redundant * model.storageReadStroops,
    });
  }
  return out;
}

/**
 * `has()` immediately before a `get()`/`remove()` on the same key: the probe is
 * redundant because the read already returns `Option`, and the delete is
 * idempotent.
 */
function ruleRedundantExistenceProbe(
  estimate: OperationEstimate,
  options: SuggestionOptions,
): Suggestion[] {
  const { model } = options;
  const probes = estimate.facts.accesses.filter((a) => a.kind === "exists");
  if (probes.length === 0) return [];

  const byKey = new Map<string, StorageAccess[]>();
  for (const access of estimate.facts.accesses) {
    if (access.kind === "exists") continue;
    const key = `${access.tier}::${access.key}`;
    const list = byKey.get(key) ?? [];
    list.push(access);
    byKey.set(key, list);
  }

  const redundant: StorageAccess[] = [];
  for (const probe of probes) {
    const siblings = byKey.get(`${probe.tier}::${probe.key}`) ?? [];
    if (siblings.length === 0) continue;
    // Only flag it when the sibling access is in the same function body, i.e.
    // close enough on the source that the probe is clearly a guard, not a
    // separate branch.
    const near = siblings.some((s) => s.via === probe.via && Math.abs(s.line - probe.line) <= 12);
    if (near) redundant.push(probe);
  }
  if (redundant.length === 0) return [];

  return [
    {
      id: "redundant-existence-probe",
      category: "redundant-access",
      severity: "low",
      operation: estimate.operation,
      title: `Drop ${redundant.length} existence probe(s) guarding a read or delete`,
      detail:
        `\`has()\` is used as a guard immediately before touching the same entry. The ` +
        `subsequent \`get()\` already returns an \`Option\` and \`remove()\` is idempotent, so ` +
        `the probe only widens the footprint.`,
      evidence: redundant.map((a) => describe(a)),
      estimatedSavingStroops: redundant.length * model.storageExistsStroops,
    },
  ];
}

/**
 * Storage writes inside a loop: each iteration rents a new ledger entry, so the
 * footprint (and the cost) grows linearly with the batch size. Batching helpers
 * that share the invariant writes amortise this.
 */
function ruleLoopAmplifiedWrites(estimate: OperationEstimate, options: SuggestionOptions): Suggestion[] {
  const { model, iterations } = options;
  const looped = estimate.facts.accesses.filter((a) => a.perIteration && (a.kind === "write" || a.kind === "read"));
  if (looped.length === 0) return [];

  const writes = looped.filter((a) => a.kind === "write");
  const reads = looped.filter((a) => a.kind === "read");
  const sharedWrites = estimate.facts.accesses.filter((a) => a.kind === "write" && !a.perIteration);
  const savingPerIteration =
    writes.length * model.storageWriteStroops + reads.length * model.storageReadStroops;
  const avoidable = Math.max(0, writes.length - sharedWrites.length);

  return [
    {
      id: "loop-amplified-storage",
      category: "loop-amplification",
      severity: writes.length >= 3 ? "high" : "medium",
      operation: estimate.operation,
      title: `Storage access inside a loop grows the footprint ${iterations}×`,
      detail:
        `\`${estimate.operation}\` performs ${writes.length} write(s) and ${reads.length} read(s) ` +
        `inside a loop. At ${iterations} iteration(s) that is ${writes.length * iterations} write(s) ` +
        `and ${reads.length * iterations} read(s). Move the invariant writes outside the loop (as ` +
        `\`deposit_batch\` already does for the shared pool state) and batch the per-item writes ` +
        `into a single packed entry per item where the schema allows it; ` +
        `up to ${avoidable} write(s) per iteration are avoidable.`,
      evidence: looped.map((a) => `${describe(a)}${a.perIteration ? " [per iteration]" : ""}`),
      estimatedSavingStroops: avoidable * model.storageWriteStroops * Math.max(1, iterations - 1),
    },
  ];
}

/**
 * Multiple instance-tier writes per call. Instance entries are cheap in rent
 * but still cost a write fee and a footprint slot each, so a reentrancy guard
 * that flips several independent flags can use one packed entry.
 */
function ruleInstanceGuardWrites(estimate: OperationEstimate, options: SuggestionOptions): Suggestion[] {
  const { model } = options;
  const instanceWrites = estimate.facts.accesses.filter((a) => a.tier === "instance" && a.kind === "write");
  if (instanceWrites.length < 2) return [];

  const distinct = new Set(instanceWrites.map((a) => a.key));
  const reducible = distinct.size - 1;
  return [
    {
      id: "instance-guard-writes",
      category: "instance-guard",
      severity: instanceWrites.length >= 4 ? "medium" : "low",
      operation: estimate.operation,
      title: `Collapse ${instanceWrites.length} instance-tier writes into one guard entry`,
      detail:
        `\`${estimate.operation}\` writes ${instanceWrites.length} separate instance entries ` +
        `(${distinct.size} distinct keys) to arm and disarm its guard. Packing the flags into a ` +
        `single \`#[contracttype]\` struct removes ${reducible} write(s) per call while keeping ` +
        `the reentrancy semantics intact.`,
      evidence: instanceWrites.map((a) => describe(a)),
      estimatedSavingStroops: reducible * model.storageWriteStroops,
    },
  ];
}

/**
 * Legacy per-field keys are read as a fallback and then removed on commit: a
 * one-time migration penalty paid inside the hot path. An eager migration
 * entry point moves the cost out of the user-facing operation.
 */
function ruleLegacyMigrationPath(estimate: OperationEstimate): Suggestion[] {
  const removals = estimate.facts.accesses.filter((a) => a.kind === "remove");
  if (removals.length < 2) return [];

  const readKeys = new Set(estimate.facts.accesses.filter((a) => a.kind === "read").map((a) => a.key));
  const migration = removals.filter((a) => readKeys.has(a.key));
  if (migration.length < 2) return [];

  const groups = namespaceGroups(migration);
  const namespaces = [...groups.keys()];

  return [
    {
      id: "legacy-migration-in-hot-path",
      category: "migration",
      severity: "medium",
      operation: estimate.operation,
      title: `Move the ${migration.length}-key legacy migration out of \`${estimate.operation}\``,
      detail:
        `\`${estimate.operation}\` reads legacy per-field entries as a fallback and deletes them ` +
        `on commit, so the first call after an upgrade pays ${migration.length} extra reads and ` +
        `${migration.length} extra deletes (namespaces: ${namespaces.join(", ")}). The tree already ` +
        `ships eager \`migrate_deposit_state\` / \`migrate_borrow_limits\` helpers; calling the ` +
        `relevant one from an admin transaction makes the hot path cost constant.`,
      evidence: migration.map((a) => describe(a)),
      estimatedSavingStroops: migration.length * 1_000,
    },
  ];
}

/**
 * The transaction footprint is the dominant cost driver on Soroban: every
 * distinct entry the transaction touches must be supplied to the ledger.
 */
function ruleFootprintWidth(estimate: OperationEstimate, options: SuggestionOptions): Suggestion[] {
  const { model } = options;
  const footprint = estimate.storage.footprintEntries;
  if (footprint < 6) return [];

  const shares: string[] = [];
  if (estimate.storage.writes >= 2) shares.push(`${estimate.storage.writes} writes`);
  if (estimate.storage.reads >= 2) shares.push(`${estimate.storage.reads} reads`);
  if (estimate.storage.exists > 0) shares.push(`${estimate.storage.exists} existence probes`);
  if (estimate.storage.removes > 0) shares.push(`${estimate.storage.removes} deletes`);

  return [
    {
      id: "wide-footprint",
      category: "footprint",
      severity: footprint >= 10 ? "high" : "medium",
      operation: estimate.operation,
      title: `\`${estimate.operation}\` touches ${footprint} ledger entries`,
      detail:
        `A single \`${estimate.operation}\` call spans a ${footprint}-entry footprint ` +
        `(${shares.join(", ")}). On Soroban the footprint drives both the resource fee and the ` +
        `TTL/rent burden, so shrinking it — by packing related keys, caching invariant reads in ` +
        `a loaded slot, or splitting rarely-needed reads behind a view call — is usually worth ` +
        `more than shaving CPU instructions.`,
      evidence: footprintEvidence(estimate),
      estimatedSavingStroops: estimate.storage.writes * model.storageWriteStroops,
    },
  ];
}

/**
 * Several outbound calls per invocation, each priced separately and each pulling
 * the callee's own footprint into the transaction.
 */
function ruleCrossContractCalls(estimate: OperationEstimate, options: SuggestionOptions): Suggestion[] {
  const { model } = options;
  const calls = estimate.facts.crossContractCalls;
  if (calls.length < 2) return [];

  return [
    {
      id: "cross-contract-fanout",
      category: "cross-contract",
      severity: calls.length >= 3 ? "medium" : "low",
      operation: estimate.operation,
      title: `\`${estimate.operation}\` makes ${calls.length} outbound contract calls`,
      detail:
        `Each outbound invocation costs a cross-contract fee and widens the transaction ` +
        `footprint with the callee's entries. Cache the value for the duration of the ` +
        `transaction (for example, read the oracle price once and reuse it) and expose the ` +
        `rarely-needed lookup as a separate view entry point.`,
      evidence: calls.map(
        (c) => `\`${c.kind}\` (${shortFile(c.file)}:${c.line}${c.via ? ` via \`${c.via}\`` : ""})`,
      ),
      estimatedSavingStroops: (calls.length - 1) * model.crossContractStroops,
    },
  ];
}

/**
 * Unresolved calls mean the static index under-counts this operation. Surfacing
 * that keeps the estimate honest instead of silently reporting a low number.
 */
function ruleUnresolvedCalls(estimate: OperationEstimate): Suggestion[] {
  const unresolved = estimate.facts.unresolvedCalls;
  if (unresolved.length === 0) return [];

  return [
    {
      id: "unresolved-calls",
      category: "footprint",
      severity: "info",
      operation: estimate.operation,
      title: `${unresolved.length} call(s) in \`${estimate.operation}\` were not statically resolved`,
      detail:
        `The indexer could not map these calls to a definition in the scanned tree, so the ` +
        `storage cost of \`${estimate.operation}\` is a lower bound. Most are SDK or external-crate ` +
        `helpers (token transfers, the reentrancy guard) that live in another crate.`,
      evidence: unresolved.map((c) => `\`${c}()\``),
      estimatedSavingStroops: null,
    },
  ];
}

const RULES = [
  ruleRedundantExistenceProbe,
  ruleRepeatedKeyReads,
  rulePackableNamespace,
  ruleLoopAmplifiedWrites,
  ruleInstanceGuardWrites,
  ruleLegacyMigrationPath,
  ruleFootprintWidth,
  ruleCrossContractCalls,
  ruleUnresolvedCalls,
];

/** Run every rule over every priced operation and return ranked suggestions. */
export function buildSuggestions(
  operations: OperationEstimate[],
  options: SuggestionOptions,
): Suggestion[] {
  const resolved = options;
  const disabled = new Set(resolved.disabled ?? []);
  const wanted = resolved.operations ? new Set(resolved.operations) : null;
  const floor = resolved.minSeverity ? SEVERITY_ORDER[resolved.minSeverity] : 0;

  const out: Suggestion[] = [];
  for (const estimate of operations) {
    if (wanted && !wanted.has(estimate.operation)) continue;
    for (const rule of RULES) {
      for (const suggestion of rule(estimate, resolved)) {
        if (disabled.has(suggestion.id)) continue;
        if (SEVERITY_ORDER[suggestion.severity] < floor) continue;
        out.push(suggestion);
      }
    }
  }

  return out.sort((a, b) => {
    const bySeverity = SEVERITY_ORDER[b.severity] - SEVERITY_ORDER[a.severity];
    if (bySeverity !== 0) return bySeverity;
    const bySaving = (b.estimatedSavingStroops ?? -1) - (a.estimatedSavingStroops ?? -1);
    if (bySaving !== 0) return bySaving;
    return a.operation.localeCompare(b.operation) || a.id.localeCompare(b.id);
  });
}

/** The rule ids this module can emit, for `--help` and test assertions. */
export const RULE_IDS: string[] = [
  "redundant-existence-probe",
  "repeated-key-read",
  "packable-key-namespace",
  "loop-amplified-storage",
  "instance-guard-writes",
  "legacy-migration-in-hot-path",
  "wide-footprint",
  "cross-contract-fanout",
  "unresolved-calls",
];
