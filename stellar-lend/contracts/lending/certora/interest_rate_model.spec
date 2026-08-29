// Formal Verification Specification for Interest Rate Model Boundary Conditions
//
// This document defines the formal verification specifications for the
// InterestRateModel boundary conditions using the Certora Prover framework.
//
// Reference: Issue #865 - Formal Verification for Interest Rate Model Boundary Conditions

methods {
    function calculate_borrow_rate(i128) external returns (i128) envfree;
    function calculate_supply_rate(i128, i128, i128) external returns (i128) envfree;
    function calculate_utilization(i128, i128) external returns (i128) envfree;
}

// ============================================================================
// IRM-001: Zero Utilization Borrow Rate
// ============================================================================

rule irm_001_zero_utilization_equals_base_rate(i128 base_rate, i128 slope1, i128 slope2, i128 optimal_utilization) {
    require base_rate >= 0;
    require slope1 >= 0;
    require slope2 >= 0;
    require optimal_utilization >= 0;
    require optimal_utilization <= 10000;

    int256 rate = calculate_borrow_rate(0);

    assert rate == base_rate,
        "IRM-001 VIOLATION: rate(0%) must equal base_rate exactly";
}

// ============================================================================
// IRM-002: Full Utilization Borrow Rate is Maximum
// ============================================================================

rule irm_002_full_utilization_is_maximum(i128 base_rate, i128 slope1, i128 slope2, i128 optimal_utilization) {
    require base_rate >= 0;
    require slope1 >= 0;
    require slope2 >= 0;
    require optimal_utilization >= 0;
    require optimal_utilization <= 10000;

    int256 rate_at_zero = calculate_borrow_rate(0);
    int256 rate_at_full = calculate_borrow_rate(10000);

    assert rate_at_full >= rate_at_zero,
        "IRM-002 VIOLATION: rate(100%) must be >= rate(0%)";
}

// ============================================================================
// IRM-003: Monotonicity
// ============================================================================

rule irm_003_monotonicity(i128 base_rate, i128 slope1, i128 slope2, i128 optimal_utilization, i128 u1, i128 u2) {
    require base_rate >= 0;
    require slope1 >= 0;
    require slope2 >= 0;
    require optimal_utilization >= 0;
    require optimal_utilization <= 10000;
    require u1 >= 0;
    require u1 <= 10000;
    require u2 >= 0;
    require u2 <= 10000;
    require u1 <= u2;

    int256 rate1 = calculate_borrow_rate(u1);
    int256 rate2 = calculate_borrow_rate(u2);

    assert rate1 <= rate2,
        "IRM-003 VIOLATION: borrow rate must be monotonically non-decreasing in utilization";
}

// ============================================================================
// IRM-004: Kink Continuity
// ============================================================================

rule irm_004_kink_continuity(i128 base_rate, i128 slope1, i128 slope2, i128 optimal_utilization) {
    require base_rate >= 0;
    require slope1 >= 0;
    require slope2 >= 0;
    require optimal_utilization >= 0;
    require optimal_utilization <= 10000;

    int256 at_kink = calculate_borrow_rate(optimal_utilization);

    if (optimal_utilization < 10000) {
        int256 just_above = calculate_borrow_rate(optimal_utilization + 1);

        assert just_above >= at_kink,
            "IRM-004 VIOLATION: rate must not decrease immediately above the kink";

        assert just_above - at_kink <= (slope2 / 10000) + 1,
            "IRM-004 VIOLATION: discontinuity at kink exceeds one slope2 step";
    }
}

// ============================================================================
// IRM-005: No Overflow for Realistic Parameters
// ============================================================================

rule irm_005_no_overflow(i128 base_rate, i128 slope1, i128 slope2, i128 optimal_utilization, i128 utilization) {
    require base_rate >= 0;
    require base_rate <= 1000000000000;
    require slope1 >= 0;
    require slope1 <= 1000000000000;
    require slope2 >= 0;
    require slope2 <= 1000000000000;
    require optimal_utilization >= 0;
    require optimal_utilization <= 10000;
    require utilization >= 0;
    require utilization <= 10000;

    int256 result = calculate_borrow_rate(utilization);

    assert result >= 0,
        "IRM-005 VIOLATION: borrow rate must never be negative";
}

// ============================================================================
// IRM-006: Supply Rate Bounds
// ============================================================================

rule irm_006_supply_rate_bounds(i128 base_rate, i128 slope1, i128 slope2, i128 optimal_utilization, i128 utilization, i128 reserve_factor) {
    require base_rate >= 0;
    require slope1 >= 0;
    require slope2 >= 0;
    require optimal_utilization >= 0;
    require optimal_utilization <= 10000;
    require utilization >= 0;
    require utilization <= 10000;
    require reserve_factor >= 0;
    require reserve_factor <= 10000;

    int256 borrow_rate = calculate_borrow_rate(utilization);
    int256 supply_rate = calculate_supply_rate(borrow_rate, utilization, reserve_factor);

    assert supply_rate >= 0,
        "IRM-006 VIOLATION: supply rate must be non-negative";

    assert supply_rate <= borrow_rate,
        "IRM-006 VIOLATION: supply rate must not exceed borrow rate";
}

// ============================================================================
// IRM-007: Utilization Calculation Correctness
// ============================================================================

rule irm_007_utilization_calculation(i128 total_borrows, i128 total_supply) {
    require total_borrows >= 0;
    require total_supply >= 0;

    int256 utilization = calculate_utilization(total_borrows, total_supply);

    if (total_supply == 0) {
        assert utilization == 0,
            "IRM-007 VIOLATION: utilization must be 0 when total_supply is 0";
    } else {
        assert utilization >= 0,
            "IRM-007 VIOLATION: utilization must be non-negative";

        assert utilization <= 10000,
            "IRM-007 VIOLATION: utilization must not exceed 100%";
    }
}

// ============================================================================
// IRM-008: Rate Curve Area Sanity
// ============================================================================

rule irm_008_rate_curve_area_sanity(i128 base_rate, i128 slope1, i128 slope2, i128 optimal_utilization) {
    require base_rate >= 0;
    require slope1 >= 0;
    require slope2 >= 0;
    require optimal_utilization >= 0;
    require optimal_utilization <= 10000;

    int256 r0 = calculate_borrow_rate(0);
    int256 r_kink = calculate_borrow_rate(optimal_utilization);
    int256 r_max = calculate_borrow_rate(10000);

    int256 area_below = (r0 + r_kink) * optimal_utilization / 2;
    int256 area_above = (r_kink + r_max) * (10000 - optimal_utilization) / 2;
    int256 expected_area = area_below + area_above;

    assert expected_area >= 0,
        "IRM-008 VIOLATION: area under rate curve must be non-negative";
}
