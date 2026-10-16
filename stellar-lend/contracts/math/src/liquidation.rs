//! Liquidation math shared by the liquidation paths.
//!
//! `hello-world::liquidate`, `liquidation-strategy` and the off-chain
//! liquidation bots all need the same five calculations: how unhealthy a
//! position is, how much of its debt may be repaid, what that debt is worth in
//! collateral terms, how much collateral that seizes once the incentive is
//! applied, and how the incentive splits between the liquidator and the
//! protocol. Each of those used to be open-coded at the call site.
//!
//! All ratios are in basis points and every operation is checked.

use crate::checked::{apply_bps, checked_add, checked_mul_div, checked_sub, BPS_DIVISOR};
use crate::error::MathError;

/// Upper bound on a dynamic liquidation penalty (20%).
///
/// Matches `hello-world::liquidate::MAX_PENALTY_BPS`; penalties above this would
/// let a liquidator take more than the position's safety buffer.
pub const MAX_PENALTY_BPS: i128 = 2_000;

/// Health factor in basis points: `collateral_value * 10_000 / debt_value`.
///
/// Returns `i128::MAX` for a debt-free position, which is infinitely healthy and
/// compares correctly against any threshold.
///
/// # Example
/// ```
/// use stellarlend_math::liquidation::health_factor_bps;
/// // 150 collateral against 100 debt is 150%.
/// assert_eq!(health_factor_bps(150, 100).unwrap(), 15_000);
/// ```
pub fn health_factor_bps(collateral_value: i128, debt_value: i128) -> Result<i128, MathError> {
    if debt_value == 0 {
        return Ok(i128::MAX);
    }
    checked_mul_div(collateral_value, BPS_DIVISOR, debt_value)
}

/// Whether a position's health factor has fallen below `threshold_bps`.
pub fn is_liquidatable(
    collateral_value: i128,
    debt_value: i128,
    threshold_bps: i128,
) -> Result<bool, MathError> {
    Ok(health_factor_bps(collateral_value, debt_value)? < threshold_bps)
}

/// Liquidation penalty that scales with how far a position sits below the
/// liquidation threshold.
///
/// At or above the threshold the penalty is `base_incentive_bps`. Below it the
/// penalty interpolates linearly toward [`MAX_PENALTY_BPS`] in proportion to the
/// shortfall, so deeply underwater positions pay liquidators more:
///
/// ```text
/// severity = (threshold - health_factor) / threshold
/// penalty  = base + severity * (MAX_PENALTY_BPS - base)
/// ```
///
/// The result is capped at [`MAX_PENALTY_BPS`].
pub fn dynamic_penalty_bps(
    collateral_value: i128,
    total_debt: i128,
    base_incentive_bps: i128,
    threshold_bps: i128,
) -> Result<i128, MathError> {
    if total_debt == 0 || threshold_bps <= 0 {
        return Ok(base_incentive_bps);
    }

    let health = health_factor_bps(collateral_value, total_debt)?;
    if health >= threshold_bps {
        return Ok(base_incentive_bps);
    }

    let shortfall = checked_sub(threshold_bps, health)?;
    let penalty_range = checked_sub(MAX_PENALTY_BPS, base_incentive_bps)?;
    if penalty_range <= 0 {
        return Ok(base_incentive_bps.min(MAX_PENALTY_BPS));
    }
    let extra = checked_mul_div(shortfall, penalty_range, threshold_bps)?;
    let penalty = checked_add(base_incentive_bps, extra)?;
    Ok(penalty.min(MAX_PENALTY_BPS))
}

/// Value of a collateral amount expressed in the debt asset.
///
/// `collateral_amount * collateral_price / debt_price`
///
/// # Errors
/// [`MathError::DivisionByZero`] when `debt_price` is zero — callers map this to
/// their own "price unavailable" error.
pub fn collateral_value_in_debt(
    collateral_amount: i128,
    collateral_price: i128,
    debt_price: i128,
) -> Result<i128, MathError> {
    if debt_price == 0 {
        return Err(MathError::DivisionByZero);
    }
    checked_mul_div(collateral_amount, collateral_price, debt_price)
}

/// Amount of the collateral asset equivalent to a debt amount.
///
/// `debt_amount * debt_price / collateral_price` — the inverse of
/// [`collateral_value_in_debt`].
pub fn debt_value_in_collateral(
    debt_amount: i128,
    debt_price: i128,
    collateral_price: i128,
) -> Result<i128, MathError> {
    if collateral_price == 0 {
        return Err(MathError::DivisionByZero);
    }
    checked_mul_div(debt_amount, debt_price, collateral_price)
}

/// Maximum debt that may be repaid in one liquidation, from the close factor.
///
/// `min(total_debt * close_factor_bps / 10_000, requested)` when `requested` is
/// positive, otherwise the close-factor cap itself.
pub fn max_repayable(
    total_debt: i128,
    close_factor_bps: i128,
    requested: i128,
) -> Result<i128, MathError> {
    let cap = apply_bps(total_debt, close_factor_bps)?;
    if requested <= 0 {
        return Ok(cap);
    }
    Ok(cap.min(requested))
}

