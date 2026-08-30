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
            self.base_rate_bps
                + (utilization_bps * self.slope_bps) / self.kink_utilization_bps
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
