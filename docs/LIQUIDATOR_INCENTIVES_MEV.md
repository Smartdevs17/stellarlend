# Liquidator incentives and MEV protection

Issue #1021. `contracts/lending-risk/src/liquidator_incentives.rs` provides two
independent pieces a liquidation path can use: a bonus schedule that rewards
committed liquidators, and a guard that refuses liquidations that look like
ordering abuse. Both are pure functions and plain structs (no storage, no `Env`).

## Incentives

A liquidator earns the asset's **base bonus** plus a **loyalty bonus** that grows
with the volume it has already liquidated.

| Cumulative volume liquidated | Loyalty bonus |
|---|---|
| under 100_000_000 | 0 bps |
| at least 100_000_000 | +25 bps |
| at least 1_000_000_000 | +50 bps |
| at least 10_000_000_000 | +100 bps |

`effective_bonus_bps(base, volume, asset_max)` returns `base + loyalty`, capped at
the asset's own maximum (from the collateral registry,
[COLLATERAL_FACTOR_REGISTRY.md](./COLLATERAL_FACTOR_REGISTRY.md)) and at
`MAX_TOTAL_BONUS_BPS` (1_500, 15%). Incentives therefore never push a bonus past
what the asset's tier allows.

`liquidator_reward(repaid_value, bonus_bps)` is `repaid * bonus / 10_000`.
`LiquidatorStats` keeps a liquidator's volume, count and rewards; `record` updates
all three or none.

## MEV protection

`MevGuardConfig::check_liquidation(last_liquidated_ledger, current_ledger,
liquidations_this_ledger, reference_price, observed_price)` returns `Ok(())` or
the first failing check, in this order:

| Check | Refused when | Error |
|---|---|---|
| Cooldown | the same position was liquidated fewer than `min_ledgers_between_liquidations` ledgers ago (or its recorded ledger is in the future) | `CooldownActive` |
| Ledger cap | this ledger already holds `max_liquidations_per_ledger` liquidations | `LedgerCapReached` |
| Price band | the observed price is more than `max_price_deviation_bps` from the reference price (for example a TWAP) | `PriceDeviationTooHigh` |

A non-positive price is `InvalidPrice`, and arithmetic overflow is `Overflow`.
Defaults: 2 ledgers between liquidations of one position, 20 per ledger, a 5%
band.

What each check is for:

- **Cooldown** stops bundling repeated partial liquidations of one position around
  a price move.
- **Ledger cap** bounds how much any one ledger can be manipulated.
- **Price band** rejects a liquidation priced off a spot price that has been pushed
  away from the reference, the signature of a sandwich or oracle push.

The guard does not replace commit-reveal or private routing; it is a cheap
on-chain backstop that needs only a reference price and two counters.

## Using it

```text
bonus  = effective_bonus_bps(entry.liquidation_bonus_bps, stats.total_volume, entry.tier.bounds().max_liquidation_bonus_bps)?
guard.check_liquidation(last_ledger, ledger_now, count_this_ledger, twap, spot)?
reward = liquidator_reward(repaid_value, bonus)?
stats.record(repaid_value, reward)?
```

The contract supplies the ledger numbers, prices and per-position and
per-ledger counters from its own storage.
