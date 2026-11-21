/**
 * Domain-event bus types.
 *
 * The bus decouples contract/indexer ingestion from off-chain consumers
 * (webhooks, analytics, caches, notifications). Producers publish a
 * DomainEvent; handlers subscribe by event type or '*' for all events.
 */

import type { IndexedEvent } from '../services/eventIndex/types';

export const DOMAIN_EVENT_VERSION = 1 as const;

/** Canonical domain event envelope published on the bus. */
export interface DomainEvent<TPayload = Record<string, unknown>> {
  /** Stable identifier, defaults to the underlying indexed event id. */
  id: string;
  /** Domain event name, e.g. `lending.deposit`, `liquidation.liquidate`. */
  name: string;
  /** Schema version for the envelope itself. */
  version: typeof DOMAIN_EVENT_VERSION;
  /** Unix epoch milliseconds when the domain event was produced. */
  occurredAt: number;
  /** Ledger that produced the event, when known. */
  ledger?: number;
  /** Contract that emitted the source event, when known. */
  contract?: string;
  /** Source indexed event type, e.g. `deposit` or `proto_evt`. */
  sourceType: string;
  /** Original indexed event for traceability and replay. */
  source: IndexedEvent;
  /** Normalized payload for downstream consumers. */
  payload: TPayload;
}

export type DomainEventHandler<TPayload = Record<string, unknown>> = (
  event: DomainEvent<TPayload>
) => void | Promise<void>;

export interface DomainEventSubscription {
  unsubscribe(): void;
}

export interface DomainEventBusStats {
  published: number;
  delivered: number;
  failedHandlers: number;
  activeHandlers: number;
  handlersByEvent: Record<string, number>;
}

export interface PublishResult {
  event: DomainEvent;
  handlersInvoked: number;
  handlerFailures: number;
}

export interface DomainEventBus {
  publish(event: DomainEvent): Promise<PublishResult>;
  subscribe<TPayload = Record<string, unknown>>(
    eventName: string,
    handler: DomainEventHandler<TPayload>
  ): DomainEventSubscription;
  subscribeAll(handler: DomainEventHandler): DomainEventSubscription;
  /** Publish a mapped indexed event. No-op when no mapping exists. */
  publishIndexedEvent(event: IndexedEvent): Promise<PublishResult | null>;
  stats(): DomainEventBusStats;
  clear(): void;
}
