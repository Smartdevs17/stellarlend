# StellarLend Event Schema v1

Schema Version: 1

This document defines the event compatibility rules for the StellarLend Soroban contracts. The
machine-readable companion file is `event-schema.v1.json`.

## Compatibility Rules

- Existing event topics are append-only. Do not rename or remove a topic without adding a migration note.
- New typed events should use `#[contractevent]` and lower snake case topic names.
- Manual `env.events().publish(...)` calls are allowed only for legacy compatibility and must be listed in the schema JSON.
- Event topics must fit Soroban's four-topic limit, including static topics and `#[topic]` fields.
- Field names must use lower snake case and payload fields should stay append-only.
- Amounts are raw token units. Percentages use basis points where `10000` is `100%`.
- Indexers should persist raw topics and decoded payloads, then annotate decoded rows with `_schema_version` and `_event_topic`.

## Common Field Semantics

| Field | Meaning |
| --- | --- |
| `user` | End-user account that initiated or owns a position-changing action. |
| `actor` | Authorized protocol account that performed an administrative action. |
| `caller` | Authenticated account that invoked an administrative or governance entrypoint. |
| `asset` | Soroban token contract address; `None` represents native asset where supported. |
| `amount` | Raw integer amount in the asset's smallest unit. |
| `fee` | Raw integer fee in the asset's smallest unit. |
| `timestamp` | Ledger timestamp in seconds. |
| `*_bps` | Basis points; `10000` is `100%`. |

## Versioning

Version 1 is additive. It standardizes documentation, CI validation, and indexer metadata without
changing existing runtime event topics or payload shapes.

For a future breaking change:

1. Add a new `event-schema.vN.json` file.
2. Keep the old schema available for historical decoding.
3. Add an explicit migration section to `docs/event-indexing.md`.
4. Update indexers to decode both versions during the migration window.
