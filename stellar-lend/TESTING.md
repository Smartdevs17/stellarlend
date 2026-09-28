# Contract testing

StellarLend contract tests share one framework, the `test-utils` crate in
`contracts/test-utils`, and one runner, `scripts/testing/run-contract-tests.sh`.
This page explains where tests live, what the framework provides, and how to
run, benchmark and measure coverage.

## Where tests live

| Location | Kind | Built on `test-utils` |
|---|---|---|
| `contracts/test-utils/src/**`, `contracts/test-utils/tests/` | The framework's own unit tests, fixture/seeding tests and fixture gas benchmarks | yes |
| `contracts/lending/tests/` | Integration suites against the public `LendingContractClient`: journeys, fuzzing, events, auth, scenarios | yes, through `tests/common/mod.rs` |
| `contracts/common/src/protocol_integration_test.rs` | Message bus, cache and shared-type checks | yes |
| `tests/integration/flash-loan-liquidation/` | Flash-loan liquidation models | yes, uses the reference health-factor model |
| `contracts/<crate>/src/*_test.rs` | Per-crate unit tests next to the code | not yet |
| `../tests/e2e/`, `../tests/chaos/`, `../tests/stress/` | TypeScript tests for the API and off-chain services | no, these are Jest suites |

Suites in the runner's registry must build on `main`. Some crates' in-crate
`#[cfg(test)]` modules currently fail to compile (for example `hello-world`,
the `stellarlend-lending` lib tests and `lending-core`). Add those crates to the
registry once they compile again. Until then, the lending integration suites
under `contracts/lending/tests/` build against the public client, so they run
regardless of the state of the lib tests.

## The framework (`test-utils`)

Add it as a dev-dependency:

```toml
[dev-dependencies]
test-utils = { path = "../test-utils" }
```

| Module | Use it for |
|---|---|
| `environment` | `TestEnv` (env, admin, generated users, time and ledger helpers), `snapshotless_env()` for suites that create many envs |
| `fixtures` | Value fixtures (`AmountFixtures`, `TimeFixtures`, `RateFixtures`, `AssetConfigFixture`) and deployed state (`ProtocolFixture`, `TokenFixture`) |
| `mock_contracts` | `PriceOracle` (the `price(asset)` interface the lending contract calls), `MockToken`, `MockOracle` |
| `seeding` | Deterministic `SeedRng` and a `Seeder` that funds users and moves prices |
| `scenario` | JSON scenarios executed step by step against a real contract |
| `bench` | `GasMeter`, `GasBudgets` and `BenchReport`, for per-step gas with budgets and JSON reports |
| `gas` | Low-level budget snapshots and CPU ceilings |
| `assertions`, `invariants`, `reference` | Domain assertions, protocol invariants and plain-integer reference math |
| `events`, `harness` | Event recording and the cross-contract harness |
| `suite`, `edge_cases` | A test-case registry and the edge-case catalog |

`packages/test-framework` was a second framework with overlapping fixtures and
a scenario runner that never executed anything. It did not compile and nothing
depended on it. Its scenarios, edge-case catalog, suite registry and reference
math now live in `test-utils`, and the crate has been removed.

### Fixtures

`ProtocolFixture` deploys what most suites need in one call: a
snapshot-free env with auths mocked, an admin, users, Stellar Asset Contract
tokens and a `PriceOracle`.

```rust
use test_utils::{ProtocolFixture, ORACLE_PRICE_SCALE};

let f = ProtocolFixture::builder()
    .users(3)
    .tokens(2)
    .token_price(1, ORACLE_PRICE_SCALE / 2) // token 1 quoted at 0.5
    .initial_balance(10_000)                // every user holds 10k of every token
    .build();

f.set_price(0, 2 * ORACLE_PRICE_SCALE);
assert_eq!(f.token(0).balance(f.user(0)), 10_000);
```

Contract-specific fixtures sit on top of this. The lending suites'
`tests/common/mod.rs` registers the contract with `snapshotless_env()` and
`register_price_oracle()`, and adds only lending-specific setup (`deploy`,
`setup`, `snapshot`).

Use `snapshotless_env()` or `TestEnv::snapshotless()` in any suite that creates
many environments. `Env::default()` writes a `test_snapshots/` file for every
env it drops.

### Test data seeding

All randomness comes from `SeedRng`, a dependency-free SplitMix64. With the
same seed, a test produces the same data on every machine. The seed defaults to
`DEFAULT_SEED`, and `TEST_SEED` overrides it:

