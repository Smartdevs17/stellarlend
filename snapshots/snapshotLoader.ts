/**
 * Structured loader for StellarLend lending snapshots.
 *
 * Responsibilities
 * ────────────────
 * 1. Read `lending.snap.json` from disk and validate it.
 * 2. Migrate older snapshot versions to the current schema.
 * 3. Write updates: append a diff entry to `lending.snap.diff.jsonl`, and
 *    rewrite `lending.snap.json` only when necessary (compaction threshold).
 * 4. Emit structured observability events (logs + inline metrics) on every
 *    significant operation so operators can monitor snapshot health without
 *    having to parse the files themselves.
 *
 * Hot-path optimisation
 * ─────────────────────
 * Full-file rewrites are expensive when positions > a few thousand.  The
 * loader keeps the last loaded snapshot in memory and skips the disk read on
 * subsequent calls to load() unless the file has been modified (mtime check).
 * On write, it computes a diff and only flushes the full file when the diff
 * log entry count crosses COMPACTION_THRESHOLD.
 *
 * File layout
 * ───────────
 * snapshots/
 *   lending.snap.json          ← full snapshot (always current)
 *   lending.snap.diff.jsonl    ← append-only diff log (one JSON per line)
 */

import fs from 'fs';
import path from 'path';

import type {
  LendingSnapshot,
  LendingSnapshotV1,
  LendingSnapshotV2,
  CurrentLendingSnapshot,
  CurrentSnapshotState,
} from './schema';
import { CURRENT_SNAPSHOT_VERSION } from './schema';
import { validateSnapshot } from './validate';
import { canonicalizeSnapshot } from './format';
import {
  computeDiff,
  applyDiff,
  serialiseDiffEntry,
  parseDiffEntry,
  type DiffLogEntry,
  type StateDiff,
} from './snapshotDiff';

// ── Configuration ────────────────────────────────────────────────────────────

/** Compact (rewrite the full file) after this many diff entries accumulate. */
const COMPACTION_THRESHOLD = 50;

const SNAP_DIR = __dirname;
const SNAP_FILE = path.join(SNAP_DIR, 'lending.snap.json');
const DIFF_FILE = path.join(SNAP_DIR, 'lending.snap.diff.jsonl');

// ── Observability ────────────────────────────────────────────────────────────

export interface SnapshotMetrics {
  loads: number;
  cacheHits: number;
  writes: number;
  compactions: number;
  migrations: number;
  validationErrors: number;
  diffEntriesWritten: number;
  lastLoadMs: number;
  lastWriteMs: number;
  lastCompactionMs: number;
}

const metrics: SnapshotMetrics = {
  loads: 0,
  cacheHits: 0,
  writes: 0,
  compactions: 0,
  migrations: 0,
  validationErrors: 0,
  diffEntriesWritten: 0,
  lastLoadMs: 0,
  lastWriteMs: 0,
  lastCompactionMs: 0,
};

export function getSnapshotMetrics(): Readonly<SnapshotMetrics> {
  return { ...metrics };
}

/** Reset metrics — only used in tests. */
export function resetSnapshotMetrics(): void {
  Object.assign(metrics, {
    loads: 0,
    cacheHits: 0,
    writes: 0,
    compactions: 0,
    migrations: 0,
    validationErrors: 0,
    diffEntriesWritten: 0,
    lastLoadMs: 0,
    lastWriteMs: 0,
    lastCompactionMs: 0,
  });
}

// ── Simple console logger ─────────────────────────────────────────────────────
// Kept minimal — the loader is used by scripts outside the API process which
// may not have the full Winston logger available.

type LogLevel = 'info' | 'warn' | 'error' | 'debug';

function log(level: LogLevel, message: string, meta?: Record<string, unknown>): void {
  const ts = new Date().toISOString();
  const metaStr = meta ? ` ${JSON.stringify(meta)}` : '';
  const line = `[${ts}] [snapshot:${level.toUpperCase()}] ${message}${metaStr}`;
  if (level === 'error') {
    console.error(line);
  } else if (level === 'warn') {
    console.warn(line);
  } else {
    console.log(line);
  }
}

// ── Migration ─────────────────────────────────────────────────────────────────

function migrateV1toV2(snap: LendingSnapshotV1): LendingSnapshotV2 {
  metrics.migrations++;
  log('info', 'Migrating snapshot v1 → v2');
  return {
    version: 2,
    kind: 'contract-state',
    recordedAt: snap.recordedAt,
    seq: 0,
    state: {
      positions: snap.state.positions.map((p) => ({ ...p })),
      pools: snap.state.pools.map((p) => ({ ...p })),
      governance: snap.state.governance.map((g) => ({ ...g })),
    },
  };
}

