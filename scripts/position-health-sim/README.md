# Lending Pool Position Health Simulation (#1013)

Simulates market conditions against the lending pool's **own** health factor and
liquidation threshold math, so a price move can be checked before it happens
on-chain.

```
$ index.ts --position leveraged-xlm-borrow --grid flash_crash

Position: leveraged-xlm-borrow
Health factor: 1.0000 (critical)  ····················
Collateral value 20000 · debt value 16000 · threshold 8000 bps
Distance to liquidation: 0.00% collateral price fall (break-even multiplier 1.0000)
Recovery to 1.5x: 50.00% collateral price rise
Liquidatable now: no

Price grid — Flash crash: A single-session collapse, the shape the contract's liquidation threshold exists for.
Collateral move   HF        Risk           ΔHF
           -20%   0.8000 liquidatable    -20.00%   █████   <- liquidates here
           -30%   0.7000 liquidatable    -30.00%   ███████  <- liquidates here
           -40%   0.6000 liquidatable    -40.00%   ██████████  <- liquidates here
           -50%   0.5000 liquidatable    -50.00%   ████████████  <- liquidates here
```

## Why

The repository computes the health factor in eight places, and **none of the
off-chain ones applies the liquidation threshold**:

| Surface | Formula |
| --- | --- |
| **`views.rs:147` (the contract)** | `(cv · LT / 10000) · 10000 / dv` |
| `risk-engine/stress-tester/engine.ts:191` | `cv / dv` |
| `services/risk-simulation.service.ts:68` | `cv / dv` |
| `services/position-simulator.ts:272` | `collateral / debt` |
| `controllers/simulation.controller.ts:55` | `collateral / debt` |
| `services/portfolio.service.ts:23` | threshold hardcoded to `1.2` (12000 bps) |
| `tests/e2e/scenarios/harness.ts:76` | threshold hardcoded to `0.85` (8500 bps) |
| `hello-world/src/analytics.rs:468` | no threshold, and not even compiled |

With the pool's default `liquidation_threshold_bps = 8000`, the contract's health
factor is **0.8×** the raw ratio. Every surface above therefore reports a
position as comfortably healthy where the contract would call it liquidatable —
and the stress tester and the risk-simulation service disagree with the contract
for every position they look at.

There was also no way to ask the *threshold* question. It is admin-settable across
1..10000 bps, and nothing showed what a change to it does to the boundary where
positions get liquidated.

This tool answers both, using the contract's arithmetic rather than an
approximation of it.

## What it does

1. **Prices a position the way `get_user_position` does** — same formula, same
   sentinel, same "no oracle ⇒ 0" behaviour.
2. **Simulates market conditions** — the four historical scenarios already
   committed at `scenarios/*.json`, plus evenly spaced price shock grids.
3. **Computes the break-even** — the collateral price multiplier that puts the
   health factor on exactly 1.0, derived from the contract's own output so it
   stays right for any threshold.
4. **Sweeps the liquidation threshold** — what a governance change to
   `liquidation_threshold_bps` does to the boundary, which is the question a
   threshold proposal actually turns on.

## Fidelity to the contract

The math is a port of `stellar-lend/contracts/lending/src/views.rs`:
`collateral_value`, `debt_value`, `compute_health_factor`,
`get_max_liquidatable_amount` and `get_liquidation_incentive_amount`. Four
details a naive port gets wrong, each pinned by a test:

- **Two truncating divisions.** The contract computes
  `weighted = cv·bps/10000` and then `hf = weighted·10000/dv`. That is *not*
  `cv·bps/dv` in integer arithmetic. `computeHealthFactor(1501, 1000, …, 6667)`
  must be `10_000`, not `10_007`.
- **No oracle yields `0`** — not infinity, not 1.0 — and
  `get_max_liquidatable_amount` treats that as "cannot act", so no liquidation
  can proceed without price data.
- **Overflow yields `0`, except the incentive, which saturates.** The contract
  works in `I256` then does `to_i128().unwrap_or(0)`; the liquidation incentive
  does `to_i128().unwrap_or(i128::MAX)`.
- **A debt-free position returns `100_000_000`**, not infinity.

### The constants are read, not copied

