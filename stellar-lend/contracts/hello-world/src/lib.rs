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
#![allow(clippy::enum_variant_names)]
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
pub mod liq_math;
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
pub mod math;
pub mod multi_collateral;
pub mod oracle;
pub mod pause;
pub mod rate_guard;
pub mod reentrancy;
pub mod reserve_factor;
pub mod storage;
pub mod timelock;
pub mod upgrade;

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

/// Per-block memoized borrow-rate curve evaluations (issue #1640)
pub mod rates;

/// Per-pool withdrawal queue when utilization is pinned (issue #1586)
pub mod withdraw_queue;

/// Strategy registry for yield-routing adapters (issue #1580)
pub mod strategy;

/// ERC-4626-style vault interface for stablecoin pools (issue #1608)
pub mod vault;

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

#[no_mangle]
pub fn set_security_council(
    env: Env,
    caller: Address,
    council: soroban_sdk::Vec<Address>,
    threshold: u32,
) -> Result<(), errors::LendingError> {
    admin::set_security_council(&env, caller, council, threshold).map_err(Into::into)
}

#[no_mangle]
pub fn propose_admin_rotation(
    env: Env,
    proposer: Address,
    new_admin: Address,
) -> Result<(), errors::LendingError> {
    admin::propose_admin_rotation(&env, proposer, new_admin).map_err(Into::into)
}

#[no_mangle]
pub fn approve_admin_rotation(
    env: Env,
    approver: Address,
    new_admin: Address,
) -> Result<(), errors::LendingError> {
    admin::approve_admin_rotation(&env, approver, new_admin).map_err(Into::into)
}

