#![allow(unused)]
pub use shared_events::*;

use soroban_sdk::{contractevent, Env};

// Minimal event set required by `upgrade.rs`.
// These are emitted by publishing the struct instance (Soroban SDK pattern).

// ─── Cross-contract integration event helpers (Issue #688) ──────────────────

/// Payload for a multi-contract state transition (lending + oracle + token).
#[derive(Clone, Debug)]
#[contractevent]
pub struct CrossContractStepEvent {
    pub scenario: soroban_sdk::String,
    pub step_index: u32,
    pub step_name: soroban_sdk::String,
    pub success: bool,
    pub timestamp: u64,
}

/// Payload for an invariant check result during state simulation.
#[derive(Clone, Debug)]
#[contractevent]
pub struct InvariantCheckEvent {
    pub scenario: soroban_sdk::String,
    pub invariant_id: soroban_sdk::String,
    pub passed: bool,
    pub observed: i128,
    pub expected: i128,
}

/// Publish a cross-contract step marker (useful for indexing test runs).
pub fn emit_cross_contract_step(
    env: &Env,
    scenario: &str,
    step_index: u32,
    step_name: &str,
    success: bool,
) {
    let topics = (
        soroban_sdk::symbol_short!("xstep"),
        soroban_sdk::Symbol::new(env, if success { "ok" } else { "fail" }),
    );
    let data = (scenario, step_index, step_name);
    env.events().publish(topics, data);
}

/// Publish an invariant observation for multi-contract state simulation.
pub fn emit_invariant_check(
    env: &Env,
    invariant_id: &str,
    passed: bool,
    observed: i128,
    expected: i128,
) {
    let topics = (
        soroban_sdk::symbol_short!("invar"),
        soroban_sdk::Symbol::new(env, if passed { "pass" } else { "fail" }),
    );
    let data = (invariant_id, observed, expected);
    env.events().publish(topics, data);
}
