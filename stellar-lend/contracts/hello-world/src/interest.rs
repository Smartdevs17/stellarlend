//! # Interest Capitalisation Compounding at Per-Block Cadence
//!
//! Implements periodic interest compounding where accrued interest is
//! capitalised (added to principal) at fixed block intervals rather than
//! continuously. This provides predictable, auditable compounding events
//! and reduces storage writes by batching index updates.
//!
//! ## Design
//! - Admin configures the compounding interval (in ledger blocks).
//! - `compound_interest` is called at each interval to capitalise accrued
//!   interest into the global lending index.
//! - If called before the interval elapses, it's a no-op (idempotent).
//! - Every compounding event emits an event with the index values.
//! - The module integrates with the existing `LendingIndex` from
//!   `interest_rate.rs` — no duplicate state.

use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Symbol};

use crate::admin;
use crate::interest_rate::{get_lending_index, InterestRateDataKey, LendingIndex, INDEX_SCALE};

// ── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CompoundingError {
    Unauthorized = 1,
    InvalidInterval = 2,
    Overflow = 3,
    NotInitialized = 4,
}

// ── Types ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum CompoundingDataKey {
    /// Compounding interval in ledger blocks.
    CompoundingInterval,
    /// Last block at which compounding occurred.
    LastCompoundLedger,
    /// Total number of compounding events.
    CompoundCount,
    /// Accumulated interest capitalised (for observability).
    TotalCapitalised,
}

/// Compounding configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompoundingConfig {
    /// Number of ledger blocks between compounding events.
    pub interval_ledgers: u32,
    /// Last ledger at which compounding occurred.
    pub last_compound_ledger: u32,
    /// Total compounding events executed.
    pub compound_count: u64,
}

/// Result of a compounding event.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompoundingResult {
    pub borrow_index: i128,
    pub supply_index: i128,
    pub ledgers_elapsed: u32,
    pub compound_count: u64,
}

// ── Admin: Configuration ────────────────────────────────────────────────────

/// Set the compounding interval in ledger blocks.
pub fn set_compounding_interval(
    env: &Env,
    admin: Address,
    interval_ledgers: u32,
) -> Result<(), CompoundingError> {
    admin::require_admin(env, &admin).map_err(|_| CompoundingError::Unauthorized)?;
    if interval_ledgers == 0 {
        return Err(CompoundingError::InvalidInterval);
    }
    env.storage()
        .persistent()
        .set(&CompoundingDataKey::CompoundingInterval, &interval_ledgers);
    env.events().publish(
        (Symbol::new(env, "compounding_interval_set"), admin),
        interval_ledgers,
    );
    Ok(())
}

/// Get the current compounding configuration.
pub fn get_compounding_config(env: &Env) -> CompoundingConfig {
    let interval = env
        .storage()
        .persistent()
        .get::<_, u32>(&CompoundingDataKey::CompoundingInterval)
        .unwrap_or(100);
    let last = env
        .storage()
        .persistent()
        .get::<_, u32>(&CompoundingDataKey::LastCompoundLedger)
        .unwrap_or(0);
    let count = env
        .storage()
        .persistent()
        .get::<_, u64>(&CompoundingDataKey::CompoundCount)
        .unwrap_or(0);
    CompoundingConfig {
        interval_ledgers: interval,
        last_compound_ledger: last,
        compound_count: count,
    }
}

// ── Compounding execution ───────────────────────────────────────────────────

