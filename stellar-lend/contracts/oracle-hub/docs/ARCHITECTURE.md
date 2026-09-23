# Oracle Hub – Architecture

## Overview

The hub is a single Soroban contract (`OracleHubContract`) whose logic is split
into focused modules. Prices are produced by combining one or more per-asset
`PriceFeed` slots, each of which is either **push** (an oracle signs a price)
or **pull** (an external `PriceProvider` contract is queried live). Feed
quotes are filtered for outliers and combined with the asset's effective
aggregation strategy. Health monitoring and emergency freezes gate whether an
asset may serve prices at all.

## Module layout

| Module | Responsibility |
| ------ | -------------- |
| `lib.rs` | Contract entry points, auth, global freeze, orchestration |
| `types.rs` | Data types, constants, events |
| `storage.rs` | `DataKey` layout |
| `interface.rs` | `PriceProvider` trait + generated `PriceProviderClient` |
| `provider.rs` | Pull-based provider fetch, validation, timestamp clamp |
| `aggregation.rs` | Median and weighted strategies, outlier filtering |
| `health.rs` | Feed classification, failure counters, circuit breaker |
| `upgrade.rs` | Staged/apply upgrade mechanism |
| `tests/` | 6 focused test suites (55 tests) |

## Price production flow

```
report_price (push) ──► PricePoint per (asset, priority)
provider get_price  ──► fetch_provider_price ─► PricePoint per (asset, priority)
                             (pull)

                        get_price(asset):
                            for priority in Primary..Fallback:
                                skip disabled / no point feeds
                                jump out stale feeds (auto-disable; event)
                                collect FeedQuote
                            aggregate(quotes, effective_strategy):
                                reject outliers > 20% from median
                                Median   -> median price, avg confidence
                                Weighted -> Σ(price*w*conf)/Σ(w*conf)
                            recover auto-breaker if healthy
                            return AggregatedPrice
```

Effective strategy resolution in `get_aggregation_strategy`:
per-asset override (`DataKey::Strategy(asset)`) else the hub default
(`DataKey::DefaultStrategy`, initialized to `Median`).

## Storage layout

All keys live in instance storage. `DataKey` variants (see `storage.rs`):

| Key | Type | Purpose |
| --- | ---- | ------- |
| `Governance` | `Address` | Governor (auth for management ops) |
| `Admin` | `Address` | Admin (initialized, reserved) |
| `Version` | `u32` | Protocol version, bumped by upgrade |
| `Frozen` | `bool` | Global freeze flag |
| `DefaultStrategy` | `AggregationStrategy` | Hub-wide default |
| `Strategy(Bytes)` | `AggregationStrategy` | Per-asset override |
| `FeedCount` | `u32` | Total registered feed slots |
| `Feed(Bytes, u32)` | `PriceFeed` | Feed config per (asset, priority) |
| `LatestPrice(Bytes, u32)` | `PricePoint` | Last report / pull per slot |
| `ConsecutiveFailures(Bytes)` | `u32` | Health failure counter |
| `LastSuccess(Bytes)` | `u64` | Last successful read timestamp |
| `AssetBreaker(Bytes)` | `BreakerState` | Per-asset circuit breaker |
| `ProposedWasm` | `BytesN<32>` | Staged upgrade hash |

## Pluggable provider interface

```rust
#[contractclient(name = "PriceProviderClient")]
pub trait PriceProvider {
    fn get_price(env: Env, asset: Bytes) -> ProviderPrice;
}
```

`ProviderPrice { price: i128, decimals: u32, timestamp: u64, confidence: u32 }`.

Provider contract expectations (enforced/assumed by `provider.rs`):

- `price > 0`, otherwise the pull reverts (`InvalidPrice`-style assert). A
  failing pull makes the whole `get_price` revert — never serve a wrong price.
- `timestamp` is clamped down to the current ledger time so a future-stamped
  quote can never be treated as fresher than it is.
