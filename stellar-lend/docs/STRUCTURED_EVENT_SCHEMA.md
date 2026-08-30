# Structured Event Schema (Issue #824)

Refactors protocol event emission onto a single, versioned, self-describing
schema so off-chain consumers (indexers, dashboards, webhooks) can track state
changes without hard-coding every bespoke event name.

## Layers

| Layer | Where | Purpose |
| --- | --- | --- |
| **Typed events** | `contracts/hello-world/src/events.rs` (`DepositEvent`, `LiquidationEvent`, …) | Source of truth for payload detail. Unchanged. |
| **Structured envelope** | `events::StructuredEventV1` | One uniform, versioned event emitted *alongside* the typed events. |
| **Off-chain catalog** | `api/src/routes/events.ts` → `GET /api/events/schema` | Machine-readable mirror of the on-chain schema. |

The structured layer is **additive**. Existing `emit_*` helpers and their event
output are untouched, so current consumers are unaffected (no regression).

## `StructuredEventV1`

Topic tuple: `("proto_evt", module, action, actor)`

| Field | Type | Topic | Notes |
| --- | --- | --- | --- |
| `module` | `EventModule` | ✓ | `Lending`, `Collateral`, `Liquidation`, `Oracle`, `Governance`, `Treasury`, `Risk`, `FlashLoan`, `Admin`, `Emergency` |
| `action` | `EventAction` | ✓ | `Deposit`, `Withdraw`, `Borrow`, `Repay`, `Liquidate`, `PriceUpdate`, `ParamsUpdate`, `Pause`, `Unpause`, `ProposalCreated`, `VoteCast`, `Execute`, `Claim`, `FlashLoan`, `Other` |
| `actor` | `Address` | ✓ | Primary account responsible for the change |
| `schema_version` | `u32` | | Equals `EVENT_SCHEMA_VERSION` |
| `action_name` | `Symbol` | | Concrete verb; equals `action` unless `action == Other` |
| `asset` | `Option<Address>` | | `None` = native XLM / not applicable |
| `amount` | `i128` | | Primary signed amount, base units; `0` when not applicable |
| `counterparty` | `Option<Address>` | | Borrower, delegatee, recipient… |
| `metadata` | `Vec<StructuredEventField>` | | `{ key: Symbol, value: i128 }` numeric annotations |
| `timestamp` | `u64` | | Ledger timestamp at emission |

### Emitting

```rust
use crate::events::{StructuredEvent, EventModule, EventAction};

StructuredEvent::new(env, EventModule::Lending, EventAction::Borrow, user.clone())
    .with_asset(asset)
    .with_amount(amount)
    .with_counterparty(pool.clone())
    .with_meta(env, "health_factor", hf)
    .emit(env);
```

For an already-built value use `events::emit_structured(env, envelope)`.

## Versioning

`EVENT_SCHEMA_VERSION` (currently `1`) is bumped on any backwards-incompatible
change to `StructuredEventV1` (field removal, reorder, or type change). Appending
an optional `metadata` entry does **not** require a bump. Consumers should read
`schema_version` and refuse to decode versions newer than they understand.

## Off-chain catalog

| Endpoint | Returns |
| --- | --- |
| `GET /api/events/schema` | Full catalog: version, module/action vocabularies, every event definition |
| `GET /api/events/schema/:name` | One event definition (`404` if unknown) |
| `GET /api/events/version` | `{ "schemaVersion": 1 }` |
| `GET /api/events/modules` | Canonical module identifiers |
| `GET /api/events/actions` | Canonical action verbs |

Keep `EVENT_SCHEMA_VERSION` and the `structured_event_v1` definition in
`api/src/services/events.service.ts` in lock-step with the contract.

## Tests

- `contracts/hello-world/src/tests/structured_events_test.rs` — builder field
  population, defaults, timestamp capture, topic layout, `emit_structured`,
  independence from typed emitters.
- `api/src/__tests__/events.routes.test.ts` — catalog shape, envelope topic
  layout, per-event lookup, `404` handling, vocabulary endpoints.
