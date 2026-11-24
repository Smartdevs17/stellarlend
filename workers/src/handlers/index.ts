/**
 * workers/src/handlers/index.ts
 *
 * Registers every domain-event handler on the supplied bus and returns a
 * single teardown function that unsubscribes them all.
 *
 * Usage
 * -----
 *   import { registerAllHandlers } from 'workers/src/handlers';
 *   import { getDomainEventBus } from 'api/src/events';
 *
 *   const { teardown } = registerAllHandlers(getDomainEventBus(), {
 *     logger: winstonLogger,
 *     sinks: { lending: myLendingSink, liquidation: myLiqSink },
 *   });
 *
 *   // On graceful shutdown:
 *   teardown();
 */

import { registerLendingHandlers } from './lending.handler';
import { registerLiquidationHandler } from './liquidation.handler';
import { registerOracleHandler } from './oracle.handler';
import { registerRiskHandler } from './risk.handler';
import { registerProtocolHandler } from './protocol.handler';

import type { LendingSink } from './lending.handler';
import type { LiquidationSink } from './liquidation.handler';
import type { OracleSink } from './oracle.handler';
import type { RiskSink } from './risk.handler';
import type { ProtocolSink } from './protocol.handler';
import type { DomainEventBus, HandlerRegistration, WorkerLogger } from './types';

// ── Public re-exports so consumers import from one place ─────────────────────

export * from './types';
export * from './lending.handler';
export * from './liquidation.handler';
export * from './oracle.handler';
export * from './risk.handler';
export * from './protocol.handler';

// ── Aggregate options ─────────────────────────────────────────────────────────

export interface AllHandlerSinks {
  lending?: LendingSink;
  liquidation?: LiquidationSink;
  oracle?: OracleSink;
  risk?: RiskSink;
  protocol?: ProtocolSink;
}

export interface RegisterAllHandlersOptions {
  logger?: WorkerLogger;
  sinks?: AllHandlerSinks;
  /**
   * Warn on oracle price moves above this threshold (passed to
   * registerOracleHandler).  Defaults to 0n (disabled).
   */
  oracleLargeMoveThreshold?: bigint;
}

// ── Aggregate result ──────────────────────────────────────────────────────────

export interface AllHandlerRegistration {
  /** Individual registrations, keyed by handler name. */
  registrations: Record<string, HandlerRegistration>;
  /** Unsubscribe every registered handler from the bus. */
  teardown(): void;
}

// ── Factory ───────────────────────────────────────────────────────────────────

/**
 * Wire every domain-event handler to `bus` in one call.
 *
 * Returns `{ registrations, teardown }`.  Call `teardown()` on graceful
 * shutdown or in test `afterEach` hooks to prevent subscription leaks.
 */
export function registerAllHandlers(
  bus: DomainEventBus,
  options: RegisterAllHandlersOptions = {}
): AllHandlerRegistration {
  const { logger, sinks = {}, oracleLargeMoveThreshold } = options;

  const all: HandlerRegistration[] = [
    registerLendingHandlers(bus, { logger, sink: sinks.lending }),
    registerLiquidationHandler(bus, { logger, sink: sinks.liquidation }),
    registerOracleHandler(bus, { logger, sink: sinks.oracle, largeMoveThreshold: oracleLargeMoveThreshold }),
    registerRiskHandler(bus, { logger, sink: sinks.risk }),
    registerProtocolHandler(bus, { logger, sink: sinks.protocol }),
  ];

  const registrations = Object.fromEntries(all.map((r) => [r.name, r]));

  return {
    registrations,
    teardown() {
      for (const reg of all) {
        reg.teardown();
      }
    },
  };
}
