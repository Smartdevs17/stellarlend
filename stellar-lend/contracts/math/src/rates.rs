//! Interest-rate curve math shared by every module that prices borrowing.
//!
//! Before this module the same four curves were written out independently in
//! `hello-world::interest_rate`, `lending-interest` and the risk modules, each
//! with its own overflow handling. The functions here are the single
//! implementation: every rate is in basis points (10,000 bps = 100% APY), every
//! utilization is in basis points, and every operation is checked.
//!
//! ## Curves
//!
//! | Curve | Shape |
//! |---|---|
//! | [`linear_rate`] | `base + utilization * slope / 10_000` |
//! | [`kink_rate`] | two-segment piecewise linear, steepening past the kink |
//! | [`jump_rate`] | linear everywhere, plus a jump term above the kink |
//! | [`exponential_rate`] | `base + slope * u^2 + jump * u^3` |
//! | [`dual_slope_rate`] | linear below the kink, second slope on the excess |
//!
//! [`RateCurve`] bundles the parameters and [`RateCurve::borrow_rate`] picks the
//! curve, so callers keep their own config types and convert at the boundary.

use crate::checked::{apply_bps, checked_add, checked_mul_div, checked_sub, clamp, BPS_DIVISOR};
use crate::error::MathError;

/// Seconds in a 365-day year, used to annualize rates.
pub const SECONDS_PER_YEAR: i128 = 365 * 86_400;

/// Which curve maps utilization to a borrow rate.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RateModelKind {
    /// Single slope across the whole utilization range.
    Linear = 0,
    /// Two-segment piecewise linear curve with a kink.
    Kink = 1,
    /// Linear curve plus an additive jump above the kink.
    Jump = 2,
    /// Convex curve driven by the square and cube of utilization.
    Exponential = 3,
    /// Aave-style two-slope curve: the slopes apply to utilization directly
    /// rather than being normalized by the width of each segment.
    DualSlope = 4,
}

/// Parameters of an interest-rate curve, all in basis points.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RateCurve {
    /// Which curve to evaluate.
    pub kind: RateModelKind,
    /// Rate at 0% utilization.
    pub base_rate_bps: i128,
    /// Utilization at which the curve steepens (e.g. 8_000 = 80%).
    pub kink_utilization_bps: i128,
    /// Slope below the kink.
    pub multiplier_bps: i128,
    /// Slope above the kink.
    pub jump_multiplier_bps: i128,
}

impl RateCurve {
    /// Evaluates the configured curve at `utilization_bps`.
    ///
    /// # Example
    /// ```
    /// use stellarlend_math::rates::{RateCurve, RateModelKind};
    /// let curve = RateCurve {
    ///     kind: RateModelKind::Kink,
    ///     base_rate_bps: 100,
    ///     kink_utilization_bps: 8_000,
    ///     multiplier_bps: 2_000,
    ///     jump_multiplier_bps: 10_000,
    /// };
    /// // At the kink: base + full slope.
    /// assert_eq!(curve.borrow_rate(8_000).unwrap(), 2_100);
    /// ```
    pub fn borrow_rate(&self, utilization_bps: i128) -> Result<i128, MathError> {
        match self.kind {
            RateModelKind::Linear => linear_rate(utilization_bps, self),
            RateModelKind::Kink => kink_rate(utilization_bps, self),
            RateModelKind::Jump => jump_rate(utilization_bps, self),
            RateModelKind::Exponential => exponential_rate(utilization_bps, self),
            RateModelKind::DualSlope => dual_slope_rate(utilization_bps, self),
        }
    }
}

/// `base + utilization * multiplier / 10_000`.
pub fn linear_rate(utilization_bps: i128, curve: &RateCurve) -> Result<i128, MathError> {
    let increase = apply_bps(utilization_bps, curve.multiplier_bps)?;
    checked_add(curve.base_rate_bps, increase)
}

