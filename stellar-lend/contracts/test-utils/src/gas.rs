//! Gas / instruction measurement helpers for cross-contract tests (Issue #688).
//!
//! Thin wrappers around `soroban_sdk` testutils budget so multi-contract
//! scenarios can assert relative gas ordering and absolute ceilings.
//!
//! Soroban resets metering before every top-level contract invocation, so a
//! measurement covers the last invocation made inside the measured closure.
//! Measure one contract call per closure; [`crate::bench::GasMeter`] builds
//! per-step reports on the same rule.

#![allow(unused)]

use soroban_sdk::Env;

/// Snapshot of CPU/memory costs at a point in a test.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GasSnapshot {
    pub cpu_instructions: u64,
    pub memory_bytes: u64,
}

/// Reset the budget to unlimited (fresh measurement window).
pub fn reset_budget(env: &Env) {
    env.cost_estimate().budget().reset_unlimited();
}

/// Capture current budget usage as a snapshot.
pub fn snapshot(env: &Env) -> GasSnapshot {
    let budget = env.cost_estimate().budget();
    GasSnapshot {
        cpu_instructions: budget.cpu_instruction_cost(),
        memory_bytes: budget.memory_bytes_cost(),
    }
}

/// Measure the CPU/memory cost of `f`. The budget is reset first, so the
/// result does not depend on `_before`, which is kept for existing callers.
pub fn measure_delta<F>(env: &Env, _before: &GasSnapshot, f: F) -> GasSnapshot
where
    F: FnOnce(),
{
    timed(env, f).1
}

/// Assert a measurement is under an absolute CPU-instruction ceiling.
pub fn assert_under_cpu_ceiling(label: &str, snap: &GasSnapshot, ceiling: u64) {
    assert!(
        snap.cpu_instructions <= ceiling,
        "{}: expected <= {} cpu instructions, got {}",
        label,
        ceiling,
        snap.cpu_instructions
    );
}

/// Assert measured A is not more expensive than measured B (ordering check).
pub fn assert_cheaper_or_equal(label: &str, a: &GasSnapshot, b: &GasSnapshot) {
    assert!(
        a.cpu_instructions <= b.cpu_instructions,
        "{}: expected {} <= {} cpu instructions",
        label,
        a.cpu_instructions,
        b.cpu_instructions
    );
}

/// Run `f` and return (result, cost of `f`).
pub fn timed<F, T>(env: &Env, f: F) -> (T, GasSnapshot)
where
    F: FnOnce() -> T,
{
    reset_budget(env);
    let result = f();
    (result, snapshot(env))
}

/// Convenience: run and assert a ceiling in one call.
pub fn assert_within_cpu<F>(env: &Env, label: &str, ceiling: u64, f: F)
where
    F: FnOnce(),
{
    let (_, cost) = timed(env, f);
    assert_under_cpu_ceiling(label, &cost, ceiling);
}
