#!/usr/bin/env node
/**
 * Snapshot testing CLI (#1064).
 *
 * Records and diffs golden snapshots of the protocol's contract state.
 * A snapshot is a deterministic, key-sorted capture of positions/pools/
 * governance; tests then fail when the current state diverges from the
 * recorded golden snapshot.
 *
 * Run (no install needed, Node >= 22):
 *   # Record (or update) the golden snapshot
 *   node --experimental-strip-types scripts/snapshot-test/index.ts \
 *     --state state.json --snapshot snapshots/lending.snap.json --record
 *
 *   # Compare current state against the golden snapshot (exit 1 on diff)
 *   node --experimental-strip-types scripts/snapshot-test/index.ts \
 *     --state state.json --snapshot snapshots/lending.snap.json
 *
 *   # Compare and only allow listed JSON-pointer paths to change (allowlist)
 *   node --experimental-strip-types scripts/snapshot-test/index.ts \
 *     --state state.json --snapshot snapshots/lending.snap.json \
 *     --allow 'pools/0/interestRateBps'
 *
 * `--state` accepts either a raw ContractState (scripts/state-exporter shape)
 * or a state-exporter export wrapped as { state: {...} }.
 */

import * as fs from "node:fs";
import {
  type ContractState,
  type SnapshotFile,
  type SnapshotDiff,
  buildSnapshot,
  compareSnapshots,
  hashSnapshot,
} from "./snapshot.ts";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}
function flag(name: string): boolean {
  return process.argv.includes(`--${name}`);
}

function readState(path: string): ContractState {
  const raw = JSON.parse(fs.readFileSync(path, "utf8")) as ContractState | { state: ContractState };
  return "state" in raw ? raw.state : raw;
}

function renderRows(kind: string, rows: SnapshotDiff["sections"]["positions"]["added"]): string {
  if (rows.length === 0) return "";
  const header = `  ${kind} (${rows.length}):`;
  const lines = rows.map((r) => `    - ${r.key}| ${JSON.stringify(r.before ?? r.after)}`);
  return [header, ...lines].join("\n");
}

function renderDiff(diff: SnapshotDiff): string {
  const out: string[] = [];
  for (const section of Object.keys(diff.sections) as (keyof SnapshotDiff["sections"])[]) {
    const d = diff.sections[section];
    if (d.added.length + d.removed.length + d.changed.length === 0) continue;
    out.push(`[${section}]`);
    if (d.added.length) out.push(renderRows("+ added", d.added));
    if (d.removed.length) out.push(renderRows("- removed", d.removed));
    if (d.changed.length) {
      out.push(`  ~ changed (${d.changed.length}):`);
      for (const c of d.changed) {
        out.push(`    ${c.key}`);
        out.push(`      before: ${JSON.stringify(c.before)}`);
        out.push(`      after:  ${JSON.stringify(c.after)}`);
      }
    }
  }
  if (diff.changedPaths.length) out.push(`Changed leaf paths: ${diff.changedPaths.join(", ")}`);
  return out.join("\n");
}

function main(): void {
  const statePath = arg("state");
  const snapshotPath = arg("snapshot");
  const record = flag("record");
  const allowRaw = arg("allow");
  const out = arg("out");
  const allow = allowRaw ? allowRaw.split(",").map((p) => p.trim()).filter(Boolean) : [];

  if (!statePath || !snapshotPath) {
    console.error("Usage: snapshot-test --state <state.json> --snapshot <golden.json> [--record] [--allow 'a,b'] [--out report.json]");
    process.exit(2);
  }

  const state = readState(statePath);

  if (record) {
    const snap = buildSnapshot(state);
    fs.writeFileSync(snapshotPath, JSON.stringify(snap, null, 2));
    const oldHash = fs.existsSync(snapshotPath) ? hashSnapshot(normalizeFromSnapshot(snapshotPath)) : "(new)";
    void oldHash;
    console.log(`Snapshot recorded -> ${snapshotPath} (sha256 ${hashSnapshot(snap.state).slice(0, 16)}…)`);
    process.exit(0);
  }

  if (!fs.existsSync(snapshotPath)) {
    console.error(`Snapshot not found: ${snapshotPath} — record it first with --record`);
    process.exit(2);
  }

  const golden = JSON.parse(fs.readFileSync(snapshotPath, "utf8")) as SnapshotFile;
  const diff = compareSnapshots(golden.state, state);

  const allowedSet = new Set(allow);
  const blocked = diff.changedPaths.filter((p) => !allowedSet.has(p));

  const result = {
    match: blocked.length === 0,
    snapshot: snapshotPath,
    currentSha256: hashSnapshot(state),
    goldenSha256: hashSnapshot(golden.state),
    diff,
  };

  if (out) fs.writeFileSync(out, JSON.stringify(result, null, 2));

  if (result.match) {
    console.log(`Snapshot OK: ${snapshotPath}`);
    console.log(`  sha256 ${result.currentSha256.slice(0, 16)}…`);
    if (allow.length) console.log(`  (${allow.length} allowlisted path(s) ignored)`);
    process.exit(0);
  }

  console.error(`Snapshot MISMATCH: ${snapshotPath}`);
  console.error(renderDiff(diff));
  if (blocked.length !== diff.changedPaths.length) {
    console.error(`Non-allowlisted paths: ${blocked.join(", ")}`);
  }
  process.exit(1);
}

/** Read the raw state held inside a recorded snapshot file. */
function normalizeFromSnapshot(path: string): ContractState {
  const snap = JSON.parse(fs.readFileSync(path, "utf8")) as SnapshotFile;
  return snap.state;
}

main();