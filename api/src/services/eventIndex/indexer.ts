/**
 * EventIndexer — ingests contract events from Soroban RPC and serves queries,
 * archival, replay and analytics over them (issue #685).
 */

import logger from '../../utils/logger';
import { EventArchiver } from './archive';
import type { EventSink } from './elasticsearch';
import { replayEvents, ReplayResult, ReplaySource } from './replay';
import { CURRENT_SCHEMA_VERSION, normalizeEvent } from './schema';
import {
  decodeCursor,
  encodeCursor,
  EventStore,
  matchesQuery,
  MAX_QUERY_LIMIT,
  DEFAULT_QUERY_LIMIT,
} from './store';
import type {
  ArchiveManifest,
  ArchiveSegment,
  EventPage,
  EventQuery,
  IndexedEvent,
  RawContractEvent,
} from './types';

const DAY_MS = 24 * 60 * 60 * 1000;

/** Where events come from — implemented by `StellarService` for Soroban RPC. */
export interface EventSource {
  latestLedger(): Promise<number>;
  fetchEvents(request: { startLedger?: number; cursor?: string; limit: number }): Promise<{
    events: RawContractEvent[];
    cursor: string;
    latestLedger: number;
  }>;
}

export interface EventIndexerOptions {
  source: EventSource;
  archiver: EventArchiver;
  store?: EventStore;
  sinks?: EventSink[];
  /** Events older than this stay hot; older ones are archivable. */
  hotRetentionMs?: number;
  /** Ledgers to look back on first sync when no start ledger is configured. */
  initialLookbackLedgers?: number;
  startLedger?: number;
  pageSize?: number;
  maxPagesPerSync?: number;
}

export interface SyncResult {
  fetched: number;
  indexed: number;
  pages: number;
  latestNetworkLedger: number;
}

export type AnalyticsBucket = 'hour' | 'day';

function compareAsc(a: IndexedEvent, b: IndexedEvent): number {
  return a.ledger - b.ledger || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}

export class EventIndexer {
  readonly store: EventStore;
  private readonly source: EventSource;
  private readonly archiver: EventArchiver;
  private readonly sinks: EventSink[];
  private readonly hotRetentionMs: number;
  private readonly initialLookbackLedgers: number;
  private readonly startLedger?: number;
  private readonly pageSize: number;
  private readonly maxPagesPerSync: number;

  private cursor: string | null = null;
  private syncing: Promise<SyncResult> | null = null;
  private timer: NodeJS.Timeout | null = null;
  private lastSyncAt: number | null = null;
  private lastError: string | null = null;
  private latestNetworkLedger = 0;
  private totalIngested = 0;
  private sinkFailures: Record<string, number> = {};

  constructor(options: EventIndexerOptions) {
    this.source = options.source;
    this.archiver = options.archiver;
    this.store = options.store ?? new EventStore();
    this.sinks = options.sinks ?? [];
    this.hotRetentionMs = options.hotRetentionMs ?? 30 * DAY_MS;
    this.initialLookbackLedgers = options.initialLookbackLedgers ?? 17_280; // ~1 day
    this.startLedger = options.startLedger;
    this.pageSize = options.pageSize ?? 200;
    this.maxPagesPerSync = options.maxPagesPerSync ?? 50;
  }

  // ─── Ingestion ────────────────────────────────────────────────────────────

  /** Pull new events from the source. Concurrent calls share one run. */
  sync(): Promise<SyncResult> {
    if (!this.syncing) {
      this.syncing = this.runSync().finally(() => {
        this.syncing = null;
      });
    }
    return this.syncing;
  }

  /** Ingest already-decoded events (backfills, tests, alternate sources). */
  async ingest(raw: RawContractEvent[]): Promise<number> {
    const events = raw
      .filter((r) => r.inSuccessfulContractCall !== false)
      .map((r) => normalizeEvent(r));
    const added = this.store.add(events);
    this.totalIngested += added;
    if (added) await this.writeSinks(events);
    return added;
  }

