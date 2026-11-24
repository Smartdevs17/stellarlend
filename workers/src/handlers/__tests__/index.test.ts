/**
 * workers/src/handlers/__tests__/index.test.ts
 *
 * Integration test: wire the real InMemoryDomainEventBus through
 * registerAllHandlers, publish one event per domain, verify every sink
 * fires exactly once, then confirm teardown silences all subscriptions.
 */

import { InMemoryDomainEventBus } from '../../../../api/src/events/bus';
import { DOMAIN_EVENT_NAMES } from '../../../../api/src/events/names';
import type { IndexedEvent } from '../../../../api/src/services/eventIndex/types';
import { registerAllHandlers } from '../index';
import type { WorkerLogger } from '../types';
import type { LendingSink } from '../lending.handler';
import type { LiquidationSink } from '../liquidation.handler';
import type { OracleSink } from '../oracle.handler';
import type { RiskSink } from '../risk.handler';
import type { ProtocolSink } from '../protocol.handler';

// ── Helpers ───────────────────────────────────────────────────────────────────

function base(overrides: Partial<IndexedEvent>): IndexedEvent {
  return {
    id: 'e-base',
    type: 'deposit',
    contract: 'CCONTRACT',
    topic: [],
    data: {},
    timestamp: 1_700_000_000_000,
    ledger: 1,
    accounts: ['GACTOR'],
    actor: 'GACTOR',
    envelope: 'typed',
    schemaVersion: 1,
    schemaStatus: 'current',
    asset: 'CUSDC',
    amount: '100',
    ...overrides,
  };
}

function silentLogger(): WorkerLogger {
  return { info: jest.fn(), warn: jest.fn(), error: jest.fn() };
}

// ── Integration tests ─────────────────────────────────────────────────────────