/// Two-segment piecewise linear curve.
///
/// Below or at the kink the slope is `multiplier / kink`; above it the slope is
/// `jump_multiplier / (10_000 - kink)`, starting from `base + multiplier`. A
/// kink of 0 or 10,000 degenerates safely rather than dividing by zero.
pub fn kink_rate(utilization_bps: i128, curve: &RateCurve) -> Result<i128, MathError> {
    if utilization_bps <= curve.kink_utilization_bps {
        if curve.kink_utilization_bps == 0 {
            return Ok(curve.base_rate_bps);
        }
        let increase = checked_mul_div(
            utilization_bps,
            curve.multiplier_bps,
            curve.kink_utilization_bps,
        )?;
        return checked_add(curve.base_rate_bps, increase);
    }

    let rate_at_kink = checked_add(curve.base_rate_bps, curve.multiplier_bps)?;
    let excess = checked_sub(utilization_bps, curve.kink_utilization_bps)?;
    let max_excess = checked_sub(BPS_DIVISOR, curve.kink_utilization_bps)?;
    if max_excess == 0 {
        return Ok(rate_at_kink);
    }
    let additional = checked_mul_div(excess, curve.jump_multiplier_bps, max_excess)?;
    checked_add(rate_at_kink, additional)
}

/// Linear curve with an additive jump term above the kink.
pub fn jump_rate(utilization_bps: i128, curve: &RateCurve) -> Result<i128, MathError> {
    let mut rate = linear_rate(utilization_bps, curve)?;
    if utilization_bps > curve.kink_utilization_bps {
        let excess = checked_sub(utilization_bps, curve.kink_utilization_bps)?;
        let max_excess = checked_sub(BPS_DIVISOR, curve.kink_utilization_bps)?;
        if max_excess == 0 {
            return Ok(rate);
        }
        let jump = checked_mul_div(excess, curve.jump_multiplier_bps, max_excess)?;
        rate = checked_add(rate, jump)?;
    }
    Ok(rate)
}

/// `base + multiplier * u^2 + jump_multiplier * u^3`, with `u` in basis points.
pub fn exponential_rate(utilization_bps: i128, curve: &RateCurve) -> Result<i128, MathError> {
    let squared = apply_bps(utilization_bps, utilization_bps)?;
    let cubed = apply_bps(squared, utilization_bps)?;
    let quadratic = apply_bps(squared, curve.multiplier_bps)?;
    let cubic = apply_bps(cubed, curve.jump_multiplier_bps)?;
    checked_add(checked_add(curve.base_rate_bps, quadratic)?, cubic)
}

/// Two-slope curve where each slope applies to utilization directly.
///
/// Below the kink this is identical to [`linear_rate`]. Above it, the excess
/// utilization is charged at `jump_multiplier` instead of `multiplier`:
///
/// ```text
/// rate = base + kink * multiplier / 10_000 + (u - kink) * jump_multiplier / 10_000
/// ```
///
/// This differs from [`kink_rate`], where each slope is normalized by the width
/// of its segment. Both parameterizations are in use across the protocol, so
/// both live here rather than being approximated by one another.
pub fn dual_slope_rate(utilization_bps: i128, curve: &RateCurve) -> Result<i128, MathError> {
    if utilization_bps <= curve.kink_utilization_bps {
        return linear_rate(utilization_bps, curve);
    }
    let kink_component = apply_bps(curve.kink_utilization_bps, curve.multiplier_bps)?;
    let excess = checked_sub(utilization_bps, curve.kink_utilization_bps)?;
    let excess_component = apply_bps(excess, curve.jump_multiplier_bps)?;
    checked_add(
        checked_add(curve.base_rate_bps, kink_component)?,
        excess_component,
    )
}

/// Pool utilization in basis points, capped at 100%.
///
/// Returns 0 when there are no deposits.
pub fn utilization_bps(total_borrows: i128, total_deposits: i128) -> Result<i128, MathError> {
    if total_deposits <= 0 || total_borrows <= 0 {
        return Ok(0);
    }
    let utilization = checked_mul_div(total_borrows, BPS_DIVISOR, total_deposits)?;
    Ok(utilization.min(BPS_DIVISOR))
}

/// Applies an emergency adjustment then clamps to the configured floor/ceiling.
///
/// The adjustment is signed: governance can push rates up in a liquidity crunch
/// or down to relieve borrowers, and the floor/ceiling still bound the result.
pub fn apply_rate_bounds(
    rate_bps: i128,
    adjustment_bps: i128,
    floor_bps: i128,
    ceiling_bps: i128,
) -> Result<i128, MathError> {
    let adjusted = checked_add(rate_bps, adjustment_bps)?;
    Ok(clamp(adjusted, floor_bps, ceiling_bps))
}

