/**
 * workers/src/handlers/protocol.handler.ts
 *
 * Indexer-worker handler for protocol.pause_state_changed domain events.
 *
 * A protocol pause is an emergency action — the handler always logs at WARN
 * level and calls the sink immediately so callers can trigger alerts, block
 * new user operations, or update a status page.
 */

import {
  DOMAIN_EVENT_NAMES,
  defaultWorkerLogger,
  type DomainEvent,
  type DomainEventBus,
  type HandlerRegistration,
  type WorkerLogger,
} from './types';

// ── Payload shape ─────────────────────────────────────────────────────────────

export interface ProtocolPausePayload {
  eventId: string;
  ledger?: number;
  contract?: string | null;
  actor?: string | null;
  timestamp: number;
  module?: string;
  /**
   * The action that produced the pause event: `pause` | `unpause` or a
   * legacy type string like `pause_state_changed`.
   */
  action?: string;
  data?: Record<string, unknown>;
}

// ── Sink interface ────────────────────────────────────────────────────────────

export interface ProtocolSink {
  onProtocolPauseChanged?(event: DomainEvent<ProtocolPausePayload>): Promise<void> | void;
}

// ── Options ───────────────────────────────────────────────────────────────────

export interface RegisterProtocolHandlerOptions {
  logger?: WorkerLogger;
  sink?: ProtocolSink;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function isPause(payload: ProtocolPausePayload): boolean {
  const action = payload.action ?? '';
  // A "pause" action — anything that isn't explicitly an unpause.
  return action !== 'unpause';
}

// ── Factory ───────────────────────────────────────────────────────────────────

/**
 * Subscribe a protocol-pause handler to `bus` and return a registration
 * whose `teardown()` unsubscribes it.
 */
export function registerProtocolHandler(
  bus: DomainEventBus,
  options: RegisterProtocolHandlerOptions = {}
): HandlerRegistration {
  const log = options.logger ?? defaultWorkerLogger;
  const sink = options.sink;

  const sub = bus.subscribe<ProtocolPausePayload>(
    DOMAIN_EVENT_NAMES.ProtocolPaused,
    async (event) => {
      const p = event.payload;
      const paused = isPause(p);

      // Always WARN — protocol pause is an emergency-level event.
      log.warn('protocol.pause_state_changed', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        actor: p.actor,
        action: p.action,
        paused,
        data: p.data,
      });

      await sink?.onProtocolPauseChanged?.(event);
    }
  );

  return {
    name: 'protocol',
    teardown() {
      sub.unsubscribe();
    },
  };
}
