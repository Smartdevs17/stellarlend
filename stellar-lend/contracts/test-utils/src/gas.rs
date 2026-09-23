//! Gas / instruction measurement helpers for cross-contract tests (Issue #688).
//!
//! Thin wrappers around `soroban_sdk` testutils budget so multi-contract
//! scenarios can assert relative gas ordering and absolute ceilings.

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

/// Measure CPU/memory delta of a closure relative to a prior snapshot.
pub fn measure_delta<F>(env: &Env, before: &GasSnapshot, f: F) -> GasSnapshot
where
    F: FnOnce(),
{
    f();
    let after = snapshot(env);
    GasSnapshot {
        cpu_instructions: after
            .cpu_instructions
            .saturating_sub(before.cpu_instructions),
        memory_bytes: after.memory_bytes.saturating_sub(before.memory_bytes),
    }
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

/// Run `f` and return (result, GasSnapshot delta).
pub fn timed<F, T>(env: &Env, f: F) -> (T, GasSnapshot)
where
    F: FnOnce() -> T,
{
    let before = snapshot(env);
    let result = f();
    let after = snapshot(env);
    let delta = GasSnapshot {
        cpu_instructions: after
            .cpu_instructions
            .saturating_sub(before.cpu_instructions),
        memory_bytes: after.memory_bytes.saturating_sub(before.memory_bytes),
    };
    (result, delta)
}

/// Convenience: run and assert a ceiling in one call.
pub fn assert_within_cpu<F>(env: &Env, label: &str, ceiling: u64, f: F)
where
    F: FnOnce(),
{
    let before = snapshot(env);
    f();
    let after = snapshot(env);
    let delta = GasSnapshot {
        cpu_instructions: after
            .cpu_instructions
            .saturating_sub(before.cpu_instructions),
        memory_bytes: after.memory_bytes.saturating_sub(before.memory_bytes),
    };
    assert_under_cpu_ceiling(label, &delta, ceiling);
}