  start(intervalMs: number): void {
    if (this.timer) return;
    const tick = () => {
      this.sync().catch(() => undefined); // errors are recorded in status()
    };
    tick();
    this.timer = setInterval(tick, intervalMs);
    this.timer.unref();
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  private async runSync(): Promise<SyncResult> {
    const result: SyncResult = { fetched: 0, indexed: 0, pages: 0, latestNetworkLedger: 0 };
    try {
      for (let page = 0; page < this.maxPagesPerSync; page += 1) {
        const request = this.cursor
          ? { cursor: this.cursor, limit: this.pageSize }
          : { startLedger: await this.initialStartLedger(), limit: this.pageSize };
        const res = await this.source.fetchEvents(request);
        result.pages += 1;
        result.fetched += res.events.length;
        result.indexed += await this.ingest(res.events);
        result.latestNetworkLedger = this.latestNetworkLedger = res.latestLedger;
        if (res.cursor) this.cursor = res.cursor;
        if (res.events.length < this.pageSize) break;
      }
      this.lastError = null;
      return result;
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
      logger.warn('Event indexer sync failed', { error: this.lastError });
      throw err;
    } finally {
      this.lastSyncAt = Date.now();
    }
  }

  private async initialStartLedger(): Promise<number> {
    if (this.startLedger !== undefined) return this.startLedger;
    const latest = await this.source.latestLedger();
    return Math.max(1, latest - this.initialLookbackLedgers);
  }

  private async writeSinks(events: IndexedEvent[]): Promise<void> {
    await Promise.all(
      this.sinks.map(async (sink) => {
        try {
          await sink.write(events);
        } catch (err) {
          this.sinkFailures[sink.name] = (this.sinkFailures[sink.name] ?? 0) + 1;
          logger.warn('Event sink write failed', {
            sink: sink.name,
            error: (err as Error).message,
          });
        }
      })
    );
  }

  // ─── Query ────────────────────────────────────────────────────────────────

  async query(q: EventQuery): Promise<EventPage> {
    if (!q.includeArchived) return this.store.query(q);

    const started = performance.now();
    const limit = Math.min(Math.max(1, q.limit ?? DEFAULT_QUERY_LIMIT), MAX_QUERY_LIMIT);
    const order = q.order ?? 'desc';
    const hot = this.store.query({ ...q, limit });

    // Archived events are strictly older than hot ones, so the next page is
    // within (hot page) ∪ (archived matches past the cursor).
    const cursor = q.cursor ? decodeCursor(q.cursor) : null;
    const pastCursor = (e: IndexedEvent) => {
      if (!cursor) return true;
      const cmp = compareAsc(e, { ledger: cursor.ledger, id: cursor.id } as IndexedEvent);
      return order === 'asc' ? cmp > 0 : cmp < 0;
    };
    const archived = (await this.archivedMatching(q)).filter(
      (e) => pastCursor(e) && !this.store.get(e.id)
    );

    const merged = [...hot.events, ...archived].sort((a, b) =>
      order === 'asc' ? compareAsc(a, b) : compareAsc(b, a)
    );
    const events = merged.slice(0, limit);
    const hasMore = merged.length > limit || hot.nextCursor !== null;
    const lastEvent = events[events.length - 1];
    return {
      events,
      nextCursor: hasMore && lastEvent ? encodeCursor(lastEvent) : null,
      matched: hot.matched,
      tookMs: performance.now() - started,
      source: 'hot+archive',
    };
  }

  get(id: string): IndexedEvent | undefined {
    return this.store.get(id);
  }

  private async archivedMatching(q: EventQuery): Promise<IndexedEvent[]> {
    const segments = await this.archiver.segmentsFor(q);
    const out: IndexedEvent[] = [];
    for (const segment of segments) {
      for (const e of await this.archiver.readSegment(segment)) if (matchesQuery(e, q)) out.push(e);
    }
    return out;
  }

  // ─── Archival ─────────────────────────────────────────────────────────────

  /**
   * Move events older than `olderThanMs` (default: hot retention) to cold
   * storage. If the archive write fails the events are restored to the hot
   * store, so archival never loses data.
   */
  async archive(
    olderThanMs: number = this.hotRetentionMs,
    now: number = Date.now()
  ): Promise<{
    archived: number;
    segments: ArchiveSegment[];
    cutoff: number;
  }> {
    const cutoff = now - olderThanMs;
    const evicted = this.store.evict((e) => e.timestamp < cutoff);
    try {
      const segments = await this.archiver.archive(evicted);
      return { archived: evicted.length, segments, cutoff };
    } catch (err) {
      this.store.add(evicted);
      throw err;
    }
  }

  archiveManifest(): Promise<ArchiveManifest> {
    return this.archiver.getManifest();
  }

  // ─── Replay ───────────────────────────────────────────────────────────────

  async replay(
    q: EventQuery & { source?: ReplaySource; maxTransitions?: number }
  ): Promise<ReplayResult> {
    const window: EventQuery = { ...q, account: undefined, cursor: undefined };
    const archived = q.includeArchived === false ? [] : await this.archivedMatching(window);
    const hot = this.store.snapshot().filter((e) => matchesQuery(e, window));
    const seen = new Set<string>();
    const ordered = [...archived, ...hot]
      .filter((e) => (seen.has(e.id) ? false : (seen.add(e.id), true)))
      .sort(compareAsc);
    return replayEvents(ordered, {
      source: q.source,
      account: q.account,
      maxTransitions: q.maxTransitions,
    });
  }

  // ─── Analytics & status ───────────────────────────────────────────────────

  analytics(options: { from?: number; to?: number; bucket?: AnalyticsBucket; top?: number } = {}) {
    const bucketMs = options.bucket === 'day' ? DAY_MS : 60 * 60 * 1000;
    const top = options.top ?? 10;
    const byType: Record<string, number> = {};
    const byModule: Record<string, number> = {};
    const bySchemaStatus: Record<string, number> = {};
    const volumeByAsset: Record<string, Record<string, bigint>> = {};
    const actorCounts = new Map<string, number>();
    const series = new Map<number, Record<string, number>>();
    let total = 0;

    for (const e of this.store.snapshot()) {
      if (options.from !== undefined && e.timestamp < options.from) continue;
      if (options.to !== undefined && e.timestamp > options.to) continue;
      total += 1;
      byType[e.type] = (byType[e.type] ?? 0) + 1;
      if (e.module) byModule[e.module] = (byModule[e.module] ?? 0) + 1;
      bySchemaStatus[e.schemaStatus] = (bySchemaStatus[e.schemaStatus] ?? 0) + 1;
      if (e.actor) actorCounts.set(e.actor, (actorCounts.get(e.actor) ?? 0) + 1);

      const bucket = Math.floor(e.timestamp / bucketMs) * bucketMs;
      const point = series.get(bucket) ?? {};
      point[e.type] = (point[e.type] ?? 0) + 1;
      series.set(bucket, point);

      if (e.envelope === 'typed' && e.amount !== undefined) {
        const asset = e.asset ?? 'native';
        const perType = (volumeByAsset[e.type] ??= {});
        perType[asset] = (perType[asset] ?? 0n) + BigInt(e.amount);
      }
    }

    return {
      totalEvents: total,
      byType,
      byModule,
      bySchemaStatus,
      timeseries: [...series.entries()]
        .sort(([a], [b]) => a - b)
        .map(([bucketStart, counts]) => ({
          bucketStart,
          total: Object.values(counts).reduce((s, n) => s + n, 0),
          counts,
        })),
      topAccounts: [...actorCounts.entries()]
        .sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
        .slice(0, top)
        .map(([account, events]) => ({ account, events })),
      volumeByAsset: Object.fromEntries(
        Object.entries(volumeByAsset).map(([type, assets]) => [
          type,
          Object.fromEntries(Object.entries(assets).map(([a, v]) => [a, v.toString()])),
        ])
      ),
      bucket: options.bucket ?? 'hour',
      queryLatency: this.store.latencyStats(),
    };
  }

  async status() {
    const manifest = await this.archiver.getManifest();
    const latestIndexedLedger = this.store.latestLedger();
    return {
      running: this.timer !== null,
      syncing: this.syncing !== null,
      schemaVersion: CURRENT_SCHEMA_VERSION,
      cursor: this.cursor,
      lastSyncAt: this.lastSyncAt,
      lastError: this.lastError,
      totalIngested: this.totalIngested,
      hotEvents: this.store.size,
      hotRetentionMs: this.hotRetentionMs,
      latestIndexedLedger,
      latestNetworkLedger: this.latestNetworkLedger,
      lagLedgers: this.latestNetworkLedger
        ? Math.max(0, this.latestNetworkLedger - latestIndexedLedger)
        : null,
      archive: {
        segments: manifest.segments.length,
        events: manifest.segments.reduce((s, seg) => s + seg.count, 0),
        oldestLedger: manifest.segments[0]?.fromLedger ?? null,
        newestLedger: manifest.segments[manifest.segments.length - 1]?.toLedger ?? null,
      },
      sinks: this.sinks.map((s) => ({ name: s.name, failures: this.sinkFailures[s.name] ?? 0 })),
      queryLatency: this.store.latencyStats(),
    };
  }
}
