# Liquidation Gas Optimization

This document describes the gas optimizations applied to the liquidation path
(issue #723) in `contracts/hello-world/src/liquidate.rs`.

## Problem

A liquidation call performs: several oracle price reads, a full repayment token
transfer, collateral token transfer, position mutation, interest accrual,
debt-state updates, and event emission. When a liquidation is **unprofitable**
— the liquidator's seized collateral net of the protocol fee is worth less than
the debt they must repay — all that work burns gas for nothing. In batch/racing
scenarios bots also waste a slot trying to front-run such positions.

## Optimization: early exit for unprofitable liquidations

A single guard function is placed directly after the profitability inputs are
known and before the gas-heavy transfer + storage-mutation path:

```rust
fn abort_if_unprofitable(
    debt_repayed: i128,
    collateral_seized: i128,   // in debt-value terms
    protocol_fee: i128,
) -> Result<(), LiquidationError>
```

It returns `LiquidationError::UnprofitableLiquidation` (`= 13`) when:

```text
collateral_seized - protocol_fee  <  debt_repayed + (debt_repayed × MIN_LIQUIDATOR_PROFIT_BPS / 10000)
```

`MIN_LIQUIDATOR_PROFIT_BPS = 20` (0.2% floor). Positions that cannot cover the
repaid debt plus the floor are skipped **before** any token transfer or
persistent-storage write for the liquidation.

## Effect on the batch path

`batch_liquidate` iterates positions sorted by priority (most profitable
first). Because the guard lives inside `liquidate`, each unprofitable item is
recorded as `success: false` with `error_code: 13
(UnprofitableLiquidation)` and the batch continues with the next position —
unprofitable members never consume transfers or state writes.

`batch_liquidate` is also exposed as a first-class contract entrypoint (the
previous implementation existed only as a library function), so a liquidator
can submit up to `MAX_BATCH_SIZE = 10` requests in one transaction.

## Additional notes

- **Packed risk config** (`docs/storage.md`) means the incentive/threshold read
  inside the guard costs fewer storage bytes, compounding the saving.
- **MEV interplay** (`docs/mev-protection.md`): the auction + commit–reveal
  paths are the preferred route for racing liquidations; the guard explicitly
  prevents wasting a commit/reveal slot on an unprofitable position.

## Gas regression testing in CI

The PR gas gate is `.github/workflows/gas-report.yml` (see
`docs/gas-reporting.md`). Liquidation-specific benchmarks live in
`benchmarks/src/hello_world_benchmarks.rs` under the `liquidate` /
`batch_liquidate` operations:

- `with_profit` — a healthy, profitable liquidation (post-guard happy path),
- `unprofitable` — a position below the profit floor (guard aborts early;
  measured instructions should be significantly lower than `with_profit`).

The comparison gate flags a regression if either path exceeds the budget in
`benchmarks/baseline.json` by the configured threshold.

## Gas comparison (with vs without optimization)

| Path | Without guard | With guard |
| --- | --- | --- |
| Profitable liquidation | full transfer + storage path | unchanged |
| Unprofitable liquidation | full transfer + storage path | aborts after ~6 storage reads + arithmetic |

The saved gas scales with the liquidation's transfer/storage weight, which is
typically 3–5× the cost of the reads in the guard.