/// Compound interest at the current ledger if the interval has elapsed.
///
/// This is the main entry point called by the protocol at each block (or
/// lazily on the next user interaction). It:
/// 1. Checks if enough ledgers have elapsed since the last compound.
/// 2. If not, returns the current index unchanged (idempotent no-op).
/// 3. If yes, updates the lending index via `update_lending_index` and
///    records the compounding event.
///
/// Returns a `CompoundingResult` with the updated index values.
pub fn compound_interest(env: &Env) -> Result<CompoundingResult, CompoundingError> {
    let current_ledger = env.ledger().sequence();
    let interval = env
        .storage()
        .persistent()
        .get::<_, u32>(&CompoundingDataKey::CompoundingInterval)
        .unwrap_or(100);
    let last_compound = env
        .storage()
        .persistent()
        .get::<_, u32>(&CompoundingDataKey::LastCompoundLedger)
        .unwrap_or(0);

    let ledgers_elapsed = current_ledger.saturating_sub(last_compound);
    if ledgers_elapsed < interval {
        // Not enough ledgers elapsed — return current index unchanged.
        let idx = get_lending_index(env);
        return Ok(CompoundingResult {
            borrow_index: idx.borrow_index,
            supply_index: idx.supply_index,
            ledgers_elapsed,
            compound_count: env
                .storage()
                .persistent()
                .get::<_, u64>(&CompoundingDataKey::CompoundCount)
                .unwrap_or(0),
        });
    }

    // Update the global lending index (this does the actual interest accrual).
    let updated_idx = crate::interest_rate::update_lending_index(env)
        .map_err(|_| CompoundingError::Overflow)?;

    // Record the compounding event.
    env.storage()
        .persistent()
        .set(&CompoundingDataKey::LastCompoundLedger, &current_ledger);
    let count = env
        .storage()
        .persistent()
        .get::<_, u64>(&CompoundingDataKey::CompoundCount)
        .unwrap_or(0)
        .saturating_add(1);
    env.storage()
        .persistent()
        .set(&CompoundingDataKey::CompoundCount, &count);

    // Track total capitalised for observability.
    let total_cap = env
        .storage()
        .persistent()
        .get::<_, i128>(&CompoundingDataKey::TotalCapitalised)
        .unwrap_or(0);
    let capitalised = updated_idx
        .borrow_index
        .saturating_sub(INDEX_SCALE);
    let new_total = total_cap.saturating_add(capitalised);
    env.storage()
        .persistent()
        .set(&CompoundingDataKey::TotalCapitalised, &new_total);

    env.events().publish(
        (Symbol::new(env, "interest_compounded"),),
        (updated_idx.borrow_index, updated_idx.supply_index, count),
    );

    Ok(CompoundingResult {
        borrow_index: updated_idx.borrow_index,
        supply_index: updated_idx.supply_index,
        ledgers_elapsed,
        compound_count: count,
    })
}

/// Check whether compounding is due at the current ledger.
pub fn is_compounding_due(env: &Env) -> bool {
    let current_ledger = env.ledger().sequence();
    let interval = env
        .storage()
        .persistent()
        .get::<_, u32>(&CompoundingDataKey::CompoundingInterval)
        .unwrap_or(100);
    let last = env
        .storage()
        .persistent()
        .get::<_, u32>(&CompoundingDataKey::LastCompoundLedger)
        .unwrap_or(0);
    current_ledger.saturating_sub(last) >= interval
}

/// Get the total interest capitalised across all compounding events.
pub fn get_total_capitalised(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get::<_, i128>(&CompoundingDataKey::TotalCapitalised)
        .unwrap_or(0)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{contract, contractimpl};

    #[contract]
    struct CompoundingTestContract;

    #[contractimpl]
    impl CompoundingTestContract {}

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.timestamp = 100;
            l.sequence = 1;
        });
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, CompoundingTestContract);
        env.as_contract(&contract_id, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
        });
        (env, admin, contract_id)
    }

    #[test]
    fn compounding_not_due_before_interval() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_compounding_interval(&env, admin, 10).unwrap();
            // At ledger 1, last_compound=0, elapsed=1 < 10 → not due.
            assert!(!is_compounding_due(&env));
            let result = compound_interest(&env).unwrap();
            assert_eq!(result.ledgers_elapsed, 1);
            assert_eq!(result.compound_count, 0);
        });
    }

    #[test]
    fn compounding_due_after_interval() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_compounding_interval(&env, admin, 5).unwrap();
            // Advance to ledger 10.
            env.ledger().with_mut(|l| l.sequence = 10);
            assert!(is_compounding_due(&env));
            let result = compound_interest(&env).unwrap();
            assert_eq!(result.compound_count, 1);
            assert!(result.borrow_index >= INDEX_SCALE);
        });
    }

    #[test]
    fn compounding_idempotent_within_interval() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_compounding_interval(&env, admin, 10).unwrap();
            env.ledger().with_mut(|l| l.sequence = 20);
            let r1 = compound_interest(&env).unwrap();
            assert_eq!(r1.compound_count, 1);
            // Immediately call again — still within interval.
            let r2 = compound_interest(&env).unwrap();
            assert_eq!(r2.compound_count, 1);
        });
    }

    #[test]
    fn multiple_compounding_events() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_compounding_interval(&env, admin, 5).unwrap();
            env.ledger().with_mut(|l| l.sequence = 10);
            compound_interest(&env).unwrap();
            env.ledger().with_mut(|l| l.sequence = 20);
            compound_interest(&env).unwrap();
            env.ledger().with_mut(|l| l.sequence = 30);
            let r = compound_interest(&env).unwrap();
            assert_eq!(r.compound_count, 3);
        });
    }

    #[test]
    fn invalid_interval_rejected() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            assert_eq!(
                set_compounding_interval(&env, admin, 0),
                Err(CompoundingError::InvalidInterval)
            );
        });
    }
}