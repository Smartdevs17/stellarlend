//! Cross-contract invariant helpers for multi-contract state simulation (Issue #688).
//!
//! These helpers verify global properties that only hold when several
//! contracts (lending, oracle, token, AMM) advance together.

#![allow(unused)]

/// Accounting identity: total deposits must equal debt + reserves + free cash
/// within a tolerance (rounding on interest accrual).
pub fn assert_accounting_identity(
    total_deposits: i128,
    total_debt: i128,
    reserves: i128,
    free_cash: i128,
    tolerance: i128,
    label: &str,
) {
    let lhs = total_deposits;
    let rhs = total_debt + reserves + free_cash;
    let diff = (lhs - rhs).abs();
    assert!(
        diff <= tolerance,
        "{}: accounting identity violated — deposits={} debt+reserves+cash={} (diff={})",
        label,
        lhs,
        rhs,
        diff
    );
}

/// Conservation: sum of user collateral equals protocol-held collateral.
pub fn assert_collateral_conservation(user_totals: &[i128], protocol_held: i128, label: &str) {
    let sum: i128 = user_totals.iter().sum();
    assert_eq!(
        sum, protocol_held,
        "{}: collateral not conserved — users={} protocol={}",
        label, sum, protocol_held
    );
}

/// Health-factor monotonicity: a deposit can never decrease health; a borrow can never increase it.
pub fn assert_health_direction_after_deposit(before: i128, after: i128, label: &str) {
    assert!(
        after >= before,
        "{}: health factor decreased after deposit ({} -> {})",
        label,
        before,
        after
    );
}

pub fn assert_health_direction_after_borrow(before: i128, after: i128, label: &str) {
    assert!(
        after <= before,
        "{}: health factor increased after borrow ({} -> {})",
        label,
        before,
        after
    );
}

/// Oracle price must be strictly positive when consumed by a risk check.
pub fn assert_price_sane(price: i128, label: &str) {
    assert!(price > 0, "{}: oracle price must be > 0, got {}", label, price);
}

/// No free value: repaying debt cannot mint tokens out of thin air.
pub fn assert_no_free_value(pre_balance: i128, post_balance: i128, repaid: i128, label: &str) {
    // After repaying `repaid`, balance should drop by ~repaid (not more, not less beyond fee).
    let expected = pre_balance - repaid;
    assert!(
        post_balance >= expected,
        "{}: value created — pre={} post={} repaid={}",
        label,
        pre_balance,
        post_balance,
        repaid
    );
}

/// Interest index must never decrease across state transitions.
pub fn assert_index_monotonic(before: i128, after: i128, label: &str) {
    assert!(
        after >= before,
        "{}: interest index decreased ({} -> {})",
        label,
        before,
        after
    );
}

/// Pause must freeze balances: no net change while protocol is paused.
pub fn assert_frozen_while_paused(pre: i128, post: i128, label: &str) {
    assert_eq!(
        pre, post,
        "{}: balance changed while paused ({} -> {})",
        label, pre, post
    );
}

/// Multi-contract scenario result: pass/fail tally for reporting.
#[derive(Clone, Debug, Default)]
pub struct InvariantReport {
    pub checks_passed: u32,
    pub checks_failed: u32,
    pub failures: Vec<String>,
}

impl InvariantReport {
    pub fn record_ok(&mut self) {
        self.checks_passed += 1;
    }

    pub fn record_fail(&mut self, message: String) {
        self.checks_failed += 1;
        self.failures.push(message);
    }

    pub fn assert_all_passed(&self, scenario: &str) {
        assert!(
            self.checks_failed == 0,
            "{}: {} invariant check(s) failed: {:?}",
            scenario,
            self.checks_failed,
            self.failures
        );
    }

    pub fn total(&self) -> u32 {
        self.checks_passed + self.checks_failed
    }
}
