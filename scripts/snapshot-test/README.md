# Snapshot Testing for Contract State (#1064)

Record golden snapshots of the protocol's structured state — user positions,
pool configs, governance records — and fail CI when the current state diverges.

## Why

Contract state mutations are the highest-risk area of the protocol. Snapshot
tests capture the full state before/after operations (or after a scenario run)
and diff it against a recorded golden snapshot. Any unexpected change in any
row or field is reported at the leaf-path level, making regressions surface as
clear, auditable diffs instead of silent state drift.

The snapshot format and state shape are identical to the `state-exporter`
(#499), so snapshots can be recorded straight from a state export.

## Usage (Node >= 22, no install needed)

```bash
# Record / update the golden snapshot from a current state sample
node --experimental-strip-types scripts/snapshot-test/index.ts \
  --state state.json --snapshot snapshots/lending.snap.json --record

# Compare current state against the golden snapshot (exit 1 on ANY divergence)
node --experimental-strip-types scripts/snapshot-test/index.ts \
  --state state.json --snapshot snapshots/lending.snap.json

# Allow a small allowlist of expected volatile paths (e.g. live interest accrual)
node --experimental-strip-types scripts/snapshot-test/index.ts \
  --state state.json --snapshot snapshots/lending.snap.json \
  --allow 'pools/USDC/interestRateBps'

# Emit a machine-readable diff report
node --experimental-strip-types scripts/snapshot-test/index.ts \
  --state state.json --snapshot snapshots/lending.snap.json --out diff.json
```

Exit codes: `0` match, `1` mismatch, `2` usage error.

Snapshots are key-sorted and normalized (stable JSON serialization), so
recording is deterministic and order-insensitive.

## Tests

```bash
node --experimental-strip-types --test scripts/snapshot-test/snapshot.test.ts
```

## Covered acceptance criteria

- Record golden snapshots of user positions, pool configs, and governance.
- Deterministic snapshot hashing (order-insensitive sha256).
- Row-level added / removed / changed detection keyed by section primary key.
- Leaf-path-level diff report (JSON-pointer style) for precise debugging.
- Allowlist for expected volatile fields.
- Scriptable exit codes for CI gating.

## Follow-ups (out of scope here)

- Capture snapshots directly from a live Soroban RPC source (see the
  state-exporter) or from the e2e harness after scenario runs.
- Automatic snapshot review/regeneration via a CI comment command.