`HEALTH_FACTOR_SCALE`, `PRICE_SCALE`, `HEALTH_FACTOR_NO_DEBT`,
`COLLATERAL_RATIO_MIN` and the three admin defaults (`unwrap_or(8000)`,
`unwrap_or(5000)`, `unwrap_or(1000)`) are **parsed out of the Rust source**. If
the contract renames one, the tool fails with the name rather than silently
simulating against a stale copy — there is a test for that too, plus one that
asserts the two truncation lines are still in `views.rs`, so a refactor there
cannot quietly leave the port behind.

## Usage (Node >= 22, no install needed)

```bash
# A shipped example position
node --experimental-strip-types scripts/position-health-sim/index.ts \
  --position leveraged-xlm-borrow

# One historical scenario, with an explicit threshold
node --experimental-strip-types scripts/position-health-sim/index.ts \
  --position healthy-xlm-borrow --scenario luna-ust-collapse --threshold-bps 8500

# Your own position file
node --experimental-strip-types scripts/position-health-sim/index.ts \
  --position /path/to/position.json --grid both_sides --format markdown

# What scenarios and grids are available?
node --experimental-strip-types scripts/position-health-sim/index.ts --list-scenarios

# Gate: fail if any market condition liquidates the position
node --experimental-strip-types scripts/position-health-sim/index.ts \
  --position leveraged-xlm-borrow --grid flash_crash --fail-on liquidatable
```

### Position file

```jsonc
{
  "name": "my-position",
  "description": "What this position is.",
  "oraclePresent": true,                    // false models the contract's hf = 0 path
  "collateral": [
    // `amount` is the token's base units, `price` is the contract's
    // `price(asset)` return value (8 decimals). With price = 100_000_000 the
    // contract's value equals the raw amount, which is what MockOracle returns.
    { "asset": "XLM", "amount": "20000", "price": "100000000" }
  ],
  "debt": { "asset": "USDC", "amount": "16000", "price": "100000000" },
  "liquidationThresholdBps": "8000",         // optional; the contract default otherwise
  "closeFactorBps": "5000",                 // optional
  "liquidationIncentiveBps": "1000"         // optional
}
```

Amounts and prices are strings so values beyond a JS float's precision survive
JSON. Five examples ship in `positions/`: a healthy borrower, one sitting exactly
on the boundary, one already liquidatable, a multi-asset position, and one with no
oracle configured.

### Options

| Flag | Default | Meaning |
| --- | --- | --- |
| `--position <name\|file>` | `leveraged-xlm-borrow` | Position to simulate |
| `--positions-dir <path>` | `positions/` | Where named positions are looked up |
| `--scenario <id>` | all | Historical scenario; repeatable |
| `--grid <name>` | `flash_crash` | Price shock grid |
| `--threshold-bps <n>` | contract default | Override `liquidation_threshold_bps` |
| `--close-factor-bps <n>` | contract default | Override `close_factor_bps` |
| `--incentive-bps <n>` | contract default | Override `liquidation_incentive_bps` |
| `--thresholds <list>` | governance grid | Threshold sweep values in bps |
| `--no-scenarios` | — | Skip the historical scenario table |
| `--format <fmt>` | `text` | `text`, `json` or `markdown` |
| `--out <file>` | stdout | Write the report to a file |
| `--fail-on <level>` | — | Fail when any condition reaches this risk level |
| `--list-scenarios` | — | Print the scenarios and shock grids, then exit |

### Exit codes

`0` ok · `1` gate failed · `2` usage error — scriptable for CI.

## The scenario corpus is used, not duplicated

`scenarios/*.json` already holds the 2008 crisis, the 2020 crash, the 3AC/FTX
contagion and the Luna/UST collapse. **Nothing on `main` loaded those files** — the
API's stress tester carries its own in-memory copy of the same four events plus
four more, and there are three further scenario libraries in the codebase.

This tool reads the committed JSON rather than adding a fifth copy. A scenario
that names assets the position does not hold says so, so a scenario that does not
apply is visible instead of silently doing nothing.

Four shock grids cover the shapes a threshold sweep needs: `gentle_downside`,
`flash_crash`, `black_swan` and `both_sides` (the last one exists to confirm the
health factor moves monotonically in *both* directions).

## Risk bands

