# contracts/events (issue #1761)

One canonical event shape for pool, risk, and liquidation modules. Today each
module publishes its own struct with different field names, which forces every
indexer to carry a per-module translation table. This crate defines the shared
envelope, stable topics, and validation once.

## Envelope

```
topic, version, module, action, severity, actor, asset, counterparty,
trace_id, timestamp, amount, health_factor_bps, payload
```

- `topic` is derived from `(module, action)`, never hard-coded at call sites.
- `timestamp` is always the ledger/off-chain event time in Unix seconds.
- `trace_id` links related pool → risk → liquidation events for observability.
- `payload` carries the module-specific fields without changing the envelope.

## Usage

```rust
use stellarlend_events::{
    emit, EventAction, EventContext, EventModule, EventPayload, EventSeverity,
    InMemoryTelemetry, new_event,
};

let telemetry = InMemoryTelemetry::default();
let event = new_event(
    EventModule::Pool,
    EventAction::Deposit,
    EventSeverity::Info,
    EventContext {
        actor: "user",
        asset: Some("XLM"),
        counterparty: None,
        trace_id: "tx-1",
        timestamp: 1_700_000_000,
    },
    1_000,
    None,
    EventPayload::Pool {
        balance_after: 1_000,
        debt_after: 0,
    },
);
// emit(&sink, &telemetry, &event)?;
```

`contracts/pool/src/events.rs` contains the typed builders for all three
modules. The `EventSink` and `EventTelemetry` traits let the contract layer
decide how to publish and how to expose counters, while validation stays here.

## Validation

```bash
cargo test --manifest-path contracts/events/Cargo.toml
```
