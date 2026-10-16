# Lending Protocol Gas Budget Planner (#1012)

Turns a lender's expected interaction pattern into a gas budget: what each call
costs, what the pattern costs per period, how that sits against a budget, where
the volume ceiling is, and which levers would bring it down.

```
$ index.ts --preset multi-asset-rebalancer --budget-xlm 0.5

Plan — 40 call(s) per 30 day(s):
Operation              Calls  Unit stroops  Subtotal        Share
withdraw                  12       26,540   318,480      48%   ██████████████
deposit                   24        7,296   175,098      26%   ████████
repay                      2       44,403    88,806      13%   ███
borrow                     2       42,548    85,096      13%   ███

Total per period : 667,480 stroops (0.0667480 XLM)
Annualised       : 8,121,007 stroops (0.8121007 XLM)
Budget           : 5,000,000 stroops · 13.35% used · 4,332,520 headroom · within

Volume projections:
  2x   1,334,960 stroops  (0.1334960 XLM)  26.7% of budget  within
  5x   3,337,400 stroops  (0.3337400 XLM)  66.75% of budget  within
  10x  6,674,800 stroops  (0.6674800 XLM)  133.5% of budget  over
```

## Why

A lender deciding how much XLM to hold for gas has to answer a question nothing
in the repo could answer: *for the operations I actually intend to run, what does
a period cost, and where does it break?*

The pieces existed but were not connected:

- `POST /api/gas/estimate` prices **one call**, chosen at the moment it is made.
- `stellar-lend/benchmarks/baseline.json` holds a per-call **CPU-instruction
  budget** that CI enforces.
- `stellar-lend/contracts/lending/src/deposit_batch.rs` documents exactly what
  batching saves.

None of them takes a *pattern*. This tool multiplies the per-call costs by a
lender's own counts, checks the result against a budget, projects it forward, and
surfaces the levers — so the answer is available before any money moves.

## Where the numbers come from

The costs are **read from the repository**, never restated here:

| Input | Supplies |
| --- | --- |
| `api/src/services/gas/estimator.ts` | `BASE_FEE`, `STORAGE_WRITE_COST`, `CROSS_CONTRACT_CALL_COST`, `OPERATION_COMPLEXITY`, `BASELINE_CPU_COSTS`, and the `cpu / 100` conversion |
| `stellar-lend/benchmarks/baseline.json` | `gas_budgets` — the committed CPU-instruction budget per call |
| `stellar-lend/benchmarks/gas-baseline.json` | measured CPU counts, used in place of the API baselines when present |

Because the per-call price is computed with the same constants and the same
formula as `calculateBaselineFee`, **a number from this tool matches what
`/api/gas/estimate` returns for the same operation**, and neither can drift
without the tests noticing. A test asserts that equality against the real files.

Two details the loaders get right:

- **The two benchmark vocabularies differ.** `gas-baseline.json` was recorded
  against the `hello-world` contract, where a deposit is `deposit_collateral`
  and a borrow is `borrow_asset`. `BENCHMARK_VOCABULARY` maps between them, and
  the `lending::` budget is preferred over the `hello_world::` one.
- **The most expensive measured scenario wins.** `liquidate` is recorded as
  `write` (394,438) and `early_exit_unprofitable` (112,300) — different code
  paths, not cold/warm variants. Averaging them understates the real cost by a
  third, which is the wrong way to be wrong about a budget.

## Usage (Node >= 22, no install needed)

```bash
# A built-in pattern
node --experimental-strip-types scripts/gas-budget-planner/index.ts --preset steady_lender

# Your own pattern, against a budget
node --experimental-strip-types scripts/gas-budget-planner/index.ts \
  --plan scripts/gas-budget-planner/patterns/multi-asset-rebalancer.json \
  --budget-xlm 0.5 --xlm-price 0.11

# What if activity doubles? (the default projections are 2x, 5x, 10x)
node --experimental-strip-types scripts/gas-budget-planner/index.ts \
  --preset leveraged-loop --scale 3 --project 3,6,12

# Just the per-call price table
node --experimental-strip-types scripts/gas-budget-planner/index.ts --operation-costs

# Markdown for a PR comment or job summary
node --experimental-strip-types scripts/gas-budget-planner/index.ts \
  --preset steady_lender --format markdown --out plan.md

# Gate CI: fail when a plan does not fit its budget
node --experimental-strip-types scripts/gas-budget-planner/index.ts \
  --plan plan.json --budget-stroops 20000000 --fail-over-budget
```

### Plan file

```jsonc
{
  "name": "my-lender",
  "description": "What this pattern is for.",
  "periodDays": 30,
  "batchSize": 4,             // deposits go through deposit_batch
  "operations": { "deposit": 24, "withdraw": 12, "borrow": 2, "repay": 2 },
  "budgetStroops": 20000000,  // optional; --budget-* wins
  "xlmPriceUsd": 0.11,        // optional; enables the USD column
  "amortisation": { "borrow": 0.5 }  // optional per-operation override
}
```

