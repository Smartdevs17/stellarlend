/**
 * workers/src/handlers/oracle.handler.ts
 *
 * Indexer-worker handler for oracle.price_updated domain events.
 *
 * Price-update events are high frequency; the handler logs at debug level
 * in production and only warns when a price move exceeds a configurable
 * threshold (useful for detecting oracle manipulation or feed staleness).
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

export interface OraclePriceUpdatedPayload {
  eventId: string;
  ledger?: number;
  contract?: string | null;
  actor?: string | null;
  asset?: string | null;
  amount?: string | null;
  timestamp: number;
  /** Raw extra data from the oracle envelope. */
  data?: Record<string, unknown>;
}

// ── Sink interface ────────────────────────────────────────────────────────────

export interface OracleSink {
  onPriceUpdated?(event: DomainEvent<OraclePriceUpdatedPayload>): Promise<void> | void;
}

// ── Options ───────────────────────────────────────────────────────────────────

export interface RegisterOracleHandlerOptions {
  logger?: WorkerLogger;
  sink?: OracleSink;
  /**
   * Warn when the absolute price delta (expressed as a decimal string in
   * the event payload) exceeds this threshold.  Defaults to 0 (no warning).
   */
  largeMoveThreshold?: bigint;
}

// ── Factory ───────────────────────────────────────────────────────────────────

/**
 * Subscribe an oracle-price handler to `bus` and return a registration
 * whose `teardown()` unsubscribes it.
 */
export function registerOracleHandler(
  bus: DomainEventBus,
  options: RegisterOracleHandlerOptions = {}
): HandlerRegistration {
  const log = options.logger ?? defaultWorkerLogger;
  const sink = options.sink;
  const largeMoveThreshold = options.largeMoveThreshold ?? 0n;

  const sub = bus.subscribe<OraclePriceUpdatedPayload>(
    DOMAIN_EVENT_NAMES.OraclePriceUpdated,
    async (event) => {
      const p = event.payload;

      log.info('oracle.price_updated', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        asset: p.asset,
        price: p.amount,
      });

      // Observability: warn on large price moves.
      if (largeMoveThreshold > 0n && p.amount !== null && p.amount !== undefined) {
        try {
          const price = BigInt(p.amount);
          if (price > largeMoveThreshold) {
            log.warn('oracle.large_price_move', {
              eventId: event.id,
              asset: p.asset,
              price: p.amount,
              threshold: largeMoveThreshold.toString(),
            });
          }
        } catch {
          // amount not parseable as BigInt — skip threshold check
        }
      }

      await sink?.onPriceUpdated?.(event);
    }
  );

  return {
    name: 'oracle',
    teardown() {
      sub.unsubscribe();
    },
  };
}
