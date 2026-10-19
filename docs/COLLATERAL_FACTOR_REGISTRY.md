# Collateral factor registry with risk-based parameters

Issue #1020. `contracts/lending-risk/src/collateral_registry.rs` registers each
collateral asset under a **risk tier**. The tier fixes ceilings for the asset's
parameters, so a volatile asset cannot be listed with a blue chip's settings, and
every change is validated as a whole.

## Tiers

All values are basis points.

| Tier | Max collateral factor | Max liquidation threshold | Max liquidation bonus | Min gap (threshold - factor) |
|---|---|---|---|---|
| `Conservative` | 8_000 | 8_500 | 500 | 300 |
| `Moderate` | 7_000 | 7_500 | 800 | 300 |
| `Aggressive` | 5_000 | 6_000 | 1_200 | 500 |
| `Isolated` | 3_500 | 4_500 | 1_500 | 500 |

Riskier tiers borrow less, liquidate earlier and pay liquidators more. `Isolated`
is for new or illiquid assets.

## Entry

`CollateralEntry { asset_id, tier, collateral_factor_bps, liquidation_threshold_bps, liquidation_bonus_bps, supply_cap, is_frozen }`.

- `collateral_factor_bps`: share of the collateral's value that can be borrowed.
- `liquidation_threshold_bps`: debt-to-collateral ratio at which the position is liquidatable.
- `liquidation_bonus_bps`: extra collateral a liquidator receives.
- `supply_cap`: most of the asset accepted (0 = uncapped).

## Validation (`validate_entry`)

In order:

1. **Basic ranges**: factor and threshold are positive, threshold is at most 10_000, bonus and cap are not negative (`InvalidParameter`).
2. **Tier ceilings**: factor, threshold and bonus do not exceed the tier's (`ExceedsTierBound`).
3. **Gap**: `threshold - factor >= min gap`, so a fresh borrow is not instantly liquidatable (`ThresholdGapTooSmall`).
4. **Coverage**: `threshold * (1 + bonus) <= 100%`, so paying the bonus at the threshold still leaves the debt covered (`BonusUndercollateralizes`). With the shipped ceilings this cannot fail; it guards future tier edits.

## Operations

| Method | Behaviour |
|---|---|
| `register(entry)` | validates; `AssetExists`, `RegistryFull` (32 assets) |
| `update_risk_params(id, cf, lt, bonus)` | validates the result as a whole; nothing changes on error |
| `set_tier(id, tier)` | the asset's current parameters must fit the new tier, otherwise tighten them first |
| `set_supply_cap(id, cap)` | 0 = uncapped; negative is rejected |
| `set_frozen(id, bool)` | a frozen asset adds **no** borrowing power and cannot be changed |
| `remove(id)` | frees the slot |
| `borrow_capacity(id, value)` | `value * factor / 10_000`, or 0 when frozen |
| `liquidation_limit(id, value)` | `value * threshold / 10_000` |

Overflow surfaces as `RegistryError::Overflow`.

## Using it from a contract

The registry is a plain value with no storage. A contract keeps it (or its
entries) in its own storage, calls the mutating methods behind its admin or
governance checks, and uses `borrow_capacity` / `liquidation_limit` in place of
hard-coded factors. Parameter changes should go through the timelocks in
[PARAMETER_STORE.md](./PARAMETER_STORE.md).
