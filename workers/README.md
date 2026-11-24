# stellarlend-workers

Indexer-worker domain-event handlers for StellarLend (issue [#1763]).

## Overview

The `workers` package subscribes to the domain-event bus and processes every
contract event emitted by the Soroban indexer pipeline.  It sits between the
`api` indexer (which publishes) and downstream side-effects (database writes,
webhooks, analytics, alert pipelines) which plug in through **sink interfaces**.

```
Soroban RPC
    │
    ▼
EventIndexer (api/src/services/eventIndex)
    │  publishIndexedEvent()
    ▼
InMemoryDomainEventBus (api/src/events/bus)
    │  subscribe()
    ▼
registerAllHandlers()   ◄──  workers/src/handlers/index.ts
    ├── lending    (deposit · withdraw · borrow · repay · flash_loan)
    ├── liquidation
    ├── oracle     (price_updated)
    ├── risk       (params_updated)
    └── protocol   (pause_state_changed)
         │
         ▼
      LendingSink / LiquidationSink / … (injected by the caller)
```

## Package structure

```
workers/
├── src/
│   └── handlers/
│       ├── types.ts               # WorkerLogger, HandlerRegistration, re-exports
│       ├── lending.handler.ts     # deposit · withdraw · borrow · repay · flash_loan
│       ├── liquidation.handler.ts # liquidation.liquidate + large-liquidation warn
│       ├── oracle.handler.ts      # oracle.price_updated + large-move threshold
│       ├── risk.handler.ts        # risk.params_updated (full audit log)
│       ├── protocol.handler.ts    # protocol.pause_state_changed (always WARN)
│       ├── index.ts               # registerAllHandlers() — wires every handler
│       └── __tests__/
│           ├── lending.handler.test.ts
│           ├── liquidation.handler.test.ts
│           ├── oracle.handler.test.ts
│           └── index.test.ts      # integration: bus → all handlers → sinks
├── package.json
├── tsconfig.json
└── README.md
```

## Quick start

```ts
import { getDomainEventBus } from 'api/src/events';
import { registerAllHandlers } from 'workers/src/handlers';

const { teardown } = registerAllHandlers(getDomainEventBus(), {
  logger: winstonLogger,          // optional; defaults to structured console
  sinks: {
    lending:     myLendingStore,  // implements LendingSink
    liquidation: myLiqStore,      // implements LiquidationSink
    oracle:      myPriceFeed,     // implements OracleSink
    risk:        myRiskAudit,     // implements RiskSink
    protocol:    myAlertPager,    // implements ProtocolSink
  },
  oracleLargeMoveThreshold: 500_000n,  // optional; warn on big price moves
});

// On graceful shutdown:
process.on('SIGTERM', teardown);
```

Each sink interface only requires the methods you care about — partial
implementations are fine.

## Sink interfaces

| Handler | Sink interface | Methods |
|---------|---------------|---------|
| `lending.handler.ts` | `LendingSink` | `onDeposit` · `onWithdraw` · `onBorrow` · `onRepay` · `onFlashLoan` |
| `liquidation.handler.ts` | `LiquidationSink` | `onLiquidation` |
| `oracle.handler.ts` | `OracleSink` | `onPriceUpdated` |
| `risk.handler.ts` | `RiskSink` | `onRiskParamsUpdated` |
| `protocol.handler.ts` | `ProtocolSink` | `onProtocolPauseChanged` |

## Observability

Every handler emits structured log lines via the injected `WorkerLogger`.

| Event | Log level | Log key |
|-------|-----------|---------|
| Any lending op | `info` | `lending.<action>` |
| Liquidation | `info` | `liquidation.liquidate` |
| Liquidation > 1 × 10¹² base units | `warn` | `liquidation.large_liquidation` |
| Oracle price update | `info` | `oracle.price_updated` |
| Oracle move > threshold | `warn` | `oracle.large_price_move` |
| Risk params change | `info` | `risk.params_updated` |
| Protocol pause/unpause | **`warn`** | `protocol.pause_state_changed` |

For process-wide Prometheus metrics (publish rate, handler latency, failure
counts) wrap the bus with `ObservableDomainEventBus` from
`backend/src/events/bus.ts` before passing it to `registerAllHandlers`.

```ts
import { ObservableDomainEventBus, getDomainEventBus } from 'backend/src/events/bus';
import { register } from 'prom-client';

const bus = new ObservableDomainEventBus(getDomainEventBus(), {
  registry: register,
  logger:   winstonLogger,
});

registerAllHandlers(bus, { logger: winstonLogger, sinks: { … } });
```

Exposed Prometheus metrics:

| Metric | Type | Labels |
|--------|------|--------|
| `stellarlend_event_bus_published_total` | Counter | `event_name` |
| `stellarlend_event_bus_handler_invocations_total` | Counter | `event_name`, `outcome` (`success`\|`failure`) |
| `stellarlend_event_bus_publish_duration_seconds` | Histogram | `event_name` |
| `stellarlend_event_bus_active_handlers` | Gauge | — |

## Running tests

```bash
# from the repo root
npm test -w stellarlend-workers

# or inside this package
npm test
```

Coverage thresholds: **90 % lines / functions / statements**, **80 % branches**.

## Related

- `api/src/events/` — core bus implementation (`InMemoryDomainEventBus`)
- `api/src/services/eventIndex/` — indexer that feeds the bus
- `backend/src/events/bus.ts` — `ObservableDomainEventBus` + Prometheus metrics
- Issue [#1763] — domain-event bus between contracts and indexer worker
