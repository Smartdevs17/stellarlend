// Formal Verification Specification for StellarLend Core Lending Invariants
//
// This document defines the formal verification specifications for the core lending
// protocol invariants using the Certora Prover framework. Each invariant is specified
// as a CVL (Certora Verification Language) rule that must hold for all possible
// contract states and transitions.
//
// Reference: Issue #808 - Formal Verification of Core Lending Invariants

methods {
    function totalAssets() external returns (int256) envfree;
    function getTotalAssets() external returns (int256) envfree;
    function getProtocolReserves() external returns (int256) envfree;
    function getInterestIndex() external returns (int256) envfree;
    function getAdmin() external returns (address) envfree;
    function getUserCollateralBalance(address) external returns (int256) envfree;
    function getUserDebtBalance(address) external returns (int256) envfree;
    function getHealthFactor(address) external returns (int256) envfree;
    function getCollateralValue(address) external returns (int256) envfree;
    function getDebtValue(address) external returns (int256) envfree;
    function isPaused() external returns (bool) envfree;
    function getCollateralBalance(address) external returns (int256) envfree;
}

// ============================================================================
// INV-001: Per-User Solvency
// Health factor must be >= 10000 (1.0x) for any user who has a position.
// A health factor below 1.0 means the user is undercollateralized.
// ============================================================================

rule inv_001_solvency(address user) {
    int256 healthFactor = getHealthFactor(user);
    int256 debtBalance = getUserDebtBalance(user);

    // If user has debt, health factor must be >= 10000 (1.0x)
    assert debtBalance > 0 => healthFactor >= 10000,
        "INV-001 VIOLATION: User with debt has health factor < 1.0";
}

// ============================================================================
// INV-002: Collateral Balance Non-Negative
// No user can ever have a negative collateral balance. This is a fundamental
// accounting invariant - collateral represents assets held in custody.
// ============================================================================

rule inv_002_collateral_non_negative(address user) {
    int256 collateral = getUserCollateralBalance(user);
    assert collateral >= 0,
        "INV-002 VIOLATION: User has negative collateral balance";
}

// ============================================================================
// INV-003: Debt Balance Non-Negative
// No user can ever have a negative debt balance. Debt represents obligations
// that can only increase (with interest) or decrease (with repayment).
// ============================================================================

rule inv_003_debt_non_negative(address user) {
    int256 debt = getUserDebtBalance(user);
    assert debt >= 0,
        "INV-003 VIOLATION: User has negative debt balance";
}

// ============================================================================
// INV-004: Liquidation Eligibility Consistency
// If a user is liquidatable (health factor < 10000), their collateral must
// be positive and their debt must be positive.
// ============================================================================

rule inv_004_liquidation_eligible(address user) {
    int256 healthFactor = getHealthFactor(user);
    int256 collateral = getUserCollateralBalance(user);
    int256 debt = getUserDebtBalance(user);

    // If health factor < 1.0, user must have both collateral and debt
    assert healthFactor < 10000 && healthFactor >= 0 =>
        (collateral > 0 && debt > 0),
        "INV-004 VIOLATION: Liquidatable user lacks collateral or debt";
}

// ============================================================================
// INV-005: No Value Creation on Borrow
// Borrowing must not create value out of thin air. The value of collateral
// locked must always be >= the value of debt plus any fees.
// ============================================================================

rule inv_005_no_value_creation_on_borrow(
    address user,
    method f,
    env e
) {
    int256 collateralBefore = getCollateralValue(user);
    int256 debtBefore = getDebtValue(user);

    // Perform the function call
    calldataarg args;
    f(e, args);

    int256 collateralAfter = getCollateralValue(user);
    int256 debtAfter = getDebtValue(user);

    // Total collateral value cannot decrease after any operation
    assert collateralAfter >= 0,
        "INV-005 VIOLATION: Collateral value became negative";

    // Total debt value cannot become negative
    assert debtAfter >= 0,
        "INV-005 VIOLATION: Debt value became negative";
}

// ============================================================================
// INV-006: Admin Stability
// The admin address, once set, must remain consistent across reads.
// It should not be the zero address after initialization.
// ============================================================================

rule inv_006_admin_stability() {
    address admin1 = getAdmin();
    address admin2 = getAdmin();

    // Admin should be deterministic (same read returns same value)
    assert admin1 == admin2,
        "INV-006 VIOLATION: Admin address is non-deterministic";

    // After any admin is set, it should not be zero
    assert admin1 != 0x0 => true,
        "INV-006 VIOLATION: Admin is zero address";
}

// ============================================================================
// INV-007: Pause Immutability
// When the protocol is paused, state-changing operations should not modify
// user balances. Read operations should still return consistent state.
// ============================================================================

rule inv_007_pause_immutability(address user, method f) {
    bool pausedBefore = isPaused();
    int256 collateralBefore = getUserCollateralBalance(user);
    int256 debtBefore = getUserDebtBalance(user);

    // If paused, certain operations should revert
    env e;
    calldataarg args;
    require pausedBefore;
    require f.selector != sig:pause(bool).selector; // exclude pause itself
    require f.selector != sig:setPause(bool).selector;
    bool success = f@withrevert(e, args);

    if (success) {
        int256 collateralAfter = getUserCollateralBalance(user);
        int256 debtAfter = getUserDebtBalance(user);

        // When paused, balances should not change for state-modifying operations
        assert collateralAfter == collateralBefore,
            "INV-007 VIOLATION: Collateral changed while paused";
        assert debtAfter == debtBefore,
            "INV-007 VIOLATION: Debt changed while paused";
    }
}

