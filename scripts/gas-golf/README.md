# Gas golf

A leaderboard and optimization harness for gas golf competitions: contributors
submit gas-optimised implementations, the harness proves they are still correct,
and the board ranks them on measured gas.

```
node --experimental-strip-types scripts/gas-golf/index.ts
```

No dependencies, no build step. Node >= 22 (`--experimental-strip-types` is
enough to run the TypeScript directly).

## The two rules

Everything here follows from two facts about this repository.

**1. Correctness gates the score.** A gas optimisation that changes behaviour is
not an optimisation, it is a bug. Every submission is compared against a
maintainer-owned reference implementation over a fixed vector set before it is
allowed to be ranked, at a tolerance of exactly `0`. The tolerance and the
allowed-output-path list come from the course definition, never from the
submitter — a submission cannot widen its own pass mark. This is the same
comparison `scripts/differential-test` performs, with competitive rules bolted
on.

**2. Only a same-session delta is a score.** Instruction counts are
toolchain- and build-dependent, so an absolute number means nothing. A figure is
only reported alongside the reference's figure from the *same* benchmark run;
`--require-fresh` refuses a report stitched together from two commits. This is
the same argument `gas-regression.yml` makes about cold caches: a cold,
reproducible build keeps a measurement comparable with how the committed
baseline was produced.

The score itself is `instructions / budget` — the repository's existing
`FunctionRow.utilizationPct` (`api/src/services/gasReport/report.ts:191`), which
is already served at `/api/analytics/gas/contract/budgets`. Reusing it means the
leaderboard and the gas report can never disagree about what "over budget" means.
Lower is better. Ties break on memory bytes, then submission id.

Storage read/write counts are **not** a ranking input. They are hand-declared
integer literals in every `BenchmarkResult::new(...)` call and are hardcoded to
`0` for `disk_read_entries` / `write_entries` in `framework.rs:317-319`. They are
carried through as `declared` provenance and displayed, never scored.

## Commands

```
index.ts [--submissions <index.json>] [options]
index.ts --list-challenges
index.ts --list-courses
index.ts --gate all|<id>
```

| Option | Meaning |
| --- | --- |
| `--submissions <file>` | Submission index. Default `submissions/index.json`. |
| `--reference <file>` | Benchmark report holding the reference's figures for the same session. Default `stellar-lend/benchmark-results.json`. |
| `--gate all\|<id>` | Run only the correctness gate. |
| `--list-challenges` | Print the open targets and their budgets. |
| `--list-courses` | Print the gated courses. |
| `--format text\|json\|markdown` | Output format. Default `text`. |
| `--out <file>` | Write to a file instead of stdout. |
| `--require-fresh` | Fail when a report's rows come from more than one commit. |

Exit codes: `0` ok, `1` a gate failed, `2` usage error.

## What the board says today

On this branch, **nothing is rankable**, and the board says so rather than
printing a table of zeroes:

```
No rankable entries.

Why there is nothing to rank:
  - [empty] stellar-lend/benchmark-results.json does not exist. Run ./run-benchmarks.sh
    to produce a report; without one there is nothing to score against.
```

`stellar-lend/benchmarks/baseline.json` has `"results": []`, and
`stellar-lend/benchmarks/gas-baseline.json` holds 39 real measurements that are
all `hello-world` — not one `lending::*` row. The existing coverage check in
`gas_benchmark_report.py` is set-membership only, so a report full of
`instructions: 0` would pass it. Ingestion therefore checks for zero rows
explicitly: a `0` means unmeasured, not free.

The populated path is exercised end to end against clearly-labelled synthetic
numbers in `fixtures/` — see `fixtures/README.md` for why they are invented and
what not to do with them.

## Adding a challenge

1. The operation must already be in
   `stellar-lend/benchmarks/public-functions.json`. A submission cannot introduce
   a new operation, because an operation with no budget is *unbudgeted* rather
   than un-gated, and its score would be un-gatable.
2. Add a course in `gate.ts` (`COURSES`): the challenge id, a reference
   implementation, a vector file, `tolerance: 0`, `allow: []`.
3. Add the vector set under `scenarios/`. Name every vector: a finding has to be
   able to point at the case that broke.
4. Add a submission under `submissions/` and list it in
   `submissions/index.json`.
5. The budget comes from `baseline.json` automatically — either the operation's
   own entry or, failing that, its operation type.

An operation already declared in `framework.rs` as an optimisation target
(`lending::interest_full_recompute` at 600k, `lending::interest_incremental` at
220k, `lending::interest_same_block` at 80k) is **not** in
`public-functions.json`, so it is not an open target. Those are the benchmark
suite's own comparison group, not a competition.

## The worked example

`submissions/collapsed-division.ts` is an invalid submission kept as the gate's
regression fixture. It collapses

```
(collateralValue * thresholdBps / 10000) * 10000 / debtValue
```

into a single division, which looks like a free saving. At collateral `1501`,
threshold `6667` and debt `1000`, the contract's two truncations give exactly
`10000` — the liquidation boundary — while the collapsed form gives `10007`,
comfortably above it. The rewrite would declare a liquidatable position healthy.

```
node --experimental-strip-types scripts/gas-golf/index.ts \
  --submissions scripts/gas-golf/submissions/invalid.json
# collapsed-division  REJECT  diverges from reference: … first at $/healthFactor
#                      (mismatch): "10000" vs "10007"      → exit 1
```

It lives in `submissions/invalid.json` rather than `submissions/index.json` so
the default board run is not permanently red — a competition index that ships a
known-bad entry trains people to ignore the exit code.

## Tests

```
node --experimental-strip-types --test scripts/gas-golf/*.test.ts
```

- `leaderboard.test.ts` — course parsing, measurement integrity, scoring, rendering
- `harness.test.ts` — the gate: every course against its own vectors, the exploit, the budget-override refusal
- `cli.test.ts` — the CLI end to end, including the three exit paths and the property that a fast wrong implementation is ranked nowhere
