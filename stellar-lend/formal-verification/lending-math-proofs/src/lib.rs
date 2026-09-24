//! # Kani proof harnesses for the `stellarlend-math` lending operations
//!
//! ## Running
//!
//! ```sh
//! cargo kani --manifest-path formal-verification/lending-math-proofs/Cargo.toml
//! ```
//!
//! Kani uses CBMC (C Bounded Model Checker) with a Z3/CVC5 SMT backend to
//! exhaustively verify the properties below over all possible i128 inputs
//! within bounded bit-width.
//!
//! ## Properties verified
//!
//! 1. **Utilization bounds**: `calculate_utilization` returns a non-negative
//!    value; it is `0` when supply is zero and never exceeds 100% (10,000 bps)
//!    when `total_borrows <= total_supply`.
//! 2. **Liquidatability consistency**: `is_liquidatable` is exactly equivalent
//!    to `health_factor_bps(...) < 10_000`, and a zero-debt position is always
//!    reported as maximally healthy and never liquidatable.
//! 3. **Close-factor invariant**: `max_liquidatable` is non-negative and never
//!    exceeds the debt when the close factor is within `[0, 10_000]` bps.
//! 4. **Interest is monotonic**: `accrue_interest` and `compound_interest`
//!    never decrease a non-negative principal for non-negative rates, and
//!    return the principal unchanged for a zero rate/time/principal or zero
//!    compounding periods.
//! 5. **Seize-amount coverage**: `seize_amount` always covers at least the
//!    repaid debt for a non-negative bonus in `[0, 10_000]` bps.
//! 6. **Borrow-rate properties**: the kinked `InterestRateModel` returns a
//!    non-negative rate that is never below the configured base rate.
//!
//! See also `lending_math.smt2` for the corresponding SMT-LIB 2 encodings.

use stellarlend_math::lending::{
    accrue_interest, calculate_utilization, compound_interest, health_factor_bps, is_liquidatable,
    max_liquidatable, seize_amount, InterestRateModel,
};

const BPS_DIVISOR: i128 = 10_000;

// ---------------------------------------------------------------------------
// Utilization
// ---------------------------------------------------------------------------

/// Proof: `calculate_utilization` is non-negative, returns `0` for zero
/// supply, and never exceeds 100% when `borrows <= supply`.
#[cfg(kani)]
#[kani::proof]
fn kani_proof_calculate_utilization_bounds() {
    let total_borrows: i128 = kani::any();
    let total_supply: i128 = kani::any();
    kani::assume(total_borrows >= 0);
    kani::assume(total_supply >= 0);

    let utilization = calculate_utilization(total_borrows, total_supply);
    kani::assert(utilization >= 0, "utilization must be non-negative");

    if total_supply == 0 {
        kani::assert(utilization == 0, "utilization is 0 when supply is 0");
    } else if total_borrows <= total_supply {
        kani::assert(
            utilization <= BPS_DIVISOR,
            "utilization must not exceed 100% when borrows <= supply",
        );
    }
}

// ---------------------------------------------------------------------------
// Health factor / liquidatability
// ---------------------------------------------------------------------------

/// Proof: `is_liquidatable` is defined exactly as `health_factor < 10000`,
/// and a zero-debt position is never liquidatable.
#[cfg(kani)]
#[kani::proof]
fn kani_proof_health_factor_liquidatability_consistency() {
    let collateral_value: i128 = kani::any();
    let debt_value: i128 = kani::any();
    let liquidation_threshold_bps: i128 = kani::any();
    kani::assume(collateral_value >= 0);
    kani::assume(debt_value >= 0);
    kani::assume(liquidation_threshold_bps >= 0);

    let hf = health_factor_bps(collateral_value, debt_value, liquidation_threshold_bps);

    kani::assert(
        is_liquidatable(collateral_value, debt_value, liquidation_threshold_bps)
            == (hf < BPS_DIVISOR),
        "is_liquidatable must equal (health factor < 10000)",
    );

    if debt_value == 0 {
        kani::assert(hf == i128::MAX, "zero debt is maximally healthy");
        kani::assert(
            !is_liquidatable(collateral_value, debt_value, liquidation_threshold_bps),
            "zero debt is never liquidatable",
        );
    }
}

