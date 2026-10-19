use crate::MathError;

/// Basis points divisor (100% = 10,000).
pub const BPS_DIVISOR: i128 = 10_000;

/// Seconds in a year for interest calculations.
pub const SECONDS_PER_YEAR: i128 = 31_536_000;

/// Interest rate model with a kinked utilization curve (two-segment piecewise linear).
#[derive(Clone, Debug)]
pub struct InterestRateModel {
    /// Base rate in basis points (e.g., 200 = 2%).
    pub base_rate_bps: i128,
    /// Kink utilization in basis points (e.g., 8000 = 80%).
    pub kink_utilization_bps: i128,
    /// Slope below kink in basis points.
    pub slope_bps: i128,
    /// Slope above kink (jump multiplier) in basis points.
    pub jump_slope_bps: i128,
}

impl InterestRateModel {
    /// Calculate the borrow rate for a given utilization level.
    ///
    /// Below the kink: `base_rate + utilization * slope / kink`
    /// Above the kink: `rate_at_kink + (utilization - kink) * jump_slope / (10000 - kink)`
    pub fn calculate_borrow_rate(&self, utilization_bps: i128) -> i128 {
        if utilization_bps <= self.kink_utilization_bps {
            // Below or at kink: linear slope
            if self.kink_utilization_bps == 0 {
                return self.base_rate_bps;
            }
            self.base_rate_bps + (utilization_bps * self.slope_bps) / self.kink_utilization_bps
        } else {
            // Above kink: jump slope
            let rate_at_kink = self.base_rate_bps + self.slope_bps;
            let excess = utilization_bps - self.kink_utilization_bps;
            let max_excess = BPS_DIVISOR - self.kink_utilization_bps;
            if max_excess == 0 {
                return rate_at_kink;
            }
            rate_at_kink + (excess * self.jump_slope_bps) / max_excess
        }
    }

    /// Calculate the supply rate from the borrow rate, utilization, and reserve factor.
    ///
    /// `supply_rate = borrow_rate * utilization * (1 - reserve_factor) / 10000`
    pub fn calculate_supply_rate(
        &self,
        borrow_rate_bps: i128,
        utilization_bps: i128,
        reserve_factor_bps: i128,
    ) -> i128 {
        let factor = BPS_DIVISOR - reserve_factor_bps;
        (borrow_rate_bps * utilization_bps * factor) / (BPS_DIVISOR * BPS_DIVISOR)
    }
}

/// Calculate pool utilization in basis points.
///
/// `utilization = (total_borrows * 10000) / total_supply`
pub fn calculate_utilization(total_borrows: i128, total_supply: i128) -> i128 {
    if total_supply == 0 {
        return 0;
    }
    (total_borrows * BPS_DIVISOR) / total_supply
}

/// Calculate simple interest accrual.
///
/// Returns `principal + (principal * rate_bps * time_elapsed) / (10000 * SECONDS_PER_YEAR)`.
pub fn accrue_interest(principal: i128, rate_bps: i128, time_elapsed: i128) -> i128 {
    if principal == 0 || rate_bps == 0 || time_elapsed == 0 {
        return principal;
    }
    let interest = (principal * rate_bps * time_elapsed) / (BPS_DIVISOR * SECONDS_PER_YEAR);
    principal + interest
}

/// Calculate discrete compound interest.
///
/// `result = principal * (1 + rate_bps / 10000) ^ periods`
pub fn compound_interest(principal: i128, rate_bps: i128, periods: u32) -> i128 {
    if principal == 0 || rate_bps == 0 || periods == 0 {
        return principal;
    }
    let mut result = principal;
    for _ in 0..periods {
        result = result + (result * rate_bps) / BPS_DIVISOR;
    }
    result
}

/// Calculate the health factor in basis points.
///
/// `health_factor = (collateral_value * liquidation_threshold) / debt_value`
///
/// Returns 0 if debt is zero (no risk). A value < 10000 means the position is liquidatable.
pub fn health_factor_bps(
    collateral_value: i128,
    debt_value: i128,
    liquidation_threshold_bps: i128,
) -> i128 {
    if debt_value == 0 {
        return i128::MAX; // No debt = infinitely healthy
    }
    (collateral_value * liquidation_threshold_bps) / debt_value
}

/// Check if a position is liquidatable (health factor < 10000).
pub fn is_liquidatable(
    collateral_value: i128,
    debt_value: i128,
    liquidation_threshold_bps: i128,
) -> bool {
    health_factor_bps(collateral_value, debt_value, liquidation_threshold_bps) < BPS_DIVISOR
}

/// Calculate loan-to-value ratio in basis points.
///
/// `ltv = (debt_value * 10000) / collateral_value`
pub fn ltv_bps(debt_value: i128, collateral_value: i128) -> i128 {
    if collateral_value == 0 {
        return i128::MAX;
    }
    (debt_value * BPS_DIVISOR) / collateral_value
}

