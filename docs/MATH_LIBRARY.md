# Shared Math Library (`stellarlend-math`)

Written for: contract engineers working on interest, liquidation or risk code.

Every arithmetic rule the protocol depends on lives in one crate:
`stellar-lend/contracts/math`. Interest-rate curves, liquidation sizing,
fixed-point conversion and the checked primitives underneath them are defined
once, so two modules cannot drift apart on a rounding rule or an overflow
policy.

## Why one crate

Before consolidation, the kinked rate curve existed in three places
(`hello-world::interest_rate`, `lending-interest`, the risk modules) with three
different overflow behaviours, and liquidation sizing was open-coded at each
call site. A fix to one copy did not reach the others. Now those modules are
thin adapters: they convert their own config types into
[`RateCurve`](#rates-interest-rate-curves) and map the library's `MathError`
onto their own error enum.

## Module map

| Module | Use it for |
|---|---|
| `checked` | Environment-free checked `i128` arithmetic for hot paths |
| `rates` | Interest-rate curves, utilization, index accrual |
| `liquidation` | Health factors, penalties, seize amounts, batch priority |
| `lending` | Higher-level lending helpers built on the above |
| `fixed_point` | WAD (1e18) / RAY (1e27) fixed-point arithmetic |
| `mul_div` | `I256`-backed `mul_div` for intermediates wider than `i128` |
| `compound`, `exponential` | Compounding, `exp`/`ln` approximations |
| `int128`, `rounding`, `precision` | Primitives, rounding modes, precision tracking |

## `checked` — hot-path arithmetic

`mul_div::mul_div` routes through the `I256` host object. That is exact for any
intermediate, but it costs a host call per operation. `checked::checked_mul_div`
stays in native `i128` and needs no `Env`:

```rust
use stellarlend_math::checked::{apply_bps, checked_mul_div, ratio_bps};

let fee = apply_bps(amount, 250)?;          // 2.5% of amount
let utilization = ratio_bps(borrows, supply)?;
let scaled = checked_mul_div(a, b, d)?;
```

When `a * b` would overflow, `checked_mul_div` retries with a divide-first
decomposition (`(a/d)*b + ((a%d)*b)/d`) rather than failing, so accuracy holds
for large operands without paying for `I256` on every call. It reports
`MathError::Overflow` only when the *result* is genuinely unrepresentable.

**Which to use:** rate curves, health factors and liquidation sizing multiply
protocol-scale amounts by basis points, so they use `checked`. Reach for
`mul_div` when an intermediate genuinely exceeds `i128` — RAY-scaled (1e27)
products, for instance.

The measured difference over 100 iterations, from
`contracts/math/tests/gas_benchmarks.rs`:

| Path | CPU instructions / 100 ops |
|---|---|
| `checked_mul_div` | 0 |
| `mul_div` (`I256`) | 1,301,400 |

The benchmarks assert that the rate curves, liquidation sizing and interest
accrual each consume **zero** host CPU budget, so a change that routes them back
through the host fails the test suite rather than quietly raising fees.

## `rates` — interest-rate curves

All rates and utilizations are in basis points (10,000 bps = 100% APY).

| Curve | Shape |
|---|---|
| `linear_rate` | `base + u * slope / 10_000` |
| `kink_rate` | Two segments; each slope normalized by its segment width |
| `jump_rate` | Linear everywhere, plus an additive jump above the kink |
| `exponential_rate` | `base + slope * u² + jump * u³` |
| `dual_slope_rate` | Linear below the kink; the excess charged at the second slope |

`kink_rate` and `dual_slope_rate` are both real parameterizations in use in the
protocol and are **not** interchangeable: `kink_rate` divides each slope by the
width of its segment, `dual_slope_rate` applies the slopes to utilization
directly. Both live here rather than one approximating the other.

```rust
use stellarlend_math::rates::{RateCurve, RateModelKind};

let curve = RateCurve {
    kind: RateModelKind::Kink,
    base_rate_bps: 100,          // 1% at zero utilization
    kink_utilization_bps: 8_000, // steepens at 80%
    multiplier_bps: 2_000,
    jump_multiplier_bps: 10_000,
};
let borrow_rate = curve.borrow_rate(utilization)?;
```

Supporting functions:

- `utilization_bps(borrows, deposits)` — capped at 100%, zero for an empty pool.
- `apply_rate_bounds(rate, adjustment, floor, ceiling)` — applies a signed
  emergency adjustment, then clamps. Inverted bounds resolve to the floor, so a
  misconfigured pair can never widen a bound.
- `supply_rate_from_spread(borrow, spread, floor)` — the spread model.
- `supply_rate_from_reserve_factor(borrow, utilization, reserve_factor)` — the
  reserve-factor model.
- `simple_interest(principal, rate_bps, elapsed_seconds)`.
- `index_growth_factor` / `accrue_index` — advance a compounding borrow or
  supply index. Indexes never move backwards.

## `liquidation` — sizing and health

```rust
use stellarlend_math::liquidation::{
    dynamic_penalty_bps, health_factor_bps, max_repayable, split_proceeds,
};

let health = health_factor_bps(collateral_value, debt_value)?; // i128::MAX if debt-free
let penalty = dynamic_penalty_bps(collateral_value, debt, base_incentive, threshold)?;
let repay = max_repayable(total_debt, close_factor_bps, requested)?;
let split = split_proceeds(
    collateral_equivalent, collateral_balance, repay, penalty, protocol_fee_bps,
)?;
```

- `dynamic_penalty_bps` interpolates linearly from the base incentive toward
  `MAX_PENALTY_BPS` (20%) as a position falls below the liquidation threshold,
  and is capped there.
- `split_proceeds` caps the seizure at the borrower's balance and charges the
  protocol fee on the **incentive only**, never on the principal the liquidator
  repaid. It returns `MathError::Underflow` when the fee configuration would
  leave the liquidator with a negative amount.
- `is_profitable` gates the gas-heavy transfer path for batch liquidations.
- `priority_score` orders a batch, highest first. It uses saturating arithmetic
  deliberately: scoring is a heuristic and must never abort a batch.

## Error handling

The library reports failures as `MathError` (`Overflow`, `Underflow`,
`DivisionByZero`, `NegativeSqrt`, `ExponentTooLarge`). Contract modules map that
onto their own `#[contracterror]` enum at the boundary, so the library's type
never leaks into a contract's public signatures:

```rust
fn map_math_error(err: MathError) -> InterestRateError {
    match err {
        MathError::DivisionByZero => InterestRateError::DivisionByZero,
        _ => InterestRateError::Overflow,
    }
}
```

## Testing

```bash
cd stellar-lend
cargo test -p stellarlend-math                      # unit + doc tests
cargo test -p stellarlend-math --test gas_benchmarks -- --nocapture
cargo tarpaulin -p stellarlend-math                 # coverage
```

Every public function has unit tests covering its ordinary case, its boundary
cases (zero, the kink, the cap) and its failure modes. Doc examples are compiled
and run as doc-tests, so the snippets above cannot go stale.

## Adding a function

1. Put it in the module that matches its domain, not in a new one.
2. Return `Result<_, MathError>` for anything that can overflow. Saturating
   arithmetic is only for heuristics that must not abort a transaction, and the
   doc comment must say so.
3. Use `checked::*` unless an intermediate genuinely exceeds `i128`.
4. Document the formula in the doc comment, with a runnable example.
5. Cover ordinary, boundary and failure cases in tests.
6. If it lands on a hot path, add it to `tests/gas_benchmarks.rs`.
