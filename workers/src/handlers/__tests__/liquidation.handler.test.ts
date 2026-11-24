/**
 * workers/src/handlers/__tests__/liquidation.handler.test.ts
 *
 * Unit tests for registerLiquidationHandler.
 *
 * Covers: normal liquidation, large-liquidation warning, sink isolation,
 * teardown, and no-sink logging-only mode.
 */

import { InMemoryDomainEventBus } from '../../../../api/src/events/bus';
import { DOMAIN_EVENT_NAMES } from '../../../../api/src/events/names';
import type { IndexedEvent } from '../../../../api/src/services/eventIndex/types';
import {
  registerLiquidationHandler,
  type LiquidationSink,
  type LiquidationEventPayload,
} from '../liquidation.handler';
import type { DomainEvent, WorkerLogger } from '../types';

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: 'liq-001',
    type: 'liquidation',
    contract: 'CCONTRACT',
    topic: [],
    data: {
      debt_asset: 'CUSDC',
      debt_liquidated: '200',
      collateral_asset: 'CXLM',
      collateral_seized: '10000',
    },
    timestamp: 1_700_000_000_000,
    ledger: 20,
    accounts: ['GLIQUIDATOR', 'GBORROWER'],
    actor: 'GLIQUIDATOR',
    counterparty: 'GBORROWER',
    envelope: 'typed',
    schemaVersion: 1,
    schemaStatus: 'current',
    ...overrides,
  };
}

function silentLogger(): WorkerLogger {
  return { info: jest.fn(), warn: jest.fn(), error: jest.fn() };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('registerLiquidationHandler', () => {
  let bus: InMemoryDomainEventBus;

  beforeEach(() => {
    bus = new InMemoryDomainEventBus();
  });

  afterEach(() => {
    bus.clear();
  });

  it('calls sink.onLiquidation for a liquidation event', async () => {
    const onLiquidation = jest.fn<void, [DomainEvent<LiquidationEventPayload>]>();
    const reg = registerLiquidationHandler(bus, {
      logger: silentLogger(),
      sink: { onLiquidation },
    });

    await bus.publishIndexedEvent(makeEvent());

    expect(onLiquidation).toHaveBeenCalledTimes(1);
    const evt = onLiquidation.mock.calls[0]![0];
    expect(evt.name).toBe(DOMAIN_EVENT_NAMES.Liquidation);
    expect(evt.payload.debtAsset).toBe('CUSDC');
    expect(evt.payload.debtAmount).toBe('200');
    expect(evt.payload.collateralAsset).toBe('CXLM');
    expect(evt.payload.collateralAmount).toBe('10000');

    reg.teardown();
  });

  it('logs liquidation with liquidator and borrower fields', async () => {
    const log = silentLogger();
    const reg = registerLiquidationHandler(bus, { logger: log });

    await bus.publishIndexedEvent(makeEvent({ id: 'liq-log' }));

    expect(log.info).toHaveBeenCalledWith(
      'liquidation.liquidate',
      expect.objectContaining({
        eventId: 'liq-log',
        liquidator: 'GLIQUIDATOR',
        borrower: 'GBORROWER',
      })
    );

    reg.teardown();
  });

  it('emits a large_liquidation warning when debtAmount exceeds 1e12', async () => {
    const log = silentLogger();
    const reg = registerLiquidationHandler(bus, { logger: log });

    // 1_500_000_000_000 > 1_000_000_000_000
    await bus.publishIndexedEvent(
      makeEvent({
        id: 'liq-big',
        data: {
          debt_asset: 'CUSDC',
          debt_liquidated: '1500000000000',
          collateral_asset: 'CXLM',
          collateral_seized: '75000000000000',
        },
      })
    );

    expect(log.warn).toHaveBeenCalledWith(
      'liquidation.large_liquidation',
      expect.objectContaining({ eventId: 'liq-big', debtAmount: '1500000000000' })
    );

    reg.teardown();
  });

  it('does NOT warn for a normal-sized liquidation', async () => {
    const log = silentLogger();
    const reg = registerLiquidationHandler(bus, { logger: log });

    await bus.publishIndexedEvent(makeEvent({ id: 'liq-small' }));

    expect(log.warn).not.toHaveBeenCalled();
    reg.teardown();
  });

  it('does not propagate a throwing sink', async () => {
    const onLiquidation = jest.fn(() => { throw new Error('sink error'); });
    const reg = registerLiquidationHandler(bus, {
      logger: silentLogger(),
      sink: { onLiquidation },
    });

    const result = await bus.publishIndexedEvent(makeEvent({ id: 'liq-err' }));

    expect(result?.handlerFailures).toBe(1);
    reg.teardown();
  });

  it('teardown removes the subscription', async () => {
    const onLiquidation = jest.fn();
    const reg = registerLiquidationHandler(bus, {
      logger: silentLogger(),
      sink: { onLiquidation },
    });

    await bus.publishIndexedEvent(makeEvent({ id: 'before' }));
    expect(onLiquidation).toHaveBeenCalledTimes(1);

    reg.teardown();

    await bus.publishIndexedEvent(makeEvent({ id: 'after' }));
    expect(onLiquidation).toHaveBeenCalledTimes(1);
  });

  it('exposes name "liquidation"', () => {
    const reg = registerLiquidationHandler(bus, { logger: silentLogger() });
    expect(reg.name).toBe('liquidation');
    reg.teardown();
  });

  it('works without a sink (logging-only mode)', async () => {
    const log = silentLogger();
    const reg = registerLiquidationHandler(bus, { logger: log });

    await expect(bus.publishIndexedEvent(makeEvent())).resolves.not.toThrow();
    expect(log.info).toHaveBeenCalled();
    reg.teardown();
  });
});