/// Supply rate derived from a spread: `borrow_rate - spread`, floored.
pub fn supply_rate_from_spread(
    borrow_rate_bps: i128,
    spread_bps: i128,
    floor_bps: i128,
) -> Result<i128, MathError> {
    let rate = checked_sub(borrow_rate_bps, spread_bps)?;
    Ok(rate.max(floor_bps))
}

/// Supply rate derived from utilization and the reserve factor.
///
/// `supply_rate = borrow_rate * utilization * (10_000 - reserve_factor) / 10_000^2`
pub fn supply_rate_from_reserve_factor(
    borrow_rate_bps: i128,
    utilization_bps: i128,
    reserve_factor_bps: i128,
) -> Result<i128, MathError> {
    let retained = checked_sub(BPS_DIVISOR, reserve_factor_bps)?;
    let after_utilization = apply_bps(borrow_rate_bps, utilization_bps)?;
    apply_bps(after_utilization, retained)
}

/// Simple (non-compounding) interest accrued over `elapsed_seconds`.
///
/// `interest = principal * rate_bps * elapsed / (10_000 * seconds_per_year)`
pub fn simple_interest(
    principal: i128,
    rate_bps: i128,
    elapsed_seconds: i128,
) -> Result<i128, MathError> {
    if principal <= 0 || rate_bps <= 0 || elapsed_seconds <= 0 {
        return Ok(0);
    }
    let annual = apply_bps(principal, rate_bps)?;
    checked_mul_div(annual, elapsed_seconds, SECONDS_PER_YEAR)
}

/// Growth factor to apply to a compounding index, scaled by `index_scale`.
///
/// Returns `index_scale * (1 + rate * elapsed / year)`, i.e. the multiplier that
/// advances a borrow or supply index over `elapsed_seconds`.
pub fn index_growth_factor(
    rate_bps: i128,
    elapsed_seconds: i128,
    index_scale: i128,
) -> Result<i128, MathError> {
    if index_scale <= 0 {
        return Err(MathError::DivisionByZero);
    }
    if rate_bps <= 0 || elapsed_seconds <= 0 {
        return Ok(index_scale);
    }
    let growth = simple_interest(index_scale, rate_bps, elapsed_seconds)?;
    checked_add(index_scale, growth)
}

