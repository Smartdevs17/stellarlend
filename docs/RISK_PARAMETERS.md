# Risk Parameters

Every admin-configurable risk figure in the lending protocol, with its default,
its allowed bounds, and the reasoning behind it. All values are **basis points
(bps)** where 10,000 = 100% unless a row says otherwise. "Admin" means the
protocol admin set at `initialize`; every setter re-checks `require_auth` and
emits an update event.

Source of truth: the constants and getters in
`stellar-lend/contracts/lending/src/` (`borrow.rs`, `interest_rate.rs`,
`utilization_breaker.rs`, `risk_monitor.rs`, `flash_loan.rs`, `insurance.rs`,
`mev_protection.rs`). If a value here disagrees with the code, the code wins —
please open a PR or an issue.

## Core solvency parameters

| Parameter | Default | Bounds | Setter | Why |
| --- | --- | --- | --- | --- |
| Minimum collateral ratio | **15,000 (150%)** | fixed constant | — (compile-time) | Buffer against price movement: a position opens far from the liquidation point so ordinary volatility does not immediately threaten it. Enforced on borrow and on collateral withdrawal. |
| Liquidation threshold | **8,000 (80%)** | admin-set, ≤ 10,000 | `set_liquidation_threshold_bps` | The collateral-value/debt ratio below which a position is liquidatable. Kept strictly below the opening ratio so liquidation is a later stage, not a surprise. |
| Close factor | **5,000 (50%)** | `> 0` and `≤ 10,000` | `set_close_factor_bps` | Caps how much of a position one liquidation can repay, so a single liquidator cannot seize everything at once and borrowers keep recovery options. |
| Liquidation incentive | **1,000 (10%)** | `100–2,500` (1%–25%) | `set_liquidation_incentive_bps` (global), per-asset override | Compensates liquidators for taking on debt; bounded so distressed borrowers are not penalized beyond what clearing the position requires. Out-of-bounds values revert with `IncentiveOutOfBounds` (#17). |
| Debt ceiling | admin-set | `> 0`, `≥` min borrow | borrow limits setter | Caps protocol-wide debt so exposure is a policy decision, not an emergent one. Borrows above it revert with `DebtCeilingReached` (#2). |
| Minimum borrow amount | admin-set | `> 0`, `≤` debt ceiling | borrow limits setter | Prevents dust positions whose monitoring and liquidation cost exceeds their value; below it borrows revert with `BelowMinimumBorrow` (#8). |
| Max collateral assets per position | **8** | fixed constant (`MAX_COLLATERAL_ASSETS`) | — (compile-time) | Bounds the work (and thus the cost) of health-factor evaluation per call; the 9th asset reverts with `TooManyCollateralAssets` (#14). |

## Interest rate model

`InterestRateConfig` (`interest_rate.rs`) is a kinked two-slope curve: rates
rise gently up to the kink, then jump, so utilization above the kink is priced
sharply and repayed first.

| Field | Default (bps) | Meaning |
| --- | --- | --- |
| `base_rate_bps` | 500 | Borrow rate at zero utilization |
| `kink_utilization_bps` | 8,000 | Utilization where the slope changes |
| `slope_bps` | 2,000 | Slope below the kink |
| `jump_slope_bps` | 10,000 | Additional slope above the kink |
| `rate_floor_bps` | 0 | Lowest rate the model may return |
| `rate_ceiling_bps` | 10,000 | Highest rate the model may return |
| `spread_bps` | 200 | Spread between borrow and supply rates |

Stable-rate borrowing adds: a stable premium of **100 bps (1%)** over the
variable rate, recalculation every **6 hours**, and a rate-switch fee of
**10 bps (0.10%)**.

## Utilization circuit breaker

`UtilizationBreakerConfig` throttles new borrowing near the debt ceiling so a
pool cannot be driven to 100% utilization in one burst. States, from the module
docs:

| State | A borrow is accepted when |
| --- | --- |
| Disabled (default, `enabled = false`) | always |
| Armed | post-borrow utilization `< trip_utilization_bps`, or amount ≤ `max_borrow_while_tripped` |
| Tripped | amount ≤ `max_borrow_while_tripped` |

| Parameter | Bounds | Meaning |
| --- | --- | --- |
| `enabled` | bool, default `false` | Master switch; the breaker is inert until configured |
| `trip_utilization_bps` | `1..=10,000` | Utilization at/above which the breaker trips |
| `reset_utilization_bps` | `≤ trip` | Utilization below which a tripped breaker re-arms (hysteresis) |
| `max_borrow_while_tripped` | `≥ 0` | Largest single borrow while throttled; `0` = hard stop |

Repayments are never throttled. A rejected borrow reverts with
`UtilizationThrottled` (#13); configure and reset via
`set_utilization_breaker` / `reset_utilization_breaker`.

## Risk alert thresholds (monitoring)

`RiskAlertThresholds` (`risk_monitor.rs`) emits deduplicated events as
utilization reaches `warning` / `critical` / `emergency` levels. Disabled while
unset (all zeros); they do not block anything — they are for indexers and
off-chain monitoring. Set via the thresholds admin entry point with
`warning ≤ critical ≤ emergency`.

## Flash loan parameters

| Parameter | Default | Bounds | Why |
| --- | --- | --- | --- |
| Fee | admin-set | `≤ 1,000` (10%) per loan | Revenue for the pool; bounded so it cannot approach the profit of a legitimate arbitrage. |
| Max loan-to-liquidity | 5,000 (50%) | admin-set | Keeps an atomic loan from consuming all liquidity available to users. |
| Max price impact | 2,000 (20%) | admin-set | Refuses loans whose repayment path would move the price beyond the pool's tolerance. |
| Max TWAP deviation | 1,000 (10%) | admin-set | Rejects loans against a sporadically priced asset. |
| TWAP window / observations | 600 s / 32 | admin-set | Smooths the price oracle window used to value the loan. |

## Insurance parameters

| Parameter | Default | Bounds | Why |
| --- | --- | --- | --- |
| Coverage limit | admin-set | `≤ 5,000` (50%) | Caps insured share of a position so the marketplace cannot underwrite the whole pool. |
| Emergency fund share | 2,000 (20%) | fixed | Portion of premiums set aside to cover insolvencies. |
| Premium | base 50 bps | `10–500` | Bounds the cost of cover; base is the starting quote before risk adjustments. |

## MEV and rate-guard defaults

- **Reveal delay**: **30 s** (`mev_protection.rs`) — the delay between commit
  and reveal for protected operations, enough to break same-block sandwiching
  without stalling normal flows.
- **Rate guard denominator**: 10,000 bps (`rate_guard.rs`) — sanity bounds for
  rate-of-change checks on price and utilization inputs.

## Changing parameters safely

1. Apply changes with the multisig admin flow where available
   (`ms_propose_set_min_cr`, `ms_approve`, `ms_execute`), not a single key.
2. Check the secondary effects: raising the lending rate ceiling only matters if
   the kink is high; raising the close factor shortens liquidations but
   increases their size; moving the liquidation threshold closer to the minimum
   collateral ratio narrows the safety buffer.
3. Watch the events: every setter emits an update event with previous and new
   values, which indexers can alarm on (see
   [`event-indexing.md`](event-indexing.md)).
4. Parameters that are compile-time constants (columns marked "fixed") change
   only in a contract upgrade, which is deliberate: they are part of the
   safety envelope the audits were performed against.