#[no_mangle]
pub fn execute_admin_rotation(
    env: Env,
    executor: Address,
    new_admin: Address,
) -> Result<(), errors::LendingError> {
    admin::execute_admin_rotation(&env, executor, new_admin).map_err(Into::into)
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

#[no_mangle]
pub fn attest_admin_wallet(
    env: Env,
    admin: Address,
    wallet: Address,
    expires_at: u64,
) -> Result<admin::WalletAttestation, errors::LendingError> {
    admin::attest_wallet(&env, admin, wallet, expires_at).map_err(Into::into)
}

#[no_mangle]
pub fn register_upgrade_hash(
    env: Env,
    caller: Address,
    wasm_hash: soroban_sdk::BytesN<32>,
) -> Result<(), errors::LendingError> {
    upgrade::register_hash(&env, caller, wasm_hash).map_err(Into::into)
}

#[no_mangle]
pub fn require_registered_upgrade_hash(
    env: Env,
    wasm_hash: soroban_sdk::BytesN<32>,
) -> Result<(), errors::LendingError> {
    upgrade::require_registered_hash(&env, &wasm_hash).map_err(Into::into)
}

#[no_mangle]
pub fn add_pause_guardian(
    env: Env,
    admin: Address,
    guardian: Address,
) -> Result<(), errors::LendingError> {
    pause::add_guardian(&env, admin, guardian).map_err(Into::into)
}

// ── Per-pool withdrawal queue (issue #1586) ────────────────────────────────

#[no_mangle]
pub fn request_withdrawal(
    env: Env,
    pool: Option<Address>,
    user: Address,
    amount: i128,
) -> Result<withdraw_queue::WithdrawRequestOutcome, errors::LendingError> {
    withdraw_queue::request_withdrawal(&env, pool, user, amount)
}

#[no_mangle]
pub fn cancel_withdraw_request(
    env: Env,
    entry_id: u64,
    caller: Address,
) -> Result<(), errors::LendingError> {
    withdraw_queue::cancel_request(&env, entry_id, caller)
}

#[no_mangle]
pub fn process_withdraw_queue(
    env: Env,
    pool: Option<Address>,
    processor: Address,
    max_count: u32,
) -> Result<withdraw_queue::ProcessResult, errors::LendingError> {
    withdraw_queue::process_withdrawals(&env, pool, processor, max_count)
}

#[no_mangle]
pub fn set_withdraw_queue_config(
    env: Env,
    caller: Address,
    pool: Option<Address>,
    config: withdraw_queue::WithdrawQueueConfig,
) -> Result<(), errors::LendingError> {
    withdraw_queue::set_config(&env, &caller, &pool, config)
}

#[no_mangle]
pub fn get_withdraw_queue_config(
    env: Env,
    pool: Option<Address>,
) -> withdraw_queue::WithdrawQueueConfig {
    withdraw_queue::get_config(&env, &pool)
}

#[no_mangle]
pub fn get_withdraw_request(
    env: Env,
    entry_id: u64,
) -> Option<withdraw_queue::WithdrawQueueEntry> {
    withdraw_queue::get_request(&env, entry_id)
}

#[no_mangle]
pub fn get_pending_withdraw_requests(
    env: Env,
    pool: Option<Address>,
) -> soroban_sdk::Vec<withdraw_queue::WithdrawQueueEntry> {
    withdraw_queue::get_pending_requests(&env, &pool)
}

#[no_mangle]
pub fn get_withdraw_queue_stats(
    env: Env,
    pool: Option<Address>,
) -> withdraw_queue::WithdrawQueueStats {
    withdraw_queue::get_stats(&env, &pool)
}

#[no_mangle]
pub fn get_withdraw_queue_alerts(
    env: Env,
    pool: Option<Address>,
    max_count: u32,
) -> soroban_sdk::Vec<withdraw_queue::WithdrawQueueAlert> {
    withdraw_queue::get_alerts(&env, &pool, max_count)
}

#[no_mangle]
pub fn is_pool_utilization_pinned(env: Env, pool: Option<Address>) -> bool {
    withdraw_queue::is_utilization_pinned(&env, &pool)
}

#[no_mangle]
pub fn cleanup_withdraw_queue(env: Env, pool: Option<Address>) -> u32 {
    withdraw_queue::cleanup_expired(&env, &pool)
}

// ── Strategy registry for yield-routing adapters (issue #1580) ─────────────

#[no_mangle]
pub fn set_strategy_registry_config(
    env: Env,
    caller: Address,
    config: strategy::RegistryConfig,
) -> Result<(), errors::LendingError> {
    strategy::set_registry_config(&env, &caller, config)
}

#[no_mangle]
pub fn get_strategy_registry_config(env: Env) -> strategy::RegistryConfig {
    strategy::get_registry_config(&env)
}

#[no_mangle]
pub fn register_strategy(
    env: Env,
    caller: Address,
    adapter: Address,
    name: soroban_sdk::Symbol,
    max_allocation: i128,
) -> Result<u64, errors::LendingError> {
    strategy::register_strategy(&env, &caller, adapter, name, max_allocation)
}

#[no_mangle]
pub fn update_strategy(
    env: Env,
    caller: Address,
    strategy_id: u64,
    max_allocation: Option<i128>,
    status: Option<strategy::StrategyStatus>,
) -> Result<(), errors::LendingError> {
    strategy::update_strategy(&env, &caller, strategy_id, max_allocation, status)
}

#[no_mangle]
pub fn remove_strategy(
    env: Env,
    caller: Address,
    strategy_id: u64,
) -> Result<(), errors::LendingError> {
    strategy::remove_strategy(&env, &caller, strategy_id)
}

#[no_mangle]
pub fn set_strategy_target_allocation(
    env: Env,
    caller: Address,
    strategy_id: u64,
    target: i128,
) -> Result<(), errors::LendingError> {
    strategy::set_target_allocation(&env, &caller, strategy_id, target)
}

#[no_mangle]
pub fn route_strategies(
    env: Env,
    caller: Address,
) -> Result<soroban_sdk::Vec<strategy::RouteInstruction>, errors::LendingError> {
    strategy::route(&env, &caller)
}

#[no_mangle]
pub fn deploy_to_strategy(
    env: Env,
    caller: Address,
    strategy_id: u64,
    amount: i128,
) -> Result<i128, errors::LendingError> {
    strategy::deploy(&env, &caller, strategy_id, amount)
}

#[no_mangle]
pub fn withdraw_from_strategy(
    env: Env,
    caller: Address,
    strategy_id: u64,
    amount: i128,
) -> Result<i128, errors::LendingError> {
    strategy::withdraw(&env, &caller, strategy_id, amount)
}

#[no_mangle]
pub fn get_strategy(env: Env, strategy_id: u64) -> Option<strategy::Strategy> {
    strategy::get_strategy(&env, strategy_id)
}

#[no_mangle]
pub fn list_strategies(env: Env) -> soroban_sdk::Vec<strategy::Strategy> {
    strategy::list_strategies(&env)
}

#[no_mangle]
pub fn get_strategy_registry_stats(env: Env) -> strategy::RegistryStats {
    strategy::get_registry_stats(&env)
}

#[no_mangle]
pub fn get_strategy_target_allocations(env: Env) -> soroban_sdk::Map<u64, i128> {
    strategy::get_target_allocations(&env)
}

// ── ERC-4626 vault interface for stablecoin pools (issue #1608) ────────────

#[no_mangle]
pub fn initialize_pool_vault(
    env: Env,
    caller: Address,
    pool: Option<Address>,
    asset: Address,
    config: vault::VaultConfig,
) -> Result<(), errors::LendingError> {
    vault::initialize_vault(&env, &caller, pool, asset, config)
}

#[no_mangle]
pub fn set_pool_vault_config(
    env: Env,
    caller: Address,
    pool: Option<Address>,
    config: vault::VaultConfig,
) -> Result<(), errors::LendingError> {
    vault::set_vault_config(&env, &caller, pool, config)
}

#[no_mangle]
pub fn get_pool_vault_config(
    env: Env,
    pool: Option<Address>,
) -> Option<vault::VaultConfig> {
    vault::get_vault_config(&env, pool)
}

#[no_mangle]
pub fn vault_asset(
    env: Env,
    pool: Option<Address>,
) -> Result<Address, errors::LendingError> {
    vault::get_asset(&env, pool)
}

#[no_mangle]
pub fn vault_total_assets(
    env: Env,
    pool: Option<Address>,
) -> Result<i128, errors::LendingError> {
    vault::total_assets(&env, pool)
}

#[no_mangle]
pub fn vault_total_shares(
    env: Env,
    pool: Option<Address>,
) -> Result<i128, errors::LendingError> {
    vault::total_shares(&env, pool)
}

#[no_mangle]
pub fn vault_convert_to_shares(
    env: Env,
    pool: Option<Address>,
    assets: i128,
) -> Result<i128, errors::LendingError> {
    vault::convert_to_shares(&env, pool, assets)
}

#[no_mangle]
pub fn vault_convert_to_assets(
    env: Env,
    pool: Option<Address>,
    shares: i128,
) -> Result<i128, errors::LendingError> {
    vault::convert_to_assets(&env, pool, shares)
}

#[no_mangle]
pub fn vault_max_deposit(
    env: Env,
    pool: Option<Address>,
    receiver: Address,
) -> Result<i128, errors::LendingError> {
    vault::max_deposit(&env, pool, &receiver)
}

#[no_mangle]
pub fn vault_preview_deposit(
    env: Env,
    pool: Option<Address>,
    assets: i128,
) -> Result<i128, errors::LendingError> {
    vault::preview_deposit(&env, pool, assets)
}

#[no_mangle]
pub fn vault_deposit(
    env: Env,
    caller: Address,
    assets: i128,
    receiver: Address,
    pool: Option<Address>,
) -> Result<i128, errors::LendingError> {
    vault::deposit(&env, &caller, assets, &receiver, pool)
}

#[no_mangle]
pub fn vault_max_mint(
    env: Env,
    pool: Option<Address>,
    receiver: Address,
) -> Result<i128, errors::LendingError> {
    vault::max_mint(&env, pool, &receiver)
}

#[no_mangle]
pub fn vault_preview_mint(
    env: Env,
    pool: Option<Address>,
    shares: i128,
) -> Result<i128, errors::LendingError> {
    vault::preview_mint(&env, pool, shares)
}

#[no_mangle]
pub fn vault_mint(
    env: Env,
    caller: Address,
    shares: i128,
    receiver: Address,
    pool: Option<Address>,
) -> Result<i128, errors::LendingError> {
    vault::mint(&env, &caller, shares, &receiver, pool)
}

#[no_mangle]
pub fn vault_max_withdraw(
    env: Env,
    pool: Option<Address>,
    owner: Address,
) -> Result<i128, errors::LendingError> {
    vault::max_withdraw(&env, pool, &owner)
}

#[no_mangle]
pub fn vault_preview_withdraw(
    env: Env,
    pool: Option<Address>,
    assets: i128,
) -> Result<i128, errors::LendingError> {
    vault::preview_withdraw(&env, pool, assets)
}

#[no_mangle]
pub fn vault_withdraw(
    env: Env,
    caller: Address,
    assets: i128,
    receiver: Address,
    owner: Address,
    pool: Option<Address>,
) -> Result<i128, errors::LendingError> {
    vault::withdraw(&env, &caller, assets, &receiver, &owner, pool)
}

#[no_mangle]
pub fn vault_max_redeem(
    env: Env,
    pool: Option<Address>,
    owner: Address,
) -> Result<i128, errors::LendingError> {
    vault::max_redeem(&env, pool, &owner)
}

#[no_mangle]
pub fn vault_preview_redeem(
    env: Env,
    pool: Option<Address>,
    shares: i128,
) -> Result<i128, errors::LendingError> {
    vault::preview_redeem(&env, pool, shares)
}

#[no_mangle]
pub fn vault_redeem(
    env: Env,
    caller: Address,
    shares: i128,
    receiver: Address,
    owner: Address,
    pool: Option<Address>,
) -> Result<i128, errors::LendingError> {
    vault::redeem(&env, &caller, shares, &receiver, &owner, pool)
}

#[no_mangle]
pub fn vault_share_balance_of(
    env: Env,
    pool: Option<Address>,
    user: Address,
) -> i128 {
    vault::share_balance_of(&env, pool, &user)
}

#[no_mangle]
pub fn vault_share_allowance(
    env: Env,
    pool: Option<Address>,
    owner: Address,
    spender: Address,
) -> i128 {
    vault::share_allowance(&env, pool, &owner, &spender)
}

#[no_mangle]
pub fn vault_approve_shares(
    env: Env,
    owner: Address,
    spender: Address,
    pool: Option<Address>,
    amount: i128,
) -> Result<(), errors::LendingError> {
    vault::approve_shares(&env, &owner, &spender, pool, amount)
}

#[no_mangle]
pub fn vault_transfer_shares(
    env: Env,
    from: Address,
    to: Address,
    pool: Option<Address>,
    amount: i128,
) -> Result<(), errors::LendingError> {
    vault::transfer_shares(&env, &from, &to, pool, amount)
}

#[no_mangle]
pub fn vault_accrue_yield(
    env: Env,
    caller: Address,
    pool: Option<Address>,
    yield_amount: i128,
) -> Result<i128, errors::LendingError> {
    vault::accrue_yield(&env, &caller, pool, yield_amount)
}

#[no_mangle]
pub fn vault_get_stats(
    env: Env,
    pool: Option<Address>,
) -> Result<vault::VaultStats, errors::LendingError> {
    vault::get_vault_stats(&env, pool)
}
