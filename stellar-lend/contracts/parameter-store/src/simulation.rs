//! Parameter impact simulation (issue #697).
//!
//! A governance voter needs to know what a parameter change *does* before
//! voting on it, not after it takes effect. This module projects the effect of
//! a proposed value onto a snapshot of pool state: how borrowing power moves,
//! how much debt becomes liquidatable, and how the borrow rate shifts.
//!
//! The projections are deliberately simple and deterministic — they read a
//! snapshot the caller supplies rather than reaching into pool contracts, so
//! the same simulation can be run off-chain by the API and on-chain by a voter
//! and produce the same numbers. They use [`stellarlend_math`] so the
//! arithmetic matches what the pool will actually do.

use soroban_sdk::{contracttype, Env, Symbol, Vec};
use stellarlend_math::checked::{apply_bps, ratio_bps};
use stellarlend_math::rates::{kink_rate, RateCurve, RateModelKind};

use crate::{ParameterType, BPS_DIVISOR};

/// The pool state a simulation is run against.
///
/// Supplied by the caller (indexer, API, or a view call on the pool) so that
/// simulation stays a pure function of its inputs.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PoolSnapshot {
    /// Total collateral value across the pool, in the pool's quote asset.
    pub total_collateral: i128,
    /// Total outstanding debt, in the same units.
    pub total_debt: i128,
    /// Total deposits available, in the same units.
    pub total_deposits: i128,
    /// Debt currently held by positions whose health factor sits within
    /// `at_risk_band_bps` of the liquidation threshold.
    pub at_risk_debt: i128,
    /// Width of the at-risk band, in basis points of health factor.
    pub at_risk_band_bps: i128,
}

/// How severe a simulated change looks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum ImpactSeverity {
    /// No material effect on borrowers or pool risk.
    Negligible = 0,
    /// A routine adjustment.
    Low = 1,
    /// Worth reviewing: meaningful movement in borrowing power or rates.
    Moderate = 2,
    /// Risk-bearing: positions may become liquidatable, or rates jump sharply.
    High = 3,
}

/// The projected effect of one parameter change.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ParameterImpact {
    pub parameter: ParameterType,
    pub current_value: i128,
    pub proposed_value: i128,
    /// Signed change, in basis points of the current value. Zero when the
    /// current value is zero (no baseline to measure against).
    pub relative_change_bps: i128,
    /// Change in total borrowing power, in the pool's quote asset. Positive
    /// means borrowers can take on more debt.
    pub borrowing_power_delta: i128,
    /// Debt that would newly fall below the liquidation threshold.
    pub newly_liquidatable_debt: i128,
    /// Change in the borrow rate, in basis points. Positive means dearer.
    pub borrow_rate_delta_bps: i128,
    pub severity: ImpactSeverity,
    /// Machine-readable warnings, e.g. `ltv_above_threshold`.
    pub warnings: Vec<Symbol>,
}

/// Relative change from `current` to `proposed`, in basis points.
///
/// Returns 0 when there is no baseline, which keeps an initial value from
/// looking like an infinite increase.
fn relative_change_bps(current: i128, proposed: i128) -> i128 {
    if current == 0 {
        return 0;
    }
    ratio_bps(proposed.saturating_sub(current), current.abs()).unwrap_or(0)
}