- read-only; no authorization required from the hub.

`fetch_provider_price(asset, provider)` exposes live pulls to callers.

## Health monitoring

Per-feed classification (`classify_feed`) produces `FeedStatusCode`:

| Status | Condition |
| ------ | --------- |
| `Active` | enabled, not frozen, last point within staleness window |
| `Stale` | enabled, no point or point older than the threshold |
| `Disabled` | feed explicitly disabled by governance or auto-disabled |
| `Frozen` | global freeze or asset breaker open |

Circuit breaker state machine (`health.rs`):

```
get_price succeeds ──(recover_breaker_if_healthy)──► breaker cleared, failures reset
       │
failures: external monitor calls monitor_oracle_health(asset):
       0 ─► 1 ─► 2 ─► 3+ (AUTO_BREAKER_FAILURE_THRESHOLD)
                            └──► open BreakerState{open_until: now+600, auto: true}
                                 └──► is_asset_frozen=true; get_price reverts (Frozen)
                                 └──► after cooldown: reads resume; successful read clears
```

- **Auto breaker** (`auto=true`): opened by the failure counter, self-heals on
  the first successful `get_price` after cooldown.
- **Governance freeze** (`auto=false`): opened by `freeze_asset`, only closed by
  `unfreeze_asset`; never self-heals.
- **Global freeze** (`Frozen=true`): halts all pricing and all mutation;
  `get_price`, `report_price`, `stage_upgrade`, and feed changes revert.

`check_feed_health(asset) -> Vec<FeedStatus>` and
`get_health(asset) -> OracleHealthStatus` are read-only views for off-chain
monitors.

## Aggregation

`outlier filter`: quote is kept iff `deviation_bps(price, median) <= 2000`
(20 %). If every quote were an outlier the code degrades to the full set so the
system never bricks. A single quote short-circuits (returns it directly).

- Median: upper median of kept prices (index `len/2`); confidence = mean of
  kept confidences; timestamp = latest.
- Weighted: effective weight `w = feed.weight_bps * max(confidence, 1)`;
  `price = Σ(price·w) / Σw`; confidence = confidence-weighted mean. Feed
  weight defaults to `10_000` bps.

## Upgrade mechanism

```
stage_upgrade(hash) [governance, not frozen] ──► DataKey::ProposedWasm = hash
upgrade()            [governance, not frozen] ──► require staged
    1. remove ProposedWasm
    2. Version += 1
    3. emit UpgradeExecutedEvent(old, new, hash)
    4. env.deployer().update_current_contract_wasm(hash)   // atomic swap
```

Instance storage (all `DataKey` entries above) survives the swap, so upgraded
code must keep a storage-compatible layout. The swap itself is a Soroban VM
operation: it cannot be exercised in the unit-test env (which rejects
`upload_wasm`), so it is validated in integration deployments; unit tests cover
the full governance lifecycle and the VM's rejection of invalid targets.

## Events

All re-exported from `types.rs`: `FeedRegisteredEvent`, `FeedUpdatedEvent`,
`FeedDisabledEvent`, `FeedEnabledEvent`, `FeedAutoDisabledEvent`,
`PriceReportedEvent`, `PricePulledEvent`, `FrozenEvent`, `UnfrozenEvent`,
`AssetStrategyUpdatedEvent`, `DefaultStrategyUpdatedEvent`,
`HealthFailureEvent`, `HealthSuccessEvent`, `BreakerOpenedEvent`,
`BreakerUnfrozenEvent`, `UpgradeStagedEvent`, `UpgradeExecutedEvent`.

## Security model

- All management paths require governance `require_auth`.
- `report_price` authorizes the *registered oracle address* for the slot.
- Pull providers are untrusted inputs: prices are validated (`> 0`) and
  timestamps clamped before use.
- Health failures only auto-freeze the affected asset, never the whole hub.
- A disabled or stale feed never participates in aggregation; a halt returns
  errors rather than a fabricated price.