// ---------------------------------------------------------------------------
// Close factor
// ---------------------------------------------------------------------------

/// Proof: `max_liquidatable` is non-negative and never exceeds the debt when
/// the close factor is within `[0, 10000]` bps.
#[cfg(kani)]
#[kani::proof]
fn kani_proof_max_liquidatable_respects_close_factor() {
    let debt_value: i128 = kani::any();
    let close_factor_bps: i128 = kani::any();
    kani::assume(debt_value >= 0);
    kani::assume(close_factor_bps >= 0 && close_factor_bps <= BPS_DIVISOR);

    let max = max_liquidatable(debt_value, close_factor_bps);
    kani::assert(max >= 0, "max liquidatable must be non-negative");
    kani::assert(max <= debt_value, "max liquidatable cannot exceed the debt");
}

// ---------------------------------------------------------------------------
// Interest accrual
// ---------------------------------------------------------------------------

/// Proof: `accrue_interest` returns the principal unchanged for a zero
/// principal/rate/time, and never decreases a non-negative principal when the
/// rate and elapsed time are non-negative.
#[cfg(kani)]
#[kani::proof]
fn kani_proof_accrue_interest_monotonic() {
    let principal: i128 = kani::any();
    let rate_bps: i128 = kani::any();
    let time_elapsed: i128 = kani::any();
    kani::assume(principal >= 0);
    kani::assume(rate_bps >= 0);
    kani::assume(time_elapsed >= 0);

    if principal == 0 || rate_bps == 0 || time_elapsed == 0 {
        kani::assert(
            accrue_interest(principal, rate_bps, time_elapsed) == principal,
            "zero principal/rate/time returns the principal unchanged",
        );
    } else {
        kani::assert(
            accrue_interest(principal, rate_bps, time_elapsed) >= principal,
            "accrued interest is never negative",
        );
    }
}

/// Proof: `compound_interest` never decreases a non-negative principal for a
/// non-negative rate. Bounded compounding periods for tractability.
#[cfg(kani)]
#[kani::proof]
fn kani_proof_compound_interest_monotonic() {
    let principal: i128 = kani::any();
    let rate_bps: i128 = kani::any();
    let periods: u32 = kani::any();
    kani::assume(principal >= 0);
    kani::assume(rate_bps >= 0);
    kani::assume(periods <= 5);

    kani::assert(
        compound_interest(principal, rate_bps, periods) >= principal,
        "compound interest never decreases the principal",
    );
}

// ---------------------------------------------------------------------------
// Seize amount
// ---------------------------------------------------------------------------

/// Proof: `seize_amount` always covers at least the repaid debt when the
/// liquidation bonus is in `[0, 10000]` bps.
#[cfg(kani)]
#[kani::proof]
fn kani_proof_seize_amount_covers_repayment() {
    let repay_amount: i128 = kani::any();
    let bonus_bps: i128 = kani::any();
    kani::assume(repay_amount >= 0);
    kani::assume(bonus_bps >= 0 && bonus_bps <= BPS_DIVISOR);

    kani::assert(
        seize_amount(repay_amount, bonus_bps) >= repay_amount,
        "seize amount must cover at least the repaid debt",
    );
}

// ---------------------------------------------------------------------------
// Borrow rate model
// ---------------------------------------------------------------------------

