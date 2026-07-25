import { normalizeEvent, groupCountsByLedger } from './normalize.js';
import { verifyLedgerIntegrity, summarizeIntegrity } from './integrity.js';
import type { EventFetcher } from '../rpc/event-fetcher.js';
import type { EventRepository } from '../db/repository.js';
import type { EventArchiverConfig, SyncState } from '../types.js';
import type { Logger } from '../utils/logger.js';

export interface ArchiveCycleResult {
  fetched: number;
  archived: number;
  lastLedger: number;
  integrityFailed: number;
}

/**
 * Core archiver: backfill + incremental sync of protocol events into the warehouse.
 */
export class EventArchiver {
  private running = false;
  private timer: ReturnType<typeof setInterval> | undefined;

  constructor(
    private readonly config: EventArchiverConfig,
    private readonly fetcher: EventFetcher,
    private readonly repo: EventRepository,
    private readonly logger: Logger
  ) {}

  async getState(): Promise<SyncState> {
    return this.repo.getSyncState();
  }

  /**
   * Backfill from genesis (or configured start ledger) until caught up.
   */
  async backfill(untilLedger?: number): Promise<ArchiveCycleResult> {
    const state = await this.repo.getSyncState();
    let cursor = Math.max(state.lastLedger, this.config.startLedger);
    const latest = untilLedger ?? (await this.fetcher.getLatestLedger());
    let totalFetched = 0;
    let totalArchived = 0;
    let integrityFailed = 0;
    let lastLedger = cursor;

    this.logger.info('Starting event backfill', { cursor, latest });

    while (cursor < latest) {
      const result = await this.archiveFrom(cursor, this.config.batchSize);
      totalFetched += result.fetched;
      totalArchived += result.archived;
      integrityFailed += result.integrityFailed;
      lastLedger = result.lastLedger;

      if (result.fetched === 0 || result.lastLedger <= cursor) {
        // Advance past empty window to avoid infinite loops on sparse history
        cursor = Math.min(cursor + this.config.batchSize, latest);
      } else {
        cursor = result.lastLedger;
      }
    }

    return {
      fetched: totalFetched,
      archived: totalArchived,
      lastLedger,
      integrityFailed,
    };
  }

  /**
   * Single incremental poll cycle.
   */
  async pollOnce(): Promise<ArchiveCycleResult> {
    const state = await this.repo.getSyncState();
    const start = Math.max(state.lastLedger, this.config.startLedger);
    return this.archiveFrom(start, this.config.batchSize);
  }

  async archiveFrom(startLedger: number, limit: number): Promise<ArchiveCycleResult> {
    const raw = await this.fetcher.fetchEvents(startLedger, limit);
    const normalized = raw
      .map((event) => normalizeEvent(event))
      .filter((event): event is NonNullable<typeof event> => event !== null);

    const contractSk = await this.repo.ensureContract(
      this.config.contractId,
      this.config.stellarNetwork
    );
    const archived = await this.repo.insertEvents(contractSk, normalized);

    const counts = groupCountsByLedger(normalized);
    let integrityFailed = 0;
    for (const [ledger, count] of counts) {
      const check = verifyLedgerIntegrity(ledger, count, count);
      await this.repo.recordLedgerIntegrity(
        check.ledger,
        check.archivedCount,
        check.expectedCount,
        check.integrityOk
      );
      if (!check.integrityOk) integrityFailed += 1;
    }

    const lastLedger =
      normalized.length > 0
        ? Math.max(...normalized.map((e) => e.ledger))
        : startLedger;

    if (archived > 0 || lastLedger > startLedger) {
      await this.repo.updateSyncState(lastLedger, archived);
    }

    const summary = summarizeIntegrity(
      [...counts.entries()].map(([ledger, count]) =>
        verifyLedgerIntegrity(ledger, count, count)
      )
    );

    this.logger.info('Archive cycle complete', {
      fetched: raw.length,
      archived,
      lastLedger,
      integrity: summary,
    });

    return {
      fetched: raw.length,
      archived,
      lastLedger,
      integrityFailed,
    };
  }

  async runRetention(): Promise<number> {
    const moved = await this.repo.applyRetention(this.config.detailRetentionDays);
    this.logger.info('Retention applied', {
      retentionDays: this.config.detailRetentionDays,
      dailyRowsTouched: moved,
    });
    return moved;
  }

  start(): void {
    if (this.running) return;
    this.running = true;
    this.logger.info('Event archiver started', {
      pollIntervalMs: this.config.pollIntervalMs,
      contractId: this.config.contractId,
    });

    const tick = async () => {
      try {
        await this.pollOnce();
      } catch (err) {
        this.logger.error('Incremental sync failed', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    };

    void tick();
    this.timer = setInterval(() => void tick(), this.config.pollIntervalMs);
  }

  stop(): void {
    this.running = false;
    if (this.timer) clearInterval(this.timer);
    this.timer = undefined;
    this.logger.info('Event archiver stopped');
  }
}
