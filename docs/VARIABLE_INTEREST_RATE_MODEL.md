# Variable interest rate model with utilization-based pricing

Issue #1018. The borrow rate rises with pool **utilization**: cheap when the pool
is mostly idle, steep when it is nearly fully borrowed, so borrowers are pushed to
repay and depositors are pushed to supply before liquidity runs out.

The implementation is `contracts/lending-interest` (`InterestRateModel`,
`calculate_utilization`, the index cache and accrual helpers). The curve maths is
shared with the rest of the protocol through `stellarlend-math`.

## Utilization

```
utilization_bps = total_borrows * 10_000 / total_supply      (0 when total_supply is 0)
```

`calculate_utilization` returns basis points (0 to 10_000) and reports overflow as
an error instead of a wrong number.

## Borrow rate: two slopes around a kink

`InterestRateModel { base_rate, slope1, slope2, optimal_utilization }` (all basis
points). `optimal_utilization` is the kink.

| Utilization | Rate |
|---|---|
| at or below the kink | `base_rate + utilization * slope1 / 10_000` |
| above the kink | `base_rate + kink * slope1 / 10_000 + (utilization - kink) * slope2 / 10_000` |

`slope2` is much larger than `slope1`, which is the "jump" that makes the last
stretch of utilization expensive.

Worked from the formula, with `base_rate = 200`, `slope1 = 400`, `slope2 = 6_000`,
`optimal_utilization = 8_000`:

| Utilization | Borrow rate |
|---|---|
| 50% (5_000) | 200 + 5_000 * 400 / 10_000 = 400 bps (4.00%) |
| 80% (8_000, the kink) | 200 + 8_000 * 400 / 10_000 = 520 bps (5.20%) |
| 90% (9_000) | 520 + (9_000 - 8_000) * 6_000 / 10_000 = 1_120 bps (11.20%) |

## Supply rate

```
supply_rate = borrow_rate * (10_000 - reserve_factor) / 10_000 * utilization / 10_000
```

Depositors earn the borrow rate, less the protocol's reserve factor, spread over
the whole pool (the `utilization` term). At zero utilization nobody pays interest
and the supply rate is zero.

## Accrual

Interest is tracked with a global index (`InterestIndexCache`) rather than per
position history. A position stores the index it last saw; accrual multiplies by
the growth of the index since then. See [INTEREST_CACHE.md](../stellar-lend/docs/INTEREST_CACHE.md)
for the cache, and [INTEREST_NUMERIC_ASSUMPTIONS.md](../stellar-lend/docs/INTEREST_NUMERIC_ASSUMPTIONS.md)
for precision and overflow behaviour. Continuous compounding variants
(`accrue_interest_continuous`, `update_interest_cache_continuous`) are available.

## Guarantees

- Overflow in utilization, the rate or accrual returns `MathError`, never a
  silently wrong value.
- The rate is monotonic in utilization: a higher utilization never lowers it
  (`test_interest_rate_model_below_kink` covers the ordering across the kink).

## Choosing parameters

- Put the kink where you want the pool to sit in normal operation (commonly
  80-90%).
- Keep `slope1` small enough that ordinary borrowing is cheap, and `slope2` large
  enough that pushing past the kink is clearly uneconomic.
- Governance changes to these values belong behind the parameter store's
  timelocks ([PARAMETER_STORE.md](./PARAMETER_STORE.md)).
