/**
 * workers/src/handlers/liquidation.handler.ts
 *
 * Indexer-worker handler for liquidation.liquidate domain events.
 *
 * Liquidation events carry extra debt/collateral fields beyond the common
 * lending payload, so they get their own dedicated module and sink interface.
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

export interface LiquidationEventPayload {
  eventId: string;
  ledger?: number;
  contract?: string | null;
  /** Address that triggered the liquidation (the liquidator). */
  actor?: string | null;
  /** Address of the borrower being liquidated. */
  counterparty?: string | null;
  asset?: string | null;
  amount?: string | null;
  timestamp: number;
  debtAsset?: string | null;
  debtAmount?: string | null;
  collateralAsset?: string | null;
  collateralAmount?: string | null;
}

// ── Sink interface ────────────────────────────────────────────────────────────

export interface LiquidationSink {
  onLiquidation?(event: DomainEvent<LiquidationEventPayload>): Promise<void> | void;
}

// ── Options ───────────────────────────────────────────────────────────────────

export interface RegisterLiquidationHandlerOptions {
  logger?: WorkerLogger;
  sink?: LiquidationSink;
}

// ── Factory ───────────────────────────────────────────────────────────────────

/**
 * Subscribe a liquidation handler to `bus` and return a registration
 * whose `teardown()` unsubscribes it.
 */
export function registerLiquidationHandler(
  bus: DomainEventBus,
  options: RegisterLiquidationHandlerOptions = {}
): HandlerRegistration {
  const log = options.logger ?? defaultWorkerLogger;
  const sink = options.sink;

  const sub = bus.subscribe<LiquidationEventPayload>(
    DOMAIN_EVENT_NAMES.Liquidation,
    async (event) => {
      const p = event.payload;

      log.info('liquidation.liquidate', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        liquidator: p.actor,
        borrower: p.counterparty,
        debtAsset: p.debtAsset,
        debtAmount: p.debtAmount,
        collateralAsset: p.collateralAsset,
        collateralAmount: p.collateralAmount,
      });

      // Alert on unusually large liquidations (observability hook).
      if (p.debtAmount !== null && p.debtAmount !== undefined) {
        const amount = BigInt(p.debtAmount);
        if (amount > 1_000_000_000_000n) {
          log.warn('liquidation.large_liquidation', {
            eventId: event.id,
            debtAmount: p.debtAmount,
            debtAsset: p.debtAsset,
            liquidator: p.actor,
            borrower: p.counterparty,
          });
        }
      }

      await sink?.onLiquidation?.(event);
    }
  );

  return {
    name: 'liquidation',
    teardown() {
      sub.unsubscribe();
    },
  };
}
