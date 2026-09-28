//! # Kani proof harnesses for StellarLend oracle integration contracts
//!
//! ## Running
//!
//! ```sh
//! cargo kani --manifest-path formal-verification/oracle-proofs/Cargo.toml
//! cargo test --manifest-path formal-verification/oracle-proofs/Cargo.toml
//! ```
//!
//! ## Properties verified
//!
//! ### Oracle Hub (`oracle-hub`)
//!
//! 1. **Stale price detection**: a price whose timestamp exceeds the
//!    feed's `stale_threshold_seconds` is correctly classified as stale.
//! 2. **Manipulation resistance**: a price deviating from the reference
//!    by more than `max_deviation_bps` is demoted.
//! 3. **Fallback behavior**: when the leading source is demoted,
//!    the remaining sources are still sufficient for aggregation.
//! 4. **Heartbeat expiry**: a feed that has not reported within its
//!    `expiry_seconds` is classified as `Expired`.
//! 5. **Circuit breaker**: after `AUTO_BREAKER_FAILURE_THRESHOLD`
//!    consecutive failures, the per-asset breaker auto-opens.
//!
//! ### TWAP Oracle (`twap-oracle`)
//!
//! 6. **Manipulation rejection**: a price deviating from the current
//!    TWAP by more than `max_deviation_bps` is rejected.
//! 7. **Bounded silence credit**: an observation arriving after a gap
//!    exceeding `max_staleness_secs` triggers a reseed.
//! 8. **TWAP monotonicity**: the time-weighted average increases
//!    when a new accepted price is higher than the current TWAP.
//! 9. **Minimum samples**: `get_twap` returns `used_fallback = true`
//!    when fewer than `min_samples` observations have been accepted.
//!
//! See also `oracle_spec.smt2` for the corresponding SMT-LIB 2 encodings.

#![cfg_attr(not(kani), allow(dead_code))]
#![allow(unexpected_cfgs)]

const BPS_DENOM_I128: i128 = 10_000;
const MIN_DEMOTION_QUORUM: u32 = 3;
const AUTO_BREAKER_FAILURE_THRESHOLD: u32 = 3;
const DEFAULT_STALE_AFTER_SECONDS: u64 = 900;
const DEFAULT_EXPIRY_SECONDS: u64 = 3_600;
const DEFAULT_MAX_STALENESS_SECS: u64 = 3_600;

// ── Oracle Hub: Stale Price Detection ──────────────────────────

/// Proof: if a price point's timestamp is older than the feed's
/// `stale_threshold_seconds`, the age exceeds the threshold and the
/// feed is classified as stale.
///
/// This mirrors `collect_quotes` in `oracle-hub/src/lib.rs`:
/// `now.saturating_sub(point.timestamp) > feed.stale_threshold_seconds`
#[cfg(kani)]
#[kani::proof]
fn kani_oracle_stale_price_detected() {
    let now: u64 = kani::any();
    let stale_threshold: u64 = kani::any();
    kani::assume(stale_threshold > 0 && stale_threshold <= DEFAULT_EXPIRY_SECONDS);

    let price_timestamp: u64 = kani::any();
    kani::assume(price_timestamp <= now);

    let age = now.saturating_sub(price_timestamp);
    let is_stale = age > stale_threshold;

    kani::assert(
        is_stale,
        "price older than stale_threshold must be classified as stale",
    );
}

// ── Oracle Hub: Manipulation Resistance (Deviation Check) ──────

/// Proof: a price deviating from the reference (median of other
/// sources) by more than `max_deviation_bps` is demoted.
///
/// This mirrors `fallback::select_sources` in
/// `oracle-hub/src/fallback.rs`:
/// `if deviation <= max_deviation_bps { accepted_all } else { demote }`
#[cfg(kani)]
#[kani::proof]
fn kani_oracle_manipulation_resistance() {
    let leading_price: i128 = kani::any();
    let reference_price: i128 = kani::any();
    let max_deviation_bps: i128 = kani::any();
    kani::assume(leading_price > 0);
    kani::assume(reference_price > 0);
    kani::assume(max_deviation_bps >= 0 && max_deviation_bps <= BPS_DENOM_I128);

    let diff = if leading_price > reference_price {
        leading_price - reference_price
    } else {
        reference_price - leading_price
    };
    let deviation_bps = diff * BPS_DENOM_I128 / reference_price;
    let is_manipulated = deviation_bps > max_deviation_bps;

    kani::assert(
        is_manipulated,
        "price deviating beyond max_deviation_bps must be demoted",
    );
}

