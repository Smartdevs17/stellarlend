# Snapshots

Canonical snapshot format for StellarLend contract-test and tooling state.

## Format

All snapshots use the same envelope:

```json
{
  "version": 2,
  "kind": "contract-state",
  "recordedAt": "ISO-8601 timestamp",
  "seq": 0,
  "state": {
    "positions": [],
    "pools": [],
    "governance": []
  }
}
```

Rules:
- Top-level keys are fixed and ordered by tooling: `version`, `kind`, `recordedAt`, `seq`, `state`.
- `seq` increments on every persisted diff-bearing write.
- Unknown top-level fields are rejected by `validateSnapshot`.
- Use `snapshotLoader.writeSnapshot` to persist changes; it writes canonical JSON and appends diffs to `lending.snap.diff.jsonl`.