/// Calculate the maximum liquidatable amount respecting the close factor.
///
/// `max_liquidatable = (debt_value * close_factor_bps) / 10000`
pub fn max_liquidatable(debt_value: i128, close_factor_bps: i128) -> i128 {
    (debt_value * close_factor_bps) / BPS_DIVISOR
}

/// Calculate the liquidation bonus amount.
///
/// `bonus = (repay_amount * bonus_bps) / 10000`
pub fn liquidation_bonus(repay_amount: i128, bonus_bps: i128) -> i128 {
    (repay_amount * bonus_bps) / BPS_DIVISOR
}

/// Calculate the seize amount (debt repaid + liquidation bonus).
pub fn seize_amount(repay_amount: i128, bonus_bps: i128) -> i128 {
    repay_amount + liquidation_bonus(repay_amount, bonus_bps)
}

/// Validate that a collateral ratio meets the minimum requirement.
///
/// Returns `Ok(())` if `collateral_value * BPS_DIVISOR >= debt_value * min_ratio_bps`.
pub fn validate_collateral_ratio(
    collateral_value: i128,
    debt_value: i128,
    min_ratio_bps: i128,
) -> Result<(), MathError> {
    let required = debt_value
        .checked_mul(min_ratio_bps)
        .ok_or(MathError::Overflow)?;
    let actual = collateral_value
        .checked_mul(BPS_DIVISOR)
        .ok_or(MathError::Overflow)?;
    if actual < required {
        Err(MathError::Underflow)
    } else {
        Ok(())
    }
}

/// Calculate the maximum borrow amount given collateral and LTV.
///
/// `max_borrow = (collateral_value * ltv_bps) / 10000`
pub fn max_borrow_amount(collateral_value: i128, ltv_bps: i128) -> i128 {
    (collateral_value * ltv_bps) / BPS_DIVISOR
}

/// Calculate collateral ratio in basis points.
///
/// `ratio = (collateral_value * 10000) / debt_value`
pub fn collateral_ratio_bps(collateral_value: i128, debt_value: i128) -> i128 {
    if debt_value == 0 {
        return i128::MAX;
    }
    (collateral_value * BPS_DIVISOR) / debt_value
}

// ===========================================================================
// Issue #1407 — Position health simulation (read-only helper)
// ===========================================================================

/// Read-only position health simulation.
///
/// Applies a hypothetical price change (in basis points; negative = drop) to
/// the collateral value and returns the resulting health factor in bps. No
/// state is mutated.
pub fn simulate_health_factor(
    collateral_value: i128,
    debt_value: i128,
    liquidation_threshold_bps: i128,
    price_change_bps: i128,
) -> i128 {
    if debt_value == 0 {
        return i128::MAX;
    }
    let scaled = 10_000i128
        .checked_add(price_change_bps)
        .unwrap_or(0)
        .max(0);
    let new_collateral = collateral_value
        .checked_mul(scaled)
        .and_then(|v| v.checked_div(BPS_DIVISOR))
        .unwrap_or(0);
    health_factor_bps(new_collateral, debt_value, liquidation_threshold_bps)
}

// ===========================================================================
// Issue #1406 — Close-factor limits
// ===========================================================================

/// Minimum enforceable close factor (1 bps).
pub const MIN_CLOSE_FACTOR_BPS: i128 = 1;
/// Maximum enforceable close factor (100% = 10_000 bps).
pub const MAX_CLOSE_FACTOR_BPS: i128 = 10_000;

/// Validate that a close factor lies within the enforced limits
/// ([`MIN_CLOSE_FACTOR_BPS`] ..= [`MAX_CLOSE_FACTOR_BPS`]).
pub fn validate_close_factor_limits(bps: i128) -> Result<(), MathError> {
    if bps < MIN_CLOSE_FACTOR_BPS || bps > MAX_CLOSE_FACTOR_BPS {
        return Err(MathError::Overflow);
    }
    Ok(())
}

// ===========================================================================
// Issue #1405 — Bad-debt handling (accounting + reserve usage)
// ===========================================================================

/// Calculate the bad-debt portion of a position: the amount by which debt
/// exceeds the collateral value at the liquidation threshold. Returns `0` for
/// healthy positions (no bad debt).
pub fn bad_debt_amount(
    collateral_value: i128,
    debt_value: i128,
    liquidation_threshold_bps: i128,
) -> i128 {
    if debt_value == 0 {
        return 0;
    }
    let collateral_at_threshold = collateral_value
        .checked_mul(liquidation_threshold_bps)
        .and_then(|v| v.checked_div(BPS_DIVISOR))
        .unwrap_or(0);
    debt_value.saturating_sub(collateral_at_threshold)
}

/// Apply protocol reserves to cover bad debt.
///
/// Returns `(remaining_reserves, remaining_bad_debt)` after using reserves to
/// cover as much bad debt as possible.
pub fn apply_reserves_to_bad_debt(reserves: i128, bad_debt: i128) -> (i128, i128) {
    let coverage = reserves.min(bad_debt);
    (reserves - coverage, bad_debt - coverage)
}

