/**
 * workers/src/handlers/types.ts
 *
 * Shared types for indexer-worker domain-event handlers.
 *
 * Every handler module exports a factory function that:
 *   1. Subscribes to the bus for one or more event names.
 *   2. Returns a HandlerRegistration whose `teardown()` unsubscribes cleanly.
 *
 * Re-exporting the core domain types here keeps handler files decoupled from
 * the api package path depth.
 */

// ── Re-exports from api core ──────────────────────────────────────────────────

export type {
  DomainEvent,
  DomainEventBus,
  DomainEventHandler,
  DomainEventSubscription,
  PublishResult,
} from '../../../api/src/events/types';

export { DOMAIN_EVENT_NAMES } from '../../../api/src/events/names';
export type { DomainEventName } from '../../../api/src/events/names';

export type { IndexedEvent } from '../../../api/src/services/eventIndex/types';

// ── Worker-local types ────────────────────────────────────────────────────────

/**
 * Minimal structured logger interface.
 * Satisfied by Winston, Pino, console, and test stubs alike.
 */
export interface WorkerLogger {
  info(message: string, meta?: Record<string, unknown>): void;
  warn(message: string, meta?: Record<string, unknown>): void;
  error(message: string, meta?: Record<string, unknown>): void;
}

/**
 * Returned by every `register*Handler` factory.
 * Call `teardown()` to unsubscribe from the bus (required in tests and on
 * graceful shutdown).
 */
export interface HandlerRegistration {
  /** Human-readable name of this handler group. */
  readonly name: string;
  /** Cancel all bus subscriptions this registration holds. */
  teardown(): void;
}

/**
 * Optional hook injected into handlers during testing or for side-effects
 * (e.g. write to a database, emit a webhook).
 *
 * Each handler module defines its own concrete Sink interface so callers
 * only provide what a specific handler needs.
 */
export type HandlerOptions<TSink = undefined> = TSink extends undefined
  ? { logger?: WorkerLogger }
  : { logger?: WorkerLogger; sink: TSink };

// ── Default no-op logger (safe console fallback) ─────────────────────────────

export const defaultWorkerLogger: WorkerLogger = {
  info: (msg, meta) =>
    console.log(JSON.stringify({ level: 'info', message: msg, ...meta, ts: Date.now() })),
  warn: (msg, meta) =>
    console.warn(JSON.stringify({ level: 'warn', message: msg, ...meta, ts: Date.now() })),
  error: (msg, meta) =>
    console.error(JSON.stringify({ level: 'error', message: msg, ...meta, ts: Date.now() })),
};
