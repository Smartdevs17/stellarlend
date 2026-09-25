# Differential Testing Between Implementations (#1067)

Run the same scenarios through two independent implementations of the same
behavior and verify the outputs agree — the oracle technique for proving a
reimplementation matches a reference.

## Why

Refactors, ports, and "optimizations" are only trustworthy if they behave
identically to what they replace. Differential testing encodes that proof:
feed identical inputs to **implementation A** (reference / old / on-chain math)
and **implementation B** (reimplementation / optimized / cross-language port)
and fail CI whenever their outputs diverge beyond a tolerance.

## Usage (Node >= 22, no install needed)

```bash
# Compare two implementations over the bundled scenarios
node --experimental-strip-types scripts/differential-test/index.ts \
  --scenarios scripts/differential-test/scenarios/interest-scenarios.json \
  --impl-a scripts/differential-test/implementations/interest-v1.ts \
  --impl-b scripts/differential-test/implementations/interest-v2.ts \
  --tolerance 0.0005

# Allow known divergences by output path (JSON-pointer style)
node --experimental-strip-types scripts/differential-test/index.ts \
  --scenarios scenarios.json --impl-a a.ts --impl-b b.ts \
  --allow '$/sqrtApprox'

# Emit a machine-readable report
... --out report.json
```

### Scenario file

```jsonc
[
  { "name": "small-loan-1y", "inputs": { "principal": 1000, "rate": 0.06, "years": 1 } }
]
```

### Implementation contract

Each implementation module must export:

```ts
export const name = "my-impl";
export async function run(scenario: { name: string; inputs: Record<string, unknown> }): Promise<unknown> {
  return { /* JSON-serializable result */ };
}
```

Relative paths resolve against the current directory first, then against the
`scripts/differential-test` directory (so `implementations/x.ts` works from
anywhere).

## Comparison semantics

- Numbers: equal within **relative** tolerance (`|a-b| / max(|a|,|b|)`,
  default `1e-6`).
- Everything else: deep equality after stable (key-sorted) serialization.
- Findings are leaf-level JSON pointers (`$/total`), labeled
  `mismatch` / `type-mismatch` / `missing`.

## Exit codes

`0` equivalent · `1` diverged · `2` usage error — scriptable for CI gating.

## Waiting on…

Computing two real implementations for every contract function is a lifting
effort; the runner is the reusable shell. Connect it to:
- the Rust integration suite (Env-based view outputs) vs a JS reference model
  of the same math (health factor, accrued interest, collateralization);
- two contract revisions via the e2e harness (`tests/e2e/scenarios/harness.ts`).

## Tests

```bash
node --experimental-strip-types --test scripts/differential-test/differential.test.ts
```

## Covered acceptance criteria

- Run identical scenarios through two independent implementations.
- Numeric comparison with relative tolerance.
- Leaf-level divergence reporting (path, values, kind).
- Allowlist for known/acceptable differences.
- Scriptable exit codes for CI gating.