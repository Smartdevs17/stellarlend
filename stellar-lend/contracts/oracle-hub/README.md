# Oracle Hub

A dedicated, governance-managed price feed hub that decouples price aggregation
from lending logic. The hub owns feed registration, aggregation strategy
selection, health monitoring, emergency freeze controls, and an upgrade
mechanism so consuming protocols can simply read reliable prices.

## Highlights

- **Pluggable price providers.** Two feed modes:
  - `Push`: a registered oracle signs prices with `report_price`.
  - `Pull`: the hub queries any external contract implementing the
    `PriceProvider` interface (`get_price(Env, Bytes) -> ProviderPrice`)
    through the generated `PriceProviderClient`.
- **Aggregation strategies.** `Median` (default, robust to a corrupt feed) or
  `Weighted` (confidence- and feed-weight-adjusted mean), set globally or
  overridden per asset.
- **Outlier rejection.** Quotes deviating more than 20 % from the median are
  excluded before aggregation. Feed weights are configured in basis points.
- **Health monitoring.** Per-feed staleness classification, consecutive failure
  tracking, and a per-asset circuit breaker that auto-opens after 3 failures
  and self-heals on a successful read.
- **Upgrade mechanism.** Governance stages a WASM hash, then atomically swaps
  the live contract code via `update_current_contract_wasm`; instance storage
  and the version counter survive the swap.
- **Emergency controls.** Global freeze plus per-asset governor freeze.

## Quick start

```bash
cargo test -p oracle-hub          # 55 unit tests
cargo clippy -p oracle-hub --all-targets
cargo fmt -p oracle-hub
```

## Integration

1. **Deploy and initialize** with a governance address:
   ```text
   initialize(governance, admin)
   ```
2. **Register feeds** (governance):
   ```text
   register_feed(asset, oracle, priority, stale_threshold_seconds, mode, weight_bps)
   ```
   - Push mode: `oracle` is the reporter authorized to call `report_price`.
   - Pull mode: `oracle` is a `PriceProvider` contract address.
3. **Price reporters push** new points:
   ```text
   report_price(asset, price, confidence, priority)
   ```
4. **Consumers read** the aggregated price:
   ```text
   price(asset)                     // i128
   get_price(asset)                 // AggregatedPrice
   ```

Priority slots: `Primary (0)`, `Secondary (1)`, `Fallback (2)`. Stale feeds are
auto-disabled; if every feed is stale or disabled, the read reverts with
`NoActiveFeeds`.

## Health monitoring loop

An off-chain watcher calls `monitor_oracle_health(asset)` after each observed
fetch failure. After `AUTO_BREAKER_FAILURE_THRESHOLD` (3) consecutive recorded
failures the per-asset breaker opens for `DEFAULT_BREAKER_COOLDOWN_SECONDS`
(600 s), and the asset stops serving prices. `record_oracle_success(asset)`
resets the counters; a successful `get_price` also self-heals an auto-opened
breaker. `check_feed_health(asset)` and `get_health(asset)` are read-only views.

## Testing

The suite lives in `src/tests/`:

| Suite | Coverage |
| ----- | -------- |
| `feed_test` | registration, defaults, update, disable/enable, per-asset isolation, auth |
| `aggregation_test` | median, weighted, confidence weighting, outlier rejection, per-asset strategies, staleness fallback |
| `provider_test` | pull aggregation, mixed push/pull, provider views, invalid price rejection |
| `health_test` | feed classification, breaker trip/cooldown/self-heal, success reset |
| `freeze_test` | global and per-asset freeze/thaw, precedence, auth |
| `upgrade_test` | staging, pending visibility, frozen/unauthorized gating, invalid-wasm rejection |

Pull feeds are exercised against a `MockProvider` that implements
`PriceProvider`.

## Upgrade caveat

The real code swap requires a compiled soroban contract (the unit-test env
rejects `upload_wasm`), so swap execution is verified by integration
deployments; unit tests cover the full governance/state lifecycle. See
`docs/ARCHITECTURE.md`.