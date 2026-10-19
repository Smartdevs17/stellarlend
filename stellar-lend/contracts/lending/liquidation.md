# Multi-Collateral Positions and Liquidation

Issues #1193 (per-asset liquidation incentive) and #1195 (multi-collateral positions).

## Collateral

A borrow position can hold up to `MAX_COLLATERAL_ASSETS` (8) distinct collateral
assets. `deposit_collateral` and `borrow` add to the balance of the given asset,
creating an entry the first time an asset is used. A ninth distinct asset is
rejected with `TooManyCollateralAssets`; topping up a held asset is always allowed.

| Function | Returns |
|---|---|
| `get_user_collaterals(user)` | `Map<asset, amount>` of every collateral asset |
| `get_user_collateral(user)` | Legacy single-asset view: the asset with the largest raw balance |
| `get_collateral_value(user)` | Sum over assets of `amount * price / 1e8` |
| `get_health_factor(user)` | `collateral_value * liquidation_threshold / debt_value`, 10_000 = 1.0 |

Positions written before multi-collateral support are read from the old
single-asset record and moved into the map on their next write, so existing
positions need no migration step.

## Liquidation

```rust
liquidate(liquidator, borrower, debt_asset, collateral_asset, amount) -> LiquidationResult
```

A position is liquidatable once its health factor, computed over **all** its
collateral assets, is below 1.0. The liquidator chooses which collateral asset to
seize.

1. `repay = min(amount, total_debt * close_factor)`.
2. `seize = repay * debt_price * (1 + incentive) / collateral_price`, using the
   incentive of the seized asset (below).
3. If the borrower holds less of that asset than `seize`, all of it is seized and
   `repay` shrinks to what it covers, so the incentive is always honoured.
4. Debt is reduced interest first, then principal; the seized amount moves from
   the borrower's position to the liquidator's.

Seized amounts round down, in the borrower's favour.

| Error | When |
|---|---|
| `PositionHealthy` | Health factor is at or above 1.0, or there is no debt in `debt_asset` |
| `PriceUnavailable` | The oracle is unset or returns no usable price for the debt asset or **any** collateral asset |
| `InsufficientCollateral` | The borrower holds none of `collateral_asset` |
| `AssetNotSupported` | The borrower's debt is in a different asset |
| `SelfLiquidation` | `liquidator == borrower` |
| `ProtocolPaused` | Liquidation is paused |

A missing price aborts the liquidation instead of counting that collateral as
worthless, so an oracle outage cannot make a healthy position liquidatable.

Settlement uses the borrow module's ledger, like `repay`: the borrow module holds
no tokens in custody, so neither the repayment nor the seized collateral is a
token transfer.

Each liquidation emits the shared `LiquidationEvent` (`liquidator` and `borrower`
topics; `debt_liquidated`, `collateral_seized`, `incentive_amount` in the payload).

## Liquidation incentive (penalty)

The incentive is the bonus a liquidator receives on top of the debt they repay,
paid by the borrower. Each collateral asset can override the global value.

| Function | Purpose |
|---|---|
| `set_liquidation_incentive_bps(admin, bps)` | Global incentive for assets without an override |
| `get_liquidation_incentive_bps()` | Global incentive (default 1_000 = 10%) |
| `set_asset_liq_incentive_bps(admin, asset, Some(bps))` | Override for one collateral asset |
| `set_asset_liq_incentive_bps(admin, asset, None)` | Clear the override; the asset uses the global value again |
| `get_asset_liq_incentive_bps(asset)` | Incentive in effect for that asset |

Both setters are admin-only and require
`MIN_LIQUIDATION_INCENTIVE_BPS <= bps <= MAX_LIQUIDATION_INCENTIVE_BPS`
(100 to 2_500, i.e. 1% to 25%), rejecting anything else with
`IncentiveOutOfBounds`:

- Below 1%, liquidating rarely covers a liquidator's costs, so unhealthy
  positions linger.
- At 25% with the default 80% liquidation threshold,
  `threshold * (1 + incentive) = 100%`: a liquidation never seizes more value
  than backs the debt it repays.

Every change emits `LiqIncentiveUpdatedEvent` (`admin` topic; `asset` is `None`
for the global value, plus `previous_bps` and `new_bps`).
