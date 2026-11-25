#![no_std]
//! Shared, environment-free pool arithmetic (issue #1759).
//!
//! Contracts, indexers and SDK tools all need the same formulas. Keeping them
//! here avoids the copy-per-module drift that made utilisation and health
//! factor calculations disagree between the pool and the liquidation path.
//! No `soroban_sdk::Env`, storage or host object is used in this crate.

pub use stellarlend_safe_math::{
    apply_bps, checked_mul_div, SafeMathError as MathError, BPS_DIVISOR,
};

/// Seconds in a 365-day year, used by all rate helpers.
pub const SECONDS_PER_YEAR: i128 = 365 * 86_400;
/// Scale used by lending indexes.
pub const INDEX_SCALE: i128 = 1_000_000_000_000;
/// Health factor reported for a position with no debt.
pub const NO_DEBT_HEALTH_FACTOR: i128 = i128::MAX;

/// Express `numerator / denominator` in basis points.
///
/// A zero denominator yields `0`; negative inputs are rejected.
pub fn ratio_bps(numerator: i128, denominator: i128) -> Result<i128, MathError> {
    if numerator < 0 || denominator < 0 {
        return Err(MathError::InvalidInput);
    }
    if denominator == 0 {
        return Ok(0);
    }
    checked_mul_div(numerator, BPS_DIVISOR, denominator)
}

/// Pool utilisation in basis points, clamped to `10_000`.
pub fn utilization_bps(total_borrows: i128, total_supply: i128) -> Result<i128, MathError> {
    if total_borrows < 0 || total_supply < 0 {
        return Err(MathError::InvalidInput);
    }
    if total_supply == 0 {
        return Ok(0);
    }
    let raw = ratio_bps(total_borrows, total_supply)?;
    Ok(if raw > BPS_DIVISOR { BPS_DIVISOR } else { raw })
}

/// Simple accrued interest using the contract's 365-day year.
pub fn simple_interest(
    principal: i128,
    rate_bps: i128,
    elapsed_seconds: i128,
) -> Result<i128, MathError> {
    if principal < 0 || rate_bps < 0 || elapsed_seconds < 0 {
        return Err(MathError::InvalidInput);
    }
    checked_mul_div(
        checked_mul_div(principal, rate_bps, BPS_DIVISOR)?,
        elapsed_seconds,
        SECONDS_PER_YEAR,
    )
}

/// Grow an index by `rate_bps` over `elapsed_seconds`.
pub fn accrue_index(
    current_index: i128,
    rate_bps: i128,
    elapsed_seconds: i128,
) -> Result<i128, MathError> {
    if current_index <= 0 || rate_bps < 0 || elapsed_seconds < 0 {
        return Err(MathError::InvalidInput);
    }
    let growth = checked_mul_div(
        checked_mul_div(current_index, rate_bps, BPS_DIVISOR)?,
        elapsed_seconds,
        SECONDS_PER_YEAR,
    )?;
    current_index.checked_add(growth).ok_or(MathError::Overflow)
}

/// Health factor in basis points: `collateral * threshold / debt`.
///
/// Debt-free positions return [`NO_DEBT_HEALTH_FACTOR`]. A zero threshold is
/// invalid because it would make every indebted position liquidatable by
/// definition.
pub fn health_factor_bps(
    collateral_value: i128,
    debt_value: i128,
    liquidation_threshold_bps: i128,
) -> Result<i128, MathError> {
    if collateral_value < 0 || debt_value < 0 {
        return Err(MathError::InvalidInput);
    }
    if liquidation_threshold_bps <= 0 || liquidation_threshold_bps > BPS_DIVISOR {
        return Err(MathError::InvalidInput);
    }
    if debt_value == 0 {
        return Ok(NO_DEBT_HEALTH_FACTOR);
    }
    // Keep the threshold in the numerator: `collateral * threshold / debt`.
    // Applying bps first would divide by the divisor twice and report a
    // health factor of 1 instead of the intended basis-point value.
    checked_mul_div(collateral_value, liquidation_threshold_bps, debt_value)
}

