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

## Memoized risk reads (#1774)

Liquidation sweeps and multi-asset health checks used to resolve the same
asset's parameters from the registry once per position. `PoolRiskView`
(`src/risk.rs`) resolves each asset at most once per block height, backed by
`RiskWeightCache` from `contracts/risk`:

```rust
use stellarlend_pool::PoolRiskView;

let mut view = PoolRiskView::new(&config);
for position in positions {
    let limit = view.liquidation_limit(ledger_sequence, position.asset_id, position.value)?;
    // ...
}
let stats = view.stats(); // hits, misses, resets, bypasses, failures
```

- Results are identical to `PoolConfig::borrow_capacity` / `liquidation_limit`.
- The view borrows the config immutably, so parameters cannot change under it.
- Reading at a different height (forward or rewound) drops every cached entry,
  so a long-lived keeper never serves weights from an earlier ledger.
- Unknown assets are cached as absent; if more distinct ids than slots are
  probed in one block, extra reads are computed directly (`bypasses`).

Observability: export `stats().hit_rate_bps()` and `stats().misses` (registry
loads) as metrics; a falling hit rate or rising `bypasses` means a sweep is
touching more distinct assets per block than expected.