Four examples ship in `patterns/`: a steady lender, a leveraged loop, a
multi-asset rebalancer that batches, and a watchtower liquidator.

### Options

| Flag | Default | Meaning |
| --- | --- | --- |
| `--plan <file>` | — | Plan file to read |
| `--preset <name>` | — | Built-in pattern; see `--list-presets` |
| `--budget-xlm <n>` / `--budget-stroops <n>` | — | Budget to check against; stroops wins |
| `--xlm-price <usd>` | — | USD price of XLM, enables the USD column |
| `--scale <n>` | `1` | Multiply every count, for what-if runs |
| `--project <list>` | `2,5,10` | Volume multipliers to project |
| `--operation-costs` | — | Print the per-call cost table and exit |
| `--format <fmt>` | `text` | `text`, `json` or `markdown` |
| `--out <file>` | stdout | Write the report to a file |
| `--fail-over-budget` | — | Exit `1` when the plan exceeds the budget |
| `--baselines <path>` | `gas-baseline.json` | Measured CPU counts to prefer |
| `--estimator <path>` | `api/src/services/gas/estimator.ts` | Cost constants and per-call costs |
| `--budgets <path>` | `benchmarks/baseline.json` | Committed instruction budgets |
| `--list-presets` / `--list-rules` | — | Print the built-ins and exit |

### Exit codes

`0` ok · `1` over budget · `2` usage error — scriptable for CI.

## Batching

`deposit_batch` is the one batching entry point in the lending pool, and it is
the lever that matters most for a high-frequency lender. The contract's own table
(`deposit_batch.rs`) shows a batch of N deposits pays **one** authorization, one
reentrancy guard, one pause lookup, one packed deposit-state read/write and one
user-position read/write — the same storage footprint as a single `deposit`,
whatever N is. So the batch costs one call's worth and the marginal cost per item
is that divided by N, capped at `MAX_BATCH_DEPOSITS`.

Set `batchSize` and the plan prices deposits accordingly, then reports what the
batching saved against pricing the same deposits one at a time. In the example
above that is 525,294 stroops — **79% of the plan**. The planner does not assume
batching exists for any other operation; a plan file can opt one in through
`amortisation` to model a different contract revision.

## Suggestions

Six rules, each derived from the plan's own numbers rather than fixed advice, so
a differently-shaped plan gets different findings.

| Rule | Fires when | Remedy is |
| --- | --- | --- |
| `batch-deposits` | the plan batches deposits | lender behaviour — already applied, size shown |
| `concentrated-operation` | one operation is ≥40% of the plan's cost | lender behaviour — call it less |
| `storage-write-dominated` | writes are ≥15% of the plan for one operation | a contract change — pack the entries |
| `instruction-budget-pressure` | a call is ≥50% of its committed instruction budget | watch the gas budget gate |
| `budget-growth-limit` | the budget is exhausted within ~2× the planned volume | raise the budget or batch |
| `ample-headroom` | the plan uses ≤50% of its budget | informational |

`storage-write-dominated` deliberately does not fire on every line. Writes are a
large share of every call in this cost model, so reporting all of them would say
nothing; it reports only the operations whose write cost is material to *this*
plan, and derives the figure from the line's **amortised** unit cost so a batched
operation is not credited with writes it does not perform.

An operation the cost table does not know about is never priced at zero — it is
reported as `unknown-operation` at high severity, so a typo in a plan file cannot
quietly shrink the budget.

## Tests

```bash
node --experimental-strip-types --test scripts/gas-budget-planner/planner.test.ts
```

57 tests covering the three loaders (including the vocabulary bridge and the
most-expensive-scenario choice), the cost formula checked against the API's, every
rule, the budget and projection arithmetic, all renderers, the presets, the
shipped plan files, and two tests that run against the repository's own data —
one asserting the per-call price equals what `calculateBaselineFee` would produce.

## Files

| File | Role |
| --- | --- |
| `index.ts` | CLI, flag parsing, exit codes |
| `cost-model.ts` | reads the repository's cost data and prices a call |
| `planner.ts` | pattern → budget, the six rules, `text`/`json`/`markdown` renderers |
| `presets.ts` | built-in interaction patterns |
| `types.ts` | shared data shapes |
| `patterns/*.json` | example plan files |

## Related

- `api/src/services/planner/budget-planner.ts` — the capital-allocation and
  yield planner. Same word, different question: that one allocates capital, this
  one budgets gas.
- `docs/BUDGET_PLANNER.md` — the capital planner's API documentation.
- `run-benchmarks.sh`, `scripts/gas-report.sh` — measured instruction counts and
  the regression gate. This tool consumes their output rather than re-measuring.
- `stellar-lend/contracts/lending/src/deposit_batch.rs` — the source of the
  batching amortisation.