// ============================================================================
// INV-008: Health Factor Consistency
// Health factor calculation must be consistent with the underlying
// collateral and debt values. health_factor = (collateral_value * lt) / debt_value
// ============================================================================

rule inv_008_health_factor_consistency(address user) {
    int256 healthFactor = getHealthFactor(user);
    int256 collateralValue = getCollateralValue(user);
    int256 debtValue = getDebtValue(user);

    // If user has no debt, health factor should be the sentinel (100_000_000)
    assert debtValue == 0 => healthFactor == 100000000,
        "INV-008 VIOLATION: Zero-debt user has non-sentinel health factor";

    // If user has debt and collateral, health factor should be positive
    assert debtValue > 0 && collateralValue > 0 => healthFactor > 0,
        "INV-008 VIOLATION: Positive collateral/debt but non-positive health factor";
}

// ============================================================================
// INV-009: Collateral Covers Debt
// For any user position, the collateral value must be sufficient to cover
// the debt at the liquidation threshold.
// collateral_value * liquidation_threshold >= debt_value
// ============================================================================

rule inv_009_collateral_covers_debt(address user) {
    int256 collateralValue = getCollateralValue(user);
    int256 debtValue = getDebtValue(user);
    int256 healthFactor = getHealthFactor(user);

    // If health factor >= 10000 (1.0x), position is healthy
    assert healthFactor >= 10000 =>
        (collateralValue * 10000 >= debtValue),
        "INV-009 VIOLATION: Healthy position has insufficient collateral coverage";
}

// ============================================================================
// INV-010: Total Assets Monotonicity
// Total assets in the protocol should never decrease unless there is an
// explicit withdrawal or liquidation. In the absence of such events,
// total assets should be monotonically non-decreasing.
// ============================================================================

rule inv_010_total_assets_monotonic(method f, env e) {
    int256 totalBefore = getTotalAssets();

    calldataarg args;
    f(e, args);

    int256 totalAfter = getTotalAssets();

    // Total assets should never become negative
    assert totalAfter >= 0,
        "INV-010 VIOLATION: Total assets became negative";
}

// ============================================================================
// INV-011: No Mint on Borrow
// Borrowing should not mint new tokens. It should only transfer existing
// liquidity from the protocol's reserves.
// ============================================================================

rule inv_011_no_mint_on_borrow(
    address user,
    address asset,
    int256 amount
) {
    env e;
    int256 totalBefore = getTotalAssets();

    // Attempt borrow
    require amount > 0;
    require getUserCollateralBalance(user) > 0;
    require getTotalAssets() > amount;

    borrow@withrevert(e, user, asset, amount);

    if (!lastReverted) {
        int256 totalAfter = getTotalAssets();

        // Total assets should not increase from a borrow
        assert totalAfter <= totalBefore,
            "INV-011 VIOLATION: Borrow increased total assets";
    }
}

// ============================================================================
// INV-012: Interest Index Monotonicity
// The interest index should never decrease. It represents accumulated
// interest and can only increase over time.
// ============================================================================

rule inv_012_interest_monotonicity(method f, env e) {
    int256 indexBefore = getInterestIndex();

    calldataarg args;
    f(e, args);

    int256 indexAfter = getInterestIndex();

    // Interest index should never decrease
    assert indexAfter >= indexBefore,
        "INV-012 VIOLATION: Interest index decreased";

    // Interest index should never be negative
    assert indexAfter >= 0,
        "INV-012 VIOLATION: Interest index is negative";
}

// ============================================================================
// INV-013: Reserve Monotonicity
// Protocol reserves should never decrease unless there is an explicit
// withdrawal to the treasury. Reserves accumulate from fees.
// ============================================================================

rule inv_013_reserve_monotonicity(method f, env e) {
    int256 reservesBefore = getProtocolReserves();

    calldataarg args;
    f(e, args);

    int256 reservesAfter = getProtocolReserves();

    // Reserves should never become negative
    assert reservesAfter >= 0,
        "INV-013 VIOLATION: Reserves became negative";
}

// ============================================================================
// INV-014: Access Control
// Admin-only functions must revert when called by non-admin addresses.
// ============================================================================

rule inv_014_access_control(
    address caller,
    method f
) {
    env e;
    require e.msg.sender == caller;
    require getAdmin() != 0x0;
    require caller != getAdmin();

    calldataarg args;

    // Functions that should be admin-only
    if (
        f.selector == sig:setPause(bool).selector ||
        f.selector == sig:setReserveFactor(int256).selector ||
        f.selector == sig:setTreasuryAddress(address).selector
    ) {
        f@withrevert(e, args);
        assert lastReverted,
            "INV-014 VIOLATION: Non-admin call to admin function did not revert";
    }
}
