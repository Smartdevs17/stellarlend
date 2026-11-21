import { InMemoryDomainEventBus } from '../bus';
import { DOMAIN_EVENT_NAMES } from '../names';
import type { IndexedEvent } from '../../services/eventIndex/types';

function indexedEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: '000000000001-0000000001',
    type: 'deposit',
    contract: 'CCONTRACT',
    topic: [],
    data: { asset: 'CUSDC', amount: '100' },
    timestamp: 1700000000000,
    ledger: 1,
    accounts: ['GUSER'],
    actor: 'GUSER',
    envelope: 'typed',
    schemaVersion: 1,
    schemaStatus: 'current',
    asset: 'CUSDC',
    amount: '100',
    ...overrides,
  };
}

describe('InMemoryDomainEventBus', () => {
  it('publishes mapped indexed events to specific subscribers', async () => {
    const bus = new InMemoryDomainEventBus();
    const received: string[] = [];
    bus.subscribe(DOMAIN_EVENT_NAMES.LendingDeposit, (event) => {
      received.push(`${event.name}:${event.payload.amount}`);
    });

    const result = await bus.publishIndexedEvent(indexedEvent());

    expect(result?.event.name).toBe(DOMAIN_EVENT_NAMES.LendingDeposit);
    expect(result?.handlersInvoked).toBe(1);
    expect(received).toEqual(['lending.deposit:100']);
  });

  it('supports wildcard subscribers and isolates handler failures', async () => {
    const bus = new InMemoryDomainEventBus();
    const seen: string[] = [];
    bus.subscribeAll((event) => {
      seen.push(event.name);
    });
    bus.subscribe(DOMAIN_EVENT_NAMES.LendingBorrow, () => {
      throw new Error('boom');
    });
    bus.subscribe(DOMAIN_EVENT_NAMES.LendingBorrow, (event) => {
      seen.push(`ok:${event.id}`);
    });

    const result = await bus.publishIndexedEvent(indexedEvent({ type: 'borrow', id: 'e2' }));

    expect(result?.handlerFailures).toBe(1);
    expect(result?.handlersInvoked).toBe(3);
    expect(seen).toEqual(['lending.borrow', 'ok:e2']);
    expect(bus.stats().failedHandlers).toBe(1);
  });

  it('unsubscribes handlers', async () => {
    const bus = new InMemoryDomainEventBus();
    const seen: string[] = [];
    const sub = bus.subscribe(DOMAIN_EVENT_NAMES.LendingRepay, (event) => seen.push(event.id));
    await bus.publishIndexedEvent(indexedEvent({ type: 'repay', id: 'r1' }));
    sub.unsubscribe();
    await bus.publishIndexedEvent(indexedEvent({ type: 'repay', id: 'r2' }));
    expect(seen).toEqual(['r1']);
  });
});
