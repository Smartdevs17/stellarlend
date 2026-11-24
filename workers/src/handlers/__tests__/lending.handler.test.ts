/**
 * workers/src/handlers/__tests__/lending.handler.test.ts
 *
 * Unit tests for registerLendingHandlers.
 *
 * Covers every subscription (deposit, withdraw, borrow, repay, flash_loan),
 * confirms the sink is called with the right event, verifies teardown removes
 * subscriptions, and checks that a failing sink does not corrupt the bus.
 */

import { InMemoryDomainEventBus } from '../../../../api/src/events/bus';
import { DOMAIN_EVENT_NAMES } from '../../../../api/src/events/names';
import type { IndexedEvent } from '../../../../api/src/services/eventIndex/types';
import {
  registerLendingHandlers,
  type LendingSink,
  type LendingEventPayload,
  type FlashLoanEventPayload,
} from '../lending.handler';
import type { DomainEvent, WorkerLogger } from '../types';

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: 'evt-001',
    type: 'deposit',
    contract: 'CCONTRACT',
    topic: [],
    data: { asset: 'CUSDC', amount: '500' },
    timestamp: 1_700_000_000_000,
    ledger: 10,
    accounts: ['GACTOR'],
    actor: 'GACTOR',
    envelope: 'typed',
    schemaVersion: 1,
    schemaStatus: 'current',
    asset: 'CUSDC',
    amount: '500',
    ...overrides,
  };
}