Anchored on the contract's own scale. The edges above 1.0 follow the repository's
existing convention in `hello-world/src/analytics.rs::calculate_user_risk_level`
(15 000 / 12 000 / 10 500); the 1.0 edge is not a convention, it is
`HEALTH_FACTOR_SCALE` — the point at which the contract stops treating a position
as healthy.

| Band | Health factor |
| --- | --- |
| `liquidatable` | `< 1.0` — the contract will act |
| `critical` | `1.0` – `1.05` — healthy, but with nothing to spare |
| `at-risk` | `≥ 1.05` |
| `moderate` | `≥ 1.2` |
| `safe` | `≥ 1.5` |
| `unknown` | the contract cannot compute it |

Note that a position at *exactly* 1.0 is **not** liquidatable — the contract tests
`hf >= HEALTH_FACTOR_SCALE`. That is a real edge, and `leveraged-xlm-borrow.json`
sits on it deliberately.

## Tests

```bash
node --experimental-strip-types --test scripts/position-health-sim/*.test.ts
```

95 tests, in two groups:

- **`health-factor.test.ts`** — the port. Every vector is transcribed from the
  contract's own tests with the file and test name in the title:
  `views_test.rs` (16 000 / 8 000 / the 6 667 boundary / 15 000), the
  `math_safety_test.rs` overflow and sentinel cases, the
  `liquidation_boundary_test.rs` close-factor and incentive boundaries, and the
  `spec/health_factor.rs` lemmas H-01 to H-07 including both monotonicity
  properties. Plus the four fidelity details above, and two tests against the real
  `views.rs` / `borrow.rs`.
- **`simulation.test.ts`** — scenario parsing and rejection, price and threshold
  sweeps, break-even and distance, all three renderers, and the shipped fixtures
  (each one's health factor is pinned to what its description claims).

## CI

`.github/workflows/position-health-sim.yml` runs the suites, publishes an example
report to the job summary, and includes two negative tests: mutating
`HEALTH_FACTOR_SCALE` in a copy of `views.rs` must make the tool fail rather than
simulate against a stale constant, and a fabricated scenario must be rejected.

## Known scope

- **`liquidate` in the lending pool is a stub.** `lib.rs:298` says
  `// Stub implementation, or call borrow::liquidate if it exists` and returns
  `Ok(())`. The simulator prices what a liquidation *would* cost via
  `get_max_liquidatable_amount` and `get_liquidation_incentive_amount`, which is
  as far as the contract currently goes. Wiring the entry point is a contract
  change outside this issue.
- **Neither `close_factor_bps` nor `liquidation_incentive_bps` is exposed in the
  `#[contractimpl]`**, so their live values cannot be read from-chain today. The
  tool uses the `borrow.rs` defaults and lets them be overridden.
- The oracle read has no staleness check in the lending pool, so a stale price
  feeds straight into the health factor. The tool prices whatever price it is
  given; it does not model staleness.
- Off-chain surfaces that disagree with the contract (the table above) are left
  alone. Fixing them is an `api/` change outside this issue — but this tool is the
  reference to fix them against.

## Files

| File | Role |
| --- | --- |
| `index.ts` | CLI, position loading and validation, exit codes |
| `contract.ts` | parses the constants and admin defaults out of the Rust source |
| `health-factor.ts` | the port of `views.rs` — value, health factor, close factor, incentive |
| `simulation.ts` | market conditions: scenarios, price grid, threshold sweep, break-even |
| `scenarios.ts` | loads `scenarios/*.json` and the built-in shock grids |
| `report.ts` | `text` / `markdown` / JSON renderers |
| `positions/*.json` | five example positions |

## Related

- `docs/POSITION_HEALTH_SIMULATION.md` — describes an intended tool. The
  on-chain types it names live in `hello-world/src/analytics.rs`, which is not in
  that crate's module tree, and the `/api/simulation/*` endpoints it lists are not
  registered in `app.ts`. This tool is the working version, on the real contract.
- `stellar-lend/contracts/lending/src/spec/health_factor.rs` — the Rust reference
  and the H-01…H-07 lemmas this tool's tests mirror.
- `api/src/services/risk-engine/stress-tester/` — the pool-level stress sweep.
  It is not mounted, and its health factor omits the threshold.
