# Lending Pool Gas Cost Estimator (#1011)

Estimates what each lending pool operation costs on Soroban, and suggests
optimizations derived from the contract's **storage patterns**.

```
Operation        Reads Writes Rem Ex Foot  X-cross  Cost (stroops)   Cost (XLM)
borrow              25     10   0  2   21        1         131,100    0.0131100   ████████████
flash_loan          10      7   0  0   11        2          90,100    0.0090100   █████████
repay                20      5   0  2   16        1          76,100    0.0076100   ████████
deposit               9      2   3  0    7        0          32,100    0.0032100   ███
deposit_batch         9      2   3  0    7        0          32,100    0.0032100   ███
```

## Why

Storage is the part of a Soroban transaction that is easiest to get wrong and
hardest to see. Every persistent entry is a **separate ledger key** with its own
write fee, its own footprint slot and its own rent. Nothing in a normal review
tells you that a single `borrow` writes ten keys, that `flash_loan` flips four
instance flags purely to arm and disarm a reentrancy guard, or that the first
`deposit` after an upgrade silently pays a three-key migration penalty.

The numbers this tool reports were not obtainable before:

| | Before | With this tool |
| --- | --- | --- |
| Storage counts per operation | hard-coded `0` in the Rust benchmark harness; a hand-maintained `OPERATION_COMPLEXITY` table in the API | indexed from the contract source, per entry point, transitively |
| Optimization suggestions | timing / batching / method heuristics, none derived from storage | nine rules keyed on storage patterns, each citing `file.rs:line` |
| API vs contract agreement | unknown | `drift` table diffs the API's table against the source |

## Usage (Node >= 22, no install needed)

```bash
# Human-readable report for every entry point
node --experimental-strip-types scripts/lending-gas-estimator/index.ts

# Focus on the user-facing operations, priced with 5-item batches
node --experimental-strip-types scripts/lending-gas-estimator/index.ts \
  --operations deposit,deposit_batch,withdraw,borrow,repay,emergency_withdraw,flash_loan \
  --iterations 5

# Markdown for a PR comment or job summary
node --experimental-strip-types scripts/lending-gas-estimator/index.ts \
  --format markdown --out gas-report.md

# Gate CI: fail when a storage pattern is worth high severity or more
node --experimental-strip-types scripts/lending-gas-estimator/index.ts \
  --min-severity medium --fail-on high

# Fold the measured CPU/memory baselines into the estimate
node --experimental-strip-types scripts/lending-gas-estimator/index.ts \
  --baselines stellar-lend/benchmarks/gas-baseline.json

# Only the packing and footprint rules
node --experimental-strip-types scripts/lending-gas-estimator/index.ts \
  --only packable-key-namespace,wide-footprint
```

### Options

| Flag | Default | Meaning |
| --- | --- | --- |
| `--contract-dir <path>` | `stellar-lend/contracts/lending` | Contract source root to index |
| `--operations <list>` | all | Comma-separated entry points to price |
| `--iterations <n>` | `1` | Multiplier for storage access inside a loop (batch sizing) |
| `--format <fmt>` | `text` | `text`, `json` or `markdown` |
| `--out <file>` | stdout | Write the report to a file |
| `--fail-on <severity>` | `critical` | Exit `1` when a finding reaches this severity |
| `--min-severity <severity>` | — | Hide findings below this severity |
| `--only <ids>` / `--disable <ids>` | — | Keep / drop suggestion rules by id |
| `--compare-source <path>` | `api/src/services/gas/estimator.ts` | Adopt its stroop constants and diff its `OPERATION_COMPLEXITY` table |
| `--baselines <path>` | — | JSON map of `operation -> { cpuInstructions, memoryBytes }` |
| `--list-rules` | — | Print the rule ids and exit |

### Exit codes

`0` ok · `1` gate failed · `2` usage error — scriptable for CI.

## What it measures

For every `pub fn` inside a `#[contractimpl] impl` block, the indexer walks the
body and every internal function it reaches, and counts:

- **`get` / `set` / `has` / `remove`** against `persistent`, `instance` and
  `temporary` storage, including aliased handles
  (`let storage = env.storage().persistent();`) and turbofish forms
  (`storage.get::<_, T>(..)`);
- **the transaction footprint** — the distinct ledger entries a call touches,
  which is the number that drives the resource fee and the rent burden;
- **outbound calls** — `env.invoke_contract` and Soroban token clients;
- **written entry size**, resolved from the `#[contracttype]` struct the value
  belongs to, so `HotStorageKey::DepositState` is reported as 48 bytes
  (three `i128` fields) rather than an unknown.

Details it handles that a grep-based approach does not:

- **transitive calls** — `deposit` in `lib.rs` delegates three levels down;
- **repeated calls** — a helper reached from two places is priced twice, because
  the runtime does execute it twice;
- **RAII** — `impl Drop for FlashLoanGuard::drop` is included even though nothing
  calls `drop`;
- **loops** — an access inside a `for` / `while` / `loop` body is flagged
  `perIteration` instead of being silently counted once;
- **noise** — `env.storage().set(..)` inside a string literal or a comment is
  not indexed, and `PoolError::Paused()` in call position is not reported as a
  missing function.

## Cost model

The defaults mirror the constants the API already uses in
`api/src/services/gas/estimator.ts`, and the tool **reads them from that file**
rather than keeping a second copy:

