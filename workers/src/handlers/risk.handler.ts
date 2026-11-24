/**
 * workers/src/handlers/risk.handler.ts
 *
 * Indexer-worker handler for risk.params_updated domain events.
 *
 * Risk-parameter changes are low-frequency but high-impact — they affect
 * every subsequent loan's collateral requirements.  The handler logs the
 * full data payload so ops teams can audit changes via the log stream.
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

export interface RiskParamsUpdatedPayload {
  eventId: string;
  ledger?: number;
  contract?: string | null;
  actor?: string | null;
  asset?: string | null;
  timestamp: number;
  module?: string;
  action?: string;
  /** Raw params diff from the contract event. */
  data?: Record<string, unknown>;
}

// ── Sink interface ────────────────────────────────────────────────────────────

export interface RiskSink {
  onRiskParamsUpdated?(event: DomainEvent<RiskParamsUpdatedPayload>): Promise<void> | void;
}

// ── Options ───────────────────────────────────────────────────────────────────

export interface RegisterRiskHandlerOptions {
  logger?: WorkerLogger;
  sink?: RiskSink;
}

// ── Factory ───────────────────────────────────────────────────────────────────

/**
 * Subscribe a risk-params handler to `bus` and return a registration
 * whose `teardown()` unsubscribes it.
 */
export function registerRiskHandler(
  bus: DomainEventBus,
  options: RegisterRiskHandlerOptions = {}
): HandlerRegistration {
  const log = options.logger ?? defaultWorkerLogger;
  const sink = options.sink;

  const sub = bus.subscribe<RiskParamsUpdatedPayload>(
    DOMAIN_EVENT_NAMES.RiskParamsUpdated,
    async (event) => {
      const p = event.payload;

      // Always log risk-param changes in full — these are auditable events.
      log.info('risk.params_updated', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        actor: p.actor,
        asset: p.asset,
        module: p.module,
        action: p.action,
        data: p.data,
      });

      await sink?.onRiskParamsUpdated?.(event);
    }
  );

  return {
    name: 'risk',
    teardown() {
      sub.unsubscribe();
    },
  };
}