/// The incentive paid on a repaid debt amount: `repaid * incentive_bps / 10_000`.
pub fn incentive_amount(repaid: i128, incentive_bps: i128) -> Result<i128, MathError> {
    apply_bps(repaid, incentive_bps)
}

/// Collateral seized for a repayment, including the incentive.
///
/// `collateral_equivalent * (10_000 + incentive_bps) / 10_000`
pub fn seize_amount(collateral_equivalent: i128, incentive_bps: i128) -> Result<i128, MathError> {
    let multiplier = checked_add(BPS_DIVISOR, incentive_bps)?;
    checked_mul_div(collateral_equivalent, multiplier, BPS_DIVISOR)
}

/// How a liquidation's proceeds split between liquidator and protocol.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LiquidationSplit {
    /// Collateral actually seized, capped at the borrower's balance.
    pub collateral_seized: i128,
    /// Protocol fee taken from the incentive.
    pub protocol_fee: i128,
    /// Collateral transferred to the liquidator, after the protocol fee.
    pub liquidator_collateral: i128,
}

/// Splits a liquidation's proceeds between the liquidator and the protocol.
///
/// The seize amount is capped at `collateral_balance` — a position can never
/// give up more collateral than it holds — and the protocol fee is charged on
/// the incentive portion only, never on the principal the liquidator repaid.
///
/// # Errors
/// [`MathError::Underflow`] when the fee exceeds the seized collateral, which
/// means the fee configuration is inconsistent with the incentive.
pub fn split_proceeds(
    collateral_equivalent: i128,
    collateral_balance: i128,
    repaid: i128,
    incentive_bps: i128,
    protocol_fee_bps: i128,
) -> Result<LiquidationSplit, MathError> {
    let uncapped = seize_amount(collateral_equivalent, incentive_bps)?;
    let collateral_seized = uncapped.min(collateral_balance);

    let incentive = incentive_amount(repaid, incentive_bps)?;
    let protocol_fee = apply_bps(incentive, protocol_fee_bps)?;
    let liquidator_collateral = checked_sub(collateral_seized, protocol_fee)?;
    if liquidator_collateral < 0 {
        return Err(MathError::Underflow);
    }

    Ok(LiquidationSplit {
        collateral_seized,
        protocol_fee,
        liquidator_collateral,
    })
}

/// Whether a liquidation clears the liquidator's minimum profit floor.
///
/// Compares the net collateral received (in debt terms) against the repaid debt
/// plus `min_profit_bps` of it. Batch callers use this to skip the gas-heavy
/// transfer path for positions that are not worth liquidating.
pub fn is_profitable(
    repaid: i128,
    net_collateral_in_debt_terms: i128,
    min_profit_bps: i128,
) -> Result<bool, MathError> {
    let floor = checked_add(repaid, apply_bps(repaid, min_profit_bps)?)?;
    Ok(net_collateral_in_debt_terms >= floor)
}

