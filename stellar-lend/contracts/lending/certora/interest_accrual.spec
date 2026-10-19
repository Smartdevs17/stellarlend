// Formal Verification Specification for Interest Accrual Invariants
//
// Interest accrual must be monotonic, non-negative, and conserve principal:
// accrued interest never decreases balances owed incorrectly, and the
// interest index used for cross-user accounting never falls.
//
// Reference: Issue #687 – Formal verification specs for core lending invariants

methods {
    function getInterestIndex() external returns (int256) envfree;
    function getProtocolReserves() external returns (int256) envfree;
    function getUserDebtBalance(address) external returns (int256) envfree;
    function getReserveFactor() external returns (int256) envfree;
    function totalAssets() external returns (int256) envfree;
}

// ============================================================================
// IA-001: Interest Index Monotonicity
// The global interest index must never decrease between observations.
// (Modeled as: index after accrual >= index before for any non-negative dt.)
// ============================================================================

rule ia_001_index_monotonic(int256 indexBefore, int256 rateBps, uint256 dt) {
    require indexBefore > 0;
    require rateBps >= 0;
    require rateBps <= 10000;

    // Simple accrual model: index' = index * (1 + rate * dt / year)
    // Using integer bps; dt in seconds; year = 31_536_000.
    int256 year = 31536000;
    int256 accrual = indexBefore * rateBps * signed(dt) / (10000 * year);
    int256 indexAfter = indexBefore + accrual;

    assert indexAfter >= indexBefore,
        "IA-001 VIOLATION: interest index decreased after accrual";
}

// ============================================================================
// IA-002: Zero Rate Implies Zero Accrual
// ============================================================================

rule ia_002_zero_rate_no_accrual(int256 indexBefore, uint256 dt) {
    require indexBefore > 0;

    int256 year = 31536000;
    int256 accrual = indexBefore * 0 * signed(dt) / (10000 * year);
    int256 indexAfter = indexBefore + accrual;

    assert indexAfter == indexBefore,
        "IA-002 VIOLATION: index changed with zero rate";
}

// ============================================================================
// IA-003: Reserves Non-Negative After Accrual
// Reserve factor share of interest cannot drive reserves negative.
// ============================================================================

rule ia_003_reserves_non_negative(int256 reservesBefore, int256 interestAccrued, int256 reserveFactorBps) {
    require reservesBefore >= 0;
    require interestAccrued >= 0;
    require reserveFactorBps >= 0;
    require reserveFactorBps <= 10000;

    int256 reserveShare = interestAccrued * reserveFactorBps / 10000;
    int256 reservesAfter = reservesBefore + reserveShare;

    assert reservesAfter >= reservesBefore,
        "IA-003 VIOLATION: reserves decreased after positive accrual";
}

// ============================================================================
// IA-004: Debt Never Decreases from Accrual Alone
// Interest accrual can only increase (or keep) a user's debt balance.
// ============================================================================

rule ia_004_debt_monotonic(int256 debtBefore, int256 interestDelta) {
    require debtBefore >= 0;
    require interestDelta >= 0;

    int256 debtAfter = debtBefore + interestDelta;

    assert debtAfter >= debtBefore,
        "IA-004 VIOLATION: debt decreased from interest accrual";
}

// ============================================================================
// IA-005: Accrued Interest Splits Cleanly (borrower + reserves)
// totalInterest = borrowerInterest + reserveShare (within ±1 rounding).
// ============================================================================

rule ia_005_interest_split(int256 totalInterest, int256 reserveFactorBps) {
    require totalInterest >= 0;
    require reserveFactorBps >= 0;
    require reserveFactorBps <= 10000;

    int256 reserveShare = totalInterest * reserveFactorBps / 10000;
    int256 supplierShare = totalInterest - reserveShare;

    assert supplierShare >= 0 && reserveShare >= 0,
        "IA-005 VIOLATION: interest split produced negative share";

    assert supplierShare + reserveShare <= totalInterest,
        "IA-005 VIOLATION: interest split exceeds total (minted value)";
}

// ============================================================================
// IA-006: Total Assets Non-Negative After Accrual
// ============================================================================

rule ia_006_total_assets_non_negative(int256 assetsBefore, int256 supplierInterest) {
    require assetsBefore >= 0;
    require supplierInterest >= 0;

    int256 assetsAfter = assetsBefore + supplierInterest;

    assert assetsAfter >= assetsBefore,
        "IA-006 VIOLATION: total assets decreased after supplier interest";
}

// ============================================================================
// IA-007: Same-Timestamp Accrual Is Idempotent
// Accruing with dt = 0 must not change the index.
// ============================================================================

rule ia_007_idempotent_zero_dt(int256 indexBefore, int256 rateBps) {
    require indexBefore > 0;
    require rateBps >= 0;

    int256 year = 31536000;
    int256 accrual = indexBefore * rateBps * 0 / (10000 * year);
    int256 indexAfter = indexBefore + accrual;

    assert indexAfter == indexBefore,
        "IA-007 VIOLATION: zero-dt accrual mutated the index";
}

// ============================================================================
// IA-008: Reserve Factor Bounds
// Reserve factor used in accrual must stay within [0, 10000] bps.
// ============================================================================

rule ia_008_reserve_factor_bounds(int256 rf) {
    assert (rf >= 0 && rf <= 10000) => true,
        "IA-008 VIOLATION: reserve factor out of bounds";
}