/// Proof: the kinked borrow-rate model returns a non-negative rate that is
/// never below the configured base rate.
#[cfg(kani)]
#[kani::proof]
fn kani_proof_borrow_rate_non_negative_and_at_least_base() {
    let base_rate_bps: i128 = kani::any();
    let kink_utilization_bps: i128 = kani::any();
    let slope_bps: i128 = kani::any();
    let jump_slope_bps: i128 = kani::any();
    let utilization_bps: i128 = kani::any();
    kani::assume(base_rate_bps >= 0);
    kani::assume(kink_utilization_bps >= 0);
    kani::assume(slope_bps >= 0);
    kani::assume(jump_slope_bps >= 0);
    kani::assume(utilization_bps >= 0);

    let model = InterestRateModel {
        base_rate_bps,
        kink_utilization_bps,
        slope_bps,
        jump_slope_bps,
    };

    let rate = model.calculate_borrow_rate(utilization_bps);
    kani::assert(rate >= 0, "borrow rate must be non-negative");
    kani::assert(
        rate >= base_rate_bps,
        "borrow rate is never below the base rate",
    );
}

// ---------------------------------------------------------------------------
// Non-kani unit tests (always compiled)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utilization_is_bounded() {
        assert_eq!(calculate_utilization(0, 0), 0);
        assert_eq!(calculate_utilization(0, 100), 0);
        assert_eq!(calculate_utilization(50, 100), 5_000); // 50%
        assert_eq!(calculate_utilization(100, 100), 10_000); // 100%
        assert_eq!(calculate_utilization(200, 100), 20_000); // over-collateralized pools
    }

    #[test]
    fn health_factor_and_liquidatability() {
        assert_eq!(health_factor_bps(120, 100, 10_000), 12_000);
        assert!(!is_liquidatable(120, 100, 10_000));

        assert_eq!(health_factor_bps(90, 100, 10_000), 9_000);
        assert!(is_liquidatable(90, 100, 10_000));

        assert_eq!(health_factor_bps(100, 0, 10_000), i128::MAX);
        assert!(!is_liquidatable(100, 0, 10_000));
    }

    #[test]
    fn max_liquidatable_respects_close_factor() {
        assert_eq!(max_liquidatable(1_000, 0), 0);
        assert_eq!(max_liquidatable(1_000, 5_000), 500);
        assert_eq!(max_liquidatable(1_000, 10_000), 1_000);
        assert_eq!(max_liquidatable(0, 5_000), 0);
    }

    #[test]
    fn seize_amount_covers_repayment() {
        assert_eq!(seize_amount(0, 1_000), 0);
        assert_eq!(seize_amount(1_000, 0), 1_000);
        assert_eq!(seize_amount(1_000, 1_000), 1_100);
    }

    #[test]
    fn accrue_interest_is_monotonic() {
        assert_eq!(accrue_interest(1_000, 0, 100), 1_000);
        assert_eq!(accrue_interest(1_000, 500, 0), 1_000);
        assert_eq!(accrue_interest(0, 500, 100), 0);
        let grown = accrue_interest(1_000, 500, 31_536_000);
        assert_eq!(grown, 1_050);
    }

    #[test]
    fn compound_interest_never_decreases_principal() {
        assert_eq!(compound_interest(1_000, 500, 0), 1_000);
        assert_eq!(compound_interest(0, 500, 3), 0);
        // 1000 * 1.05^3 = 1157.625 -> integer compounding truncates to 1157.
        assert_eq!(compound_interest(1_000, 500, 3), 1_157);
    }

    #[test]
    fn borrow_rate_is_piecewise_linear() {
        let model = InterestRateModel {
            base_rate_bps: 200,
            kink_utilization_bps: 8_000,
            slope_bps: 1_000,
            jump_slope_bps: 5_000,
        };

        assert_eq!(model.calculate_borrow_rate(0), 200);
        assert_eq!(model.calculate_borrow_rate(4_000), 700); // 200 + (4000*1000)/8000
        assert_eq!(model.calculate_borrow_rate(8_000), 1_200); // rate at kink = base + slope
        assert_eq!(model.calculate_borrow_rate(9_000), 3_700); // + excess * jump / (10000-kink)
        assert!(model.calculate_borrow_rate(10_000) >= model.calculate_borrow_rate(0));
    }
}