function migrate(snap: LendingSnapshot): CurrentLendingSnapshot {
  if (snap.version === 1) {
    return migrateV1toV2(snap);
  }
  // version === CURRENT_SNAPSHOT_VERSION (2) — no-op
  return snap as LendingSnapshotV2;
}

// ── Mtime-based load cache ────────────────────────────────────────────────────

interface LoadCache {
  snapshot: CurrentLendingSnapshot;
  mtimeMs: number;
}

let loadCache: LoadCache | null = null;

function currentMtime(filePath: string): number {
  try {
    return fs.statSync(filePath).mtimeMs;
  } catch {
    return 0;
  }
}

// ── Diff log helpers ──────────────────────────────────────────────────────────

function countDiffEntries(filePath: string): number {
  if (!fs.existsSync(filePath)) return 0;
  const content = fs.readFileSync(filePath, 'utf8');
  return content.split('\n').filter((l: string) => l.trim().length > 0).length;
}

// ── Public API ────────────────────────────────────────────────────────────────

export interface LoadResult {
  snapshot: CurrentLendingSnapshot;
  fromCache: boolean;
  migrated: boolean;
  durationMs: number;
}

/**
 * Load and validate the lending snapshot from disk.
 *
 * Returns the in-memory cached copy when the file has not changed since the
 * last call (mtime unchanged), saving a disk read and JSON parse on the hot
 * path.
 *
 * Throws if the file is missing, unreadable, or fails validation.
 */
export function loadSnapshot(filePath = SNAP_FILE): LoadResult {
  const t0 = Date.now();
  metrics.loads++;

  const mtime = currentMtime(filePath);

  if (loadCache && loadCache.mtimeMs === mtime && mtime > 0) {
    metrics.cacheHits++;
    const durationMs = Date.now() - t0;
    metrics.lastLoadMs = durationMs;
    log('debug', 'Snapshot loaded from cache', { file: filePath, durationMs });
    return { snapshot: loadCache.snapshot, fromCache: true, migrated: false, durationMs };
  }

  if (!fs.existsSync(filePath)) {
    throw new Error(`Snapshot file not found: ${filePath}`);
  }

  const raw = JSON.parse(fs.readFileSync(filePath, 'utf8')) as unknown;
  const result = validateSnapshot(raw);

  if (!result.ok) {
    metrics.validationErrors++;
    const errSummary = result.errors.slice(0, 5).join('; ');
    log('error', 'Snapshot validation failed', {
      file: filePath,
      errorCount: result.errors.length,
      errors: errSummary,
    });
    throw new Error(`Snapshot validation failed: ${errSummary}`);
  }

  const needsMigration = result.snapshot.version !== CURRENT_SNAPSHOT_VERSION;
  const current = canonicalizeSnapshot(migrate(result.snapshot));

  loadCache = { snapshot: current, mtimeMs: mtime };

  const durationMs = Date.now() - t0;
  metrics.lastLoadMs = durationMs;

  log('info', 'Snapshot loaded', {
    file: filePath,
    version: current.version,
    seq: current.seq,
    positions: current.state.positions.length,
    pools: current.state.pools.length,
    governance: current.state.governance.length,
    migrated: needsMigration,
    durationMs,
  });

  return { snapshot: current, fromCache: false, migrated: needsMigration, durationMs };
}

export interface WriteResult {
  written: boolean;
  compacted: boolean;
  diffEntryCount: number;
  durationMs: number;
  diff: StateDiff;
}

/**
 * Write an updated state to the snapshot store.
 *
 * Steps:
 *  1. Load (or use in-memory cache of) the previous full snapshot.
 *  2. Compute the diff between previous and next state.
 *  3. If the diff is empty, do nothing and return early.
 *  4. Append the diff entry to `lending.snap.diff.jsonl`.
 *  5. Build and write the new full snapshot to `lending.snap.json`.
 *  6. If the diff log now exceeds COMPACTION_THRESHOLD entries, compact:
 *     truncate the diff log (the full snapshot IS the compacted state).
 *
 * @param nextState  The new canonical state to persist.
 * @param snapFile   Override for the snapshot file path (tests only).
 * @param diffFile   Override for the diff log file path (tests only).
 */
