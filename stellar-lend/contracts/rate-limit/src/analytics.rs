//! Rate-limit analytics.
//!
//! Mirrors the `RateLimitStatus` / `RateLimitAnalytics` snapshot types from
//! `hello-world/src/rate_limiter.rs`, re-expressed against the shared library's pure
//! bucket math so operators can monitor headroom without depending on a specific
//! contract crate.

use crate::policy::{PolicyLayer, ResolvedLimit};
use crate::token_bucket::BucketState;
use alloc::string::String;

/// Read-only view of a single user or global bucket.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RateLimitStatus {
    /// Effective (resolved) configuration in force.
    pub effective: ResolvedLimit,
    /// Which policy layer produced the effective config.
    pub layer: PolicyLayer,
    /// Current bucket state.
    pub bucket: BucketState,
    /// Total capacity in scaled tokens.
    pub capacity_tokens: i128,
    /// Refill rate in scaled tokens/sec.
    pub refill_per_second: i128,
    /// Whether the user is grace-enabled for this operation.
    pub grace_enabled: bool,
}

/// Aggregated per-operation analytics snapshot for an (op, pool) pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitAnalytics {
    pub op: String,
    pub pool_key: Option<u32>,
    /// Effective configuration currently enforced.
    pub effective: ResolvedLimit,
    /// Global bucket state for the pool.
    pub global_bucket: BucketState,
    /// Global bucket capacity in scaled tokens.
    pub global_capacity_tokens: i128,
    /// Fill fraction in basis points (0 = empty, 10_000 = full).
    pub global_fill_bps: i128,
    /// Snapshot timestamp (unix seconds).
    pub snapshot_at: u64,
}

/// Compute the fill fraction in basis points for a bucket given a capacity.
pub fn fill_bps(bucket: &BucketState, capacity: i128) -> i128 {
    const BPS_SCALE: i128 = 10_000;
    if capacity <= 0 {
        return 0;
    }
    let raw = bucket
        .tokens
        .saturating_mul(BPS_SCALE)
        .checked_div(capacity)
        .unwrap_or(0);
    raw.clamp(0, BPS_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_bucket::TOKEN_SCALE;

    #[test]
    fn fill_bps_is_full_when_capacity_met() {
        let bucket = BucketState {
            tokens: 100 * TOKEN_SCALE,
            last_refill: 0,
        };
        assert_eq!(fill_bps(&bucket, 100 * TOKEN_SCALE), 10_000);
    }

    #[test]
    fn fill_bps_is_half_at_half_capacity() {
        let bucket = BucketState {
            tokens: 50 * TOKEN_SCALE,
            last_refill: 0,
        };
        assert_eq!(fill_bps(&bucket, 100 * TOKEN_SCALE), 5_000);
    }

    #[test]
    fn fill_bps_clamps_to_empty_on_zero_capacity() {
        let bucket = BucketState {
            tokens: 0,
            last_refill: 0,
        };
        assert_eq!(fill_bps(&bucket, 0), 0);
    }
}
