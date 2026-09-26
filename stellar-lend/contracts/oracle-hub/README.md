# Oracle Hub

A dedicated, governance-managed price feed hub that decouples price aggregation
from lending logic. The hub owns feed registration, multi-source aggregation,
deviation-checked fallback, price caching, health monitoring, emergency freeze
controls, and an upgrade mechanism so consuming protocols can simply read
reliable prices.

## Highlights

- **Pluggable price providers.** Two feed modes:
  - `Push`: a registered oracle signs prices with `report_price`.
  - `Pull`: the hub queries any external contract implementing the
    `PriceProvider` interface (`get_price(Env, Bytes) -> ProviderPrice`)
    through the generated `PriceProviderClient`.
- **Five sources per asset.** `Primary (0)`, `Secondary (1)`, `Fallback (2)`,
  `Quaternary (3)`, `Quinary (4)`. Only registered slots are visited, tracked
  in a per-asset index, so adding a source does not cost extra reads.
- **Aggregation strategies.** `Median` (default, robust to a corrupt feed),
  `Weighted` (confidence- and feed-weight-adjusted mean), or `TrimmedMean`
  (drops both tails before averaging), set globally or overridden per asset.
- **Deviation-checked fallback.** The leading source is compared against the
  median of the others. Beyond the configured band it is demoted, the event
  trail names the source and the reference price, and the remaining sources
  publish the price. Demotion needs a quorum of three and fails closed when it
  would leave fewer sources than governance demanded.
- **Outlier rejection.** Independently of the leader check, quotes deviating
  more than the band from the median are excluded before aggregation.
- **Decimal normalization.** Pull providers declare their own precision; the
  hub rescales every quote to a hub-wide canonical precision (8 by default)
  before it compares, weights, or averages anything.
- **Price cache.** Opt-in TTL cache so a burst of reads costs one provider
  round trip. Entries are clamped to the freshness budget of the sources behind
  them and dropped whenever governance changes anything.
- **Health monitoring.** Per-feed staleness classification, consecutive failure
  tracking, and a per-asset circuit breaker that auto-opens after 3 failures
  and self-heals on a successful read.
- **Upgrade mechanism.** Governance stages a WASM hash, then atomically swaps
  the live contract code via `update_current_contract_wasm`; instance storage
  and the version counter survive the swap.
- **Emergency controls.** Global freeze plus per-asset governor freeze.

## Quick start

```bash
cargo test -p oracle-hub          # 96 unit tests
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

Stale feeds are auto-disabled; if every feed is stale or disabled, the read
reverts with `NoActiveFeeds`.

## Reading a price

`get_price` returns an `AggregatedPrice` that says how the price was produced,
so a consumer can tighten its own risk parameters instead of guessing:

| Field | Meaning |
| ----- | ------- |
| `price` | The aggregated price in canonical decimals |
| `num_feeds` | Sources that survived selection and filtering |
| `num_active_feeds` | Sources that reported at all |
| `rejected_sources` | Sources dropped as outliers or demoted |
| `used_fallback` | The leading source was demoted, or only one source was usable |
| `source` | `Consensus`, `Sole`, or `Cached` |
| `deviation_bps` | Spread of the sources actually used (or of the demoted leader) |
| `from_cache` | Served from the TTL cache |

Prices come from a multi-source consensus by default. When `used_fallback` is
set, the price is a single source and the consumer should decide whether that
is acceptable for the position it is pricing.

## Fallback with deviation checks

`set_aggregation_params(asset, max_deviation_bps, min_sources)` (governance)
configures the band, which defaults to 20 % (`OUTLIER_DEVIATION_BPS`).

```
collect quotes from every registered slot (ascending)
  │
  ├─ fewer than three sources ──► no leader check, aggregation decides
  │
  └─ leader vs median(others):
        deviation <= band ──► leader trusted
        deviation >  band ──► leader demoted
                               DeviationRejectedEvent { price, reference, deviation_bps }
                               FallbackActivatedEvent  { rejected_priority, serving_priority }
```

- A band of `0` disables the check entirely.
- Demotion only removes sources; it never invents a price.
- If the surviving set would fall below `min_sources` and the asset has more
  than one source, the read fails closed (`InsufficientSources`) rather than
  publishing a thinner consensus than governance asked for. An asset with a
  single registered feed is always served and flagged `Sole`.

## Decimal normalization

Push reporters submit canonical-scale prices. Pull providers report their own
scale in `ProviderPrice::decimals` (up to 18) and the hub rescales to the
canonical precision — `set_price_decimals`, 8 by default — emitting
`ProviderPriceRescaledEvent` with the raw and normalized values. A quote that
cannot be rescaled without overflow rejects the read instead of silently
truncating.

## Price cache

Off by default: serving a memoized price to a lending market is a
security-relevant choice, so governance opts in.

```text
set_cache_ttl(seconds)   // 0 disables, 3600 is the ceiling
refresh_price(asset)     // force a recompute instead of serving the cache
get_cached_price(asset)  // read the entry
get_cache_stats()        // hits, misses, writes, pull_reads
invalidate_cache(asset)  // drop one entry
```

- The effective TTL is the smaller of the configured TTL and the shortest
  staleness budget among the sources that fed the aggregate, so a memoized
  price can never outlive the quotes behind it.
- Every governance mutation — feed registration, feed update, strategy or
  parameter change, precision change, freeze — bumps a configuration epoch and
  invalidates all entries. `set_cache_ttl` alone does not, because a keeper
  enabling the cache should not throw away the entry it just wrote.

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
| `fallback_test` | leader demotion, quorum rule, fail-closed `min_sources`, recovery, stale-leader fallback, band governance |
| `precision_test` | decimal rescaling, canonical precision, overflow rejection, provider diagnostics |
| `cache_test` | TTL window, staleness clamp, epoch invalidation, bypass, counters |
| `gas_test` | cached read cheaper than a recompute, bounded cost per extra source |
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
