/**
 * Process-wide event indexer, configured from the environment:
 *
 * | Variable                        | Default                 |
 * |---------------------------------|-------------------------|
 * | EVENT_INDEXER_ENABLED           | false (poll on startup) |
 * | EVENT_INDEXER_POLL_MS           | 10000                   |
 * | EVENT_INDEXER_START_LEDGER      | latest - ~1 day         |
 * | EVENT_HOT_RETENTION_DAYS        | 30                      |
 * | EVENT_ARCHIVE_DIR               | ./data/event-archive    |
 * | ELASTICSEARCH_URL               | unset (sink disabled)   |
 * | ELASTICSEARCH_API_KEY           | unset                   |
 * | ELASTICSEARCH_EVENT_INDEX       | stellarlend-events      |
 */

import path from 'path';
import { StellarService } from '../stellar.service';
import { EventArchiver, FileSystemColdStorage } from './archive';
import { ElasticsearchEventSink, EventSink } from './elasticsearch';
import { EventIndexer } from './indexer';

export * from './types';
export { EventIndexer } from './indexer';
export { EventStore } from './store';
export { EventArchiver, FileSystemColdStorage, MemoryColdStorage } from './archive';
export { ElasticsearchEventSink } from './elasticsearch';
export { CURRENT_SCHEMA_VERSION, normalizeEvent, upcast } from './schema';

const DAY_MS = 24 * 60 * 60 * 1000;

function optionalInt(value: string | undefined): number | undefined {
  if (value === undefined || value === '') return undefined;
  const n = Number(value);
  return Number.isInteger(n) ? n : undefined;
}

function createDefaultIndexer(): EventIndexer {
  const stellar = new StellarService();
  const sinks: EventSink[] = [];
  if (process.env.ELASTICSEARCH_URL) {
    sinks.push(
      new ElasticsearchEventSink({
        url: process.env.ELASTICSEARCH_URL,
        apiKey: process.env.ELASTICSEARCH_API_KEY,
        index: process.env.ELASTICSEARCH_EVENT_INDEX,
      })
    );
  }
  return new EventIndexer({
    source: {
      latestLedger: () => stellar.getLatestLedger(),
      fetchEvents: (request) => stellar.fetchContractEvents(request),
    },
    archiver: new EventArchiver(
      new FileSystemColdStorage(
        process.env.EVENT_ARCHIVE_DIR ?? path.resolve(process.cwd(), 'data/event-archive')
      )
    ),
    sinks,
    hotRetentionMs: (optionalInt(process.env.EVENT_HOT_RETENTION_DAYS) ?? 30) * DAY_MS,
    startLedger: optionalInt(process.env.EVENT_INDEXER_START_LEDGER),
  });
}

let instance: EventIndexer | null = null;

export function getEventIndexer(): EventIndexer {
  instance ??= createDefaultIndexer();
  return instance;
}

/** Swap the process-wide indexer (tests, alternate storage backends). */
export function setEventIndexer(indexer: EventIndexer | null): void {
  instance?.stop();
  instance = indexer;
}

/** Start background polling when `EVENT_INDEXER_ENABLED=true`. */
export function startEventIndexerFromEnv(): void {
  if (process.env.EVENT_INDEXER_ENABLED !== 'true') return;
  getEventIndexer().start(optionalInt(process.env.EVENT_INDEXER_POLL_MS) ?? 10_000);
}
