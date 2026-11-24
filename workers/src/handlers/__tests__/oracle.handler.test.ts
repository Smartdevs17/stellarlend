/**
 * workers/src/handlers/__tests__/oracle.handler.test.ts
 *
 * Unit tests for registerOracleHandler.
 *
 * Covers: normal price update, large-move threshold warning, non-numeric
 * amount edge case, sink isolation, teardown, and logging-only mode.
 */

import { InMemoryDomainEventBus } from '../../../../api/src/events/bus';
import { DOMAIN_EVENT_NAMES } from '../../../../api/src/events/names';
import type { IndexedEvent } from '../../../../api/src/services/eventIndex/types';
import {
  registerOracleHandler,
  type OracleSink,
  type OraclePriceUpdatedPayload,
} from '../oracle.handler';
import type { DomainEvent, WorkerLogger } from '../types';

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: 'orc-001',
    type: 'price_updated',
    contract: 'CORACLE',
    topic: [],
    data: { asset: 'CUSDC', price: '1000000' },
    timestamp: 1_700_000_000_000,
    ledger: 50,
    accounts: [],
    envelope: 'typed',
    schemaVersion: 1,
    schemaStatus: 'current',
    asset: 'CUSDC',
    amount: '1000000',
    ...overrides,
  };
}

function silentLogger(): WorkerLogger {
  return { info: jest.fn(), warn: jest.fn(), error: jest.fn() };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('registerOracleHandler', () => {
  let bus: InMemoryDomainEventBus;

  beforeEach(() => {
    bus = new InMemoryDomainEventBus();
  });

  afterEach(() => {
    bus.clear();
  });

  it('calls sink.onPriceUpdated for a price_updated event', async () => {
    const onPriceUpdated = jest.fn<void, [DomainEvent<OraclePriceUpdatedPayload>]>();
    const reg = registerOracleHandler(bus, { logger: silentLogger(), sink: { onPriceUpdated } });

    await bus.publishIndexedEvent(makeEvent());

    expect(onPriceUpdated).toHaveBeenCalledTimes(1);
    const evt = onPriceUpdated.mock.calls[0]![0];
    expect(evt.name).toBe(DOMAIN_EVENT_NAMES.OraclePriceUpdated);
    expect(evt.payload.asset).toBe('CUSDC');
    expect(evt.payload.amount).toBe('1000000');

    reg.teardown();
  });

  it('logs price update with asset and price fields', async () => {
    const log = silentLogger();
    const reg = registerOracleHandler(bus, { logger: log });

    await bus.publishIndexedEvent(makeEvent({ id: 'orc-log' }));

    expect(log.info).toHaveBeenCalledWith(
      'oracle.price_updated',
      expect.objectContaining({ eventId: 'orc-log', asset: 'CUSDC', price: '1000000' })
    );

    reg.teardown();
  });

  it('warns on a large price move when threshold is set', async () => {
    const log = silentLogger();
    const reg = registerOracleHandler(bus, {
      logger: log,
      largeMoveThreshold: 500_000n,
    });

    // amount = 1_000_000 > 500_000 → should warn
    await bus.publishIndexedEvent(makeEvent({ id: 'orc-big', amount: '1000000' }));

    expect(log.warn).toHaveBeenCalledWith(
      'oracle.large_price_move',
      expect.objectContaining({ eventId: 'orc-big', price: '1000000' })
    );

    reg.teardown();
  });

  it('does NOT warn when price is below threshold', async () => {
    const log = silentLogger();
    const reg = registerOracleHandler(bus, {
      logger: log,
      largeMoveThreshold: 2_000_000n,
    });

    // amount = 1_000_000 < 2_000_000 → no warn
    await bus.publishIndexedEvent(makeEvent({ id: 'orc-small', amount: '1000000' }));

    expect(log.warn).not.toHaveBeenCalled();
    reg.teardown();
  });

  it('does NOT warn when threshold is 0 (default disabled)', async () => {
    const log = silentLogger();
    const reg = registerOracleHandler(bus, { logger: log });

    await bus.publishIndexedEvent(makeEvent({ id: 'orc-zero', amount: '999999999999' }));

    expect(log.warn).not.toHaveBeenCalled();
    reg.teardown();
  });

  it('does not throw when amount is a non-numeric string', async () => {
    const log = silentLogger();
    const reg = registerOracleHandler(bus, {
      logger: log,
      largeMoveThreshold: 1n,
    });

    // Amount is not parseable as BigInt — handler should silently skip threshold check.
    await expect(
      bus.publishIndexedEvent(makeEvent({ id: 'orc-nan', amount: 'not-a-number' }))
    ).resolves.not.toThrow();

    expect(log.warn).not.toHaveBeenCalled();
    reg.teardown();
  });

  it('does not propagate a throwing sink', async () => {
    const onPriceUpdated = jest.fn(() => { throw new Error('sink error'); });
    const reg = registerOracleHandler(bus, {
      logger: silentLogger(),
      sink: { onPriceUpdated },
    });

    const result = await bus.publishIndexedEvent(makeEvent({ id: 'orc-err' }));

    expect(result?.handlerFailures).toBe(1);
    reg.teardown();
  });

  it('teardown removes the subscription', async () => {
    const onPriceUpdated = jest.fn();
    const reg = registerOracleHandler(bus, {
      logger: silentLogger(),
      sink: { onPriceUpdated },
    });

    await bus.publishIndexedEvent(makeEvent({ id: 'before' }));
    expect(onPriceUpdated).toHaveBeenCalledTimes(1);

    reg.teardown();

    await bus.publishIndexedEvent(makeEvent({ id: 'after' }));
    expect(onPriceUpdated).toHaveBeenCalledTimes(1);
  });

  it('exposes name "oracle"', () => {
    const reg = registerOracleHandler(bus, { logger: silentLogger() });
    expect(reg.name).toBe('oracle');
    reg.teardown();
  });
});
