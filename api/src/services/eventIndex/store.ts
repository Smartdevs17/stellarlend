/**
 * In-memory hot event store with secondary indexes.
 *
 * Events are kept in ledger order (`ledger`, then RPC `id`) in a primary list
 * plus per-type and per-account lists that share the same ordering. A query
 * picks the most selective list for its filters, binary-searches the ledger /
 * time bounds and cursor position, and only scans the matching window — so
 * lookups stay in the low milliseconds at hundreds of thousands of events
 * (see `eventIndex.performance.test.ts`). Old events are moved to cold
 * storage by the archiver, which bounds the hot set.
 */

import type { EventPage, EventQuery, IndexedEvent } from './types';

export const DEFAULT_QUERY_LIMIT = 50;
export const MAX_QUERY_LIMIT = 1000;
const LATENCY_WINDOW = 500;

function compareEvents(a: IndexedEvent, b: IndexedEvent): number {
  if (a.ledger !== b.ledger) return a.ledger - b.ledger;
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

/** First index in `list` whose element is not before `probe` (lower bound). */
function lowerBound(list: IndexedEvent[], before: (e: IndexedEvent) => boolean): number {
  let lo = 0;
  let hi = list.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (before(list[mid] as IndexedEvent)) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

function insertSorted(list: IndexedEvent[], event: IndexedEvent): void {
  const last = list[list.length - 1];
  if (!last || compareEvents(last, event) < 0) {
    list.push(event); // common case: ingestion is in ledger order
    return;
  }
  const at = lowerBound(list, (e) => compareEvents(e, event) < 0);
  list.splice(at, 0, event);
}

export function encodeCursor(event: IndexedEvent): string {
  return Buffer.from(`${event.ledger}:${event.id}`).toString('base64url');
}

export function decodeCursor(cursor: string): { ledger: number; id: string } | null {
  const raw = Buffer.from(cursor, 'base64url').toString('utf8');
  const sep = raw.indexOf(':');
  if (sep <= 0) return null;
  const ledger = Number(raw.slice(0, sep));
  if (!Number.isInteger(ledger)) return null;
  return { ledger, id: raw.slice(sep + 1) };
}

/** Does `e` satisfy the non-indexed parts of `q`? */
export function matchesQuery(e: IndexedEvent, q: EventQuery): boolean {
  if (q.type && e.type !== q.type) return false;
  if (q.account && !e.accounts.includes(q.account)) return false;
  if (q.contract && e.contract !== q.contract) return false;
  if (q.module && e.module !== q.module) return false;
  if (q.action && e.action !== q.action) return false;
  if (q.from !== undefined && e.timestamp < q.from) return false;
  if (q.to !== undefined && e.timestamp > q.to) return false;
  if (q.fromLedger !== undefined && e.ledger < q.fromLedger) return false;
  if (q.toLedger !== undefined && e.ledger > q.toLedger) return false;
  return true;
}

export class EventStore {
  private all: IndexedEvent[] = [];
  private byId = new Map<string, IndexedEvent>();
  private byType = new Map<string, IndexedEvent[]>();
  private byAccount = new Map<string, IndexedEvent[]>();
  private latencies: number[] = [];

  get size(): number {
    return this.all.length;
  }

  /** Add events, skipping ids already indexed. Returns the number added. */
  add(events: IndexedEvent[]): number {
    let added = 0;
    for (const e of events) {
      if (this.byId.has(e.id)) continue;
      this.byId.set(e.id, e);
      insertSorted(this.all, e);
      this.indexInto(this.byType, e.type, e);
      for (const account of e.accounts) this.indexInto(this.byAccount, account, e);
      added += 1;
    }
    return added;
  }

  get(id: string): IndexedEvent | undefined {
    return this.byId.get(id);
  }

  /** Remove and return every event matching `predicate` (used by archival). */
  evict(predicate: (e: IndexedEvent) => boolean): IndexedEvent[] {
    const removed: IndexedEvent[] = [];
    const kept: IndexedEvent[] = [];
    for (const e of this.all) (predicate(e) ? removed : kept).push(e);
    if (removed.length === 0) return removed;
    this.all = [];
    this.byId.clear();
    this.byType.clear();
    this.byAccount.clear();
    this.add(kept);
    return removed;
  }

  /** Ordered snapshot of all hot events (oldest first). */
  snapshot(): readonly IndexedEvent[] {
    return this.all;
  }

  types(): string[] {
    return [...this.byType.keys()].sort();
  }

  latestLedger(): number {
    return this.all[this.all.length - 1]?.ledger ?? 0;
  }

  query(q: EventQuery): EventPage {
    const started = performance.now();
    const limit = Math.min(Math.max(1, q.limit ?? DEFAULT_QUERY_LIMIT), MAX_QUERY_LIMIT);
    const order = q.order ?? 'desc';

    // Most selective index for the equality filters.
    let list = this.all;
    if (q.type) list = this.byType.get(q.type) ?? [];
    if (q.account) {
      const byAccount = this.byAccount.get(q.account) ?? [];
      if (byAccount.length < list.length) list = byAccount;
    }

    // Ledger and time bounds (timestamps are non-decreasing with ledger).
    let start = 0;
    let end = list.length;
    if (q.fromLedger !== undefined) {
      const bound = q.fromLedger;
      start = Math.max(
        start,
        lowerBound(list, (e) => e.ledger < bound)
      );
    }
    if (q.toLedger !== undefined) {
      const bound = q.toLedger;
      end = Math.min(
        end,
        lowerBound(list, (e) => e.ledger <= bound)
      );
    }
    if (q.from !== undefined) {
      const bound = q.from;
      start = Math.max(
        start,
        lowerBound(list, (e) => e.timestamp < bound)
      );
    }
    if (q.to !== undefined) {
      const bound = q.to;
      end = Math.min(
        end,
        lowerBound(list, (e) => e.timestamp <= bound)
      );
    }

    // Resume after the cursor position.
    if (q.cursor) {
      const c = decodeCursor(q.cursor);
      if (c) {
        const probe = { ledger: c.ledger, id: c.id } as IndexedEvent;
        const at = lowerBound(list, (e) => compareEvents(e, probe) <= 0);
        if (order === 'asc') start = Math.max(start, at);
        else
          end = Math.min(
            end,
            lowerBound(list, (e) => compareEvents(e, probe) < 0)
          );
      }
    }

    const residual =
      (q.type && list !== this.byType.get(q.type)) ||
      (q.account && list !== this.byAccount.get(q.account)) ||
      q.contract ||
      q.module ||
      q.action;

    const events: IndexedEvent[] = [];
    let matched = 0;
    let hasMore = false;
    if (start < end) {
      const step = order === 'asc' ? 1 : -1;
      for (let i = order === 'asc' ? start : end - 1; i >= start && i < end; i += step) {
        const e = list[i] as IndexedEvent;
        if (residual && !matchesQuery(e, q)) continue;
        if (events.length < limit) events.push(e);
        else {
          hasMore = true;
          if (!residual) break;
        }
        matched += 1;
      }
      if (!residual) matched = end - start;
    }

    const tookMs = performance.now() - started;
    this.recordLatency(tookMs);
    const lastEvent = events[events.length - 1];
    return {
      events,
      nextCursor: hasMore && lastEvent ? encodeCursor(lastEvent) : null,
      matched,
      tookMs,
      source: 'hot',
    };
  }

  /** p50 / p95 / max of recent query latencies, in ms. */
  latencyStats(): { samples: number; p50Ms: number; p95Ms: number; maxMs: number } {
    const sorted = [...this.latencies].sort((a, b) => a - b);
    const pick = (p: number) =>
      sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))] ?? 0;
    return {
      samples: sorted.length,
      p50Ms: pick(0.5),
      p95Ms: pick(0.95),
      maxMs: sorted[sorted.length - 1] ?? 0,
    };
  }

  private recordLatency(ms: number): void {
    this.latencies.push(ms);
    if (this.latencies.length > LATENCY_WINDOW) this.latencies.shift();
  }

  private indexInto(index: Map<string, IndexedEvent[]>, key: string, e: IndexedEvent): void {
    let list = index.get(key);
    if (!list) {
      list = [];
      index.set(key, list);
    }
    insertSorted(list, e);
  }
}
