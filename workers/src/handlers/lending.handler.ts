/**
 * workers/src/handlers/lending.handler.ts
 *
 * Indexer-worker handlers for lending domain events:
 *   lending.deposit | lending.withdraw | lending.borrow |
 *   lending.repay   | lending.flash_loan
 *
 * Each handler logs the event and calls an optional LendingSink so the
 * worker can be extended (database writes, webhooks, analytics) without
 * touching the subscription wiring.
 */

import {
  DOMAIN_EVENT_NAMES,
  defaultWorkerLogger,
  type DomainEvent,
  type DomainEventBus,
  type HandlerRegistration,
  type WorkerLogger,
} from './types';

// ── Payload shapes ────────────────────────────────────────────────────────────

export interface LendingEventPayload {
  eventId: string;
  ledger?: number;
  contract?: string | null;
  actor?: string | null;
  counterparty?: string | null;
  asset?: string | null;
  amount?: string | null;
  timestamp: number;
  module?: string;
  action?: string;
}

export interface FlashLoanEventPayload extends LendingEventPayload {
  phase?: string;
}

// ── Sink interface ────────────────────────────────────────────────────────────

/**
 * Optional side-effect sink injected into lending handlers.
 * Provide a full or partial implementation; unused methods can be omitted.
 */
export interface LendingSink {
  onDeposit?(event: DomainEvent<LendingEventPayload>): Promise<void> | void;
  onWithdraw?(event: DomainEvent<LendingEventPayload>): Promise<void> | void;
  onBorrow?(event: DomainEvent<LendingEventPayload>): Promise<void> | void;
  onRepay?(event: DomainEvent<LendingEventPayload>): Promise<void> | void;
  onFlashLoan?(event: DomainEvent<FlashLoanEventPayload>): Promise<void> | void;
}

// ── Options ───────────────────────────────────────────────────────────────────

export interface RegisterLendingHandlersOptions {
  logger?: WorkerLogger;
  sink?: LendingSink;
}

// ── Factory ───────────────────────────────────────────────────────────────────

/**
 * Subscribe lending-domain handlers to `bus` and return a registration
 * whose `teardown()` unsubscribes all of them.
 */
export function registerLendingHandlers(
  bus: DomainEventBus,
  options: RegisterLendingHandlersOptions = {}
): HandlerRegistration {
  const log = options.logger ?? defaultWorkerLogger;
  const sink = options.sink;

  // ── deposit ──────────────────────────────────────────────────────────────

  const depositSub = bus.subscribe<LendingEventPayload>(
    DOMAIN_EVENT_NAMES.LendingDeposit,
    async (event) => {
      log.info('lending.deposit', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        actor: event.payload.actor,
        asset: event.payload.asset,
        amount: event.payload.amount,
      });
      await sink?.onDeposit?.(event);
    }
  );

  // ── withdraw ─────────────────────────────────────────────────────────────

  const withdrawSub = bus.subscribe<LendingEventPayload>(
    DOMAIN_EVENT_NAMES.LendingWithdraw,
    async (event) => {
      log.info('lending.withdraw', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        actor: event.payload.actor,
        asset: event.payload.asset,
        amount: event.payload.amount,
      });
      await sink?.onWithdraw?.(event);
    }
  );

  // ── borrow ───────────────────────────────────────────────────────────────

  const borrowSub = bus.subscribe<LendingEventPayload>(
    DOMAIN_EVENT_NAMES.LendingBorrow,
    async (event) => {
      log.info('lending.borrow', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        actor: event.payload.actor,
        asset: event.payload.asset,
        amount: event.payload.amount,
      });
      await sink?.onBorrow?.(event);
    }
  );

  // ── repay ────────────────────────────────────────────────────────────────

  const repaySub = bus.subscribe<LendingEventPayload>(
    DOMAIN_EVENT_NAMES.LendingRepay,
    async (event) => {
      log.info('lending.repay', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        actor: event.payload.actor,
        asset: event.payload.asset,
        amount: event.payload.amount,
      });
      await sink?.onRepay?.(event);
    }
  );

  // ── flash_loan ───────────────────────────────────────────────────────────

  const flashLoanSub = bus.subscribe<FlashLoanEventPayload>(
    DOMAIN_EVENT_NAMES.FlashLoan,
    async (event) => {
      log.info('lending.flash_loan', {
        eventId: event.id,
        ledger: event.ledger,
        contract: event.contract,
        actor: event.payload.actor,
        asset: event.payload.asset,
        amount: event.payload.amount,
        phase: event.payload.phase,
      });
      await sink?.onFlashLoan?.(event);
    }
  );

  return {
    name: 'lending',
    teardown() {
      depositSub.unsubscribe();
      withdrawSub.unsubscribe();
      borrowSub.unsubscribe();
      repaySub.unsubscribe();
      flashLoanSub.unsubscribe();
    },
  };
}
