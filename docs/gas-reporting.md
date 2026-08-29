# Gas Reporting

Single source of truth for StellarLend's gas measurement, PR reporting, and
regression gating (issue #718).

## TL;DR

- **PRs**: `.github/workflows/gas-report.yml` builds the benchmark suite, diffs
  it against the committed baseline, posts the diff as a PR comment, and fails
  the check when regressions exceed the threshold.
- **`main`**: `.github/workflows/gas-benchmarks.yml` re-runs the suite and
  publishes the trend history.
- **Locally**: one entrypoint — `scripts/gas-report.sh` — covers baseline,
  report, compare, and check.

## Pipeline

```
PR or local                                        main / weekly
      │                                                 │
 scripts/gas-report.sh report                cargo run -p stellarlend-benchmarks
      │  (cargo run --bin run_benchmarks              --bin run_benchmarks --release
      │   --release)                                   │
      ▼                                                 ▼
 benchmark-results.json (raw BenchmarkReport)   gas_benchmark_report.py → dashboard
      │                                                 │
      ▼                                                 ▼
 compare vs baseline.json tracked into history.jsonl
      │
      ├── within threshold → PR comment, check green
      └── over threshold   → PR comment, check red
```

## Commands

```bash
# Everything: run + compare vs baseline + regression exit code
bash scripts/gas-report.sh

# Individual subcommands (also wrapped as npm scripts)
npm run gas:baseline                 # print the committed/derived baseline JSON
npm run gas:report                   # run suite, print current JSON
npm run gas:compare <base> <current> # markdown diff + comparison.json
npm run gas:check <comparison.json>  # {total, improvements, regressions, average_change}

# The canonical all-in-one local runner (dashboards + history + gates)
./run-benchmarks.sh
./run-benchmarks.sh --compare         # fail on >10% regression vs baseline
./run-benchmarks.sh --update-baseline # adopt current results as the baseline
```

## Files

| Path | Role |
| --- | --- |
| `scripts/gas-report.sh` | Orchestrator (baseline / report / compare / check) |
| `scripts/gas_benchmark_report.py` | Dashboard + history + gate generation |
| `run-benchmarks.sh` | Canonical local runner wrapping both |
| `stellar-lend/benchmarks/` | `run_benchmarks` suite, `baseline.json`, `gas-baseline.json`, `public-functions.json`, `config.toml`, `history.jsonl` |
| `.github/workflows/gas-report.yml` | PR gas gate + comment |
| `.github/workflows/gas-benchmarks.yml` | main/weekly trend rebuild |

## Baseline

- The committed reference is `stellar-lend/benchmarks/baseline.json`.
- While it is an empty stub, `gas:baseline` derives a meaningful reference from
  the measured `gas-baseline.json` (real per-operation hello-world numbers).
- Bless a new baseline with `./run-benchmarks.sh --update-baseline` after
  reviewing the diff — never auto-update on PRs (that would absorb regressions).

## Optimization suggestions

Over-budget operations reported by `gas-compare`/dashboard flag one of:

- **Storage access pattern** — batch reads, pack small config fields into one
  slot (`docs/storage.md`).
- **Early exit** — abort unprofitable liquidation paths before transfers
  (`docs/gas-optimization.md`).
- **Event/data-structure choice** — compact `#[contracttype]` layouts.

## Troubleshooting

- **"benchmark suite failed to run"**: the tested crate does not compile on the
  branch (pre-existing build debris or an in-flight change). This correctly
  gates the run until it builds.
- **Passing gate on empty baseline**: comparison vacuously passes — bless the
  baseline once the suite builds.