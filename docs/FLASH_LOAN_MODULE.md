# Flash loan module: configurable fees and limits

Issue #1019. `contracts/flash-loan` holds the receiver interface
(`FlashLoanReceiver`, `FlashLoanMetrics`) and, in `pool.rs`, the rules a pool
applies to every loan: fee, size limits, pool-share cap and repayment checking.
The module is pure arithmetic (no storage, no `Env`), so a pool contract calls it
around its own token transfers.

## Configuration

`FlashLoanConfig`:

| Field | Meaning | Valid range |
|---|---|---|
| `fee_bps` | fee as basis points of the borrowed amount | 0 to `MAX_FEE_BPS` (1_000 = 10%) |
| `min_amount` | smallest loan | at least 1 |
| `max_amount` | largest loan | at least `min_amount` |
| `max_pool_share_bps` | most of the pool's available liquidity one loan may take | 1 to 10_000 |
| `enabled` | when false every loan is refused | |

`FlashLoanConfig::new(..)` validates and returns `FlashLoanError::InvalidConfig`
for anything out of range. `Default` is a 9 bps fee, 1 to `i128::MAX`, 90% pool
share.

## Fee

```
fee = ceil(amount * fee_bps / 10_000)
```

Rounded **up**: a non-zero fee rate never rounds a small loan's fee to zero (so a
loan of 100 at 9 bps pays 1, not 0). A zero rate charges nothing. Overflow returns
`FlashLoanError::Overflow`.

## Flow

```text
1. fee = config.check_loan(amount, available_liquidity)?      // before sending funds
2. balance_before = pool balance
3. transfer `amount` to the receiver; call on_flash_loan(user, asset, amount, fee)
4. config.check_repayment(amount, balance_before, pool balance now)?
5. record_loan(&mut metrics, amount, fee)?
```

`check_loan` refuses, in this order: disabled pool, non-positive amount, below
`min_amount`, above `max_amount`, more than the available liquidity, more than
`max_pool_share_bps` of it. `check_repayment` requires the pool to hold at least
`balance_before + fee` afterwards. It checks the **balance**, not the callback's
word, so a receiver that lies cannot keep the funds.

Any error must revert the whole transaction so the transfer in step 3 is undone.

## Errors

`InvalidConfig`, `Disabled`, `InvalidAmount`, `BelowMinimum`, `AboveMaximum`,
`InsufficientLiquidity`, `ExceedsPoolShare`, `RepaymentTooLow`, `Overflow`.

## Metrics

`record_loan` adds a completed loan to `FlashLoanMetrics` (count, volume, fees).
It computes every total first and only then writes, so an overflow leaves the
metrics unchanged.

## Relationship to the lending contract

The flash loan logic in `contracts/hello-world/src/flash_loan.rs` and its tests
(`FLASH_LOAN_TESTS.md`) are separate from this crate. This module provides the
reusable, dependency-free rules; wiring it into a pool contract is the pool's job.