/// Priority score used to order a liquidation batch, highest first.
///
/// Combines how much collateral backs the debt (a higher ratio means more to
/// seize) with the absolute size of the repayment, normalized by `debt_scale` so
/// a single large position does not swamp the ratio term. Saturating arithmetic
/// is deliberate: scoring is a heuristic and must never abort a batch.
pub fn priority_score(
    collateral: i128,
    total_debt: i128,
    debt_amount: i128,
    debt_scale: i128,
) -> u64 {
    if total_debt <= 0 {
        return 0;
    }
    let collateral_ratio = collateral
        .saturating_mul(BPS_DIVISOR)
        .checked_div(total_debt)
        .unwrap_or(0)
        .max(0);
    let size_term = if debt_scale > 0 {
        debt_amount.checked_div(debt_scale).unwrap_or(0).max(0)
    } else {
        0
    };
    (collateral_ratio as u64).saturating_add(size_term as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_factor_cases() {
        assert_eq!(health_factor_bps(150, 100).unwrap(), 15_000);
        assert_eq!(health_factor_bps(100, 100).unwrap(), 10_000);
        assert_eq!(health_factor_bps(90, 100).unwrap(), 9_000);
        assert_eq!(health_factor_bps(0, 100).unwrap(), 0);
        assert_eq!(health_factor_bps(100, 0).unwrap(), i128::MAX);
    }

    #[test]
    fn liquidatable_below_threshold_only() {
        assert!(is_liquidatable(100, 100, 10_500).unwrap());
        assert!(!is_liquidatable(110, 100, 10_500).unwrap());
        // Debt-free positions are never liquidatable.
        assert!(!is_liquidatable(0, 0, 10_500).unwrap());
    }

    #[test]
    fn penalty_is_base_when_healthy() {
        assert_eq!(dynamic_penalty_bps(200, 100, 1_000, 10_500).unwrap(), 1_000);
        // No debt: nothing to scale against.
        assert_eq!(dynamic_penalty_bps(200, 0, 1_000, 10_500).unwrap(), 1_000);
        // Degenerate threshold falls back to the base incentive.
        assert_eq!(dynamic_penalty_bps(50, 100, 1_000, 0).unwrap(), 1_000);
    }

    #[test]
    fn penalty_scales_with_severity() {
        let mild = dynamic_penalty_bps(104, 100, 1_000, 10_500).unwrap();
        let severe = dynamic_penalty_bps(60, 100, 1_000, 10_500).unwrap();
        assert!(severe > mild, "{severe} !> {mild}");
        assert!(mild >= 1_000);
        assert!(severe <= MAX_PENALTY_BPS);
    }

    #[test]
    fn penalty_is_capped() {
        // Worthless collateral: severity is at its maximum.
        assert_eq!(
            dynamic_penalty_bps(0, 100, 1_000, 10_500).unwrap(),
            MAX_PENALTY_BPS
        );
        // A base already at or above the cap stays at the cap.
        assert_eq!(
            dynamic_penalty_bps(0, 100, MAX_PENALTY_BPS, 10_500).unwrap(),
            MAX_PENALTY_BPS
        );
        assert_eq!(
            dynamic_penalty_bps(0, 100, MAX_PENALTY_BPS + 500, 10_500).unwrap(),
            MAX_PENALTY_BPS
        );
    }

    #[test]
    fn price_conversions_round_trip() {
        // 10 units of collateral at 3, debt priced at 1 -> 30 in debt terms.
        assert_eq!(collateral_value_in_debt(10, 3, 1).unwrap(), 30);
        assert_eq!(debt_value_in_collateral(30, 1, 3).unwrap(), 10);
        assert_eq!(
            collateral_value_in_debt(10, 3, 0),
            Err(MathError::DivisionByZero)
        );
        assert_eq!(
            debt_value_in_collateral(10, 3, 0),
            Err(MathError::DivisionByZero)
        );
    }

    #[test]
    fn close_factor_caps_repayment() {
        // 50% close factor on 1,000 debt.
        assert_eq!(max_repayable(1_000, 5_000, 0).unwrap(), 500);
        // A smaller request is honored as-is.
        assert_eq!(max_repayable(1_000, 5_000, 200).unwrap(), 200);
        // A larger request is clipped to the cap.
        assert_eq!(max_repayable(1_000, 5_000, 900).unwrap(), 500);
        assert_eq!(max_repayable(0, 5_000, 900).unwrap(), 0);
    }

    #[test]
    fn seize_includes_the_incentive() {
        assert_eq!(seize_amount(1_000, 1_000).unwrap(), 1_100);
        assert_eq!(seize_amount(1_000, 0).unwrap(), 1_000);
        assert_eq!(incentive_amount(1_000, 1_000).unwrap(), 100);
    }

    #[test]
    fn split_charges_fee_on_the_incentive_only() {
        let split = split_proceeds(1_000, 10_000, 1_000, 1_000, 2_000).unwrap();
        assert_eq!(split.collateral_seized, 1_100);
        // 20% of the 100 incentive.
        assert_eq!(split.protocol_fee, 20);
        assert_eq!(split.liquidator_collateral, 1_080);
    }

    #[test]
    fn split_caps_at_available_collateral() {
        let split = split_proceeds(1_000, 900, 1_000, 1_000, 0).unwrap();
        assert_eq!(split.collateral_seized, 900);
        assert_eq!(split.liquidator_collateral, 900);
    }

    #[test]
    fn split_rejects_a_fee_larger_than_the_seizure() {
        // Nothing left to seize, but a fee is still charged on the incentive.
        let result = split_proceeds(1_000, 0, 1_000, 1_000, 10_000);
        assert_eq!(result, Err(MathError::Underflow));
    }

    #[test]
    fn profitability_floor() {
        // Exactly at the floor counts as profitable.
        assert!(is_profitable(1_000, 1_002, 20).unwrap());
        assert!(!is_profitable(1_000, 1_001, 20).unwrap());
        assert!(is_profitable(1_000, 1_500, 20).unwrap());
        // A zero floor only requires breaking even.
        assert!(is_profitable(1_000, 1_000, 0).unwrap());
    }

    #[test]
    fn priority_orders_by_collateral_ratio_then_size() {
        let well_backed = priority_score(2_000, 1_000, 1_000_000, 1_000_000);
        let thin = priority_score(1_100, 1_000, 1_000_000, 1_000_000);
        assert!(well_backed > thin);

        // Same ratio, larger repayment wins.
        let big = priority_score(2_000, 1_000, 5_000_000, 1_000_000);
        assert!(big > well_backed);
    }

    #[test]
    fn priority_handles_degenerate_inputs() {
        assert_eq!(priority_score(1_000, 0, 1_000, 1_000), 0);
        assert_eq!(priority_score(-1, 1_000, -1, 1_000), 0);
        // A zero scale drops the size term instead of dividing by zero.
        assert_eq!(priority_score(1_000, 1_000, 5_000, 0), 10_000);
        // Saturating multiplication must not panic on extreme collateral.
        let _ = priority_score(i128::MAX, 1, i128::MAX, 1);
    }
}
