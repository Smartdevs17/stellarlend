# Interest Rate Model

Issue #1194. The borrow rate is a kinked curve over pool utilization, with every
parameter configurable by the admin.

## Model

```
utilization = total_debt * 10_000 / debt_ceiling            (0 when either is 0)

borrow rate = base + utilization * slope / kink                       utilization <= kink
            = base + slope + (utilization - kink) * jump_slope / (10_000 - kink)   above it

borrow rate is clamped to [rate_floor, rate_ceiling]
supply rate = max(borrow rate - spread, rate_floor)
```

All values are basis points per year.

| Parameter | Default | Allowed |
|---|---|---|
| `base_rate_bps` | 500 | 0 to 10_000 |
| `kink_utilization_bps` | 8_000 | 1 to 9_999 |
| `slope_bps` | 2_000 | >= 0 |
| `jump_slope_bps` | 10_000 | >= 0 |
| `rate_floor_bps` | 0 | 0 to 10_000, and <= `rate_ceiling_bps` |
| `rate_ceiling_bps` | 10_000 | 0 to 10_000 |
| `spread_bps` | 200 | 0 to 10_000 |

With the defaults:

| Utilization | Borrow rate | Supply rate |
|---|---|---|
| 0% | 500 | 300 |
| 40% | 500 + 4_000 * 2_000 / 8_000 = 1_500 | 1_300 |
| 80% (kink) | 500 + 2_000 = 2_500 | 2_300 |
| 90% | 2_500 + 1_000 * 10_000 / 2_000 = 7_500 | 7_300 |
| 100% | 12_500, clamped to 10_000 | 9_800 |

## Entry points

| Function | Purpose |
|---|---|
| `get_interest_rate_config()` | Current parameters |
| `set_interest_rate_config(admin, update)` | Change parameters (admin only); returns the new config |
| `get_borrow_rate_bps()` / `get_supply_rate_bps()` | Current rates |
| `get_utilization_bps()` | Current utilization |

`update` is an `InterestRateConfigUpdate` whose fields are all `Option`: `None`
leaves a parameter unchanged. The whole update is validated before anything is
written, so an update with one invalid field changes nothing. Each change emits
`RateConfigUpdatedEvent` with the caller as topic and the new parameters.

To put rate changes under governance, set the protocol admin to the governance
or timelock contract; `set_interest_rate_config` then only succeeds when that
contract calls it.

## Accrual

Variable-rate debt accrues at the current borrow rate over the time since a
position last updated. A rate change therefore also prices the part of that
period that has not been accrued yet. Stable-rate positions keep the rate they
borrowed at.