/// Advances an index by the growth accrued over `elapsed_seconds`.
pub fn accrue_index(
    current_index: i128,
    rate_bps: i128,
    elapsed_seconds: i128,
    index_scale: i128,
) -> Result<i128, MathError> {
    let factor = index_growth_factor(rate_bps, elapsed_seconds, index_scale)?;
    checked_mul_div(current_index, factor, index_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(kind: RateModelKind) -> RateCurve {
        RateCurve {
            kind,
            base_rate_bps: 100,
            kink_utilization_bps: 8_000,
            multiplier_bps: 2_000,
            jump_multiplier_bps: 10_000,
        }
    }

    #[test]
    fn linear_is_base_at_zero_utilization() {
        assert_eq!(linear_rate(0, &curve(RateModelKind::Linear)).unwrap(), 100);
    }

    #[test]
    fn linear_rises_with_utilization() {
        let c = curve(RateModelKind::Linear);
        assert_eq!(linear_rate(5_000, &c).unwrap(), 100 + 1_000);
        assert_eq!(linear_rate(10_000, &c).unwrap(), 100 + 2_000);
    }

    #[test]
    fn kink_matches_base_at_zero_and_slope_at_kink() {
        let c = curve(RateModelKind::Kink);
        assert_eq!(kink_rate(0, &c).unwrap(), 100);
        assert_eq!(kink_rate(4_000, &c).unwrap(), 100 + 1_000);
        assert_eq!(kink_rate(8_000, &c).unwrap(), 2_100);
    }

    #[test]
    fn kink_steepens_above_the_kink() {
        let c = curve(RateModelKind::Kink);
        // Halfway through the jump segment: 2_100 + 10_000/2.
        assert_eq!(kink_rate(9_000, &c).unwrap(), 2_100 + 5_000);
        assert_eq!(kink_rate(10_000, &c).unwrap(), 2_100 + 10_000);
    }

    #[test]
    fn kink_is_monotonic() {
        let c = curve(RateModelKind::Kink);
        let mut previous = i128::MIN;
        let mut u = 0;
        while u <= 10_000 {
            let rate = kink_rate(u, &c).unwrap();
            assert!(rate >= previous, "rate dropped at utilization {u}");
            previous = rate;
            u += 100;
        }
    }

    #[test]
    fn kink_of_zero_returns_base_below_and_jumps_above() {
        let c = RateCurve {
            kink_utilization_bps: 0,
            ..curve(RateModelKind::Kink)
        };
        assert_eq!(kink_rate(0, &c).unwrap(), 100);
        assert_eq!(kink_rate(10_000, &c).unwrap(), 100 + 2_000 + 10_000);
    }

    #[test]
    fn kink_of_full_utilization_caps_at_the_kink_rate() {
        let c = RateCurve {
            kink_utilization_bps: BPS_DIVISOR,
            ..curve(RateModelKind::Kink)
        };
        assert_eq!(kink_rate(10_001, &c).unwrap(), 2_100);
    }

    #[test]
    fn jump_adds_on_top_of_linear() {
        let c = curve(RateModelKind::Jump);
        assert_eq!(jump_rate(8_000, &c).unwrap(), linear_rate(8_000, &c).unwrap());
        assert_eq!(
            jump_rate(9_000, &c).unwrap(),
            linear_rate(9_000, &c).unwrap() + 5_000
        );
    }

    #[test]
    fn jump_with_full_kink_is_linear() {
        let c = RateCurve {
            kink_utilization_bps: BPS_DIVISOR,
            ..curve(RateModelKind::Jump)
        };
        assert_eq!(
            jump_rate(10_001, &c).unwrap(),
            linear_rate(10_001, &c).unwrap()
        );
    }

    #[test]
    fn exponential_is_convex() {
        let c = curve(RateModelKind::Exponential);
        assert_eq!(exponential_rate(0, &c).unwrap(), 100);
        let low = exponential_rate(2_500, &c).unwrap();
        let mid = exponential_rate(5_000, &c).unwrap();
        let high = exponential_rate(7_500, &c).unwrap();
        assert!(mid - low < high - mid, "curve is not convex");
    }

    #[test]
    fn dual_slope_matches_linear_below_the_kink() {
        let c = curve(RateModelKind::DualSlope);
        assert_eq!(dual_slope_rate(0, &c).unwrap(), 100);
        assert_eq!(
            dual_slope_rate(5_000, &c).unwrap(),
            linear_rate(5_000, &c).unwrap()
        );
        assert_eq!(
            dual_slope_rate(8_000, &c).unwrap(),
            linear_rate(8_000, &c).unwrap()
        );
    }

    #[test]
    fn dual_slope_charges_the_excess_at_the_second_slope() {
        let c = curve(RateModelKind::DualSlope);
        // base 100 + 8000*2000/10000 + 2000*10000/10000
        assert_eq!(dual_slope_rate(10_000, &c).unwrap(), 100 + 1_600 + 2_000);
    }

    #[test]
    fn dual_slope_differs_from_normalized_kink() {
        let c = curve(RateModelKind::DualSlope);
        assert_ne!(
            dual_slope_rate(10_000, &c).unwrap(),
            kink_rate(10_000, &c).unwrap()
        );
    }

    #[test]
    fn borrow_rate_dispatches_per_kind() {
        for kind in [
            RateModelKind::Linear,
            RateModelKind::Kink,
            RateModelKind::Jump,
            RateModelKind::Exponential,
            RateModelKind::DualSlope,
        ] {
            let c = curve(kind);
            let expected = match kind {
                RateModelKind::Linear => linear_rate(6_000, &c).unwrap(),
                RateModelKind::Kink => kink_rate(6_000, &c).unwrap(),
                RateModelKind::Jump => jump_rate(6_000, &c).unwrap(),
                RateModelKind::Exponential => exponential_rate(6_000, &c).unwrap(),
                RateModelKind::DualSlope => dual_slope_rate(6_000, &c).unwrap(),
            };
            assert_eq!(c.borrow_rate(6_000).unwrap(), expected);
        }
    }

    #[test]
    fn curves_report_overflow_instead_of_wrapping() {
        let c = RateCurve {
            kind: RateModelKind::Linear,
            base_rate_bps: i128::MAX,
            kink_utilization_bps: 8_000,
            multiplier_bps: 10_000,
            jump_multiplier_bps: 10_000,
        };
        assert_eq!(linear_rate(10_000, &c), Err(MathError::Overflow));
    }

    #[test]
    fn utilization_cases() {
        assert_eq!(utilization_bps(0, 0).unwrap(), 0);
        assert_eq!(utilization_bps(100, 0).unwrap(), 0);
        assert_eq!(utilization_bps(0, 100).unwrap(), 0);
        assert_eq!(utilization_bps(50, 100).unwrap(), 5_000);
        // Borrows above deposits cap at 100%.
        assert_eq!(utilization_bps(200, 100).unwrap(), 10_000);
        // Negative inputs are treated as no exposure.
        assert_eq!(utilization_bps(-5, 100).unwrap(), 0);
    }

    #[test]
    fn rate_bounds_clamp_and_adjust() {
        assert_eq!(apply_rate_bounds(1_000, 500, 50, 10_000).unwrap(), 1_500);
        assert_eq!(apply_rate_bounds(1_000, -5_000, 50, 10_000).unwrap(), 50);
        assert_eq!(apply_rate_bounds(1_000, 50_000, 50, 10_000).unwrap(), 10_000);
        assert_eq!(
            apply_rate_bounds(i128::MAX, 1, 0, 10_000),
            Err(MathError::Overflow)
        );
    }

    #[test]
    fn supply_rate_from_spread_floors() {
        assert_eq!(supply_rate_from_spread(1_000, 200, 50).unwrap(), 800);
        assert_eq!(supply_rate_from_spread(100, 200, 50).unwrap(), 50);
    }

    #[test]
    fn supply_rate_from_reserve_factor_cases() {
        // 10% borrow rate, 50% utilization, 20% reserve factor -> 4%.
        assert_eq!(
            supply_rate_from_reserve_factor(1_000, 5_000, 2_000).unwrap(),
            400
        );
        // A 100% reserve factor pays suppliers nothing.
        assert_eq!(
            supply_rate_from_reserve_factor(1_000, 5_000, 10_000).unwrap(),
            0
        );
    }

    #[test]
    fn simple_interest_is_proportional_to_time() {
        let year = simple_interest(1_000_000, 1_000, SECONDS_PER_YEAR).unwrap();
        assert_eq!(year, 100_000);
        let half = simple_interest(1_000_000, 1_000, SECONDS_PER_YEAR / 2).unwrap();
        assert_eq!(half, 50_000);
        assert_eq!(simple_interest(1_000_000, 1_000, 0).unwrap(), 0);
        assert_eq!(simple_interest(0, 1_000, 100).unwrap(), 0);
        assert_eq!(simple_interest(1_000, 0, 100).unwrap(), 0);
        assert_eq!(simple_interest(1_000, 1_000, -1).unwrap(), 0);
    }

    #[test]
    fn index_growth_and_accrual() {
        let scale = 1_000_000_000_000i128;
        assert_eq!(index_growth_factor(0, SECONDS_PER_YEAR, scale).unwrap(), scale);
        assert_eq!(
            index_growth_factor(1_000, SECONDS_PER_YEAR, scale).unwrap(),
            scale + scale / 10
        );
        assert_eq!(
            index_growth_factor(1_000, SECONDS_PER_YEAR, 0),
            Err(MathError::DivisionByZero)
        );
        // Accruing 10% for a year lifts the index by 10%.
        assert_eq!(
            accrue_index(scale, 1_000, SECONDS_PER_YEAR, scale).unwrap(),
            scale + scale / 10
        );
        // Indexes never move backwards.
        assert_eq!(accrue_index(scale, 1_000, 0, scale).unwrap(), scale);
    }
}
