# contracts/pool (issues #1760 and #1761)

The pool is configured by one `PoolConfig` value that embeds a typed
`ParameterRegistry`. Per-asset risk parameters are no longer scattered across
independent storage keys, and a failed update cannot leave a half-written
entry.

`events.rs` is the single place where pool, risk, and liquidation payloads are
built. All builders return the shared `NormalizedEvent` envelope from
`contracts/events`, so topics, timestamps, actors, and trace ids are identical
across modules.

```rust
use stellarlend_pool::{config::PoolConfig, events};
use stellarlend_events::{InMemoryTelemetry, EventSink};

let mut config = PoolConfig::new(1, 1, 7_500, 8_300, 1_000, 5_000, 500, 1, 1_700_000_000)?;
config.register_asset(1, 7_500, 8_300, 1_000, 0, 1)?;

let event = events::deposit(
    events::context("user", Some("XLM"), None, "tx-1", 1_700_000_000),
    1_000,
    1_000,
    0,
);
```

Observability: every event carries a `trace_id` and `timestamp`; publishing
goes through `EventSink` and outcomes are recorded by `EventTelemetry`.

## Validation

```bash
cargo test --manifest-path contracts/pool/Cargo.toml
```
