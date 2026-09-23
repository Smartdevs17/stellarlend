/**
 * Event archival to cold storage.
 *
 * Events older than the hot retention window are grouped into one segment per
 * UTC day, written as gzip-compressed NDJSON, and recorded in a manifest with
 * their ledger/time bounds and a SHA-256 checksum. The hot store then evicts
 * them. Archived segments stay queryable (`includeArchived=true`) and are the
 * input for replay.
 *
 * {@link ColdStorage} is deliberately tiny so the filesystem backend can be
 * swapped for object storage (S3 / GCS) without touching the archiver.
 * Elasticsearch ILM (see `backend/elasticsearch/`) covers the search tier's
 * own hot → warm → cold lifecycle independently.
 */

import { createHash } from 'crypto';
import { promises as fs } from 'fs';
import path from 'path';
import { gunzipSync, gzipSync } from 'zlib';
import type { ArchiveManifest, ArchiveSegment, IndexedEvent } from './types';

const MANIFEST_KEY = 'manifest.json';

export interface ColdStorage {
  put(key: string, body: Buffer): Promise<void>;
  get(key: string): Promise<Buffer | null>;
}

export class FileSystemColdStorage implements ColdStorage {
  constructor(private readonly root: string) {}

  async put(key: string, body: Buffer): Promise<void> {
    const target = this.resolve(key);
    await fs.mkdir(path.dirname(target), { recursive: true });
    // Write-then-rename so a crash never leaves a truncated segment/manifest.
    const tmp = `${target}.${process.pid}.tmp`;
    await fs.writeFile(tmp, body);
    await fs.rename(tmp, target);
  }

  async get(key: string): Promise<Buffer | null> {
    try {
      return await fs.readFile(this.resolve(key));
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') return null;
      throw err;
    }
  }

  private resolve(key: string): string {
    const target = path.resolve(this.root, key);
    if (!target.startsWith(path.resolve(this.root) + path.sep)) {
      throw new Error(`invalid archive key: ${key}`);
    }
    return target;
  }
}

/** In-memory backend for tests and ephemeral deployments. */
export class MemoryColdStorage implements ColdStorage {
  readonly objects = new Map<string, Buffer>();

  async put(key: string, body: Buffer): Promise<void> {
    this.objects.set(key, Buffer.from(body));
  }

  async get(key: string): Promise<Buffer | null> {
    return this.objects.get(key) ?? null;
  }
}

function dayKey(timestamp: number): string {
  return new Date(timestamp).toISOString().slice(0, 10);
}

export class EventArchiver {
  private manifest: ArchiveManifest | null = null;
  private segmentCache = new Map<string, IndexedEvent[]>();

  constructor(
    private readonly storage: ColdStorage,
    private readonly maxCachedSegments = 32
  ) {}

  async getManifest(): Promise<ArchiveManifest> {
    if (!this.manifest) {
      const raw = await this.storage.get(MANIFEST_KEY);
      this.manifest = raw
        ? (JSON.parse(raw.toString('utf8')) as ArchiveManifest)
        : { version: 1, segments: [] };
    }
    return this.manifest;
  }

  /**
   * Write `events` to cold storage as day segments. Events for a day that is
   * already archived are merged into a new generation of that day's segment.
   */
  async archive(events: IndexedEvent[]): Promise<ArchiveSegment[]> {
    if (events.length === 0) return [];
    const manifest = await this.getManifest();

    const byDay = new Map<string, IndexedEvent[]>();
    for (const e of events) {
      const day = dayKey(e.timestamp);
      const bucket = byDay.get(day) ?? [];
      bucket.push(e);
      byDay.set(day, bucket);
    }

    const written: ArchiveSegment[] = [];
    for (const [day, dayEvents] of byDay) {
      const previous = manifest.segments.filter((s) => s.key.startsWith(`events/${day}/`));
      const merged = new Map<string, IndexedEvent>();
      for (const s of previous) for (const e of await this.readSegment(s)) merged.set(e.id, e);
      for (const e of dayEvents) merged.set(e.id, e);
      const ordered = [...merged.values()].sort(
        (a, b) => a.ledger - b.ledger || (a.id < b.id ? -1 : 1)
      );

      const body = gzipSync(Buffer.from(ordered.map((e) => JSON.stringify(e)).join('\n') + '\n'));
      // A fresh key per generation: the manifest keeps pointing at the old
      // segment until the new manifest is written, so a crash in between
      // never leaves the manifest referencing a half-replaced object.
      const generation = previous.length
        ? Math.max(...previous.map((s) => Number(/segment-(\d+)/.exec(s.key)?.[1] ?? 0))) + 1
        : 1;
      // Single pass: spreading a large day into Math.min/max overflows the stack.
      const bounds = {
        fromLedger: Infinity,
        toLedger: -Infinity,
        fromTimestamp: Infinity,
        toTimestamp: -Infinity,
      };
      for (const e of ordered) {
        bounds.fromLedger = Math.min(bounds.fromLedger, e.ledger);
        bounds.toLedger = Math.max(bounds.toLedger, e.ledger);
        bounds.fromTimestamp = Math.min(bounds.fromTimestamp, e.timestamp);
        bounds.toTimestamp = Math.max(bounds.toTimestamp, e.timestamp);
      }
      const segment: ArchiveSegment = {
        key: `events/${day}/segment-${generation}.ndjson.gz`,
        count: ordered.length,
        ...bounds,
        sha256: createHash('sha256').update(body).digest('hex'),
        createdAt: Date.now(),
      };
      await this.storage.put(segment.key, body);
      manifest.segments = manifest.segments.filter((s) => !previous.includes(s));
      manifest.segments.push(segment);
      this.segmentCache.set(segment.key, ordered);
      written.push(segment);
    }

    manifest.segments.sort((a, b) => a.fromLedger - b.fromLedger);
    await this.storage.put(MANIFEST_KEY, Buffer.from(JSON.stringify(manifest, null, 2)));
    this.trimCache();
    return written;
  }

  /** Segments overlapping the given ledger / time window. */
  async segmentsFor(window: {
    fromLedger?: number;
    toLedger?: number;
    from?: number;
    to?: number;
  }): Promise<ArchiveSegment[]> {
    const { segments } = await this.getManifest();
    return segments.filter(
      (s) =>
        (window.fromLedger === undefined || s.toLedger >= window.fromLedger) &&
        (window.toLedger === undefined || s.fromLedger <= window.toLedger) &&
        (window.from === undefined || s.toTimestamp >= window.from) &&
        (window.to === undefined || s.fromTimestamp <= window.to)
    );
  }

  /** Decode a segment, verifying its checksum against the manifest. */
  async readSegment(segment: ArchiveSegment): Promise<IndexedEvent[]> {
    const cached = this.segmentCache.get(segment.key);
    if (cached) return cached;
    const body = await this.storage.get(segment.key);
    if (!body) throw new Error(`archive segment missing: ${segment.key}`);
    const digest = createHash('sha256').update(body).digest('hex');
    if (digest !== segment.sha256) {
      throw new Error(`archive segment checksum mismatch: ${segment.key}`);
    }
    const events = gunzipSync(body)
      .toString('utf8')
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line) as IndexedEvent);
    this.segmentCache.set(segment.key, events);
    this.trimCache();
    return events;
  }

  private trimCache(): void {
    while (this.segmentCache.size > this.maxCachedSegments) {
      const oldest = this.segmentCache.keys().next().value as string;
      this.segmentCache.delete(oldest);
    }
  }
}