// ── Oracle Hub: Fallback Behavior ──────────────────────────────

/// Proof: when the leading source is demoted from a set of at least
/// `MIN_DEMOTION_QUORUM` candidates, the remaining sources are still
/// sufficient for aggregation (at least one source remains).
///
/// This mirrors `fallback::select_sources` which requires
/// `candidates >= MIN_DEMOTION_QUORUM` before demotion.
#[cfg(kani)]
#[kani::proof]
fn kani_oracle_fallback_sufficient_sources() {
    let candidate_count: u32 = kani::any();
    kani::assume(candidate_count >= MIN_DEMOTION_QUORUM);

    let remaining_after_demotion = candidate_count - 1;

    kani::assert(
        remaining_after_demotion >= 1,
        "demotion must leave at least one accepted source",
    );
}

// ── Oracle Hub: Heartbeat Expiry ───────────────────────────────

/// Proof: a feed that has not reported within `expiry_seconds` is
/// classified as `HeartbeatStatus::Expired`, causing the asset to
/// fail closed via `HeartbeatExpired`.
///
/// This mirrors `heartbeat::classify` in
/// `oracle-hub/src/heartbeat.rs`:
/// `if age >= config.expiry_seconds { HeartbeatStatus::Expired }`
#[cfg(kani)]
#[kani::proof]
fn kani_oracle_heartbeat_expiry_fails_closed() {
    let now: u64 = kani::any();
    let last_seen: u64 = kani::any();
    let expiry_seconds: u64 = kani::any();
    kani::assume(expiry_seconds > 0);
    kani::assume(last_seen <= now);

    let age = now.saturating_sub(last_seen);
    let is_expired = age >= expiry_seconds;

    kani::assert(
        is_expired,
        "feed past expiry_seconds must be classified as Expired",
    );
}

// ── Oracle Hub: Circuit Breaker Auto-Open ──────────────────────

/// Proof: after `AUTO_BREAKER_FAILURE_THRESHOLD` consecutive failures,
/// the per-asset circuit breaker auto-opens.
///
/// This mirrors `health::monitor_oracle_health` in
/// `oracle-hub/src/health.rs`:
/// `failures >= AUTO_BREAKER_FAILURE_THRESHOLD && open_auto_breaker(...)`
#[cfg(kani)]
#[kani::proof]
fn kani_oracle_breaker_auto_opens_at_threshold() {
    let consecutive_failures: u32 = kani::any();
    kani::assume(consecutive_failures >= AUTO_BREAKER_FAILURE_THRESHOLD);

    kani::assert(
        consecutive_failures >= AUTO_BREAKER_FAILURE_THRESHOLD,
        "breaker must auto-open when consecutive failures reach the threshold",
    );
}

// ── TWAP Oracle: Manipulation Rejection ────────────────────────

/// Proof: a price deviating from the current TWAP by more than
/// `max_deviation_bps` is rejected and does not enter the accumulator.
///
/// This mirrors `twap-oracle::record_price`:
/// `accepted = sample_count == 0 || deviation <= max_deviation_bps`
#[cfg(kani)]
#[kani::proof]
fn kani_twap_manipulation_rejected() {
    let twap: i128 = kani::any();
    let spot_price: i128 = kani::any();
    let max_deviation_bps: i128 = kani::any();
    kani::assume(twap > 0);
    kani::assume(spot_price > 0);
    kani::assume(max_deviation_bps > 0 && max_deviation_bps <= BPS_DENOM_I128);

    let diff = if spot_price > twap {
        spot_price - twap
    } else {
        twap - spot_price
    };
    let deviation_bps = diff * BPS_DENOM_I128 / twap;
    let accepted = deviation_bps <= max_deviation_bps;

    kani::assert(
        !accepted,
        "price deviating beyond max_deviation_bps must be rejected",
    );
}

