/**
 * backend/src/events/bus.ts
 *
 * Canonical domain-event bus for the backend layer.
 *
 * This module re-exports the core bus types and the process-wide singleton
 * from api/src/events so that backend services, workers, and consumers all
 * share one bus instance.  It also provides ObservableDomainEventBus — a
 * thin decorator that layers Prometheus metrics and structured logging on
 * top of any DomainEventBus implementation without touching the core logic.
 *
 * Usage
 * -----
 *   // Publish from anywhere in the backend
 *   import { getBackendEventBus } from 'backend/src/events/bus';
 *   await getBackendEventBus().publishIndexedEvent(rawEvent);
 *
 *   // Inject an observable bus in tests / alternate deployments
 *   import { ObservableDomainEventBus, InMemoryDomainEventBus } from 'backend/src/events/bus';
 *   const bus = new ObservableDomainEventBus(new InMemoryDomainEventBus());
 */

import { Counter, Gauge, Histogram, Registry } from 'prom-client';

// ── Re-exports from the core api package ─────────────────────────────────────
export {
  InMemoryDomainEventBus,
  getDomainEventBus,
  setDomainEventBus,
} from '../../../api/src/events/bus';

export type {
  DomainEvent,
  DomainEventBus,
  DomainEventBusStats,
  DomainEventHandler,
  DomainEventSubscription,
  PublishResult,
} from '../../../api/src/events/types';

export { DOMAIN_EVENT_NAMES } from '../../../api/src/events/names';
export type { DomainEventName } from '../../../api/src/events/names';

export { mapIndexedEventToDomainEvent } from '../../../api/src/events/mapper';

import type {
  DomainEvent,
  DomainEventBus,
  DomainEventBusStats,
  DomainEventHandler,
  DomainEventSubscription,
  PublishResult,
} from '../../../api/src/events/types';
import type { IndexedEvent } from '../../../api/src/services/eventIndex/types';

// ── Lightweight logger (no dependency on api/src/utils/logger) ───────────────

type LogFn = (message: string, meta?: Record<string, unknown>) => void;

export interface BusLogger {
  info: LogFn;
  warn: LogFn;
  error: LogFn;
}

const defaultLogger: BusLogger = {
  info: (msg, meta) =>
    console.log(JSON.stringify({ level: 'info', message: msg, ...meta, ts: Date.now() })),
  warn: (msg, meta) =>
    console.warn(JSON.stringify({ level: 'warn', message: msg, ...meta, ts: Date.now() })),
  error: (msg, meta) =>
    console.error(JSON.stringify({ level: 'error', message: msg, ...meta, ts: Date.now() })),
};

// ── Prometheus metric names ───────────────────────────────────────────────────

const METRIC_PREFIX = 'stellarlend_event_bus';

export interface BusMetrics {
  publishedTotal: Counter<'event_name'>;
  handlerInvocations: Counter<'event_name' | 'outcome'>;
  publishDurationSeconds: Histogram<'event_name'>;
  activeHandlers: Gauge<string>;
}

function createMetrics(registry: Registry): BusMetrics {
  return {
    publishedTotal: new Counter({
      name: `${METRIC_PREFIX}_published_total`,
      help: 'Total domain events published to the bus',
      labelNames: ['event_name'],
      registers: [registry],
    }),
    handlerInvocations: new Counter({
      name: `${METRIC_PREFIX}_handler_invocations_total`,
      help: 'Total handler invocations, labelled by outcome (success|failure)',
      labelNames: ['event_name', 'outcome'],
      registers: [registry],
    }),
    publishDurationSeconds: new Histogram({
      name: `${METRIC_PREFIX}_publish_duration_seconds`,
      help: 'Duration of a single publishIndexedEvent or publish call in seconds',
      labelNames: ['event_name'],
      buckets: [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1],
      registers: [registry],
    }),
    activeHandlers: new Gauge({
      name: `${METRIC_PREFIX}_active_handlers`,
      help: 'Number of currently registered event handlers (all event names combined)',
      registers: [registry],
    }),
  };
}

// ── ObservableDomainEventBus ──────────────────────────────────────────────────

/**
 * Decorator around any DomainEventBus that adds:
 *
 *   - Prometheus counters/histograms for publish rate, handler outcomes,
 *     and publish latency.
 *   - Structured log lines for every publish and every handler failure.
 *   - A gauge tracking the total number of live subscriptions.
 *
 * The underlying bus's behaviour is unchanged; ObservableDomainEventBus
 * never swallows errors — it records them and re-throws.
 */
