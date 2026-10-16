# Test Utilities

The unified test framework for StellarLend smart contracts. Every contract
suite uses it to set up environments, fixtures, seeded data, scenarios and gas
benchmarks. [`../../TESTING.md`](../../TESTING.md) covers how the suites are
organised, run in CI, benchmarked and measured for coverage.

## Usage

Add to your contract's `Cargo.toml`:

```toml
[dev-dependencies]
test-utils = { path = "../test-utils" }
```

```rust
#[cfg(test)]
mod tests {
    use test_utils::*;

    #[test]
    fn test_example() {
        let f = ProtocolFixture::builder().users(2).initial_balance(1_000).build();
        let contract_id = f.env.register(MyContract, ());
        // ...
    }
}
```

## Modules

### Environment (`environment.rs`)

```rust
let mut test_env = TestEnv::new()        // or TestEnv::snapshotless()
    .with_timestamp(1000)
    .with_ledger_sequence(1)
    .with_unlimited_budget();

let user = test_env.generate_user();
test_env.advance_time(3600);
```

- `snapshotless_env()` returns an env that doesn't write `test_snapshots/` on drop. Use it for property tests and other suites that create many envs. Auths are not mocked.
- `setup_test_env()`, `setup_test_env_with_users(count)`, `create_string(env, value)` and `advance_time(env, seconds)` are also available.

### Fixtures (`fixtures.rs`)

The value fixtures are `AmountFixtures`, `TimeFixtures`, `RateFixtures`, `AssetConfigFixture` and the default constants.

Deployed-state fixtures:

```rust
let f = ProtocolFixture::builder()
    .users(3)                               // default 2
    .tokens(2)                              // Stellar Asset Contracts, default 1
    .default_price(ORACLE_PRICE_SCALE)      // price every token starts at
    .token_price(1, ORACLE_PRICE_SCALE / 2)
    .initial_balance(10_000)                // minted to every user, every token
    .timestamp(1_700_000_000)
    .build();

f.token(0).mint(f.user(0), 500);
f.set_price(0, 2 * ORACLE_PRICE_SCALE);
f.advance_time(TimeFixtures::DAY);
```

`TokenFixture::deploy(env, admin)` deploys a single SAC.

### Data seeding (`seeding.rs`)

`SeedRng` is a deterministic SplitMix64 generator. `Seeder` funds users, quotes random prices, runs bounded price walks and generates amounts, and it records every balance it mints. `TEST_SEED` overrides `DEFAULT_SEED`.

```rust
let mut seeder = Seeder::from_env(&f);
seeder.fund_random(0, 1_000, 50_000);
let path = seeder.price_walk(1, 40, 500);
assert_eq!(seeder.seeded_for(f.user(0), 0), f.token(0).balance(f.user(0)));
```

### Mock contracts (`mock_contracts.rs`)

- `PriceOracle` exposes `price(asset)`, `set_price` and `set_default_price`. It uses 8 decimals and is the interface the lending contract calls. Register it with `register_price_oracle(env, default_price)`.
- `MockToken` supports mint, burn and balance.
- `MockOracle` exposes `get_price` and `set_price`, with 6-decimal defaults.

### Scenarios (`scenario.rs`, `scenarios/*.json`)

JSON scenarios run step by step against a real contract through a handler you supply. `expected_result` is `"success"`, `"error"` or `"error:<text>"`.

```rust
let scenario = Scenario::bundled("lending-journey.json");
ScenarioRunner::new().run(&scenario, |step| apply(step)).assert_passed();
```

### Gas benchmarks (`bench.rs`, `gas.rs`)

```rust
let budgets = GasBudgets::workspace_baseline();          // benchmarks/baseline.json
let mut meter = GasMeter::new("journey");
meter.measure(&env, "deposit", "lending::deposit", || client.deposit(&u, &a, &1_000));
budgets.assert_within(&meter);
BenchReport::new(&[meter], &budgets).write(&dir, "journey.json");
```

Soroban resets metering before every top-level contract invocation, so measure one contract call per closure. `gas.rs` has the lower-level `timed`, `assert_within_cpu` and `assert_cheaper_or_equal`.

### Assertions, invariants and reference math

- `assertions.rs`: `assert_non_negative`, `assert_in_range`, `assert_approximately_equal`, `assert_percentage_in_range`, and more.
- `invariants.rs`: accounting identity, collateral conservation, health-factor direction, index monotonicity and pause freezing, plus `InvariantReport`.
- `reference.rs`: `health_factor_bps`, `is_liquidatable`, `calculate_interest_accrual`, and tolerance assertions.

### Events and cross-contract harness (`events.rs`, `harness.rs`)

`EventRecorder` records topics in a bounded buffer. `CrossContractHarness` registers a mock token and oracle and drives labeled scenario steps.

### Suite registry and edge cases (`suite.rs`, `edge_cases.rs`)

- `TestSuite` holds `TestCase` / `NamedTest` cases and returns a result for each one.
- `EdgeCaseCatalog` documents the expected failure modes for each function.

## Running this crate's tests

```bash
cd stellar-lend
cargo test -p test-utils
```

## Contributing

When adding new utilities:

1. Add a helper here once at least two suites need it. Otherwise keep it in the suite.
2. Test it in this crate, either as a unit test in the module or in `tests/`.
3. Keep functions focused and composable, and keep existing signatures working.
4. Document it here and, if it changes how suites are written, in `TESTING.md`.