```
total = baseFee
      + ledgerWrites  × storageWriteStroops        (10,000, from STORAGE_WRITE_COST)
      + ledgerReads   × storageReadStroops         ( 1,000)
      + ledgerExists  × storageExistsStroops       (   500)
      + ledgerRemoves × storageRemoveStroops       ( 1,000)
      + scratchAccess × temporaryAccessStroops     (20,000)
      + outboundCalls × crossContractStroops       ( 5,000, from CROSS_CONTRACT_CALL_COST)
      + floor(cpuInstructions / cpuStroopDivisor)  + floor(memoryBytes / memoryStroopDivisor)
```

`baseFee` is `100` (`BASE_FEE`). 1 XLM = 10,000,000 stroops. `temporary`
(scratch) entries are never rent-bearing, so they are priced as one bucket
rather than folded into the read/write terms. The CPU and memory terms are
omitted unless `--baselines` supplies a measured value, which keeps the
storage-driven part of the estimate readable on its own.

### Baselines

`--baselines` accepts either a plain
`{ operation: { cpuInstructions, memoryBytes } }` map or the shape committed at
`stellar-lend/benchmarks/gas-baseline.json`
(`{ benchmarks: [{ operation, cpu_insns, mem_bytes }] }`). Where an operation
has several measured scenarios (cold and warm), the median is used.

The benchmark suite and the lending contract name the same operations
differently — the committed baseline was recorded against the `hello-world`
contract, where a deposit is `deposit_collateral` and a borrow is
`borrow_asset`. `BASELINE_NAME_ALIASES` bridges the two vocabularies so
`--baselines stellar-lend/benchmarks/gas-baseline.json` produces a combined
storage + resource estimate for the real lending pool.

## Suggestion rules

Every rule reads a *storage pattern* the indexer recovered. `--list-rules`
prints the current set.

| Rule | Category | Fires when |
| --- | --- | --- |
| `packable-key-namespace` | packing | one call mutates ≥2 entries of the same key enum |
| `repeated-key-read` | redundant-access | the same key is read more than once in one call |
| `redundant-existence-probe` | redundant-access | `has()` guards a nearby `get()`/`remove()` on the same key |
| `loop-amplified-storage` | loop-amplification | storage access sits inside a loop body |
| `instance-guard-writes` | instance-guard | ≥2 instance-tier writes per call |
| `legacy-migration-in-hot-path` | migration | legacy per-field keys are read as a fallback and deleted on commit |
| `wide-footprint` | footprint | a call touches ≥6 distinct ledger entries |
| `cross-contract-fanout` | cross-contract | ≥2 outbound contract calls |
| `unresolved-calls` | footprint | a call has no definition in the scanned tree |

Each finding carries a severity, the source lines that triggered it, and a
stroop estimate of the saving. Two examples from the current tree:

- `flash_loan` writes four instance entries to arm and disarm `FlashLoanGuard`.
  Packing the flags into one struct removes three writes per call.
- `DepositHotSlot::commit` deletes `TotalAmount`, `CapAmount` and `MinAmount`
  when the slot was rebuilt from legacy state, so the first `deposit` after an
  upgrade pays three extra reads and three extra deletes. The tree already
  ships `migrate_deposit_state` for exactly this.

## Reading the numbers

The report prints its own caveats, because static analysis has limits and a
reader should not have to guess them:

- counts are **static call sites**, so reads and writes in mutually exclusive
  branches are both counted — they are upper bounds. The distinct-entry
  footprint is exact;
- a helper called twice is priced twice; recursive call paths are cut to keep
  the walk finite;
- a variable key (`&key`) has no namespace, so it is never proposed as a packing
  candidate;
- helpers defined outside the scanned tree (the reentrancy guard, token
  transfers) are listed under `unresolvedCalls` and their cost is **not**
  included — the estimate is a lower bound for those operations.

## Tests

```bash
node --experimental-strip-types --test scripts/lending-gas-estimator/*.test.ts
```

83 tests covering the scanner (comment/string stripping, `#[contracttype]`
indexing, `use`-alias resolution, loop ranges), the indexer (tiers, turbofish,
RAII, loops, cross-contract, noise rejection — on both a purpose-built fixture
and the real lending contract), the cost model, and every suggestion rule.

## Files

| File | Role |
| --- | --- |
| `index.ts` | CLI, exit codes, flag parsing |
| `storage-indexer.ts` | contract source → per-entry-point storage facts |
| `rust-source.ts` | comment/literal-aware scanner, `fn` / `#[contracttype]` / `use` indexing |
| `estimator.ts` | cost model, drift audit, `text` / `json` / `markdown` renderers |
| `suggestions.ts` | the storage-pattern rules |
| `types.ts` | shared data shapes and the severity ordering |
| `fixtures/` | a small contract exercising every pattern the indexer must survive |

## Related

- `scripts/gas-report.sh`, `run-benchmarks.sh` — measured CPU/memory benchmarks
  and the regression gate. This tool complements them: they measure
  instructions, this prices the storage footprint.
- `scripts/storage-analyzer/` — storage **breaking-change** detection across
  versions. This tool is the **cost** side of the same question.
- `docs/GAS_ESTIMATION_SYSTEM.md` — the runtime `/api/gas/estimate` endpoints.
- `stellar-lend/docs/HOT_PATH_OPTIMIZATIONS.md` — the packed-storage and lazy-init
  work this tool is able to measure.