```bash
TEST_SEED=1234 cargo test -p stellarlend-lending --test scenarios
```

`Seeder` records everything it mints, so tests can assert against the seeded
totals:

```rust
let mut seeder = Seeder::from_env(&f);
seeder.fund_random(0, 1_000, 50_000);   // random balance per user
let path = seeder.price_walk(1, 40, 500); // 40 moves of at most 5% each
assert_eq!(seeder.total_seeded(0), /* sum of balances */);
```

### Scenarios

A scenario is a JSON list of actions with an expected outcome per step. The
bundled scenarios are in `contracts/test-utils/scenarios/`, and
`Scenario::bundled("lending-journey.json")` loads one. The suite supplies a
handler that maps each action to contract calls:

```rust
let result = ScenarioRunner::new().run(&scenario, |step| actors.apply(step));
result.assert_passed();
```

`expected_result` is `"success"`, `"error"` or `"error:<text>"`. The runner
stops at the first mismatch and reports the step. See
`contracts/lending/tests/scenarios.rs` for a full handler.

### Gas benchmarks

`GasMeter::measure` runs one contract call under a fresh budget and records
CPU instructions, memory and wall time. Soroban resets metering before every
top-level invocation, so measure one call per closure.

```rust
let budgets = GasBudgets::workspace_baseline(); // benchmarks/baseline.json
let mut meter = GasMeter::new("deposit_cycle");
meter.measure(&f.env, "deposit", "lending::deposit", || client.deposit(&user, &asset, &1_000));
budgets.assert_within(&meter);                  // views (`::get_*`) are reported, not enforced
BenchReport::new(&[meter], &budgets).write(&dir, "deposit.json");
```

Budgets are keyed `<contract>::<function>` in `benchmarks/baseline.json`
(`gas_budgets`). When a step has no budget, `assert_within` fails, so every new
operation needs an entry.

## Running

From the repository root:

```bash
scripts/testing/run-contract-tests.sh list      # suites and their cargo arguments
scripts/testing/run-contract-tests.sh           # run every suite with timing
scripts/testing/run-contract-tests.sh bench     # gas benchmark reports
scripts/testing/run-contract-tests.sh coverage  # lcov + summary (needs cargo-llvm-cov)
SUITES="test-utils common" scripts/testing/run-contract-tests.sh
```

| Mode | Output under `stellar-lend/target/test-reports/` |
|---|---|
| `test` | `contract-tests.json`, with passed, failed and ignored counts and wall time per suite, plus one log per suite |
| `bench` | `benchmarks/*.json`, with per-step CPU, memory, budget, over-budget flags and recommendations |
| `coverage` | `coverage/lcov.info` and `coverage/summary.txt`, plus HTML with `COVERAGE_HTML=1` |

Environment variables: `PROPTEST_CASES` (default 16), `TEST_SEED`,
`REPORT_DIR`, and `COVERAGE_MIN`, which fails the run below that line-coverage
percentage.

Coverage needs `cargo-llvm-cov`:

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
```

The deep property run with a performance baseline stays in
`scripts/testing/run-property-suite.sh` (see `contracts/lending/tests/README.md`).

## CI

`.github/workflows/contract-tests.yml` runs on changes to the framework, the
suites in the registry, the runner or the baseline. It has three jobs:

1. **Unified suite**: `cargo fmt --check` and `clippy -D warnings` on
   `test-utils`, then every suite, with the timing table in the job summary.
2. **Gas benchmarks**: the benchmark reports, uploaded as an artifact, with the
   per-step table in the job summary.
3. **Coverage**: `cargo-llvm-cov` over every suite. The job uploads
   `lcov.info` and the summary, and fails when line coverage drops below the
   `COVERAGE_MIN` floor set in the workflow.

## Adding a suite

1. Build the test on `test-utils`: use `ProtocolFixture`, or a contract fixture
   built on `snapshotless_env()`, and take random data from `SeedRng`.
2. Add the suite to `SUITE_ARGS` and `SUITE_ORDER` in
   `scripts/testing/run-contract-tests.sh`. CI, coverage and benchmarks all pick
   it up from there.
3. Add the crate's paths to the `paths:` filters in
   `.github/workflows/contract-tests.yml`.
4. If the suite meters gas, give every measured operation a budget in
   `benchmarks/baseline.json`.
