# Oracle adapter and mock bindings (Issue #1762)

## Why this shape

The old oracle adapter concept was duplicated across callers: each one knew how
to call a feed, validate a quote, retry a fallback and emit telemetry. A small
change therefore rippled through unrelated files. The adapter is now a trait
with a single binding point:

```text
PriceSource (trait)
├── ContractPriceSource  → registered PriceFeed contract
└── MockPriceSource      → in-memory mock (contracts/mock_oracle)
        │
        ▼
OracleAdapter<Primary, Fallback, Telemetry>
        │
        ├── validates quote shape and staleness
        ├── falls back on source failure
        └── emits success/fallback/failure telemetry
```

## Public API (`contracts/oracle/src/adapter.rs`)

| Item | Purpose |
| --- | --- |
| `PriceQuote` | Normalized price/decimals/timestamp/confidence value shared by all sources. |
| `PriceFeed` / `PriceFeedClient` | Generated client interface for a Soroban contract exposing `get_price`. |
| `PriceSource` | Implemented by contract bindings and mocks. |
| `ContractPriceSource` | Binds to a registered `PriceFeed` contract by address. |
| `OracleAdapter::new(primary, fallback, telemetry)` | Build the adapter; `with_max_staleness(secs)` adds an age bound. |
| `latest(env, asset)` | Return a validated quote or a typed `AdapterError`. |
| `AdapterTelemetry`, `SorobanTelemetry`, `NoopTelemetry` | Observability; events are `oa_ok`, `oa_fb`, `oa_err`. |

Validation rejects non-positive prices, zero timestamps, decimals above 18 and
confidence above 10,000 basis points. Staleness uses the ledger timestamp and a
configurable maximum age; the boundary (`age == max`) is accepted, `age > max`
is rejected. Errors are `SourceUnavailable`, `InvalidQuote`, `StaleQuote` and
`NoSource` with actionable `Display` messages.

## Mock binding (`contracts/mock_oracle/`)

- `MockPriceSource`: in-memory quotes with failure injection for unit tests.
- `MockOracleContract`: a registered contract implementing `PriceFeed` with
  `initialize`, `set_price`, `set_failure` and `get_price`; bind it with
  `ContractPriceSource` for integration tests.
- Primary and fallback sources may be different types, so a real contract
  binding can fall back to an in-memory mock in a test.

## Compatibility and configuration

The crates are self-contained (`[workspace]` in each Cargo.toml) and add no
dependencies to existing workspace members. Existing feeds only need to expose
`get_price(asset) -> PriceQuote`; no shared state format changes. Telemetry is
injected, so hosts can use Soroban events, their own logger, or no-op.

## Validation

- `cargo test --manifest-path contracts/oracle/Cargo.toml` — 5 tests.
- `cargo test --manifest-path contracts/mock_oracle/Cargo.toml` — 4 tests.