// ===========================================================================
// Issue #1404 — Liquidation bonus tiers (by health factor)
// ===========================================================================

/// A liquidation bonus tier. Positions whose health factor is below
/// `max_health_factor_bps` receive `bonus_bps`. Riskier (lower-health)
/// positions earn a higher bonus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationBonusTier {
    pub max_health_factor_bps: i128,
    pub bonus_bps: i128,
}

/// Compute the liquidation bonus using health-factor-based tiers.
///
/// Tiers should be sorted ascending by `max_health_factor_bps`. The first tier
/// whose `max_health_factor_bps` is greater than the position's health factor
/// applies; if no tier matches, the last tier is used as the default.
pub fn liquidation_bonus_tiered(
    repay_amount: i128,
    health_factor_bps: i128,
    tiers: &[LiquidationBonusTier],
) -> Result<i128, MathError> {
    let bonus_bps = if tiers.is_empty() {
        0
    } else {
        let mut selected = tiers.last().unwrap().bonus_bps;
        for tier in tiers.iter() {
            if health_factor_bps < tier.max_health_factor_bps {
                selected = tier.bonus_bps;
                break;
            }
        }
        selected
    };
    liquidation_bonus(repay_amount, bonus_bps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulate_health_factor_healthy_remains_healthy() {
        // collateral 2000, debt 1000, threshold 8000 -> hf = 16_000 bps
        assert_eq!(simulate_health_factor(2000, 1000, 8000, 0), 16_000);
    }

    #[test]
    fn simulate_health_factor_price_drop_becomes_liquidatable() {
        // collateral 1000 * 0.6 = 600, debt 800, threshold 8000 -> hf = 6000
        let hf = simulate_health_factor(1000, 800, 8000, -4000);
        assert_eq!(hf, 6_000);
        assert!(hf < BPS_DIVISOR);
    }

    #[test]
    fn simulate_health_factor_no_debt_is_max() {
        assert_eq!(
            simulate_health_factor(1000, 0, 8000, -5000),
            i128::MAX
        );
    }

    #[test]
    fn close_factor_limits_valid_and_invalid() {
        assert!(validate_close_factor_limits(1).is_ok());
        assert!(validate_close_factor_limits(5_000).is_ok());
        assert!(validate_close_factor_limits(10_000).is_ok());
        assert!(validate_close_factor_limits(0).is_err());
        assert!(validate_close_factor_limits(10_001).is_err());
        assert_eq!(MIN_CLOSE_FACTOR_BPS, 1);
        assert_eq!(MAX_CLOSE_FACTOR_BPS, 10_000);
    }

    #[test]
    fn bad_debt_amount_healthy_is_zero() {
        assert_eq!(bad_debt_amount(2000, 1000, 8000), 0);
    }

    #[test]
    fn bad_debt_amount_underwater() {
        // collateral at threshold: 1000 * 8000 / 10000 = 800; debt 1000 -> 200
        assert_eq!(bad_debt_amount(1000, 1000, 8000), 200);
    }

    #[test]
    fn apply_reserves_to_bad_debt_partial_coverage() {
        let (reserves, debt) = apply_reserves_to_bad_debt(300, 1000);
        assert_eq!((reserves, debt), (0, 700));
    }

    #[test]
    fn apply_reserves_to_bad_debt_full_coverage() {
        let (reserves, debt) = apply_reserves_to_bad_debt(1500, 1000);
        assert_eq!((reserves, debt), (500, 0));
    }

    #[test]
    fn liquidation_bonus_tiered_picks_riskier_tier() {
        let tiers = [
            LiquidationBonusTier {
                max_health_factor_bps: 15_000,
                bonus_bps: 500,
            },
            LiquidationBonusTier {
                max_health_factor_bps: 10_000,
                bonus_bps: 1000,
            },
            LiquidationBonusTier {
                max_health_factor_bps: 5_000,
                bonus_bps: 2000,
            },
        ];
        // hf 12_000 -> first tier (15_000) -> 500
        assert_eq!(liquidation_bonus_tiered(1000, 12_000, &tiers).unwrap(), 500);
        // hf 8_000 -> second tier (10_000) -> 1000
        assert_eq!(liquidation_bonus_tiered(1000, 8_000, &tiers).unwrap(), 1000);
        // hf 3_000 -> third tier (5_000) -> 2000
        assert_eq!(liquidation_bonus_tiered(1000, 3_000, &tiers).unwrap(), 2000);
        // hf 20_000 -> no tier matches -> last tier default 2000
        assert_eq!(
            liquidation_bonus_tiered(1000, 20_000, &tiers).unwrap(),
            2000
        );
    }

    #[test]
    fn liquidation_bonus_tiered_empty_is_zero() {
        let empty: &[LiquidationBonusTier] = &[];
        assert_eq!(liquidation_bonus_tiered(1000, 5_000, empty).unwrap(), 0);
    }
}
