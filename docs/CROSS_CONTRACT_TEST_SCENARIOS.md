# Cross-Contract Test Scenarios

Cross-contract integration testing for StellarLend (Issue #688). This document describes the shared harness, multi-contract state simulation, invariant helpers, and how to extend scenarios across oracle, lending, token, and AMM contracts.

## Goals

1. Exercise **real multi-contract call sequences** (oracle → lending → risk → liquidation → token) without a live devnet.
2. Verify **cross-contract invariants** that single-contract unit tests cannot see (accounting identity, conservation, pause freeze, health direction).
3. Provide **reusable Rust test-utils modules** (`events`, `gas`, `invariants`, `harness`) so workspace crates share one pattern.
4. Keep CI lightweight: pure in-memory `Env` + mock contracts; no network.

## Architecture

```
tests/e2e/scenarios/
├── harness.ts                     # In-memory lending/oracle/roles state machine
├── cross-contract-harness.e2e.test.ts
├── multi-contract-state.e2e.test.ts   # State-simulation + invariant suite
└── ...other journey tests

stellar-lend/contracts/
├── test-utils/src/
│   ├── events.rs                  # EventRecorder + topic helpers
│   ├── gas.rs                     # GasSnapshot, measure_delta, ceilings
│   ├── invariants.rs              # assert_accounting_identity, conservation, …
│   ├── harness.rs                 # CrossContractHarness + ScenarioStep
│   ├── mock_contracts.rs          # MockToken, MockOracle
│   └── environment.rs             # TestEnv (users, time, ledger)
└── common/src/events.rs           # CrossContractStepEvent, InvariantCheckEvent
```

## Harness Surfaces

### TypeScript (`tests/e2e/scenarios/harness.ts`)

| Export | Purpose |
|--------|---------|
| `buildLendingApp()` | Express app with deposit/borrow/repay/withdraw/liquidate/pause/oracle routes |
| `runCrossContractScenario(app, steps)` | Scripted multi-step runner with per-step asserts |
| `setPrice` / `assignRole` / `reset` | Shared state setup |

### Rust (`test-utils`)

| Module | Key APIs |
|--------|----------|
| `events` | `EventRecorder::record/assert_topic_seen/assert_count` |
| `gas` | `snapshot`, `measure_delta`, `assert_under_cpu_ceiling` |
| `invariants` | `assert_accounting_identity`, `assert_collateral_conservation`, `InvariantReport` |
| `harness` | `CrossContractHarness::new/with_target/set_price/mint/assert_scenario` |

## State-Simulation Scenarios

### 1. Accounting identity (deposit → borrow → repay → withdraw)

- Conservation: Σ user collateral == protocol-held collateral.
- Health direction: health never increases after a borrow.
- Debt monotonicity: repay strictly decreases outstanding debt.

### 2. Oracle shock → liquidation → recovery

- Price shock makes HF < 1 → liquidatable.
- Close-factor-limited liquidation reduces debt, seizes collateral with bonus.
- Price restore + full repay returns position to healthy, non-zero collateral.

### 3. Pause freeze

- While paused: deposit/borrow/repay/withdraw all rejected (503).
- Position snapshot (collateral, debt, HF) unchanged across pause.
- Resume restores operation.

### 4. Full journey runner

`runCrossContractScenario` executes 10 ordered steps: seed prices → deposit → borrow → check health → shock → confirm liquidatable → liquidate → restore → repay → withdraw.

## Invariant Checklist

| ID | Invariant | Helper |
|----|-----------|--------|
| X-001 | Accounting identity (deposits = debt + reserves + cash) | `assert_accounting_identity` |
| X-002 | Collateral conservation (users == protocol) | `assert_collateral_conservation` |
| X-003 | Health direction after deposit/borrow | `assert_health_direction_after_*` |
| X-004 | Oracle price > 0 when consumed | `assert_price_sane` |
| X-005 | No free value on repay | `assert_no_free_value` |
| X-006 | Interest index monotonic | `assert_index_monotonic` |
| X-007 | Balances frozen while paused | `assert_frozen_while_paused` |

## Running

```bash
cd tests/e2e
npm ci
# Full cross-contract suite
npx jest --runInBand --forceExit cross-contract-harness multi-contract-state
# Everything
npm test
```

Rust unit tests (when workspace compiles):

```bash
cd stellar-lend
cargo test -p test-utils
```

## Extending

1. Add a route or mock contract if a new surface is needed.
2. Add a step to `runCrossContractScenario` or a new `describe` block.
3. Record events via `harness.note(...)` / `EventRecorder`.
4. Assert invariants with `InvariantReport` and call `assert_scenario`.
5. Wire the new test path into `.github/workflows/integration-tests.yml`.

## CI

`.github/workflows/integration-tests.yml` runs the cross-contract E2E suites on PRs touching `tests/e2e/scenarios/**`, `test-utils/**`, or `common/**`.

## Related Issues

- **#688** — Cross-contract integration test harness with state simulation
- **#689** — Chaos engineering (network failures) — separate suite under `tests/chaos`
- **#690** — Governance lifecycle integration tests