/// Projects the effect of setting `parameter` to `proposed_value`.
///
/// `current_value` is the parameter's value today; `related` carries the
/// parameter values a cross-check needs (currently the liquidation threshold,
/// used to flag an LTV that would exceed it).
pub fn simulate(
    env: &Env,
    parameter: &ParameterType,
    current_value: i128,
    proposed_value: i128,
    snapshot: &PoolSnapshot,
    related: &RelatedParameters,
) -> ParameterImpact {
    let mut warnings = Vec::new(env);
    let mut borrowing_power_delta = 0i128;
    let mut newly_liquidatable_debt = 0i128;
    let mut borrow_rate_delta_bps = 0i128;

    match parameter {
        ParameterType::LTV => {
            // Borrowing power moves with LTV against the whole collateral base.
            let before = apply_bps(snapshot.total_collateral, current_value).unwrap_or(0);
            let after = apply_bps(snapshot.total_collateral, proposed_value).unwrap_or(0);
            borrowing_power_delta = after.saturating_sub(before);

            if related.liquidation_threshold > 0 && proposed_value >= related.liquidation_threshold
            {
                // An LTV at or above the liquidation threshold lets a borrower
                // open a position that is immediately liquidatable.
                warnings.push_back(Symbol::new(env, "ltv_above_threshold"));
            }
        }
        ParameterType::LiquidationThreshold => {
            // Tightening the threshold pulls the at-risk band under water.
            if proposed_value > current_value {
                let tightening = proposed_value.saturating_sub(current_value);
                newly_liquidatable_debt = if snapshot.at_risk_band_bps > 0 {
                    let share = tightening.min(snapshot.at_risk_band_bps);
                    apply_bps(
                        snapshot.at_risk_debt,
                        ratio_bps(share, snapshot.at_risk_band_bps).unwrap_or(0),
                    )
                    .unwrap_or(0)
                } else {
                    snapshot.at_risk_debt
                };
                if newly_liquidatable_debt > 0 {
                    warnings.push_back(Symbol::new(env, "positions_liquidatable"));
                }
            }
            if related.ltv > 0 && proposed_value <= related.ltv {
                warnings.push_back(Symbol::new(env, "threshold_below_ltv"));
            }
        }
        ParameterType::BaseInterestRate => {
            borrow_rate_delta_bps = proposed_value.saturating_sub(current_value);
        }
        ParameterType::Slope1 | ParameterType::Slope2 | ParameterType::OptimalUtilization => {
            borrow_rate_delta_bps = project_rate_delta(parameter, current_value, proposed_value, snapshot, related);
        }
        ParameterType::DebtCeiling => {
            // Headroom against outstanding debt, not against the old ceiling.
            borrowing_power_delta = proposed_value.saturating_sub(current_value);
            if proposed_value < snapshot.total_debt {
                warnings.push_back(Symbol::new(env, "ceiling_below_debt"));
            }
        }
        ParameterType::CloseFactor | ParameterType::LiquidationIncentive => {
            // These change how a liquidation is sized, not whether one happens.
        }
        ParameterType::ReserveFactor => {
            // Supply-side only: borrowers are unaffected.
        }
    }

    let relative = relative_change_bps(current_value, proposed_value);
    let severity = classify(
        parameter,
        relative,
        newly_liquidatable_debt,
        borrow_rate_delta_bps,
        snapshot,
    );

    ParameterImpact {
        parameter: parameter.clone(),
        current_value,
        proposed_value,
        relative_change_bps: relative,
        borrowing_power_delta,
        newly_liquidatable_debt,
        borrow_rate_delta_bps,
        severity,
        warnings,
    }
}

/// Re-evaluates the borrow rate at current utilization with the proposed curve
/// parameter, and reports the difference.
fn project_rate_delta(
    parameter: &ParameterType,
    current_value: i128,
    proposed_value: i128,
    snapshot: &PoolSnapshot,
    related: &RelatedParameters,
) -> i128 {
    let utilization = if snapshot.total_deposits > 0 {
        ratio_bps(snapshot.total_debt, snapshot.total_deposits)
            .unwrap_or(0)
            .min(BPS_DIVISOR)
    } else {
        0
    };

    let build = |value: i128| -> RateCurve {
        let mut curve = RateCurve {
            kind: RateModelKind::Kink,
            base_rate_bps: related.base_interest_rate,
            kink_utilization_bps: related.optimal_utilization,
            multiplier_bps: related.slope1,
            jump_multiplier_bps: related.slope2,
        };
        match parameter {
            ParameterType::Slope1 => curve.multiplier_bps = value,
            ParameterType::Slope2 => curve.jump_multiplier_bps = value,
            ParameterType::OptimalUtilization => curve.kink_utilization_bps = value,
            _ => {}
        }
        curve
    };

    let before = kink_rate(utilization, &build(current_value)).unwrap_or(0);
    let after = kink_rate(utilization, &build(proposed_value)).unwrap_or(0);
    after.saturating_sub(before)
}

