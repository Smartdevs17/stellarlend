//! Reference models: plain-integer versions of protocol math that tests
//! compare contract results against. Moved here from the former
//! `packages/test-framework` crate.

pub fn assert_equal_with_tolerance(actual: i128, expected: i128, tolerance_bps: i128) {
    let max_diff = (expected.abs() * tolerance_bps) / 10_000;
    let actual_diff = (actual - expected).abs();
    assert!(
        actual_diff <= max_diff,
        "Value {} not within {} bps of {}",
        actual,
        tolerance_bps,
        expected
    );
}

pub fn health_factor_bps(collateral_value: i128, debt_value: i128, threshold_bps: i128) -> i128 {
    if debt_value <= 0 {
        return i128::MAX;
    }
    if collateral_value <= 0 {
        return 0;
    }
    (collateral_value * threshold_bps) / debt_value
}

pub fn is_liquidatable(health_factor: i128) -> bool {
    health_factor < 10_000
}

pub fn calculate_interest_accrual(principal: i128, rate_bps: i128, seconds: u64) -> i128 {
    let seconds_per_year = 365 * 24 * 3600;
    (principal * rate_bps * seconds as i128) / (10_000 * seconds_per_year as i128)
}

pub fn assert_close(actual: i128, expected: i128, tolerance: i128) {
    let diff = (actual - expected).abs();
    assert!(
        diff <= tolerance,
        "Expected {}, got {}, difference {}",
        expected,
        actual,
        diff
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_factor_edges() {
        assert_eq!(health_factor_bps(1_000, 0, 8_000), i128::MAX);
        assert_eq!(health_factor_bps(0, 1_000, 8_000), 0);
        assert_eq!(health_factor_bps(1_000, 800, 8_000), 10_000);
        assert!(!is_liquidatable(10_000));
        assert!(is_liquidatable(9_999));
    }

    #[test]
    fn one_year_accrues_the_annual_rate() {
        assert_eq!(calculate_interest_accrual(10_000, 500, 31_536_000), 500);
        assert_eq!(calculate_interest_accrual(10_000, 500, 0), 0);
    }

    #[test]
    fn tolerance_helpers() {
        assert_equal_with_tolerance(10_050, 10_000, 50);
        assert_close(99, 100, 1);
    }

    #[test]
    #[should_panic(expected = "not within")]
    fn tolerance_is_enforced() {
        assert_equal_with_tolerance(10_051, 10_000, 50);
    }
}
