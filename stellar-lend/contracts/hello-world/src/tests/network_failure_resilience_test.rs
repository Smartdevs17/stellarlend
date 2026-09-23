//! Network failure resilience tests (Issue #689).
//!
//! Ties contract-level defenses to the chaos experiment suite in
//! `tests/chaos/` — verifies graceful degradation and recovery when the
//! protocol simulates oracle failure and network congestion:
//!
//! - Circuit breaker trips on oracle failure and auto-recovers after cooldown
//! - Rate limiter congestion reports scale limits during network stress,
//!   then fall back to normal after the report TTL
//! - Failure injection leaves user positions intact (no destructive side
//!   effects) so post-recovery operations resume cleanly

#![cfg(test)]

use crate::circuit_breaker::{
    self, CircuitBreakerConfig, CircuitBreakerReason, CircuitBreakerStatus,
};
use crate::rate_limiter::{CongestionConfig, RateLimitConfig};
use crate::{HelloContract, HelloContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

const BPS: i128 = 10_000;

fn setup(env: &Env) -> (Address, Address, HelloContractClient<'_>) {
    let contract_id = env.register(HelloContract, ());
    let client = HelloContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (contract_id, admin, client)
}

fn adaptive_congestion_config(enabled: bool) -> CongestionConfig {
    CongestionConfig {
        enabled,
        baseline_secs_per_ledger: 5,
        report_ttl_seconds: 300,
        min_factor_bps: 2_500,
        max_factor_bps: 10_000,
    }
}

// ─── Circuit breaker: oracle failure → trip → cooldown recovery ─────────────

#[test]
fn test_circuit_breaker_trips_on_oracle_failure_and_recovers() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, admin, _client) = setup(&env);
    let liquidator = Address::generate(&env);

    circuit_breaker::initialize_circuit_breaker(&env, CircuitBreakerConfig::default()).unwrap();

    // Baseline: liquidations allowed while breaker is Active.
    assert!(circuit_breaker::is_liquidation_allowed(&env, &liquidator).unwrap());

    // Simulated oracle outage trips the breaker (Tier1 pause).
    circuit_breaker::activate_circuit_breaker(
        &env,
        admin.clone(),
        CircuitBreakerReason::OracleFailure,
        false,
    )
    .unwrap();
    let state = circuit_breaker::get_circuit_breaker_state(&env).unwrap();
    assert_eq!(state.status, CircuitBreakerStatus::Tier1Paused);
    assert_eq!(state.reason, CircuitBreakerReason::OracleFailure);

    // Non-whitelisted liquidators are gated while the breaker is tripped.
    assert!(!circuit_breaker::is_liquidation_allowed(&env, &liquidator).unwrap());

    // Recovery: advance past the auto-deactivate cooldown window.
    let auto_deactivate_at = state.auto_deactivate_at.expect("cooldown scheduled");
    env.ledger().with_mut(|li| li.timestamp = auto_deactivate_at);
    assert!(circuit_breaker::is_liquidation_allowed(&env, &liquidator).unwrap());

    // State is Active again after auto-deactivation.
    let recovered = circuit_breaker::get_circuit_breaker_state(&env).unwrap();
    assert_eq!(recovered.status, CircuitBreakerStatus::Active);
}

#[test]
fn test_circuit_breaker_manual_deactivate_restores_operations() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, admin, _client) = setup(&env);
    let liquidator = Address::generate(&env);

    circuit_breaker::initialize_circuit_breaker(&env, CircuitBreakerConfig::default()).unwrap();
    circuit_breaker::activate_circuit_breaker(
        &env,
        admin.clone(),
        CircuitBreakerReason::OracleFailure,
        false,
    )
    .unwrap();
    assert!(!circuit_breaker::is_liquidation_allowed(&env, &liquidator).unwrap());

    // Operator heals the failure and manually deactivates.
    circuit_breaker::deactivate_circuit_breaker(&env, admin.clone()).unwrap();
    assert!(circuit_breaker::is_liquidation_allowed(&env, &liquidator).unwrap());
    let state = circuit_breaker::get_circuit_breaker_state(&env).unwrap();
    assert_eq!(state.status, CircuitBreakerStatus::Active);
}

#[test]
fn test_circuit_breaker_requires_admin_authorization() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, _admin, _client) = setup(&env);
    let impostor = Address::generate(&env);

    circuit_breaker::initialize_circuit_breaker(&env, CircuitBreakerConfig::default()).unwrap();
    let res = circuit_breaker::activate_circuit_breaker(
        &env,
        impostor.clone(),
        CircuitBreakerReason::OracleFailure,
        false,
    );
    assert!(res.is_err());
}

// ─── Rate limiter: congestion under network stress → TTL recovery ───────────

#[test]
fn test_congestion_report_scales_limits_then_expires() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, admin, client) = setup(&env);
    let user = Address::generate(&env);

    client.deposit_collateral(&user, &None, &1_000_000_000);
    client.rl_configure_operation(
        &admin,
        &Symbol::new(&env, "borrow"),
        &RateLimitConfig {
            window_seconds: 60,
            max_calls_per_window: 4,
            burst_calls: 0,
            grace_burst_calls: 0,
        },
    );
    client.rl_configure_congestion(&admin, &adaptive_congestion_config(true));

    // Network stress: congestion reported at 200% of normal → factor 50%.
    client.rl_report_congestion(&admin, &20_000i128);
    let state = client.rl_get_congestion_state();
    assert_eq!(state.congestion_bps, 20_000i128);
    assert_eq!(state.factor_bps, 5_000i128);

    // Under congestion the effective per-window limit is scaled down (4 → 2).
    env.ledger().with_mut(|li| li.timestamp = 1);
    client.borrow_asset(&user, &None, &1);
    client.borrow_asset(&user, &None, &1);
    assert!(client.try_borrow_asset(&user, &None, &1).is_err());

    // Recovery: report TTL elapses with no new sample → back to normal.
    env.ledger().with_mut(|li| li.timestamp = 301);
    let recovered = client.rl_get_congestion_state();
    assert_eq!(recovered.congestion_bps, BPS);
}

// ─── End-to-end: failure trip while positions exist, then full recovery ─────

#[test]
fn test_full_failure_and_recovery_cycle_preserves_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, admin, client) = setup(&env);
    let user = Address::generate(&env);
    let liquidator = Address::generate(&env);

    // Healthy protocol with an open position.
    client.deposit_collateral(&user, &None, &1_000_000_000);
    circuit_breaker::initialize_circuit_breaker(&env, CircuitBreakerConfig::default()).unwrap();
    assert!(circuit_breaker::is_liquidation_allowed(&env, &liquidator).unwrap());

    // Failure injected → trip breaker.
    circuit_breaker::activate_circuit_breaker(
        &env,
        admin.clone(),
        CircuitBreakerReason::OracleFailure,
        false,
    )
    .unwrap();
    assert!(!circuit_breaker::is_liquidation_allowed(&env, &liquidator).unwrap());

    // Position state must survive the failure (no destructive side effects).
    let position = client.get_user_position(&user);
    assert!(position.collateral > 0);
    assert_eq!(position.debt, 0);

    // Recovery: advance past cooldown; liquidations resume.
    let state = circuit_breaker::get_circuit_breaker_state(&env).unwrap();
    let auto_deactivate_at = state.auto_deactivate_at.expect("cooldown scheduled");
    env.ledger().with_mut(|li| li.timestamp = auto_deactivate_at);
    assert!(circuit_breaker::is_liquidation_allowed(&env, &liquidator).unwrap());

    // User operations still work post-recovery.
    client.deposit_collateral(&user, &None, &1_000);
}