function silentLogger(): WorkerLogger {
  return { info: jest.fn(), warn: jest.fn(), error: jest.fn() };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('registerLendingHandlers', () => {
  let bus: InMemoryDomainEventBus;

  beforeEach(() => {
    bus = new InMemoryDomainEventBus();
  });

  afterEach(() => {
    bus.clear();
  });

  // ── deposit ────────────────────────────────────────────────────────────────

  it('calls sink.onDeposit for a deposit event', async () => {
    const onDeposit = jest.fn<void, [DomainEvent<LendingEventPayload>]>();
    const reg = registerLendingHandlers(bus, { logger: silentLogger(), sink: { onDeposit } });

    await bus.publishIndexedEvent(makeEvent({ type: 'deposit', id: 'dep-1' }));

    expect(onDeposit).toHaveBeenCalledTimes(1);
    expect(onDeposit.mock.calls[0]![0].name).toBe(DOMAIN_EVENT_NAMES.LendingDeposit);
    expect(onDeposit.mock.calls[0]![0].payload.asset).toBe('CUSDC');
    expect(onDeposit.mock.calls[0]![0].payload.amount).toBe('500');

    reg.teardown();
  });

  it('logs deposit with structured fields', async () => {
    const log = silentLogger();
    const reg = registerLendingHandlers(bus, { logger: log });

    await bus.publishIndexedEvent(makeEvent({ type: 'deposit', id: 'dep-log' }));

    expect(log.info).toHaveBeenCalledWith(
      'lending.deposit',
      expect.objectContaining({ eventId: 'dep-log', asset: 'CUSDC', amount: '500' })
    );

    reg.teardown();
  });

  // ── withdraw ───────────────────────────────────────────────────────────────

  it('calls sink.onWithdraw for a withdraw event', async () => {
    const onWithdraw = jest.fn<void, [DomainEvent<LendingEventPayload>]>();
    const reg = registerLendingHandlers(bus, { logger: silentLogger(), sink: { onWithdraw } });

    await bus.publishIndexedEvent(makeEvent({ type: 'withdraw', id: 'wd-1' }));

    expect(onWithdraw).toHaveBeenCalledTimes(1);
    expect(onWithdraw.mock.calls[0]![0].name).toBe(DOMAIN_EVENT_NAMES.LendingWithdraw);

    reg.teardown();
  });

  // ── borrow ─────────────────────────────────────────────────────────────────

  it('calls sink.onBorrow for a borrow event', async () => {
    const onBorrow = jest.fn<void, [DomainEvent<LendingEventPayload>]>();
    const reg = registerLendingHandlers(bus, { logger: silentLogger(), sink: { onBorrow } });

    await bus.publishIndexedEvent(makeEvent({ type: 'borrow', id: 'brw-1' }));

    expect(onBorrow).toHaveBeenCalledTimes(1);
    expect(onBorrow.mock.calls[0]![0].name).toBe(DOMAIN_EVENT_NAMES.LendingBorrow);

    reg.teardown();
  });

  // ── repay ──────────────────────────────────────────────────────────────────

  it('calls sink.onRepay for a repay event', async () => {
    const onRepay = jest.fn<void, [DomainEvent<LendingEventPayload>]>();
    const reg = registerLendingHandlers(bus, { logger: silentLogger(), sink: { onRepay } });

    await bus.publishIndexedEvent(makeEvent({ type: 'repay', id: 'rep-1' }));

    expect(onRepay).toHaveBeenCalledTimes(1);
    expect(onRepay.mock.calls[0]![0].name).toBe(DOMAIN_EVENT_NAMES.LendingRepay);

    reg.teardown();
  });

  // ── flash_loan ─────────────────────────────────────────────────────────────

  it('calls sink.onFlashLoan for a flash_loan_initiated event', async () => {
    const onFlashLoan = jest.fn<void, [DomainEvent<FlashLoanEventPayload>]>();
    const reg = registerLendingHandlers(bus, { logger: silentLogger(), sink: { onFlashLoan } });

    await bus.publishIndexedEvent(
      makeEvent({ type: 'flash_loan_initiated', id: 'fl-1' })
    );

    expect(onFlashLoan).toHaveBeenCalledTimes(1);
    expect(onFlashLoan.mock.calls[0]![0].name).toBe(DOMAIN_EVENT_NAMES.FlashLoan);

    reg.teardown();
  });

  // ── sink isolation ─────────────────────────────────────────────────────────

  it('does not propagate a throwing sink — bus records the failure but continues', async () => {
    const onDeposit = jest.fn(() => { throw new Error('sink exploded'); });
    const reg = registerLendingHandlers(bus, {
      logger: silentLogger(),
      sink: { onDeposit },
    });

    const result = await bus.publishIndexedEvent(makeEvent({ type: 'deposit', id: 'dep-err' }));

    // The bus catches handler errors and counts them without re-throwing.
    expect(result?.handlerFailures).toBe(1);
    expect(result?.handlersInvoked).toBe(1);

    reg.teardown();
  });

  // ── teardown ───────────────────────────────────────────────────────────────

  it('teardown removes all subscriptions', async () => {
    const onDeposit = jest.fn();
    const reg = registerLendingHandlers(bus, { logger: silentLogger(), sink: { onDeposit } });

    await bus.publishIndexedEvent(makeEvent({ type: 'deposit', id: 'pre' }));
    expect(onDeposit).toHaveBeenCalledTimes(1);

    reg.teardown();

    await bus.publishIndexedEvent(makeEvent({ type: 'deposit', id: 'post' }));
    // Sink must NOT be called again after teardown.
    expect(onDeposit).toHaveBeenCalledTimes(1);
  });

  it('exposes name "lending"', () => {
    const reg = registerLendingHandlers(bus, { logger: silentLogger() });
    expect(reg.name).toBe('lending');
    reg.teardown();
  });

  // ── no-sink path ───────────────────────────────────────────────────────────

  it('works without a sink (logging-only mode)', async () => {
    const log = silentLogger();
    const reg = registerLendingHandlers(bus, { logger: log });

    // Should not throw.
    await expect(
      bus.publishIndexedEvent(makeEvent({ type: 'deposit', id: 'nk-1' }))
    ).resolves.not.toThrow();

    expect(log.info).toHaveBeenCalled();
    reg.teardown();
  });

  // ── vault_deposit alias ────────────────────────────────────────────────────

  it('handles vault_deposit as a LendingDeposit event', async () => {
    const onDeposit = jest.fn();
    const reg = registerLendingHandlers(bus, { logger: silentLogger(), sink: { onDeposit } });

    await bus.publishIndexedEvent(makeEvent({ type: 'vault_deposit', id: 'vd-1' }));

    expect(onDeposit).toHaveBeenCalledTimes(1);
    reg.teardown();
  });
});