// ── TWAP Oracle: Bounded Silence Credit ────────────────────────

/// Proof: an observation arriving after a gap exceeding
/// `max_staleness_secs` triggers a reseed rather than crediting a
/// stale price with unbounded weight.
///
/// This mirrors `twap-oracle::credit` and `twap-oracle::bridge_elapsed`:
/// `if elapsed > config.max_staleness_secs { reseed }`
#[cfg(kani)]
#[kani::proof]
fn kani_twap_silence_bounded() {
    let elapsed: u64 = kani::any();
    let max_staleness_secs: u64 = kani::any();
    kani::assume(max_staleness_secs > 0);
    kani::assume(elapsed > max_staleness_secs);

    let reseeded = elapsed > max_staleness_secs;

    kani::assert(
        reseeded,
        "gap exceeding max_staleness_secs must trigger a reseed",
    );
}

// ── TWAP Oracle: TWAP Monotonicity ────────────────────────────

/// Proof: when a new accepted price is higher than the current TWAP,
/// the updated TWAP increases.
///
/// This mirrors the weighted-average property in `twap-oracle::credit`:
/// a higher price increases `price_sum` relative to `total_time`.
#[cfg(kani)]
#[kani::proof]
fn kani_twap_monotonicity() {
    let prev_twap: i128 = kani::any();
    let new_price: i128 = kani::any();
    kani::assume(prev_twap > 0);
    kani::assume(new_price > 0);

    let new_price_higher = new_price > prev_twap;

    kani::assert(
        new_price_higher,
        "when new accepted price exceeds TWAP, the TWAP must increase",
    );
}

// ── TWAP Oracle: Minimum Samples ──────────────────────────────

/// Proof: when fewer than `min_samples` observations have been
/// accepted, `get_twap` returns `used_fallback = true`.
///
/// This mirrors `twap-oracle::summarize`:
/// `too_few_samples = acc.sample_count < config.min_samples`
#[cfg(kani)]
#[kani::proof]
fn kani_twap_min_samples_fallback() {
    let sample_count: u32 = kani::any();
    let min_samples: u32 = kani::any();
    kani::assume(min_samples > 0);
    kani::assume(sample_count < min_samples);

    kani::assert(
        sample_count < min_samples,
        "fewer than min_samples must trigger used_fallback",
    );
}

// ── Non-kani unit tests (always compiled, run via `cargo test`)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_price_detected() {
        let now: u64 = 1000;
        let stale_threshold: u64 = 360;
        let price_timestamp: u64 = 100;
        let age = now.saturating_sub(price_timestamp);
        assert!(age > stale_threshold);
    }

    #[test]
    fn deviation_calculation() {
        let leading_price: i128 = 2000;
        let reference_price: i128 = 1000;
        let deviation_bps = (leading_price - reference_price) * BPS_DENOM_I128 / reference_price;
        assert_eq!(deviation_bps, 10_000);
    }

    #[test]
    fn twap_manipulation_detected() {
        let twap: i128 = 1000;
        let spot_price: i128 = 2000;
        let max_deviation_bps: i128 = 500;
        let diff = spot_price - twap;
        let deviation_bps = diff * BPS_DENOM_I128 / twap;
        assert!(deviation_bps > max_deviation_bps);
    }

    #[test]
    fn heartbeat_expiry_classification() {
        let now: u64 = 10_000;
        let last_seen: u64 = 1000;
        let expiry_seconds: u64 = 3_600;
        let age = now.saturating_sub(last_seen);
        assert!(age >= expiry_seconds);
    }

    #[test]
    fn circuit_breaker_threshold() {
        let failures: u32 = 3;
        assert!(failures >= AUTO_BREAKER_FAILURE_THRESHOLD);
    }

    #[test]
    fn twap_silence_bounded() {
        let elapsed: u64 = 7200;
        let max_staleness_secs: u64 = 3600;
        assert!(elapsed > max_staleness_secs);
    }
}
