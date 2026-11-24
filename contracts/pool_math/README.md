# stellarlend-pool-math (issue #1759)

A dependency-free `no_std` crate holding the arithmetic that both the pool
contracts and off-chain SDK consumers need. No `Env`, storage, or host objects
are used, so the same functions can be compiled into Soroban contracts,
tools, and tests.

## API

| Function | Purpose |
| --- | --- |
| `checked_mul_div` | Overflow-safe `a * b / d` with divide-first fallback |
| `apply_bps` / `ratio_bps` | Basis-point helpers on a 10,000 scale |
| `utilization_bps` | Borrow utilisation, clamped to `10_000` |
| `health_factor_bps` | Collateral × liquidation threshold / debt |
| `max_borrow_amount` | Borrowing power after existing debt |
| `simple_interest` / `accrue_index` | Rate and index accrual |
| `seize_amount` / `max_repayable` | Liquidation sizing |

A debt-free position returns `NO_DEBT_HEALTH_FACTOR` (`i128::MAX`). Negative
inputs and zero denominators are rejected with `MathError` instead of
silently wrapping; `ratio_bps` treats a zero denominator as `0`.

The TypeScript sibling in `packages/stellarlend-math` mirrors these semantics
with `bigint`, so SDK callers do not reimplement the formulas.

## Usage

```rust
use stellarlend_pool_math::{apply_bps, health_factor_bps};

let threshold_value = apply_bps(20_000, 8_000)?; // 16_000
let health = health_factor_bps(20_000, 10_000, 8_000)?; // 16_000 bps
```

## Validation

```bash
cargo test --manifest-path contracts/pool_math/Cargo.toml
```