describe('registerAllHandlers (integration)', () => {
  let bus: InMemoryDomainEventBus;

  beforeEach(() => {
    bus = new InMemoryDomainEventBus();
  });

  afterEach(() => {
    bus.clear();
  });

  it('routes each domain event to its dedicated sink', async () => {
    // ── build sink mocks ─────────────────────────────────────────────────────

    const lendingSink: LendingSink = {
      onDeposit: jest.fn(),
      onWithdraw: jest.fn(),
      onBorrow: jest.fn(),
      onRepay: jest.fn(),
      onFlashLoan: jest.fn(),
    };

    const liquidationSink: LiquidationSink = {
      onLiquidation: jest.fn(),
    };

    const oracleSink: OracleSink = {
      onPriceUpdated: jest.fn(),
    };

    const riskSink: RiskSink = {
      onRiskParamsUpdated: jest.fn(),
    };

    const protocolSink: ProtocolSink = {
      onProtocolPauseChanged: jest.fn(),
    };

    const { teardown } = registerAllHandlers(bus, {
      logger: silentLogger(),
      sinks: { lending: lendingSink, liquidation: liquidationSink, oracle: oracleSink, risk: riskSink, protocol: protocolSink },
    });

    // ── publish one event per domain ─────────────────────────────────────────

    await bus.publishIndexedEvent(base({ type: 'deposit',          id: 'e-dep' }));
    await bus.publishIndexedEvent(base({ type: 'withdraw',         id: 'e-wd'  }));
    await bus.publishIndexedEvent(base({ type: 'borrow',           id: 'e-brw' }));
    await bus.publishIndexedEvent(base({ type: 'repay',            id: 'e-rep' }));
    await bus.publishIndexedEvent(base({ type: 'flash_loan_initiated', id: 'e-fl' }));
    await bus.publishIndexedEvent(base({ type: 'liquidation',      id: 'e-liq',
      counterparty: 'GBORROWER',
      data: { debt_asset: 'CUSDC', debt_liquidated: '50', collateral_asset: 'CXLM', collateral_seized: '250' },
    }));
    await bus.publishIndexedEvent(base({ type: 'price_updated',    id: 'e-prc', contract: 'CORACLE' }));
    await bus.publishIndexedEvent(base({ type: 'risk_params_updated', id: 'e-risk' }));
    await bus.publishIndexedEvent(base({ type: 'pause_state_changed', id: 'e-pause',
      envelope: 'structured_event_v1', action: 'pause',
    }));

    // ── assertions ───────────────────────────────────────────────────────────

    expect(lendingSink.onDeposit).toHaveBeenCalledTimes(1);
    expect(lendingSink.onWithdraw).toHaveBeenCalledTimes(1);
    expect(lendingSink.onBorrow).toHaveBeenCalledTimes(1);
    expect(lendingSink.onRepay).toHaveBeenCalledTimes(1);
    expect(lendingSink.onFlashLoan).toHaveBeenCalledTimes(1);
    expect(liquidationSink.onLiquidation).toHaveBeenCalledTimes(1);
    expect(oracleSink.onPriceUpdated).toHaveBeenCalledTimes(1);
    expect(riskSink.onRiskParamsUpdated).toHaveBeenCalledTimes(1);
    expect(protocolSink.onProtocolPauseChanged).toHaveBeenCalledTimes(1);

    teardown();
  });

  it('teardown silences all handlers — subsequent events do not reach sinks', async () => {
    const onDeposit = jest.fn();
    const onLiquidation = jest.fn();

    const { teardown } = registerAllHandlers(bus, {
      logger: silentLogger(),
      sinks: {
        lending: { onDeposit },
        liquidation: { onLiquidation },
      },
    });

    // Fire once before teardown.
    await bus.publishIndexedEvent(base({ type: 'deposit',    id: 'pre-dep' }));
    await bus.publishIndexedEvent(base({ type: 'liquidation', id: 'pre-liq',
      data: { debt_liquidated: '10' },
    }));

    expect(onDeposit).toHaveBeenCalledTimes(1);
    expect(onLiquidation).toHaveBeenCalledTimes(1);

    teardown();

    // Fire again after teardown — sinks must stay at 1.
    await bus.publishIndexedEvent(base({ type: 'deposit',    id: 'post-dep' }));
    await bus.publishIndexedEvent(base({ type: 'liquidation', id: 'post-liq',
      data: { debt_liquidated: '10' },
    }));

    expect(onDeposit).toHaveBeenCalledTimes(1);
    expect(onLiquidation).toHaveBeenCalledTimes(1);
  });

  it('registrations map contains one entry per handler group', () => {
    const { registrations, teardown } = registerAllHandlers(bus, { logger: silentLogger() });

    expect(Object.keys(registrations).sort()).toEqual(
      ['lending', 'liquidation', 'oracle', 'protocol', 'risk']
    );

    teardown();
  });

  it('a failing handler in one group does not prevent other groups from running', async () => {
    const onLiquidation = jest.fn();

    const { teardown } = registerAllHandlers(bus, {
      logger: silentLogger(),
      sinks: {
        // Deposit sink throws — liquidation sink must still run.
        lending: { onDeposit: () => { throw new Error('deposit sink boom'); } },
        liquidation: { onLiquidation },
      },
    });

    // Publish deposit — lending handler throws, bus records failure but continues.
    const depResult = await bus.publishIndexedEvent(base({ type: 'deposit', id: 'x-dep' }));
    expect(depResult?.handlerFailures).toBe(1);

    // Liquidation is a separate subscription and must be unaffected.
    await bus.publishIndexedEvent(base({ type: 'liquidation', id: 'x-liq',
      data: { debt_liquidated: '10' },
    }));
    expect(onLiquidation).toHaveBeenCalledTimes(1);

    teardown();
  });

  it('bus stats reflect all published events', async () => {
    const { teardown } = registerAllHandlers(bus, { logger: silentLogger() });

    await bus.publishIndexedEvent(base({ type: 'deposit', id: 'stat-1' }));
    await bus.publishIndexedEvent(base({ type: 'borrow',  id: 'stat-2' }));

    const stats = bus.stats();
    expect(stats.published).toBe(2);
    expect(stats.delivered).toBeGreaterThanOrEqual(2);

    teardown();
  });

  it('oracleLargeMoveThreshold is forwarded to the oracle handler', async () => {
    const log = silentLogger();
    const { teardown } = registerAllHandlers(bus, {
      logger: log,
      oracleLargeMoveThreshold: 500_000n,
    });

    await bus.publishIndexedEvent(
      base({ type: 'price_updated', id: 'orc-big', amount: '999999999', contract: 'CORACLE' })
    );

    expect(log.warn).toHaveBeenCalledWith(
      'oracle.large_price_move',
      expect.objectContaining({ eventId: 'orc-big' })
    );

    teardown();
  });
});
