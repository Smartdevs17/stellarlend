# Utilization Circuit Breaker

The lending contract can throttle new borrowing once protocol utilization passes an
admin-set threshold. Without it a pool can be borrowed to 100% of its debt ceiling in a
single burst, which leaves no headroom for liquidations or withdrawals.

Source: [`stellar-lend/contracts/lending/src/utilization_breaker.rs`](../stellar-lend/contracts/lending/src/utilization_breaker.rs)
Tests: [`utilization_breaker_test.rs`](../stellar-lend/contracts/lending/src/utilization_breaker_test.rs)

## Utilization

```
utilization_bps = total_debt * 10_000 / debt_ceiling      (clamped to 0..=10_000)
```

This is the same measure the interest-rate model and the risk-alert monitor use.

## Configuration

```rust
pub struct UtilizationBreakerConfig {
    pub enabled: bool,
    pub trip_utilization_bps: u32,      // 1..=10_000
    pub reset_utilization_bps: u32,     // <= trip_utilization_bps
    pub max_borrow_while_tripped: i128, // >= 0; 0 = no new borrows while tripped
}
```

The breaker is **off until configured**. Configuration is validated. Out-of-range or
inconsistent values are rejected with `UtilizationBreakerError::InvalidConfig`.

| Entrypoint | Who | Effect |
|---|---|---|
| `set_utilization_breaker(admin, config)` | admin | Install, change or disable. Re-evaluates the trip latch against current utilization. |
| `reset_utilization_breaker(admin)` | admin | Clear the latch without changing config. No-op when not tripped. |
| `get_utilization_breaker()` | anyone | `{ config, tripped, tripped_at, utilization_bps }` |

## Behaviour

A borrow of `amount` that would bring utilization to `post_util`:

| breaker | accepted when |
|---|---|
| disabled / unset | always |
| armed | `post_util < trip`, **or** `amount <= max_borrow_while_tripped` |
| tripped | `amount <= max_borrow_while_tripped` |

Rejected borrows fail with `BorrowError::UtilizationThrottled` (`#13`).

- **Trip:** any debt change that leaves utilization `>= trip` latches the breaker.
- **Auto reset:** once tripped, the breaker stays tripped until utilization falls
  **below `reset`** (through repayments or dust sweeps). Setting `reset < trip` gives
  hysteresis, so the breaker does not flap around the threshold.
- **Repayments are never throttled.** They can only lower utilization.
- The breaker composes with the debt ceiling. A borrow above the ceiling still fails with
  `DebtCeilingReached` first.

### Example: 1,000,000 USDC ceiling, trip 90%, reset 80%, 10,000 USDC allowance

| utilization change | event | result |
|---|---|---|
| 70% → 85% (150k borrow) | none | accepted: lands below trip |
| 85% → 89.5% (45k borrow) | none | accepted |
| 89.5% → 92% (25k borrow) | none | **rejected**: crosses trip and 25k > 10k |
| 89.5% → 90% (5k borrow) | **tripped** | accepted: within allowance |
| 90% → 84% (repayments) | none | still latched (84% ≥ reset): only borrows ≤ 10k |
| 84% → 79% (repayments) | **reset** | re-armed: normal borrowing resumes |

## Reversibility

Every effect can be undone by the admin without a code upgrade:

- **Lift throttling immediately:** `set_utilization_breaker` with `enabled: false`. A
  tripped breaker is reset and emits a `manual` reset event.
- **Loosen:** raise `trip` above current utilization. The latch resets.
- **Tighten:** lower `trip` below current utilization. The breaker trips immediately.
- **Clear the latch:** `reset_utilization_breaker`. If utilization is still at or above
  `trip`, the next borrow re-trips it.

## Events

| Event (topic) | Fields | When |
|---|---|---|
| `util_breaker_configured_event` (+ `admin` topic) | `enabled, trip_utilization_bps, reset_utilization_bps, max_borrow_while_tripped, timestamp` | every successful `set_utilization_breaker` |
| `util_breaker_tripped_event` | `utilization_bps, trip_utilization_bps, total_debt, debt_ceiling, timestamp` | armed → tripped |
| `util_breaker_reset_event` | `utilization_bps, manual, timestamp` | tripped → armed (`manual = false` for auto reset) |

A rejected borrow reverts the whole invocation, so it emits nothing. Indexers see the
failed transaction's `Error(Contract, #13)` instead.

## Operating it

```bash
stellar contract invoke --id "$LENDING" --source admin --network testnet -- \
  set_utilization_breaker --admin "$ADMIN" --config \
  '{"enabled":true,"trip_utilization_bps":9000,"reset_utilization_bps":8000,"max_borrow_while_tripped":"100000000000"}'

stellar contract invoke --id "$LENDING" --source admin --network testnet --send=no -- \
  get_utilization_breaker
```

Frontends can read the status before submitting a borrow. See
[`packages/sdk/examples/utilization-breaker.ts`](../packages/sdk/examples/utilization-breaker.ts).

## Storage

Two persistent keys, `UtilizationBreakerKey::Config` and `UtilizationBreakerKey::TrippedAt`.
Contracts deployed before this feature have neither, which means "disabled". No migration
is needed.
