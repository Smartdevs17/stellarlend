//! Interest-rate curve smoothing.
//!
//! Kinked curves jump sharply when utilization crosses a band boundary. Two
//! helpers soften that:
//! - [`smooth_rate_step`] limits how far the applied rate may move per update
//!   towards the curve's target rate (rate-of-change limiter);
//! - [`blend_rates_ema`] blends the previous rate with the target using a
//!   weight in bps (exponential moving average).
//!
//! Both are pure, integer-only and never overshoot the target.

use crate::checked::{checked_mul_div, BPS_DIVISOR};
use crate::error::MathError;

/// Moves `current_bps` towards `target_bps` by at most `max_step_bps`.
///
/// # Errors
/// [`MathError::Underflow`] when `max_step_bps` is negative.
pub fn smooth_rate_step(
    current_bps: i128,
    target_bps: i128,
    max_step_bps: i128,
) -> Result<i128, MathError> {
    if max_step_bps < 0 {
        return Err(MathError::Underflow);
    }
    let delta = target_bps - current_bps;
    Ok(current_bps + delta.clamp(-max_step_bps, max_step_bps))
}

/// `previous + (target - previous) * weight / 10_000`, truncated toward `previous`.
/// `weight_bps = 10_000` jumps straight to the target; `0` keeps the previous rate.
///
/// # Errors
/// [`MathError::Underflow`] / [`MathError::Overflow`] when `weight_bps` is outside `0..=10_000`.
pub fn blend_rates_ema(
    previous_bps: i128,
    target_bps: i128,
    weight_bps: i128,
) -> Result<i128, MathError> {
    if weight_bps < 0 {
        return Err(MathError::Underflow);
    }
    if weight_bps > BPS_DIVISOR {
        return Err(MathError::Overflow);
    }
    let moved = checked_mul_div(target_bps - previous_bps, weight_bps, BPS_DIVISOR)?;
    Ok(previous_bps + moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_limits_both_directions() {
        assert_eq!(smooth_rate_step(500, 2_000, 100), Ok(600));
        assert_eq!(smooth_rate_step(2_000, 500, 100), Ok(1_900));
    }

    #[test]
    fn step_never_overshoots_target() {
        assert_eq!(smooth_rate_step(950, 1_000, 100), Ok(1_000));
        assert_eq!(smooth_rate_step(1_000, 1_000, 100), Ok(1_000));
        assert_eq!(smooth_rate_step(1_000, 2_000, 0), Ok(1_000));
    }

    #[test]
    fn step_rejects_negative_limit() {
        assert_eq!(smooth_rate_step(0, 100, -1), Err(MathError::Underflow));
    }

    #[test]
    fn ema_blends_towards_target() {
        assert_eq!(blend_rates_ema(1_000, 3_000, 2_500), Ok(1_500));
        assert_eq!(blend_rates_ema(3_000, 1_000, 2_500), Ok(2_500));
        assert_eq!(blend_rates_ema(1_000, 3_000, 0), Ok(1_000));
        assert_eq!(blend_rates_ema(1_000, 3_000, BPS_DIVISOR), Ok(3_000));
    }

    #[test]
    fn ema_rejects_out_of_range_weight() {
        assert_eq!(blend_rates_ema(0, 1, -1), Err(MathError::Underflow));
        assert_eq!(
            blend_rates_ema(0, 1, BPS_DIVISOR + 1),
            Err(MathError::Overflow)
        );
    }

    #[test]
    fn repeated_steps_converge_across_a_kink() {
        let mut rate = 400; // below-kink rate
        let target = 5_000; // above-kink jump
        let mut updates = 0;
        while rate != target {
            let next = smooth_rate_step(rate, target, 500).unwrap();
            assert!(next - rate <= 500);
            rate = next;
            updates += 1;
        }
        assert_eq!(updates, 10);
    }
}
