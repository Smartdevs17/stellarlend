# Gas Reporting

This document describes how gas usage is measured, reported, and gated across the
StellarLend repository (issue #718). It is the single source of truth for the gas
reporting flow and should be updated whenever the pipeline changes.

## TL;DR

1. Every pull request runs `.github/workflows/gas-report.yml`.
2. It builds the benchmark suite and measures gas (CPU instructions as the proxy,
   plus memory bytes, in `run_benchmarks`).
3. The results are diffed against the committed baseline **`benchmarks/baseline.json`**.
4. Regressions above the configured threshold fail the PR check, and the full
   report is posted in `benchmarks/report.md` and uploaded as a CI artifact.
5. `main` keeps an append-only trend log in **`benchmarks/history.json`**.

## Pipeline

```
             pull request                     main / weekly
                  │                                │
  scripts/gas-report.sh                    cargo run --bin run_benchmarks
        │  (builds + runs suite)                   │
        ▼                                         ▼
  benchmarks/latest.json                 benchmarks/latest.json
        │                                         │
        ▼                                         ▼
  scripts/check_gas_benchmarks.py         history append → benchmarks/history.json
   vs benchmarks/baseline.json
        │
        ├── within threshold  →  report posted, check green
        └── over threshold    →  report posted, check red + PR comment
```

## Commands

```bash
# Run the full suite and produce the PR-style gas report
bash scripts/gas-report.sh

# Run just the suite and write raw JSON
cargo run --bin run_benchmarks -- --output benchmarks/latest.json

# Compare two reports manually
python3 scripts/check_gas_benchmarks.py \
  --baseline benchmarks/baseline.json \
  --current  benchmarks/latest.json \
  --max-regression-pct 5.0

# Regenerate the committed baseline after an intentional optimization
cargo run --bin run_benchmarks -- --output benchmarks/baseline.json
```

Environment overrides (all optional):

| Variable                | Default                          | Purpose                         |
| ----------------------- | -------------------------------- | ------------------------------- |
| `BENCH_DIR`             | `benchmarks`                     | Output directory                |
| `BASELINE`              | `benchmarks/baseline.json`       | Baseline report                 |
| `LATEST`                | `benchmarks/latest.json`         | Current run report              |
| `REPORT_MD`             | `benchmarks/report.md`           | Markdown PR report              |
| `HISTORY`               | `benchmarks/history.json`        | Append-only trend log           |
| `MAX_REGRESSION_PCT`    | `threshold` from `config.toml`   | Regression budget (default 5%)  |

## Files

| Path                        | Role                                      |
| --------------------------- | ----------------------------------------- |
| `scripts/gas-report.sh`     | Orchestrator: run → compare → format → exit code |
| `scripts/check_gas_benchmarks.py` | Regression gate                         |
| `benchmarks/src/main.rs`    | Benchmark entrypoint (`run_benchmarks`)   |
| `benchmarks/src/report.rs`  | JSON/Markdown/history serialization        |
| `benchmarks/config.toml`    | Thresholds and output paths                |
| `benchmarks/baseline.json`  | Committed reference point (gas budgets)    |
| `benchmarks/gas-baseline.json` | Legacy per-contract raw measurements |
| `benchmarks/history.json`   | Trend log (most recent 250 runs)           |
| `benchmarks/public-functions.json` | Ops that must appear in every report |

## Updating the baseline

Baselines are intentionally **not** auto-updated by the PR workflow — otherwise a
regression could be silently absorbed. To bless a new reference point:

1. Run `cargo run --bin run_benchmarks -- --output benchmarks/baseline.json`.
2. Confirm the diff is expected (`git diff --stat benchmarks/baseline.json`).
3. Reference the issue/PR that caused the change in the commit message.

## Optimization suggestions

When an operation reports **over budget**, the report suggests a category of fix:

- **Storage access pattern** — batch reads, keep hot fields together, pack small
  values into a single storage slot (see `docs/storage.md`).
- **Early exit** — bail before expensive work when a liquidation or transfer is
  unprofitable (see `docs/gas-optimization.md`).
- **Event/data structure choice** — prefer compact `#[contracttype]` layouts and
  avoid re-reads of global state across a loop.

## Troubleshooting

- **"benchmark suite failed to run"** — the crate under test does not compile on
  the PR. See the pre-existing build note in the PR body; this gates the run.
- **"no valid JSON"** — `run_benchmarks` produced no output; check `cargo run`
  errors and the `BENCH_DIR` value.
- **False-positive regression** — environmental variance; compare
  `benchmarks/latest.json` and `baseline.json` manually with the comparison
  command above before re-baselining.