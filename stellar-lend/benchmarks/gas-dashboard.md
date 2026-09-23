# Gas Benchmark Dashboard

This dashboard documents how to read and maintain StellarLend gas benchmark results.

## CI gate

The workflow at `.github/workflows/gas-benchmarks.yml` runs the benchmark binary, uploads `benchmark-results.json`, and fails the build when an operation exceeds its configured gas budget. A practical regression policy is:

- fail when any function exceeds its explicit budget;
- review any operation whose CPU instruction cost increases by more than 10% from the committed baseline;
- update `benchmarks/baseline.json` only after an intentional optimization or feature change is reviewed.

## Result fields

Each benchmark result should include:

- `contract`
- `operation`
- `instructions`
- `memory_bytes`
- `budget`
- `within_budget`
- `storage_reads`
- `storage_writes`
- `cross_contract_calls`

## Optimization checklist

- Prefer packed storage records over many small keys when values are read together.
- Cache config records loaded more than once inside the same entrypoint.
- Avoid repeated cross-contract calls in loops.
- Emit compact events and avoid duplicating data already present in storage.
- Add a focused benchmark before and after any storage-layout change.

## Storage slot analysis

When a benchmark regresses, inspect whether the operation added persistent keys, duplicate reads, or larger serialized values. Cross-contract accounting should be called out separately because token/oracle calls can dominate protocol-level gas.

## Gas optimization report (issue #684)

`api/src/services/gasReport` builds a report from:

- the latest `benchmark-results.json`, or `gas-baseline.json` if there are no results yet
- the `gas_budgets` and `operation_type_budgets` entries in `baseline.json`
- `history.jsonl`
- the contract journey reports produced by `contracts/lending/tests/user_journeys.rs`

The report includes:

- **Per-function tracking:** CPU instructions, memory, budget utilization, and status (`ok`, `near` at 80% or more, `over`, or `unbudgeted`).
- **Operation-type budgets:** each function is classified as `read`, `admin`, `user_write`, `liquidation`, `flash_loan`, or `batch`. The type budget applies when a function has no budget of its own, and it also caps every function of that type.
- **Regressions:** changes compared with `gas-baseline.json` that exceed the threshold (10% by default).
- **Recommendations:** over or near budget, regressions, a warm path that costs more than the cold path, batching that saves little per item, expensive views, heavy memory use, and journey steps over budget.
- **Trends:** the history, with the change in the average between entries.

Ways to generate or view the report:

- **CLI:** `npm run gas:generate` prints Markdown. Add `--out-json` or `--out-md` to write files, and `--fail-on-over-budget` or `--fail-on-regression` to use it as a CI gate.
- **CI:** `gas-benchmarks.yml` generates the report after each benchmark run, adds it to the job summary, and fails when a function is over budget.
- **API:** `GET /api/analytics/gas/contract` (`?format=markdown` for Markdown), plus `/budgets`, `/budgets/:type`, `/regressions`, `/recommendations`, and `/trends`.
- **Dashboard:** `frontend/src/components/GasAnalyticsDashboard.tsx`.