export function writeSnapshot(
  nextState: CurrentSnapshotState,
  snapFile = SNAP_FILE,
  diffFile = DIFF_FILE,
): WriteResult {
  const t0 = Date.now();

  // Load previous state — use cache when available, avoid extra disk I/O.
  let prev: CurrentLendingSnapshot;
  try {
    prev = loadSnapshot(snapFile).snapshot;
  } catch {
    // No previous snapshot exists yet — bootstrap with an empty baseline.
    prev = {
      version: CURRENT_SNAPSHOT_VERSION,
      kind: 'contract-state',
      recordedAt: new Date(0).toISOString(),
      seq: 0,
      state: { positions: [], pools: [], governance: [] },
    };
  }

  const diff = computeDiff(prev.state, nextState);

  if (diff.isEmpty) {
    const durationMs = Date.now() - t0;
    log('debug', 'Snapshot write skipped — no changes', { seq: prev.seq, durationMs });
    return {
      written: false,
      compacted: false,
      diffEntryCount: countDiffEntries(diffFile),
      durationMs,
      diff,
    };
  }

  const nextSeq = prev.seq + 1;
  const recordedAt = new Date().toISOString();

  // Append diff entry
  const diffEntry: DiffLogEntry = { seq: nextSeq, recordedAt, diff };
  fs.appendFileSync(diffFile, serialiseDiffEntry(diffEntry) + '\n', 'utf8');
  metrics.diffEntriesWritten++;

  // Write updated full snapshot
  const nextSnap: CurrentLendingSnapshot = {
    version: CURRENT_SNAPSHOT_VERSION,
    kind: 'contract-state',
    recordedAt,
    seq: nextSeq,
    state: nextState,
  };

  const canonicalSnap = canonicalizeSnapshot(nextSnap);
  const snapJson = JSON.stringify(canonicalSnap, null, 2) + '\n';
  fs.writeFileSync(snapFile, snapJson, 'utf8');
  metrics.writes++;

  // Invalidate cache so the next load() re-reads from disk
  loadCache = null;

  const diffEntryCount = countDiffEntries(diffFile);
  let compacted = false;

  if (diffEntryCount >= COMPACTION_THRESHOLD) {
    const tc = Date.now();
    // Compaction: the current full snapshot already contains all diffs,
    // so we can safely truncate the diff log.
    fs.writeFileSync(diffFile, '', 'utf8');
    metrics.compactions++;
    metrics.lastCompactionMs = Date.now() - tc;
    compacted = true;
    log('info', 'Snapshot diff log compacted', {
      seq: nextSeq,
      entriesBefore: diffEntryCount,
      compactionMs: metrics.lastCompactionMs,
    });
  }

  const durationMs = Date.now() - t0;
  metrics.lastWriteMs = durationMs;

  log('info', 'Snapshot written', {
    file: snapFile,
    seq: nextSeq,
    positionsChanged:
      diff.positions.added.length +
      diff.positions.modified.length +
      diff.positions.removed.length,
    poolsChanged:
      diff.pools.added.length + diff.pools.modified.length + diff.pools.removed.length,
    governanceChanged:
      diff.governance.added.length +
      diff.governance.modified.length +
      diff.governance.removed.length,
    diffEntryCount,
    compacted,
    durationMs,
  });

  return { written: true, compacted, diffEntryCount, durationMs, diff };
}

/**
 * Read and parse all entries from the diff log.
 *
 * Lines that fail to parse are skipped and counted.  This is intentional:
 * a corrupt line must not prevent reading the rest of the log.
 */
export function loadDiffLog(
  diffFile = DIFF_FILE,
): { entries: DiffLogEntry[]; skippedLines: number } {
  if (!fs.existsSync(diffFile)) {
    return { entries: [], skippedLines: 0 };
  }

  const lines = fs.readFileSync(diffFile, 'utf8').split('\n');
  const entries: DiffLogEntry[] = [];
  let skippedLines = 0;

  for (const line of lines) {
    if (!line.trim()) continue;
    const entry = parseDiffEntry(line);
    if (entry) {
      entries.push(entry);
    } else {
      skippedLines++;
      log('warn', 'Skipping unparseable diff log line', { line: line.slice(0, 80) });
    }
  }

  return { entries, skippedLines };
}

/**
 * Invalidate the in-memory load cache.  Useful in tests and after
 * out-of-process writes to the snapshot file.
 */
export function invalidateCache(): void {
  loadCache = null;
}

// Re-export diff utilities so callers can import everything from one place.
export { computeDiff, applyDiff, type StateDiff, type DiffLogEntry };