/// Maximum additional debt a position can take while respecting `ltv_bps`.
pub fn max_borrow_amount(
    collateral_value: i128,
    ltv_bps: i128,
    current_debt: i128,
) -> Result<i128, MathError> {
    if collateral_value < 0 || ltv_bps < 0 || ltv_bps > BPS_DIVISOR || current_debt < 0 {
        return Err(MathError::InvalidInput);
    }
    let capacity = apply_bps(collateral_value, ltv_bps)?;
    Ok(if capacity > current_debt {
        capacity - current_debt
    } else {
        0
    })
}

/// Collateral seized for a repayment, including the liquidation incentive.
pub fn seize_amount(repaid: i128, incentive_bps: i128) -> Result<i128, MathError> {
    if repaid < 0 || incentive_bps < 0 {
        return Err(MathError::InvalidInput);
    }
    apply_bps(
        repaid,
        BPS_DIVISOR
            .checked_add(incentive_bps)
            .ok_or(MathError::Overflow)?,
    )
}

/// Repayment capped by both the close factor and the requested amount.
pub fn max_repayable(
    total_debt: i128,
    close_factor_bps: i128,
    requested: i128,
) -> Result<i128, MathError> {
    if total_debt < 0 || close_factor_bps < 0 || close_factor_bps > BPS_DIVISOR || requested < 0 {
        return Err(MathError::InvalidInput);
    }
    // A request that covers the whole debt closes the position outright and
    // is exempt from the close factor. Partial repayments stay capped.
    if requested >= total_debt {
        return Ok(total_debt);
    }
    let cap = apply_bps(total_debt, close_factor_bps)?;
    Ok(if cap < requested { cap } else { requested })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bps_helpers_use_the_10_000_scale() {
        assert_eq!(apply_bps(20_000, 8_000), Ok(16_000));
        assert_eq!(ratio_bps(1, 4), Ok(2_500));
        assert_eq!(ratio_bps(0, 0), Ok(0));
        assert_eq!(apply_bps(-1, 100), Err(MathError::InvalidInput));
    }

    #[test]
    fn utilisation_is_clamped() {
        assert_eq!(utilization_bps(5_000, 20_000), Ok(2_500));
        assert_eq!(utilization_bps(30_000, 20_000), Ok(10_000));
        assert_eq!(utilization_bps(0, 0), Ok(0));
        assert_eq!(utilization_bps(1, -1), Err(MathError::InvalidInput));
    }

    #[test]
    fn health_factor_matches_contract_semantics() {
        assert_eq!(health_factor_bps(20_000, 10_000, 8_000), Ok(16_000));
        assert_eq!(
            health_factor_bps(20_000, 0, 8_000),
            Ok(NO_DEBT_HEALTH_FACTOR)
        );
        assert_eq!(health_factor_bps(1, 1, 0), Err(MathError::InvalidInput));
    }

    #[test]
    fn liquidation_sizing_is_bounded() {
        assert_eq!(seize_amount(1_000, 500), Ok(1_050));
        assert_eq!(max_repayable(1_000, 5_000, 900), Ok(500));
        assert_eq!(max_repayable(100, 5_000, 900), Ok(100));
        assert_eq!(
            max_borrow_amount(20_000, 7_500, 1_000),
            Ok(20_000 * 7_500 / BPS_DIVISOR - 1_000)
        );
    }

    #[test]
    fn interest_helpers_use_one_year() {
        assert_eq!(simple_interest(1_000, 1_000, SECONDS_PER_YEAR), Ok(100));
        assert_eq!(
            accrue_index(INDEX_SCALE, 1_000, SECONDS_PER_YEAR),
            Ok(INDEX_SCALE + INDEX_SCALE / 10)
        );
        assert_eq!(accrue_index(0, 100, 0), Err(MathError::InvalidInput));
    }
}
