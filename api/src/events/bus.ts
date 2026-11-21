import logger from '../utils/logger';
import type { IndexedEvent } from '../services/eventIndex/types';
import { mapIndexedEventToDomainEvent } from './mapper';
import type {
  DomainEvent,
  DomainEventBus,
  DomainEventBusStats,
  DomainEventHandler,
  DomainEventSubscription,
  PublishResult,
} from './types';

type AnyHandler = DomainEventHandler<any>;

export class InMemoryDomainEventBus implements DomainEventBus {
  private readonly handlers = new Map<string, Set<AnyHandler>>();
  private published = 0;
  private delivered = 0;
  private failedHandlers = 0;

  async publish(event: DomainEvent): Promise<PublishResult> {
    this.published += 1;
    const specific = this.handlers.get(event.name) ?? new Set<AnyHandler>();
    const all = this.handlers.get('*') ?? new Set<AnyHandler>();
    const handlers = [...specific, ...all];

    let handlerFailures = 0;
    await Promise.all(
      handlers.map(async (handler) => {
        try {
          await handler(event);
          this.delivered += 1;
        } catch (error) {
          handlerFailures += 1;
          this.failedHandlers += 1;
          logger.warn('Domain event handler failed', {
            eventId: event.id,
            eventName: event.name,
            error: (error as Error).message,
          });
        }
      })
    );

    return { event, handlersInvoked: handlers.length, handlerFailures };
  }

  subscribe<TPayload = Record<string, unknown>>(
    eventName: string,
    handler: DomainEventHandler<TPayload>
  ): DomainEventSubscription {
    const key = eventName || '*';
    const set = this.handlers.get(key) ?? new Set<AnyHandler>();
    set.add(handler as AnyHandler);
    this.handlers.set(key, set);
    return {
      unsubscribe: () => {
        const current = this.handlers.get(key);
        if (!current) return;
        current.delete(handler as AnyHandler);
        if (current.size === 0) this.handlers.delete(key);
      },
    };
  }

  subscribeAll(handler: DomainEventHandler): DomainEventSubscription {
    return this.subscribe('*', handler);
  }

  async publishIndexedEvent(event: IndexedEvent): Promise<PublishResult | null> {
    const mapped = mapIndexedEventToDomainEvent(event);
    if (!mapped) return null;
    return this.publish(mapped);
  }

  stats(): DomainEventBusStats {
    let activeHandlers = 0;
    const handlersByEvent: Record<string, number> = {};
    for (const [name, set] of this.handlers) {
      handlersByEvent[name] = set.size;
      activeHandlers += set.size;
    }
    return {
      published: this.published,
      delivered: this.delivered,
      failedHandlers: this.failedHandlers,
      activeHandlers,
      handlersByEvent,
    };
  }

  clear(): void {
    this.handlers.clear();
    this.published = 0;
    this.delivered = 0;
    this.failedHandlers = 0;
  }
}

let defaultBus: DomainEventBus | null = null;

export function getDomainEventBus(): DomainEventBus {
  defaultBus ??= new InMemoryDomainEventBus();
  return defaultBus;
}

export function setDomainEventBus(bus: DomainEventBus | null): void {
  defaultBus = bus;
}