/// The other parameter values a simulation needs for its cross-checks.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct RelatedParameters {
    pub ltv: i128,
    pub liquidation_threshold: i128,
    pub base_interest_rate: i128,
    pub slope1: i128,
    pub slope2: i128,
    pub optimal_utilization: i128,
}

/// Grades an impact from its projected effects.
///
/// Any debt becoming liquidatable is `High` regardless of how small the
/// parameter move looks: a 1% threshold change that liquidates positions
/// matters more than a 50% change to a parameter nobody is near.
fn classify(
    parameter: &ParameterType,
    relative_change_bps: i128,
    newly_liquidatable_debt: i128,
    borrow_rate_delta_bps: i128,
    snapshot: &PoolSnapshot,
) -> ImpactSeverity {
    if newly_liquidatable_debt > 0 {
        return ImpactSeverity::High;
    }
    if borrow_rate_delta_bps.abs() >= 500 {
        return ImpactSeverity::High;
    }
    if relative_change_bps == 0 && borrow_rate_delta_bps == 0 {
        return ImpactSeverity::Negligible;
    }

    let magnitude = relative_change_bps.abs();
    let risk_parameter = parameter.is_risk_parameter();
    // Risk parameters are graded one step harder than the rest: the same 10%
    // move means more on a liquidation threshold than on a reserve factor.
    let threshold_moderate = if risk_parameter { 500 } else { 1_500 };
    let threshold_high = if risk_parameter { 2_000 } else { 5_000 };

    if magnitude >= threshold_high && snapshot.total_debt > 0 {
        ImpactSeverity::High
    } else if magnitude >= threshold_moderate {
        ImpactSeverity::Moderate
    } else if magnitude > 0 || borrow_rate_delta_bps != 0 {
        ImpactSeverity::Low
    } else {
        ImpactSeverity::Negligible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> PoolSnapshot {
        PoolSnapshot {
            total_collateral: 10_000_000,
            total_debt: 4_000_000,
            total_deposits: 8_000_000,
            at_risk_debt: 500_000,
            at_risk_band_bps: 500,
        }
    }

    fn related() -> RelatedParameters {
        RelatedParameters {
            ltv: 7_500,
            liquidation_threshold: 8_500,
            base_interest_rate: 100,
            slope1: 2_000,
            slope2: 10_000,
            optimal_utilization: 8_000,
        }
    }

    #[test]
    fn ltv_increase_raises_borrowing_power() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::LTV,
            7_500,
            8_000,
            &snapshot(),
            &related(),
        );
        // 5% of 10,000,000 collateral.
        assert_eq!(impact.borrowing_power_delta, 500_000);
        assert!(impact.relative_change_bps > 0);
        assert_eq!(impact.newly_liquidatable_debt, 0);
    }

    #[test]
    fn ltv_decrease_lowers_borrowing_power() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::LTV,
            7_500,
            7_000,
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.borrowing_power_delta, -500_000);
        assert!(impact.relative_change_bps < 0);
    }

    #[test]
    fn ltv_above_liquidation_threshold_is_flagged() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::LTV,
            7_500,
            8_900,
            &snapshot(),
            &related(),
        );
        assert!(impact
            .warnings
            .contains(&Symbol::new(&env, "ltv_above_threshold")));
    }

    #[test]
    fn tightening_the_threshold_liquidates_the_at_risk_band() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::LiquidationThreshold,
            8_500,
            8_750, // half of the 500 bps at-risk band
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.newly_liquidatable_debt, 250_000);
        assert_eq!(impact.severity, ImpactSeverity::High);
        assert!(impact
            .warnings
            .contains(&Symbol::new(&env, "positions_liquidatable")));
    }

    #[test]
    fn tightening_beyond_the_band_caps_at_the_at_risk_debt() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::LiquidationThreshold,
            8_500,
            9_500,
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.newly_liquidatable_debt, 500_000);
    }

    #[test]
    fn loosening_the_threshold_liquidates_nobody() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::LiquidationThreshold,
            8_500,
            8_000,
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.newly_liquidatable_debt, 0);
    }

    #[test]
    fn threshold_below_ltv_is_flagged() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::LiquidationThreshold,
            8_500,
            7_000,
            &snapshot(),
            &related(),
        );
        assert!(impact
            .warnings
            .contains(&Symbol::new(&env, "threshold_below_ltv")));
    }

    #[test]
    fn base_rate_change_moves_the_borrow_rate_one_for_one() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::BaseInterestRate,
            100,
            400,
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.borrow_rate_delta_bps, 300);
    }

    #[test]
    fn slope_change_is_projected_at_current_utilization() {
        let env = Env::default();
        // Utilization is 4,000,000 / 8,000,000 = 50%, below the 80% kink,
        // so slope1 drives the rate: 2,000 -> 4,000 doubles the slope term.
        let impact = simulate(
            &env,
            &ParameterType::Slope1,
            2_000,
            4_000,
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.borrow_rate_delta_bps, 1_250);
        assert_eq!(impact.severity, ImpactSeverity::High);
    }

    #[test]
    fn slope2_change_below_the_kink_does_nothing() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::Slope2,
            10_000,
            20_000,
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.borrow_rate_delta_bps, 0);
    }

    #[test]
    fn debt_ceiling_below_outstanding_debt_is_flagged() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::DebtCeiling,
            10_000_000,
            1_000_000,
            &snapshot(),
            &related(),
        );
        assert!(impact
            .warnings
            .contains(&Symbol::new(&env, "ceiling_below_debt")));
        assert_eq!(impact.borrowing_power_delta, -9_000_000);
    }

    #[test]
    fn an_unchanged_value_is_negligible() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::ReserveFactor,
            1_000,
            1_000,
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.relative_change_bps, 0);
        assert_eq!(impact.severity, ImpactSeverity::Negligible);
        assert_eq!(impact.warnings.len(), 0);
    }

    #[test]
    fn risk_parameters_are_graded_harder_than_the_rest() {
        let env = Env::default();
        // A 10% move on a close factor (risk) vs. a reserve factor (not risk).
        let risky = simulate(
            &env,
            &ParameterType::CloseFactor,
            5_000,
            5_500,
            &snapshot(),
            &related(),
        );
        let ordinary = simulate(
            &env,
            &ParameterType::ReserveFactor,
            1_000,
            1_100,
            &snapshot(),
            &related(),
        );
        assert_eq!(risky.relative_change_bps, ordinary.relative_change_bps);
        assert_eq!(risky.severity, ImpactSeverity::Moderate);
        assert_eq!(ordinary.severity, ImpactSeverity::Low);
    }

    #[test]
    fn setting_a_parameter_from_zero_has_no_baseline() {
        let env = Env::default();
        let impact = simulate(
            &env,
            &ParameterType::ReserveFactor,
            0,
            1_000,
            &snapshot(),
            &related(),
        );
        assert_eq!(impact.relative_change_bps, 0);
    }

    #[test]
    fn empty_pool_does_not_divide_by_zero() {
        let env = Env::default();
        let empty = PoolSnapshot {
            total_collateral: 0,
            total_debt: 0,
            total_deposits: 0,
            at_risk_debt: 0,
            at_risk_band_bps: 0,
        };
        let impact = simulate(
            &env,
            &ParameterType::Slope1,
            2_000,
            4_000,
            &empty,
            &related(),
        );
        assert_eq!(impact.borrow_rate_delta_bps, 0);
        assert_eq!(impact.newly_liquidatable_debt, 0);
    }

    #[test]
    fn threshold_tightening_without_a_band_assumes_the_whole_at_risk_debt() {
        let env = Env::default();
        let no_band = PoolSnapshot {
            at_risk_band_bps: 0,
            ..snapshot()
        };
        let impact = simulate(
            &env,
            &ParameterType::LiquidationThreshold,
            8_500,
            8_600,
            &no_band,
            &related(),
        );
        assert_eq!(impact.newly_liquidatable_debt, no_band.at_risk_debt);
    }
}
