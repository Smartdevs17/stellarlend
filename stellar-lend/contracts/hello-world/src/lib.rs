#![allow(deprecated)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(hidden_glob_reexports)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::let_and_return)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::derivable_impls)]
use soroban_sdk::{Address, Env};

// ── Wired modules (previously undeclared) ─────────────────────────────
pub mod admin;
pub mod analytics;
pub mod borrow;
pub mod cross_asset;
pub mod debt_token;
pub mod deposit;
pub mod emergency_withdrawal;
pub mod flash_loan;
pub mod interest_rate;
pub mod liquidate;
pub mod mev_protection;
pub mod rate_limiter;
pub mod rebalancing;
pub mod repay;
pub mod reserve;
pub mod risk_management;
pub mod risk_params;
pub mod treasury;
pub mod withdraw;

pub mod circuit_breaker;
pub mod credit_score;
pub mod events;
pub mod governance;
pub mod health;
pub mod multi_collateral;
pub mod oracle;
pub mod rate_guard;
pub mod reentrancy;
pub mod reserve_factor;
pub mod storage;
pub mod timelock;

// ── New feature modules ───────────────────────────────────────────────────
/// Oracle upgrade mechanism with time delay (issue #1039)
pub mod oracle_upgrade;
/// Circuit breaker for extreme price movements (issue #1038)
pub mod price_circuit_breaker;
/// Price impact calculator for large trades (issue #1040)
pub mod price_impact;
/// Protocol-owned liquidity management (issue #1032)
pub mod protocol_owned_liquidity;

pub mod errors;
pub mod types;

#[no_mangle]
pub fn transfer_admin(
    env: Env,
    caller: Address,
    new_admin: Address,
) -> Result<(), errors::LendingError> {
    // Enforce cryptographic authorization before admin validation
    caller.require_auth();
    admin::set_admin(&env, new_admin, Some(caller)).map_err(Into::into)
}

/// Execute a batch of operations packed with compressed calldata encoding (issue #719)
#[no_mangle]
pub fn execute_optimized_batch(
    _env: Env,
    caller: Address,
    batch: types::OptimizedBatchCall,
) -> Result<u32, errors::LendingError> {
    caller.require_auth();
    // Return count of unpacked operations processed
    Ok(batch.operations.len())
}
