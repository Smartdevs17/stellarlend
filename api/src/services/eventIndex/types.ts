/**
 * Shared types for the contract event indexing pipeline (issue #685).
 *
 *   Soroban RPC getEvents ─► normalize (schema.ts) ─► EventStore (hot, in-memory)
 *                                                  ├─► EventSink (Elasticsearch)
 *                                                  └─► ColdStorage (archive.ts)
 */

/** Envelope family of an indexed event. */
export type EventEnvelope = 'typed' | 'structured_event_v1';

/**
 * How the indexer treated the event's schema version:
 * - `current`  – already at {@link CURRENT_SCHEMA_VERSION}
 * - `upcast`   – an older version, migrated to current by an upcaster
 * - `unknown`  – newer than this indexer understands; stored verbatim
 */
export type SchemaStatus = 'current' | 'upcast' | 'unknown';

export interface IndexedEvent {
  /** RPC event id (`<toid>-<index>`), globally unique and ordered. */
  id: string;
  /** Event name, e.g. `deposit`, `liquidation`, or `proto_evt` for envelopes. */
  type: string;
  contract: string;
  /** Decoded topics (JSON-safe; i128/u64 rendered as decimal strings). */
  topic: unknown[];
  /** Decoded payload (JSON-safe). */
  data: Record<string, unknown>;
  /** Ledger close time, epoch milliseconds. */
  timestamp: number;
  ledger: number;
  txHash?: string;
  /** Every Stellar account / contract address referenced by topics or data. */
  accounts: string[];
  envelope: EventEnvelope;
  /** Primary actor: first address topic (user, liquidator, envelope actor). */
  actor?: string;
  /** Secondary party (e.g. liquidated borrower, envelope counterparty). */
  counterparty?: string;
  /** Structured-envelope module (e.g. `lending`), when present. */
  module?: string;
  /** Structured-envelope action name (e.g. `borrow`), when present. */
  action?: string;
  /** Primary asset, when the payload names one. */
  asset?: string;
  /** Primary amount in base units (decimal string), when present. */
  amount?: string;
  schemaVersion: number;
  schemaStatus: SchemaStatus;
}

export interface EventQuery {
  type?: string;
  /** Matches any address referenced by the event. */
  account?: string;
  contract?: string;
  module?: string;
  action?: string;
  /** Inclusive lower bound, epoch ms. */
  from?: number;
  /** Inclusive upper bound, epoch ms. */
  to?: number;
  fromLedger?: number;
  toLedger?: number;
  /** Default `desc` (newest first). */
  order?: 'asc' | 'desc';
  limit?: number;
  /** Opaque cursor returned as `nextCursor` by a previous page. */
  cursor?: string;
  /** Also search archived (cold) segments. */
  includeArchived?: boolean;
}

export interface EventPage {
  events: IndexedEvent[];
  nextCursor: string | null;
  /** Number of hot events matched before pagination (archived excluded). */
  matched: number;
  tookMs: number;
  source: 'hot' | 'hot+archive';
}

export interface ArchiveSegment {
  key: string;
  count: number;
  fromLedger: number;
  toLedger: number;
  fromTimestamp: number;
  toTimestamp: number;
  sha256: string;
  createdAt: number;
}

export interface ArchiveManifest {
  version: 1;
  segments: ArchiveSegment[];
}

/** Raw event shape accepted by the normalizer (subset of RPC EventResponse). */
export interface RawContractEvent {
  id: string;
  ledger: number;
  ledgerClosedAt: string;
  contractId?: string | { toString(): string };
  txHash?: string;
  inSuccessfulContractCall?: boolean;
  /** Decoded topics (already run through `scValToNative`). */
  topic: unknown[];
  /** Decoded value (already run through `scValToNative`). */
  value: unknown;
}