export class ObservableDomainEventBus implements DomainEventBus {
  private readonly inner: DomainEventBus;
  private readonly metrics: BusMetrics;
  private readonly log: BusLogger;
  private subscriptionCount = 0;

  constructor(
    inner: DomainEventBus,
    options: {
      registry?: Registry;
      logger?: BusLogger;
    } = {}
  ) {
    this.inner = inner;
    this.metrics = createMetrics(options.registry ?? new Registry());
    this.log = options.logger ?? defaultLogger;
  }

  async publish(event: DomainEvent): Promise<PublishResult> {
    const timer = this.metrics.publishDurationSeconds.startTimer({ event_name: event.name });
    try {
      this.log.info('domain_event.publish', {
        eventId: event.id,
        eventName: event.name,
        ledger: event.ledger,
        contract: event.contract,
      });

      const result = await this.inner.publish(event);

      this.metrics.publishedTotal.inc({ event_name: event.name });
      const successes = result.handlersInvoked - result.handlerFailures;
      if (successes > 0) {
        this.metrics.handlerInvocations.inc(
          { event_name: event.name, outcome: 'success' },
          successes
        );
      }
      if (result.handlerFailures > 0) {
        this.metrics.handlerInvocations.inc(
          { event_name: event.name, outcome: 'failure' },
          result.handlerFailures
        );
        this.log.warn('domain_event.handler_failures', {
          eventId: event.id,
          eventName: event.name,
          handlerFailures: result.handlerFailures,
          handlersInvoked: result.handlersInvoked,
        });
      }

      return result;
    } finally {
      timer();
    }
  }

  subscribe<TPayload = Record<string, unknown>>(
    eventName: string,
    handler: DomainEventHandler<TPayload>
  ): DomainEventSubscription {
    this.subscriptionCount += 1;
    this.metrics.activeHandlers.set(this.subscriptionCount);
    this.log.info('domain_event.subscribe', { eventName, totalSubscriptions: this.subscriptionCount });

    const sub = this.inner.subscribe(eventName, handler);
    return {
      unsubscribe: () => {
        this.subscriptionCount = Math.max(0, this.subscriptionCount - 1);
        this.metrics.activeHandlers.set(this.subscriptionCount);
        this.log.info('domain_event.unsubscribe', {
          eventName,
          totalSubscriptions: this.subscriptionCount,
        });
        sub.unsubscribe();
      },
    };
  }

  subscribeAll(handler: DomainEventHandler): DomainEventSubscription {
    return this.subscribe('*', handler);
  }

  async publishIndexedEvent(event: IndexedEvent): Promise<PublishResult | null> {
    const timer = this.metrics.publishDurationSeconds.startTimer({ event_name: event.type });
    try {
      const result = await this.inner.publishIndexedEvent(event);
      if (result) {
        this.metrics.publishedTotal.inc({ event_name: result.event.name });
        const successes = result.handlersInvoked - result.handlerFailures;
        if (successes > 0) {
          this.metrics.handlerInvocations.inc(
            { event_name: result.event.name, outcome: 'success' },
            successes
          );
        }
        if (result.handlerFailures > 0) {
          this.metrics.handlerInvocations.inc(
            { event_name: result.event.name, outcome: 'failure' },
            result.handlerFailures
          );
          this.log.warn('domain_event.indexed_handler_failures', {
            indexedEventId: event.id,
            mappedEventName: result.event.name,
            handlerFailures: result.handlerFailures,
          });
        }
        this.log.info('domain_event.indexed_published', {
          indexedEventId: event.id,
          mappedEventName: result.event.name,
          handlersInvoked: result.handlersInvoked,
          ledger: event.ledger,
        });
      }
      return result;
    } finally {
      timer();
    }
  }

  stats(): DomainEventBusStats {
    return this.inner.stats();
  }

  clear(): void {
    this.inner.clear();
    this.subscriptionCount = 0;
    this.metrics.activeHandlers.set(0);
  }
}

// ── Process-wide backend bus singleton ───────────────────────────────────────

import { getDomainEventBus } from '../../../api/src/events/bus';

/**
 * Returns the process-wide domain-event bus.
 *
 * In production this is the same InMemoryDomainEventBus singleton that the
 * API process uses.  Callers that need observability should wrap it once at
 * bootstrap time:
 *
 *   const bus = new ObservableDomainEventBus(getBackendEventBus(), {
 *     registry: promClientRegistry,
 *     logger: winstonAdapter,
 *   });
 */
export function getBackendEventBus(): DomainEventBus {
  return getDomainEventBus();
}
