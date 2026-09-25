/**
 * Snapshot testing primitives (#1064).
 *
 * A snapshot captures the protocol's structured state (positions, pools,
 * governance) at a point in time. Snapshot tests compare the current state
 * against a recorded "golden" snapshot and fail when they diverge, which
 * surfaces unexpected state-mutation regressions early.
 *
 * The comparison is built on the same state shape as `scripts/state-exporter`
 * (issue #499), so a snapshot can be recorded straight from an export.
 */

import { createHash } from "node:crypto";

/** Structured contract state (mirrors scripts/state-exporter/types.ts). */
export interface ContractState {
  positions: Array<Record<string, unknown>>;
  pools: Array<Record<string, unknown>>;
  governance: Array<Record<string, unknown>>;
}

export type StateSection = keyof ContractState;

export const STATE_SECTIONS: StateSection[] = ["positions", "pools", "governance"];

/** Stable primary key per section, used to diff rows across snapshots. */
export const SECTION_KEY: Record<StateSection, string> = {
  positions: "address",
  pools: "asset",
  governance: "proposalId",
};

export interface SnapshotFile {
  version: number;
  kind: "contract-state";
  recordedAt: string;
  state: ContractState;
}

export interface SnapshotDiffRow {
  key: string;
  before?: unknown;
  after?: unknown;
}

export interface SnapshotDiff {
  match: boolean;
  sections: Record<StateSection, { added: SnapshotDiffRow[]; removed: SnapshotDiffRow[]; changed: SnapshotDiffRow[] }>;
  /** JSON-pointer style paths that changed (e.g. "pools/0/interestRateBps"). */
  changedPaths: string[];
}

/** Deterministic JSON stringification (stable key order, no whitespace). */
function stableStringify(value: unknown): string {
  return JSON.stringify(sortObject(value));
}

function sortObject(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortObject);
  if (value && typeof value === "object") {
    return Object.keys(value as Record<string, unknown>)
      .sort()
      .reduce<Record<string, unknown>>((acc, key) => {
        acc[key] = sortObject((value as Record<string, unknown>)[key]);
        return acc;
      }, {});
  }
  return value;
}

/** Normalize raw rows: sort by the section's primary key. */
export function normalizeSection(rows: Array<Record<string, unknown>>, key: string): Array<Record<string, unknown>> {
  return [...rows]
    .map((r) => JSON.parse(stableStringify(r)) as Record<string, unknown>)
    .sort((a, b) => String(a[key]).localeCompare(String(b[key])));
}

export function normalizeState(state: ContractState): ContractState {
  const normalized = {} as ContractState;
  for (const section of STATE_SECTIONS) {
    normalized[section] = normalizeSection(state[section] ?? [], SECTION_KEY[section]);
  }
  return normalized;
}

export function hashSnapshot(state: ContractState): string {
  return createHash("sha256").update(stableStringify(normalizeState(state))).digest("hex");
}

/** Compare two snapshots; returns a structured, path-level diff. */
export function compareSnapshots(before: ContractState, after: ContractState): SnapshotDiff {
  const sections = {} as SnapshotDiff["sections"];
  const changedPaths: string[] = [];

  for (const section of STATE_SECTIONS) {
    const key = SECTION_KEY[section];
    const prev = normalizeSection(before[section] ?? [], key);
    const next = normalizeSection(after[section] ?? [], key);

    const prevByKey = new Map(prev.map((r) => [String(r[key]), r]));
    const nextByKey = new Map(next.map((r) => [String(r[key]), r]));
    const allKeys = new Set([...prevByKey.keys(), ...nextByKey.keys()]);

    const added: SnapshotDiffRow[] = [];
    const removed: SnapshotDiffRow[] = [];
    const changed: SnapshotDiffRow[] = [];

    for (const k of allKeys) {
      const beforeRow = prevByKey.get(k);
      const afterRow = nextByKey.get(k);
      if (beforeRow === undefined) {
        added.push({ key: k, after: afterRow });
      } else if (afterRow === undefined) {
        removed.push({ key: k, before: beforeRow });
      } else if (stableStringify(beforeRow) !== stableStringify(afterRow)) {
        changed.push({ key: k, before: beforeRow, after: afterRow });
        for (const p of diffPaths(beforeRow, afterRow, `${section}/${k}`)) changedPaths.push(p);
      }
    }

    sections[section] = { added, removed, changed };
  }

  changedPaths.sort();
  const match = STATE_SECTIONS.every((s) => {
    const d = sections[s];
    return d.added.length === 0 && d.removed.length === 0 && d.changed.length === 0;
  });
  return { match, sections, changedPaths };
}

/** Leaf-level path diff between two records (JSON-pointer style). */
export function diffPaths(before: unknown, after: unknown, prefix = ""): string[] {
  const paths: string[] = [];
  if (stableStringify(before) === stableStringify(after)) return paths;

  if (Array.isArray(before) && Array.isArray(after)) {
    const max = Math.max(before.length, after.length);
    for (let i = 0; i < max; i++) {
      const bp = before[i];
      const ap = after[i];
      if (bp === undefined) paths.push(`${prefix}/${i}`);
      else if (ap === undefined) paths.push(`${prefix}/${i}`);
      else paths.push(...diffPaths(bp, ap, `${prefix}/${i}`));
    }
    return paths;
  }

  if (before && typeof before === "object" && after && typeof after === "object") {
    const all = new Set([...Object.keys(before as object), ...Object.keys(after as object)]);
    for (const k of all) {
      paths.push(...diffPaths((before as Record<string, unknown>)[k], (after as Record<string, unknown>)[k], `${prefix}/${k}`));
    }
    return paths;
  }

  paths.push(prefix);
  return paths;
}

export function buildSnapshot(state: ContractState, recordedAt = new Date().toISOString()): SnapshotFile {
  return { version: 1, kind: "contract-state", recordedAt, state: normalizeState(state) };
}