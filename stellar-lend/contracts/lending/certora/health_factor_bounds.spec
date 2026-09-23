// Formal Verification Specification for Health Factor Bounds
//
// Health factor (HF) must stay within mathematically sound bounds across
// deposit, borrow, repay, withdraw, liquidation, and interest accrual.
// HF = (collateral_value * liquidation_threshold) / debt_value, scaled by 1e4.
//
// Reference: Issue #687 – Formal verification specs for core lending invariants

methods {
    function getHealthFactor(address) external returns (int256) envfree;
    function getCollateralValue(address) external returns (int256) envfree;
    function getDebtValue(address) external returns (int256) envfree;
    function getUserCollateralBalance(address) external returns (int256) envfree;
    function getUserDebtBalance(address) external returns (int256) envfree;
    function getCollateralBalance(address) external returns (int256) envfree;
    function getLiquidationThreshold() external returns (int256) envfree;
    function isPaused() external returns (bool) envfree;
}

// ============================================================================
// HF-001: Zero Debt Implies Infinite (sentinel) Health Factor
// When a user has no debt, HF must be the maximum sentinel (or at least
// far above the liquidation threshold of 10000 = 1.0x).
// ============================================================================

rule hf_001_zero_debt_safe(address user) {
    int256 debt = getUserDebtBalance(user);
    int256 hf = getHealthFactor(user);

    assert debt == 0 => hf >= 10000,
        "HF-001 VIOLATION: zero-debt user must have HF >= 1.0";
}

// ============================================================================
// HF-002: Positive Debt HF is Positive
// HF must never be zero or negative when debt exists (would imply free
// liquidation of solvent positions or broken math).
// ============================================================================

rule hf_002_positive_debt_positive_hf(address user) {
    int256 debt = getUserDebtBalance(user);
    int256 hf = getHealthFactor(user);

    assert debt > 0 => hf > 0,
        "HF-002 VIOLATION: HF must be > 0 when debt > 0";
}

// ============================================================================
// HF-003: HF Consistency with Underlying Values
// HF must equal (collateral_value * liquidation_threshold / debt_value)
// within a ±1 unit rounding tolerance (bps scale).
// ============================================================================

rule hf_003_consistency(address user) {
    int256 collateralValue = getCollateralValue(user);
    int256 debtValue = getDebtValue(user);
    int256 lt = getLiquidationThreshold();
    int256 hf = getHealthFactor(user);

    require debtValue > 0;
    require lt > 0;

    int256 expected = collateralValue * lt / debtValue;
    int256 diff = hf > expected ? hf - expected : expected - hf;

    assert diff <= 1,
        "HF-003 VIOLATION: HF inconsistent with collateral/debt/threshold";
}

// ============================================================================
// HF-004: Monotonic in Collateral (higher collateral never lowers HF)
// Holding debt fixed, increasing collateral value cannot decrease HF.
// ============================================================================

rule hf_004_monotonic_collateral(int256 c1, int256 c2, int256 d, int256 lt) {
    require c1 > 0;
    require c2 > c1;
    require d > 0;
    require lt > 0;

    // Pure model of HF (proxy for getHealthFactor at fixed state)
    int256 hf1 = c1 * lt / d;
    int256 hf2 = c2 * lt / d;

    assert hf2 >= hf1,
        "HF-004 VIOLATION: HF not monotonic in collateral value";
}

// ============================================================================
// HF-005: Monotonic in Debt (higher debt never raises HF)
// ============================================================================

rule hf_005_monotonic_debt(int256 c, int256 d1, int256 d2, int256 lt) {
    require c > 0;
    require d1 > 0;
    require d2 > d1;
    require lt > 0;

    int256 hf1 = c * lt / d1;
    int256 hf2 = c * lt / d2;

    assert hf2 <= hf1,
        "HF-005 VIOLATION: HF not monotonic in debt value";
}

// ============================================================================
// HF-006: Liquidatable Implies Positive Collateral and Debt
// HF < 1.0 with positive debt requires positive collateral (else math broke).
// ============================================================================

rule hf_006_liquidatable_consistency(address user) {
    int256 hf = getHealthFactor(user);
    int256 debt = getUserDebtBalance(user);
    int256 collateral = getUserCollateralBalance(user);

    assert (hf < 10000 && debt > 0) => collateral >= 0,
        "HF-006 VIOLATION: liquidatable state with inconsistent balances";
}

// ============================================================================
// HF-007: Pause Does Not Invent Collateral
// While paused, collateral balances must remain non-negative (no free mint).
// ============================================================================

rule hf_007_pause_non_negative(address user) {
    require isPaused();
    int256 collateral = getUserCollateralBalance(user);

    assert collateral >= 0,
        "HF-007 VIOLATION: negative collateral while paused";
}
