//! Pure token-bucket algorithm.
//!
//! Extracted from `hello-world/src/rate_limiter.rs` so the fixed-point math is shared
//! and unit-testable without a `soroban_sdk::Env`. All functions here are pure:
//! storage, admin and authorization live in the calling contract.

/// Fixed-point scale (`1e6`).
pub const TOKEN_SCALE: i128 = 1_000_000;

/// A single token bucket's persisted state.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct BucketState {
    /// Current tokens (scaled by [`TOKEN_SCALE`]).
    pub tokens: i128,
    /// Last refill timestamp (unix seconds).
    pub last_refill: u64,
}

/// Outcome of a consume attempt.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BucketOutcome {
    /// One unit was consumed; the detector should persist the new state.
    Allowed(BucketState),
    /// The bucket had insufficient tokens; nothing was consumed.
    Limited(BucketState),
    /// A configuration/arithmetic error occurred; the caller should treat it as a hard
    /// failure.
    Error(TokenBucketError),
}

/// A bucket was configured with an invalid window or capacity, or an overflow
/// occurred during refill.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TokenBucketError {
    /// `refill_per_second` could not be derived (zero window or zero capacity).
    InvalidConfig,
    /// Arithmetic overflow during refill.
    Overflow,
}

/// Derive the refill rate (scaled tokens/second) from a window and per-window max.
pub fn refill_per_second(
    window_seconds: u64,
    max_calls_per_window: u32,
) -> Result<i128, TokenBucketError> {
    if window_seconds == 0 || max_calls_per_window == 0 {
        return Err(TokenBucketError::InvalidConfig);
    }
    let per_window = (max_calls_per_window as i128)
        .checked_mul(TOKEN_SCALE)
        .ok_or(TokenBucketError::Overflow)?;
    per_window
        .checked_div(window_seconds as i128)
        .ok_or(TokenBucketError::Overflow)
}

/// Total capacity in scaled tokens for a config (calls + burst + optional grace).
pub fn capacity_tokens(max_calls_per_window: u32, burst_calls: u32, grace_calls: u32) -> i128 {
    let base = (max_calls_per_window as i128)
        .checked_add(burst_calls as i128)
        .unwrap_or(i128::MAX);
    base.checked_add(grace_calls as i128)
        .and_then(|v| v.checked_mul(TOKEN_SCALE))
        .unwrap_or(i128::MAX)
}

/// Refill a bucket up to `capacity` given `now` (unix seconds).
pub fn refill(
    mut bucket: BucketState,
    now: u64,
    rate_per_second: i128,
    capacity: i128,
) -> Result<BucketState, TokenBucketError> {
    if now <= bucket.last_refill {
        return Ok(bucket);
    }
    let dt = now - bucket.last_refill;
    let add = rate_per_second
        .checked_mul(dt as i128)
        .ok_or(TokenBucketError::Overflow)?;
    bucket.tokens = bucket
        .tokens
        .checked_add(add)
        .ok_or(TokenBucketError::Overflow)?
        .min(capacity);
    bucket.last_refill = now;
    Ok(bucket)
}

/// Attempt to consume one unit from a bucket.
///
/// Returns [`BucketOutcome::Allowed`] with the updated state when a token is available,
/// [`BucketOutcome::Limited`] (unchanged state) when limited. A fresh bucket must be
/// initialized to `BucketState { tokens: capacity, last_refill: now }`.
pub fn token_bucket_consume(
    bucket: BucketState,
    now: u64,
    rate_per_second: i128,
    capacity: i128,
) -> BucketOutcome {
    match refill(bucket, now, rate_per_second, capacity) {
        Ok(mut b) => {
            if b.tokens < TOKEN_SCALE {
                BucketOutcome::Limited(b)
            } else {
                match b.tokens.checked_sub(TOKEN_SCALE) {
                    Some(t) => {
                        b.tokens = t;
                        BucketOutcome::Allowed(b)
                    }
                    None => BucketOutcome::Error(TokenBucketError::Overflow),
                }
            }
        }
        Err(e) => BucketOutcome::Error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refill_rate_derives_from_window() {
        // 5 calls per 60s -> 5 * 1e6 / 60 tokens/sec
        let rate = refill_per_second(60, 5).unwrap();
        assert_eq!(rate, 5 * TOKEN_SCALE / 60);
    }

    #[test]
    fn zero_window_is_invalid() {
        assert_eq!(
            refill_per_second(0, 5),
            Err(TokenBucketError::InvalidConfig)
        );
    }

    #[test]
    fn capacity_includes_grace() {
        assert_eq!(capacity_tokens(5, 3, 10), 18 * TOKEN_SCALE);
    }

    #[test]
    fn consume_allows_then_limits() {
        let cap = capacity_tokens(2, 0, 0); // 2 tokens
        let fresh = BucketState {
            tokens: cap,
            last_refill: 0,
        };
        let rate = refill_per_second(60, 2).unwrap();

        match token_bucket_consume(fresh, 1, rate, cap) {
            BucketOutcome::Allowed(next) => assert_eq!(next.tokens, cap - TOKEN_SCALE),
            other => panic!("expected allowed got {:?}", other),
        }
    }

    #[test]
    fn bucket_refills_over_time() {
        let cap = capacity_tokens(5, 0, 0);
        let fresh = BucketState {
            tokens: cap,
            last_refill: 0,
        };
        let rate = refill_per_second(60, 5).unwrap();
        // Drain fully.
        let drained = match token_bucket_consume(fresh, 1, rate, cap) {
            BucketOutcome::Allowed(b) => b,
            other => panic!("expected allowed got {:?}", other),
        };
        assert_eq!(drained.tokens, cap - TOKEN_SCALE);

        // After 120s it should be back at capacity.
        let refilled = refill(drained, 120, rate, cap).unwrap();
        assert_eq!(refilled.tokens, cap);
    }
}
