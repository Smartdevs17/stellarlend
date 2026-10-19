# TWAP Oracle

A manipulation-resistant Time-Weighted Average Price oracle. Every observation
credits the *previously observed* price for exactly the time that elapsed since
the last observation, so a price that held for an hour weighs a thousand times
more than one that printed for a second — and a price that was never observed
for long cannot influence the average at all.

## What it defends against

A TWAP on its own is still a spot-price oracle with extra arithmetic. Three
defences make the average hard to move:

1. **Rejection on ingestion.** `record_price` measures the incoming price
   against the current TWAP. Beyond `max_deviation_bps` (5 % by default) the
   observation is dropped: the previously accepted price keeps accruing, the
   candidate never becomes the price in effect, the rejection is counted, and
   `PriceRejectedEvent` names the price, the reference, and the deviation. A
   single spot print — in either direction — moves the average by zero, however
   extreme it is.
2. **Bounded credit after a silence.** An observation that arrives more than
   `max_staleness_secs` after the previous one reseeds the accumulator instead
   of crediting the stale price for the whole gap, so an old cheap quote cannot
   be resurrected to drag the average down. The reseed is announced with
   `AccumulatorReseededEvent`.
3. **Auditable, bounded history.** Observations live in a 32-entry ring buffer
   per asset, so the audit trail and the write cost of an observation are both
   constant, and anybody can recompute the average off-chain from
   `get_observations`.

## Configuration

```text
initialize(admin)                                        // 30 min window, 5 % band, 3 samples
set_config(admin, window_secs, max_deviation_bps, min_samples, max_staleness_secs)
```

| Field | Default | Rule |
| ----- | ------- | ---- |
| `window_secs` | 1800 | `1..=86400` |
| `max_deviation_bps` | 500 | `1..=10000` (0 is rejected) |
| `min_samples` | 3 | `1..=32` (the ring size) |
| `max_staleness_secs` | `2 × window` | `0` derives it from the window; must be `>= window_secs` and `<= 2 × 86400` |

## Using it

```text
record_price(asset, price)          // -> Ok(true) accepted, Ok(false) rejected as manipulated
force_record_price(asset, price)    // admin escape hatch for a genuine repricing
get_twap(asset)                      // TwapResult
get_liquidation_price(asset)        // TWAP, or spot when the average is not usable
check_deviation(asset, spot_price)   // Ok(TwapResult) or an error
get_health(asset)                    // keeper-friendly snapshot
get_observations(asset)              // ring buffer, oldest first
get_accumulator(asset)               // raw numerator/denominator
reset_asset(asset)                   // admin: drop an asset's history
```

`get_twap` never silently degrades. `used_fallback` is set when the average is
not yet trustworthy — fewer than `min_samples` accepted observations, less time
covered than `window_secs`, a feed older than `max_staleness_secs`, or a
rejected observation nobody has looked at — and `window_coverage_bps` reports
how much of the window the average actually spans. A consumer that cannot
accept a fallback should check `used_fallback == false` and refuse otherwise;
`get_liquidation_price` exists for consumers that explicitly want spot in that
situation, and flags it for them.

`force_record_price` is the audited way to accept a real repricing: it seeds a
fresh accumulator rather than blending the new price into the old average, so
history is not rewritten, and it publishes both
`AccumulatorReseededEvent { reason: "forced" }` and
`PriceForceRecordedEvent { operator, deviation_bps }`.

## How the average is computed

`price_sum` is the numerator (`price × seconds`) and `total_time` the
denominator, so `twap = price_sum / total_time`. The first observation seeds
the accumulator with one second of weight, which is what makes the average
defined immediately without granting it any influence over the past.

Observations at the same ledger second add no weight but still count as samples
and still update the price in effect, so a burst of reports inside one second
cannot dilute the average.

Rejected observations still mark the elapsed time as covered by the price that
was already accepted. That keeps the window filling under attack instead of
freezing the oracle at whatever coverage it had when the attack started.

## Testing

```bash
cargo test -p stellarlend-twap-oracle   # 33 unit tests
cargo clippy -p stellarlend-twap-oracle --all-targets
cargo fmt -p stellarlend-twap-oracle
```

`src/tests.rs` covers time weighting, manipulation resistance, staleness,
governance bounds, the ring buffer, and the event trail.

## Storage compatibility

`TwapConfig` and `TwapAccumulator` gained fields (`max_staleness_secs`,
`rejected_count`, `manipulation_pending`). Soroban encodes structs as maps
keyed by field name, so a deployment that already holds accumulators must be
reseeded with `reset_asset` (or redeployed) before the new code can decode
them. Assets without history are unaffected.
