# oracle-proofs

Formal verification of `oracle-hub` and `twap-oracle` price feed
integration contracts: stale price detection, manipulation resistance,
fallback behavior, and heartbeat expiry.

This crate is **not** part of the main Cargo workspace (see the root
`Cargo.toml` `exclude` list), matching the existing
`formal-verification/safe-math-proofs` pattern, so it doesn't affect
normal `cargo build`/`cargo test` runs of the contract workspace.

## Running

Bounded property tests (fast, run in normal CI, no extra tooling):

```sh
cargo test --manifest-path formal-verification/oracle-proofs/Cargo.toml
```

Exhaustive bounded model checking with [Kani](https://model-checking.github.io/kani/):

```sh
cargo install --locked kani-verifier
cargo kani --manifest-path formal-verification/oracle-proofs/Cargo.toml
```

SMT-LIB spec directly with Z3:

```sh
z3 formal-verification/oracle-proofs/oracle_spec.smt2
```

## Properties verified

### Oracle Hub (`oracle-hub`)

1. **Stale price detection**: a price whose timestamp exceeds the
   feed's `stale_threshold_seconds` is correctly classified as stale
   and the feed is auto-disabled.
2. **Manipulation resistance**: a price deviating from the median of
   other sources by more than `max_deviation_bps` is demoted.
3. **Fallback behavior**: when the leading source is demoted, the
   remaining accepted sources are still sufficient for aggregation.
4. **Heartbeat expiry**: a feed that has not reported within its
   `expiry_seconds` is classified as `Expired`, causing the asset to
   fail closed via `HeartbeatExpired`.
5. **Circuit breaker**: after `AUTO_BREAKER_FAILURE_THRESHOLD`
   consecutive failures, the per-asset breaker auto-opens.

### TWAP Oracle (`twap-oracle`)

6. **Manipulation rejection**: a price deviating from the current
   TWAP by more than `max_deviation_bps` is rejected and does not
   enter the accumulator.
7. **Bounded silence credit**: an observation arriving after a gap
   exceeding `max_staleness_secs` triggers a reseed rather than
   crediting a stale price with unbounded weight.
8. **TWAP monotonicity**: the time-weighted average increases when
   a new accepted price is higher than the current TWAP.
9. **Minimum samples**: `get_twap` returns `used_fallback = true`
   when fewer than `min_samples` observations have been accepted.

See `src/lib.rs` for the full description of properties verified,
and `oracle_spec.smt2` for the corresponding SMT-LIB 2 encoding.
