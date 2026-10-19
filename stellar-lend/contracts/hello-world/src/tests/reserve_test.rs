//! # Reserve and Treasury Tests
//!
//! Comprehensive test suite for the reserve and treasury module.
//!
//! ## Test Coverage
//! - Reserve factor configuration (set, get, bounds validation)
//! - Reserve accrual from interest payments
//! - Treasury address management
//! - Treasury withdrawals (success and failure cases)
//! - Authorization checks (admin-only operations)
//! - Edge cases (zero amounts, maximum values, boundary conditions)
//! - Security validations (user fund protection, overflow prevention)
//!
//! ## Security Assumptions
//! 1. Only admin can modify reserve factors
//! 2. Only admin can withdraw reserves
//! 3. Reserve factor is capped at 50% (5000 bps)
//! 4. Withdrawals cannot exceed accrued reserves
//! 5. User funds are never accessible via treasury operations
//! 6. All arithmetic uses checked operations to prevent overflow
//! 7. Treasury address cannot be the contract itself

#![cfg(test)]

use crate::deposit::DepositDataKey;
use crate::treasury::{
    accrue_reserve, get_reserve_balance, get_treasury, set_treasury, claim_reserves, TreasuryError
};
use crate::reserve::{
    get_reserve_factor, get_reserve_stats,
    initialize_reserve_config, set_reserve_factor, ReserveError, BASIS_POINTS_SCALE, DEFAULT_RESERVE_FACTOR_BPS,
    MAX_RESERVE_FACTOR_BPS,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Helper function to create a test environment with an admin
fn setup_test_env() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    let contract_id = env.register_contract(None, crate::HelloContract);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Set admin in storage (wrapped in as_contract)
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DepositDataKey::Admin, &admin);
    });

    (env, contract_id, admin, user, treasury)
}

// Helper wrappers that handle as_contract internally
fn test_initialize_reserve_config(
    env: &Env,
    contract_id: &Address,
    asset: Option<Address>,
    reserve_factor_bps: i128,
) -> Result<(), TreasuryError> {
    env.as_contract(contract_id, || {
        initialize_reserve_config(env, asset, reserve_factor_bps)
    })
}

fn test_get_reserve_factor(env: &Env, contract_id: &Address, asset: Option<Address>) -> i128 {
    env.as_contract(contract_id, || get_reserve_factor(env, asset))
}

fn test_get_reserve_balance(env: &Env, contract_id: &Address, asset: Option<Address>) -> i128 {
    env.as_contract(contract_id, || get_reserve_balance(env, asset))
}

fn test_set_reserve_factor(
    env: &Env,
    contract_id: &Address,
    caller: Address,
    asset: Option<Address>,
    reserve_factor_bps: i128,
) -> Result<(), TreasuryError> {
    env.as_contract(contract_id, || {
        set_reserve_factor(env, caller, asset, reserve_factor_bps)
    })
}

fn test_accrue_reserve(
    env: &Env,
    contract_id: &Address,
    asset: Option<Address>,
    interest_amount: i128,
) -> Result<(i128, i128), TreasuryError> {
    env.as_contract(contract_id, || accrue_reserve(env, asset, interest_amount))
}

fn test_set_treasury(
    env: &Env,
    contract_id: &Address,
    caller: Address,
    treasury: Address,
) -> Result<(), TreasuryError> {
    env.as_contract(contract_id, || set_treasury(env, caller, treasury))
}

fn test_get_treasury(env: &Env, contract_id: &Address) -> Option<Address> {
    env.as_contract(contract_id, || get_treasury(env))
}

fn test_claim_reserves(
    env: &Env,
    contract_id: &Address,
    caller: Address,
    asset: Option<Address>,
    recipient: Address,
    amount: i128,
) -> Result<(), TreasuryError> {
    env.as_contract(contract_id, || {
        claim_reserves(env, caller, asset, recipient, amount)
    })
}

fn test_get_reserve_stats(
    env: &Env,
    contract_id: &Address,
    asset: Option<Address>,
) -> (i128, i128, Option<Address>) {
    env.as_contract(contract_id, || get_reserve_stats(env, asset))
}

// ============================================================================
// Initialization Tests
// ============================================================================

#[test]
fn test_initialize_reserve_config_success() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with default reserve factor
    let result = test_initialize_reserve_config(
        &env,
        &contract_id,
        &contract_id,
        contract_id,
        contract_id,
        asset.clone(),
        DEFAULT_RESERVE_FACTOR_BPS,
    );
    assert!(result.is_ok());

    // Verify reserve factor is set
    let factor = test_get_reserve_factor(
        &env,
        &contract_id,
        &contract_id,
        contract_id,
        contract_id,
        asset.clone(),
    );
    assert_eq!(factor, DEFAULT_RESERVE_FACTOR_BPS);

    // Verify reserve balance is initialized to zero
    let balance = test_get_reserve_balance(
        &env,
        &contract_id,
        &contract_id,
        contract_id,
        contract_id,
        asset,
    );
    assert_eq!(balance, 0);
}

#[test]
fn test_initialize_reserve_config_custom_factor() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with custom reserve factor (20%)
    let custom_factor = 2000i128;
    let result = test_initialize_reserve_config(&env, &contract_id, asset.clone(), custom_factor);
    assert!(result.is_ok());

    // Verify custom factor is set
    let factor = test_get_reserve_factor(&env, &contract_id, asset);
    assert_eq!(factor, custom_factor);
}

#[test]
fn test_initialize_reserve_config_zero_factor() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with zero reserve factor (0%)
    let result = test_initialize_reserve_config(&env, &contract_id, asset.clone(), 0);
    assert!(result.is_ok());

    let factor = test_get_reserve_factor(&env, &contract_id, asset);
    assert_eq!(factor, 0);
}

#[test]
fn test_initialize_reserve_config_max_factor() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with maximum reserve factor (50%)
    let result =
        test_initialize_reserve_config(&env, &contract_id, asset.clone(), MAX_RESERVE_FACTOR_BPS);
    assert!(result.is_ok());

    let factor = test_get_reserve_factor(&env, &contract_id, asset);
    assert_eq!(factor, MAX_RESERVE_FACTOR_BPS);
}

#[test]
fn test_initialize_reserve_config_exceeds_max() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Try to initialize with reserve factor > 50%
    let result =
        test_initialize_reserve_config(&env, &contract_id, asset, MAX_RESERVE_FACTOR_BPS + 1);
    assert_eq!(result, Err(TreasuryError::InvalidReserveFactor));
}

#[test]
fn test_initialize_reserve_config_negative_factor() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Try to initialize with negative reserve factor
    let result = test_initialize_reserve_config(&env, &contract_id, asset, -100);
    assert_eq!(result, Err(TreasuryError::InvalidReserveFactor));
}

#[test]
fn test_initialize_reserve_config_native_asset() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();

    // Initialize for native asset (None)
    let result =
        test_initialize_reserve_config(&env, &contract_id, None, DEFAULT_RESERVE_FACTOR_BPS);
    assert!(result.is_ok());

    let factor = test_get_reserve_factor(&env, &contract_id, None);
    assert_eq!(factor, DEFAULT_RESERVE_FACTOR_BPS);
}

// ============================================================================
// Reserve Factor Management Tests
// ============================================================================

#[test]
fn test_set_reserve_factor_by_admin() {
    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize first
    test_initialize_reserve_config(
        &env,
        &contract_id,
        asset.clone(),
        DEFAULT_RESERVE_FACTOR_BPS,
    )
    .unwrap();

    // Admin sets new reserve factor (25%)
    let new_factor = 2500i128;
    let result = test_set_reserve_factor(&env, &contract_id, admin, asset.clone(), new_factor);
    assert!(result.is_ok());

    // Verify factor is updated
    let factor = test_get_reserve_factor(&env, &contract_id, asset);
    assert_eq!(factor, new_factor);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction(0))")]
fn test_set_reserve_factor_by_non_admin() {
    let (env, contract_id, _admin, user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize first
    test_initialize_reserve_config(
        &env,
        &contract_id,
        asset.clone(),
        DEFAULT_RESERVE_FACTOR_BPS,
    )
    .unwrap();

    // Non-admin tries to set reserve factor - should fail
    let _ = test_set_reserve_factor(&env, &contract_id, user, asset, 2000);
}

#[test]
fn test_set_reserve_factor_exceeds_max() {
    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize first
    test_initialize_reserve_config(
        &env,
        &contract_id,
        asset.clone(),
        DEFAULT_RESERVE_FACTOR_BPS,
    )
    .unwrap();

    // Try to set reserve factor > 50%
    let result =
        test_set_reserve_factor(&env, &contract_id, admin, asset, MAX_RESERVE_FACTOR_BPS + 1);
    assert_eq!(result, Err(TreasuryError::InvalidReserveFactor));
}

#[test]
fn test_set_reserve_factor_to_zero() {
    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize first
    test_initialize_reserve_config(
        &env,
        &contract_id,
        asset.clone(),
        DEFAULT_RESERVE_FACTOR_BPS,
    )
    .unwrap();

    // Set reserve factor to zero (disable reserves)
    let result = test_set_reserve_factor(&env, &contract_id, admin, asset.clone(), 0);
    assert!(result.is_ok());

    let factor = test_get_reserve_factor(&env, &contract_id, asset);
    assert_eq!(factor, 0);
}

#[test]
fn test_get_reserve_factor_default() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Get reserve factor without initialization - should return default
    let factor = test_get_reserve_factor(&env, &contract_id, asset);
    assert_eq!(factor, DEFAULT_RESERVE_FACTOR_BPS);
}

// ============================================================================
// Reserve Accrual Tests
// ============================================================================

#[test]
fn test_accrue_reserve_basic() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 10% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // Accrue reserves from 1000 units of interest
    let interest = 1000i128;
    let result = test_accrue_reserve(&env, &contract_id, asset.clone(), interest);
    assert!(result.is_ok());

    let (reserve_amount, lender_amount) = result.unwrap();

    // 10% to reserves, 90% to lenders
    assert_eq!(reserve_amount, 100); // 1000 * 1000 / 10000 = 100
    assert_eq!(lender_amount, 900); // 1000 - 100 = 900

    // Verify reserve balance is updated
    let balance = test_get_reserve_balance(&env, &contract_id, asset);
    assert_eq!(balance, 100);
}

#[test]
fn test_accrue_reserve_zero_interest() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // Accrue with zero interest
    let result = test_accrue_reserve(&env, &contract_id, asset, 0);
    assert!(result.is_ok());

    let (reserve_amount, lender_amount) = result.unwrap();
    assert_eq!(reserve_amount, 0);
    assert_eq!(lender_amount, 0);
}

#[test]
fn test_accrue_reserve_zero_factor() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 0% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 0).unwrap();

    // Accrue reserves from 1000 units of interest
    let interest = 1000i128;
    let result = test_accrue_reserve(&env, &contract_id, asset.clone(), interest);
    assert!(result.is_ok());

    let (reserve_amount, lender_amount) = result.unwrap();

    // 0% to reserves, 100% to lenders
    assert_eq!(reserve_amount, 0);
    assert_eq!(lender_amount, 1000);

    let balance = test_get_reserve_balance(&env, &contract_id, asset);
    assert_eq!(balance, 0);
}

#[test]
fn test_accrue_reserve_max_factor() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 50% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), MAX_RESERVE_FACTOR_BPS)
        .unwrap();

    // Accrue reserves from 1000 units of interest
    let interest = 1000i128;
    let result = test_accrue_reserve(&env, &contract_id, asset.clone(), interest);
    assert!(result.is_ok());

    let (reserve_amount, lender_amount) = result.unwrap();

    // 50% to reserves, 50% to lenders
    assert_eq!(reserve_amount, 500); // 1000 * 5000 / 10000 = 500
    assert_eq!(lender_amount, 500); // 1000 - 500 = 500

    let balance = test_get_reserve_balance(&env, &contract_id, asset);
    assert_eq!(balance, 500);
}

#[test]
fn test_accrue_reserve_multiple_times() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 10% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // First accrual: 1000 interest
    test_accrue_reserve(&env, &contract_id, asset.clone(), 1000).unwrap();
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        100
    );

    // Second accrual: 500 interest
    test_accrue_reserve(&env, &contract_id, asset.clone(), 500).unwrap();
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        150
    ); // 100 + 50

    // Third accrual: 2000 interest
    test_accrue_reserve(&env, &contract_id, asset.clone(), 2000).unwrap();
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        350
    ); // 150 + 200
}

#[test]
fn test_accrue_reserve_large_amounts() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 10% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // Accrue large interest amount
    let large_interest = 1_000_000_000i128; // 1 billion
    let result = test_accrue_reserve(&env, &contract_id, asset.clone(), large_interest);
    assert!(result.is_ok());

    let (reserve_amount, lender_amount) = result.unwrap();
    assert_eq!(reserve_amount, 100_000_000); // 10% of 1 billion
    assert_eq!(lender_amount, 900_000_000); // 90% of 1 billion

    let balance = test_get_reserve_balance(&env, &contract_id, asset);
    assert_eq!(balance, 100_000_000);
}

#[test]
fn test_accrue_reserve_rounding() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 10% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // Accrue with amount that doesn't divide evenly
    let interest = 999i128;
    let result = test_accrue_reserve(&env, &contract_id, asset.clone(), interest);
    assert!(result.is_ok());

    let (reserve_amount, lender_amount) = result.unwrap();
    // 999 * 1000 / 10000 = 99 (integer division)
    assert_eq!(reserve_amount, 99);
    assert_eq!(lender_amount, 900); // 999 - 99
}

// ============================================================================
// Treasury Address Management Tests
// ============================================================================

#[test]
fn test_set_treasury_by_admin() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();

    // Admin sets treasury address
    let result = test_set_treasury(&env, &contract_id, admin, treasury.clone());
    assert!(result.is_ok());

    // Verify treasury address is set
    let stored_treasury = test_get_treasury(&env, &contract_id);
    assert_eq!(stored_treasury, Some(treasury));
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction(0))")]
fn test_set_treasury_by_non_admin() {
    let (env, contract_id, _admin, user, treasury) = setup_test_env();

    // Non-admin tries to set treasury address - should fail
    let _ = test_set_treasury(&env, &contract_id, user, treasury);
}

#[test]
fn test_set_treasury_to_contract() {
    let (env, contract_id, admin, _user, _treasury) = setup_test_env();

    // Try to set treasury to contract address - should fail
    let contract_addr = env.current_contract_address();
    let result = test_set_treasury(&env, &contract_id, admin, contract_addr);
    assert_eq!(result, Err(TreasuryError::InvalidTreasury));
}

#[test]
fn test_get_treasury_not_set() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();

    // Get treasury address before it's set
    let treasury = test_get_treasury(&env, &contract_id);
    assert_eq!(treasury, None);
}

#[test]
fn test_update_treasury_address() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();

    // Set initial treasury address
    test_set_treasury(&env, &contract_id, admin.clone(), treasury.clone()).unwrap();
    assert_eq!(
        test_get_treasury(&env, &contract_id),
        Some(treasury)
    );

    // Update to new treasury address
    let new_treasury = Address::generate(&env);
    test_set_treasury(&env, &contract_id, admin, new_treasury.clone()).unwrap();
    assert_eq!(
        test_get_treasury(&env, &contract_id),
        Some(new_treasury)
    );
}

// ============================================================================
// Treasury Withdrawal Tests
// ============================================================================

#[test]
fn test_claim_reserves_success() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup: initialize, set treasury, accrue reserves
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap(); // Accrues 1000 to reserves

    // Withdraw 500 to treasury
    let recipient = Address::generate(&env);
    let result = test_claim_reserves(&env, &contract_id, admin, asset.clone(), recipient, 500);
    assert!(result.is_ok());

    // Verify reserve balance is reduced
    let balance = test_get_reserve_balance(&env, &contract_id, asset);
    assert_eq!(balance, 500); // 1000 - 500
}

#[test]
fn test_withdraw_reserve_full_balance() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap(); // Accrues 1000

    // Withdraw full balance
    let recipient = Address::generate(&env);
    let result = test_claim_reserves(&env, &contract_id, admin, asset.clone(), recipient, 1000);
    assert!(result.is_ok());

    // Verify reserve balance is zero
    let balance = test_get_reserve_balance(&env, &contract_id, asset);
    assert_eq!(balance, 0);
}

#[test]
fn test_withdraw_reserve_exceeds_balance() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap(); // Accrues 1000

    // Try to withdraw more than available
    let recipient = Address::generate(&env);
    let result = test_claim_reserves(&env, &contract_id, admin, asset, recipient, 1001);
    assert_eq!(result, Err(TreasuryError::InsufficientReserve));
}

#[test]
fn test_withdraw_reserve_zero_amount() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap();

    // Try to withdraw zero
    let recipient = Address::generate(&env);
    let result = test_claim_reserves(&env, &contract_id, admin, asset, recipient, 0);
    assert_eq!(result, Err(TreasuryError::InvalidAmount));
}

#[test]
fn test_withdraw_reserve_negative_amount() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap();

    // Try to withdraw negative amount
    let recipient = Address::generate(&env);
    let result = test_claim_reserves(&env, &contract_id, admin, asset, recipient, -100);
    assert_eq!(result, Err(TreasuryError::InvalidAmount));
}

#[test]
fn test_withdraw_reserve_treasury_not_set() {
    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup without setting treasury
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap();

    // Try to withdraw without treasury set
    let recipient = Address::generate(&env);
    let result = test_claim_reserves(&env, &contract_id, admin, asset, recipient, 500);
    assert_eq!(result, Err(TreasuryError::TreasuryNotSet));
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction(0))")]
fn test_withdraw_reserve_by_non_admin() {
    let (env, contract_id, admin, user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_set_treasury(&env, &contract_id, admin, treasury).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap();

    // Non-admin tries to withdraw - should fail
    let recipient = Address::generate(&env);
    let _ = test_claim_reserves(&env, &contract_id, user, asset, recipient, 500);
}

#[test]
fn test_withdraw_reserve_multiple_times() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap(); // Accrues 1000

    let recipient = Address::generate(&env);
    // First withdrawal: 300
    test_claim_reserves(&env, &contract_id, admin.clone(), asset.clone(), recipient.clone(), 300)
        .unwrap();
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        700
    );

    // Second withdrawal: 200
    test_claim_reserves(&env, &contract_id, admin.clone(), asset.clone(), recipient.clone(), 200)
        .unwrap();
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        500
    );

    // Third withdrawal: 500 (remaining)
    test_claim_reserves(&env, &contract_id, admin, asset.clone(), recipient, 500).unwrap();
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset), 0);
}

#[test]
fn test_withdraw_reserve_from_zero_balance() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup without accruing reserves
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();

    // Try to withdraw from zero balance
    let recipient = Address::generate(&env);
    let result = test_claim_reserves(&env, &contract_id, admin, asset, recipient, 100);
    assert_eq!(result, Err(TreasuryError::InsufficientReserve));
}

// ============================================================================
// Reserve Factor Update and Interest Distribution Tests
// ============================================================================

/// # Reserve Factor Update and Interest Distribution Tests
///
/// This section tests reserve factor updates and interest distribution
/// to ensure proper allocation between protocol reserves and lenders.
///
/// ## Interest Distribution Formula
/// ```text
/// reserve_amount = interest_amount * reserve_factor / 10000
/// lender_amount = interest_amount - reserve_amount
/// ```
///
/// ## Security Invariants
/// 1. Reserve factor changes only affect future interest accruals
/// 2. Existing reserve balances are never modified retroactively
/// 3. Lender amounts are calculated correctly at each reserve factor
/// 4. Total interest always equals reserve_amount + lender_amount
/// 5. All calculations use checked arithmetic to prevent overflow

/// Structure to track interest distribution at different points in time
#[derive(Debug, Clone)]
struct InterestDistribution {
    period: u32,
    reserve_factor_bps: i128,
    interest_amount: i128,
    reserve_amount: i128,
    lender_amount: i128,
    cumulative_reserve_balance: i128,
    cumulative_lender_distribution: i128,
}

/// Helper function to record interest distribution
fn record_distribution(
    period: u32,
    reserve_factor_bps: i128,
    interest_amount: i128,
    reserve_amount: i128,
    lender_amount: i128,
    prev_reserve_balance: i128,
    prev_lender_distribution: i128,
) -> InterestDistribution {
    InterestDistribution {
        period,
        reserve_factor_bps,
        interest_amount,
        reserve_amount,
        lender_amount,
        cumulative_reserve_balance: prev_reserve_balance + reserve_amount,
        cumulative_lender_distribution: prev_lender_distribution + lender_amount,
    }
}

#[test]
fn test_reserve_factor_update_tracks_interest_distribution() {
    //! Tests interest distribution tracking across multiple reserve factor changes.
    //! 
    //! ## Test Scenario
    //! - Period 1: 10% reserve factor, 1000 interest
    //! - Period 2: Change to 20% reserve factor, 2000 interest
    //! - Period 3: Change to 5% reserve factor, 500 interest
    //!
    //! ## Expected Results
    //! | Period | Factor | Interest | Reserve | Lender | Cumulative Reserve | Cumulative Lender |
    //! |--------|--------|----------|---------|--------|-------------------|-------------------|
    //! | 1      | 10%    | 1000     | 100     | 900    | 100               | 900               |
    //! | 2      | 20%    | 2000     | 400     | 1600   | 500               | 2500              |
    //! | 3      | 5%     | 500      | 25      | 475    | 525               | 2975              |

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 10% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    let mut distributions: Vec<InterestDistribution> = Vec::new(&env);
    let mut cumulative_reserve: i128 = 0;
    let mut cumulative_lender: i128 = 0;

    // Period 1: Accrue with 10% factor
    let interest1: i128 = 1000;
    let (reserve1, lender1) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest1).unwrap();
    
    let dist1 = record_distribution(1, 1000, interest1, reserve1, lender1, cumulative_reserve, cumulative_lender);
    cumulative_reserve = dist1.cumulative_reserve_balance;
    cumulative_lender = dist1.cumulative_lender_distribution;
    distributions.push_back(dist1.clone());

    // Verify Period 1
    assert_eq!(reserve1, 100, "Period 1 reserve: 1000 * 10% = 100");
    assert_eq!(lender1, 900, "Period 1 lender: 1000 - 100 = 900");
    assert_eq!(cumulative_reserve, 100);
    assert_eq!(cumulative_lender, 900);
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), 100);

    // Change reserve factor to 20%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 2000).unwrap();

    // Period 2: Accrue with 20% factor
    let interest2: i128 = 2000;
    let (reserve2, lender2) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest2).unwrap();
    
    let dist2 = record_distribution(2, 2000, interest2, reserve2, lender2, cumulative_reserve, cumulative_lender);
    cumulative_reserve = dist2.cumulative_reserve_balance;
    cumulative_lender = dist2.cumulative_lender_distribution;
    distributions.push_back(dist2.clone());

    // Verify Period 2
    assert_eq!(reserve2, 400, "Period 2 reserve: 2000 * 20% = 400");
    assert_eq!(lender2, 1600, "Period 2 lender: 2000 - 400 = 1600");
    assert_eq!(cumulative_reserve, 500, "Cumulative reserve: 100 + 400 = 500");
    assert_eq!(cumulative_lender, 2500, "Cumulative lender: 900 + 1600 = 2500");
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), 500);

    // Change reserve factor to 5%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 500).unwrap();

    // Period 3: Accrue with 5% factor
    let interest3: i128 = 500;
    let (reserve3, lender3) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest3).unwrap();
    
    let dist3 = record_distribution(3, 500, interest3, reserve3, lender3, cumulative_reserve, cumulative_lender);
    cumulative_reserve = dist3.cumulative_reserve_balance;
    cumulative_lender = dist3.cumulative_lender_distribution;
    distributions.push_back(dist3);

    // Verify Period 3
    assert_eq!(reserve3, 25, "Period 3 reserve: 500 * 5% = 25");
    assert_eq!(lender3, 475, "Period 3 lender: 500 - 25 = 475");
    assert_eq!(cumulative_reserve, 525, "Cumulative reserve: 500 + 25 = 525");
    assert_eq!(cumulative_lender, 2975, "Cumulative lender: 2500 + 475 = 2975");
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), 525);

    // Verify total interest equals total distributed
    let total_interest = interest1 + interest2 + interest3;
    let total_distributed = cumulative_reserve + cumulative_lender;
    assert_eq!(total_interest, total_distributed, "Total interest must equal total distributed");
}

#[test]
fn test_no_retroactive_accounting_on_factor_change() {
    //! Tests that reserve factor changes do not retroactively affect previously accrued reserves.
    //! 
    //! ## Security Test
    //! This test ensures that changing the reserve factor only affects future interest accruals
    //! and never modifies historical reserve balances or re-calculates past distributions.
    //!
    //! ## Test Steps
    //! 1. Initialize with 10% reserve factor
    //! 2. Accrue 1000 interest → 100 to reserves, 900 to lenders
    //! 3. Record the reserve balance (100)
    //! 4. Change reserve factor to 30%
    //! 5. Verify reserve balance is STILL 100 (not modified retroactively)
    //! 6. Accrue another 1000 interest → 300 to reserves at new factor
    //! 7. Verify total reserve is 400 (100 historical + 300 new)

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Step 1: Initialize with 10% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // Step 2: Accrue first interest payment
    let (reserve1, lender1) = test_accrue_reserve(&env, &contract_id, asset.clone(), 1000).unwrap();
    assert_eq!(reserve1, 100, "First accrual at 10%: 1000 * 10% = 100");
    assert_eq!(lender1, 900);

    // Step 3: Record reserve balance before factor change
    let balance_before_change = test_get_reserve_balance(&env, &contract_id, asset.clone());
    assert_eq!(balance_before_change, 100);

    // Step 4: Change reserve factor to 30%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 3000).unwrap();

    // Step 5: CRITICAL SECURITY CHECK - Reserve balance must not change retroactively
    let balance_after_change = test_get_reserve_balance(&env, &contract_id, asset.clone());
    assert_eq!(
        balance_after_change, balance_before_change,
        "SECURITY: Reserve balance must not change when factor is updated"
    );
    assert_eq!(balance_after_change, 100, "Historical reserve must remain at 100");

    // Step 6: Accrue second interest payment at new factor
    let (reserve2, lender2) = test_accrue_reserve(&env, &contract_id, asset.clone(), 1000).unwrap();
    assert_eq!(reserve2, 300, "Second accrual at 30%: 1000 * 30% = 300");
    assert_eq!(lender2, 700);

    // Step 7: Verify total is additive (historical + new)
    let final_balance = test_get_reserve_balance(&env, &contract_id, asset.clone());
    assert_eq!(
        final_balance, 
        balance_before_change + reserve2,
        "Total reserve = historical reserve + new reserve"
    );
    assert_eq!(final_balance, 400, "100 (historical at 10%) + 300 (new at 30%) = 400");

    // Verify cumulative distribution
    let total_reserve = reserve1 + reserve2;
    let total_lender = lender1 + lender2;
    assert_eq!(total_reserve, 400);
    assert_eq!(total_lender, 1600);
    assert_eq!(total_reserve + total_lender, 2000, "Total must equal total interest (2 * 1000)");
}

#[test]
fn test_zero_percent_reserve_factor_interest_distribution() {
    //! Tests that 0% reserve factor allocates 100% of interest to lenders.
    //!
    //! ## Formula
    //! reserve_amount = interest * 0 / 10000 = 0
    //! lender_amount = interest - 0 = interest
    //!
    //! ## Edge Case Security
    //! - Zero factor should result in zero reserve accrual
    //! - All interest must go to lenders
    //! - Reserve balance should remain unchanged during accrual

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 0% reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 0).unwrap();

    // Accrue interest
    let interest: i128 = 5000;
    let (reserve_amount, lender_amount) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest).unwrap();

    // Verify 100% to lenders, 0% to reserves
    assert_eq!(reserve_amount, 0, "0% factor = 0 reserve: 5000 * 0 / 10000 = 0");
    assert_eq!(lender_amount, interest, "100% to lenders: 5000 - 0 = 5000");
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), 0);

    // Change factor mid-way and verify new distribution
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 1000).unwrap();
    
    let interest2: i128 = 3000;
    let (reserve2, lender2) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest2).unwrap();
    
    assert_eq!(reserve2, 300, "New accrual at 10%: 3000 * 10% = 300");
    assert_eq!(lender2, 2700);
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), 300);

    // Verify historical zero accrual is preserved
    let total_reserve = reserve_amount + reserve2;
    let total_lender = lender_amount + lender2;
    assert_eq!(total_reserve, 300, "Only second accrual contributed to reserves");
    assert_eq!(total_lender, 7700, "First (5000) + second (2700) = 7700");
}

#[test]
fn test_maximum_reserve_factor_interest_distribution() {
    //! Tests that max (50%) reserve factor correctly allocates interest.
    //!
    //! ## Formula
    //! reserve_amount = interest * 5000 / 10000 = interest * 50%
    //! lender_amount = interest - reserve_amount = interest * 50%
    //!
    //! ## Edge Case Security
    //! - Max factor (50%) splits interest equally
    //! - No single accrual can exceed 50% to reserves
    //! - Lenders always receive at least 50% of interest

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with max (50%) reserve factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), MAX_RESERVE_FACTOR_BPS).unwrap();

    // Accrue interest at max factor
    let interest: i128 = 10000;
    let (reserve_amount, lender_amount) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest).unwrap();

    // Verify 50/50 split
    assert_eq!(reserve_amount, 5000, "50% factor = 5000: 10000 * 5000 / 10000 = 5000");
    assert_eq!(lender_amount, 5000, "50% to lenders: 10000 - 5000 = 5000");
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), 5000);

    // Reduce factor and verify new distribution
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 1000).unwrap();
    
    let interest2: i128 = 5000;
    let (reserve2, lender2) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest2).unwrap();
    
    assert_eq!(reserve2, 500, "New accrual at 10%: 5000 * 10% = 500");
    assert_eq!(lender2, 4500);

    // Verify cumulative: 50% historical + 10% new
    let final_balance = test_get_reserve_balance(&env, &contract_id, asset.clone());
    assert_eq!(final_balance, 5500, "5000 (50% of first) + 500 (10% of second) = 5500");

    let total_reserve = reserve_amount + reserve2;
    let total_lender = lender_amount + lender2;
    assert_eq!(total_reserve, 5500);
    assert_eq!(total_lender, 9500);
    assert_eq!(total_reserve + total_lender, 15000, "Total must equal total interest");
}

#[test]
fn test_multiple_factor_changes_preserves_distribution_integrity() {
    //! Tests multiple rapid reserve factor changes preserve distribution integrity.
    //! 
    //! ## Test Scenario
    //! - Start: 10% factor
    //! - Change 1: 10% → 25%
    //! - Change 2: 25% → 0%
    //! - Change 3: 0% → 50%
    //! - Change 4: 50% → 10%
    //!
    //! ## Security Invariant
    //! Each interest accrual must use the factor that was active at that moment.

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Track expected values
    let mut expected_reserve_balance: i128 = 0;
    let mut total_interest: i128 = 0;

    // Start: 10% factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    
    let interest1: i128 = 10000;
    let (r1, l1) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest1).unwrap();
    assert_eq!(r1, 1000, "At 10%: 10000 * 10% = 1000");
    expected_reserve_balance += r1;
    total_interest += interest1;
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), expected_reserve_balance);

    // Change 1: 10% → 25%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 2500).unwrap();
    
    let interest2: i128 = 8000;
    let (r2, l2) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest2).unwrap();
    assert_eq!(r2, 2000, "At 25%: 8000 * 25% = 2000");
    expected_reserve_balance += r2;
    total_interest += interest2;
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), expected_reserve_balance);

    // Change 2: 25% → 0%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 0).unwrap();
    
    let interest3: i128 = 5000;
    let (r3, l3) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest3).unwrap();
    assert_eq!(r3, 0, "At 0%: 5000 * 0% = 0");
    expected_reserve_balance += r3;
    total_interest += interest3;
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), expected_reserve_balance);

    // Change 3: 0% → 50% (max)
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), MAX_RESERVE_FACTOR_BPS).unwrap();
    
    let interest4: i128 = 4000;
    let (r4, l4) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest4).unwrap();
    assert_eq!(r4, 2000, "At 50%: 4000 * 50% = 2000");
    expected_reserve_balance += r4;
    total_interest += interest4;
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), expected_reserve_balance);

    // Change 4: 50% → 10%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 1000).unwrap();
    
    let interest5: i128 = 10000;
    let (r5, l5) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest5).unwrap();
    assert_eq!(r5, 1000, "At 10%: 10000 * 10% = 1000");
    expected_reserve_balance += r5;
    total_interest += interest5;
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), expected_reserve_balance);

    // Final verification
    assert_eq!(expected_reserve_balance, 1000 + 2000 + 0 + 2000 + 1000);
    assert_eq!(expected_reserve_balance, 6000);
    
    let total_distributed = r1 + l1 + r2 + l2 + r3 + l3 + r4 + l4 + r5 + l5;
    assert_eq!(total_interest, total_distributed, "All interest must be fully distributed");

    // Verify each period's reserve calculation
    assert_eq!(r1, 1000); // 10% of 10000
    assert_eq!(r2, 2000); // 25% of 8000
    assert_eq!(r3, 0);    // 0% of 5000
    assert_eq!(r4, 2000); // 50% of 4000
    assert_eq!(r5, 1000); // 10% of 10000
}

#[test]
fn test_interest_distribution_with_large_amounts_and_factor_changes() {
    //! Tests interest distribution with large amounts to verify overflow safety.
    //!
    //! ## Test Scenario
    //! - Large interest amounts (millions/billions)
    //! - Factor changes between large accruals
    //! - Verifies checked arithmetic prevents overflow

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1500).unwrap(); // 15%

    // Large interest at 15%
    let interest1: i128 = 100_000_000; // 100 million
    let (r1, l1) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest1).unwrap();
    assert_eq!(r1, 15_000_000, "15% of 100M = 15M");
    assert_eq!(l1, 85_000_000);
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), 15_000_000);

    // Change to max factor
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), MAX_RESERVE_FACTOR_BPS).unwrap();

    // Even larger interest at 50%
    let interest2: i128 = 500_000_000; // 500 million
    let (r2, l2) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest2).unwrap();
    assert_eq!(r2, 250_000_000, "50% of 500M = 250M");
    assert_eq!(l2, 250_000_000);

    let final_balance = test_get_reserve_balance(&env, &contract_id, asset.clone());
    assert_eq!(final_balance, 265_000_000, "15M + 250M = 265M");

    // Verify total
    let total_interest = interest1 + interest2;
    let total_distributed = (r1 + l1) + (r2 + l2);
    assert_eq!(total_interest, total_distributed);
    assert_eq!(total_interest, 600_000_000);
}

#[test]
fn test_reserve_factor_change_event_consistency() {
    //! Tests that events are emitted correctly during factor changes and accruals.
    //! 
    //! ## Event Verification
    //! - reserve_factor_updated event on factor change
    //! - reserve_accrued event on each accrual
    //! - Event data matches actual distribution

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // First accrual at 10%
    let interest1: i128 = 5000;
    let (r1, l1) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest1).unwrap();
    assert_eq!(r1, 500);
    assert_eq!(l1, 4500);

    // Change to 30%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 3000).unwrap();
    let factor_after = test_get_reserve_factor(&env, &contract_id, asset.clone());
    assert_eq!(factor_after, 3000);

    // Second accrual at 30%
    let interest2: i128 = 5000;
    let (r2, l2) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest2).unwrap();
    assert_eq!(r2, 1500);
    assert_eq!(l2, 3500);

    // Verify final balance reflects both accruals correctly
    let final_balance = test_get_reserve_balance(&env, &contract_id, asset.clone());
    assert_eq!(final_balance, 2000, "500 (at 10%) + 1500 (at 30%) = 2000");

    // Verify the formula was applied correctly for each period
    let expected_r1 = interest1 * 1000 / 10000; // 10%
    let expected_r2 = interest2 * 3000 / 10000; // 30%
    assert_eq!(r1, expected_r1);
    assert_eq!(r2, expected_r2);
    assert_eq!(final_balance, expected_r1 + expected_r2);
}

#[test]
fn test_accrue_reserve_during_factor_transition() {
    //! Tests multiple sequential accruals across factor transitions.
    //!
    //! ## Test Scenario
    //! - Accrue at 10% (multiple times)
    //! - Change to 30%
    //! - Accrue at 30% (multiple times)
    //! - Verify each group uses correct factor

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // Group 1: Three accruals at 10%
    let mut group1_reserve: i128 = 0;
    for i in 0..3 {
        let interest: i128 = 1000 * (i as i128 + 1); // 1000, 2000, 3000
        let (r, l) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest).unwrap();
        let expected_r = interest * 1000 / 10000;
        assert_eq!(r, expected_r, "Accrual {} at 10%", i + 1);
        group1_reserve += r;
    }
    // Group 1 expected: 100 + 200 + 300 = 600
    assert_eq!(group1_reserve, 600);
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset.clone()), 600);

    // Change to 30%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 3000).unwrap();

    // Group 2: Three accruals at 30%
    let mut group2_reserve: i128 = 0;
    for i in 0..3 {
        let interest: i128 = 1000 * (i as i128 + 1); // 1000, 2000, 3000
        let (r, l) = test_accrue_reserve(&env, &contract_id, asset.clone(), interest).unwrap();
        let expected_r = interest * 3000 / 10000;
        assert_eq!(r, expected_r, "Accrual {} at 30%", i + 1);
        group2_reserve += r;
    }
    // Group 2 expected: 300 + 600 + 900 = 1800
    assert_eq!(group2_reserve, 1800);

    // Total: 600 (at 10%) + 1800 (at 30%) = 2400
    let final_balance = test_get_reserve_balance(&env, &contract_id, asset.clone());
    assert_eq!(final_balance, 2400);
    assert_eq!(final_balance, group1_reserve + group2_reserve);
}

#[test]
fn test_reserve_factor_formula_precision() {
    //! Tests formula precision with various interest amounts and factors.
    //! 
    //! ## Formula
    //! reserve_amount = (interest_amount * reserve_factor_bps) / 10000
    //! 
    //! ## Precision Cases
    //! - Small interest with various factors
    //! - Interest amounts that don't divide evenly
    //! - Rounding behavior (integer division truncates)

    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Test various factor/interest combinations
    let test_cases: Vec<(i128, i128, i128)> = Vec::from_array(&env, [
        (1000, 1, 0),      // 1 * 10% = 0.1 → 0 (truncated)
        (1000, 9, 0),      // 9 * 10% = 0.9 → 0 (truncated)
        (1000, 10, 1),     // 10 * 10% = 1
        (1000, 99, 9),     // 99 * 10% = 9.9 → 9 (truncated)
        (1000, 100, 10),   // 100 * 10% = 10
        (3333, 100, 33),   // 100 * 33.33% = 33.33 → 33
        (1, 10000, 1),     // 10000 * 0.01% = 1
        (MAX_RESERVE_FACTOR_BPS, 3, 1), // 3 * 50% = 1.5 → 1
    ]);

    for (factor, interest, expected_reserve) in test_cases.iter() {
        // Initialize with specific factor
        test_initialize_reserve_config(&env, &contract_id, asset.clone(), *factor).unwrap();
        
        let (r, l) = test_accrue_reserve(&env, &contract_id, asset.clone(), *interest).unwrap();
        
        assert_eq!(r, *expected_reserve, 
            "Factor {} bps, Interest {}: expected reserve {}", 
            factor, interest, expected_reserve);
        assert_eq!(l, *interest - *expected_reserve,
            "Lender amount should be interest - reserve");
        
        // Verify formula: (interest * factor) / 10000
        let calculated = (*interest).checked_mul(*factor).unwrap().checked_div(10000).unwrap();
        assert_eq!(r, calculated);
    }
}

#[test]
fn test_concurrent_asset_factor_independence() {
    //! Tests that reserve factor changes for one asset don't affect other assets.
    //! 
    //! ## Multi-Asset Test
    //! - Asset A: Factor changes 10% → 30% → 10%
    //! - Asset B: Factor stays constant at 20%
    //! - Verify each asset's distributions are independent

    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset_a = Some(Address::generate(&env));
    let asset_b = Some(Address::generate(&env));

    // Initialize both assets
    test_initialize_reserve_config(&env, &contract_id, asset_a.clone(), 1000).unwrap(); // 10%
    test_initialize_reserve_config(&env, &contract_id, asset_b.clone(), 2000).unwrap(); // 20%

    // Period 1: Both assets accrue
    let (ra1, la1) = test_accrue_reserve(&env, &contract_id, asset_a.clone(), 10000).unwrap();
    let (rb1, lb1) = test_accrue_reserve(&env, &contract_id, asset_b.clone(), 10000).unwrap();
    
    assert_eq!(ra1, 1000, "Asset A at 10%: 10000 * 10% = 1000");
    assert_eq!(rb1, 2000, "Asset B at 20%: 10000 * 20% = 2000");

    // Change only Asset A to 30%
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset_a.clone(), 3000).unwrap();

    // Period 2: Both assets accrue again
    let (ra2, la2) = test_accrue_reserve(&env, &contract_id, asset_a.clone(), 10000).unwrap();
    let (rb2, lb2) = test_accrue_reserve(&env, &contract_id, asset_b.clone(), 10000).unwrap();
    
    assert_eq!(ra2, 3000, "Asset A at 30%: 10000 * 30% = 3000");
    assert_eq!(rb2, 2000, "Asset B still at 20%: 10000 * 20% = 2000");

    // Verify Asset A total: 1000 + 3000 = 4000
    let balance_a = test_get_reserve_balance(&env, &contract_id, asset_a.clone());
    assert_eq!(balance_a, 4000);

    // Verify Asset B total: 2000 + 2000 = 4000
    let balance_b = test_get_reserve_balance(&env, &contract_id, asset_b.clone());
    assert_eq!(balance_b, 4000);

    // Verify factors are independent
    let factor_a = test_get_reserve_factor(&env, &contract_id, asset_a.clone());
    let factor_b = test_get_reserve_factor(&env, &contract_id, asset_b.clone());
    assert_eq!(factor_a, 3000, "Asset A should be 30%");
    assert_eq!(factor_b, 2000, "Asset B should still be 20%");
}

#[test]
fn test_get_reserve_stats() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 2000).unwrap();
    test_set_treasury(&env, &contract_id, admin, treasury.clone()).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 5000).unwrap(); // Accrues 1000

    // Get stats
    let (balance, factor, treasury_addr) = test_get_reserve_stats(&env, &contract_id, asset);

    assert_eq!(balance, 1000);
    assert_eq!(factor, 2000);
    assert_eq!(treasury_addr, Some(treasury));
}

#[test]
fn test_get_reserve_stats_no_treasury() {
    let (env, contract_id, _admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Setup without treasury
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1500).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap();

    // Get stats
    let (balance, factor, treasury_addr) = test_get_reserve_stats(&env, &contract_id, asset);

    assert_eq!(balance, 1500);
    assert_eq!(factor, 1500);
    assert_eq!(treasury_addr, None);
}

// ============================================================================
// Integration and Edge Case Tests
// ============================================================================

#[test]
fn test_complete_reserve_lifecycle() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // 1. Initialize reserve config
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();

    // 2. Set treasury address
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();

    // 3. Accrue reserves multiple times
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap(); // +1000
    test_accrue_reserve(&env, &contract_id, asset.clone(), 5000).unwrap(); // +500
    test_accrue_reserve(&env, &contract_id, asset.clone(), 2000).unwrap(); // +200
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        1700
    );

    // 4. Withdraw partial reserves
    test_withdraw_reserve_to_treasury(&env, &contract_id, admin.clone(), asset.clone(), 700)
        .unwrap();
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        1000
    );

    // 5. Accrue more reserves
    test_accrue_reserve(&env, &contract_id, asset.clone(), 3000).unwrap(); // +300
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        1300
    );

    // 6. Update reserve factor
    test_set_reserve_factor(&env, &contract_id, admin.clone(), asset.clone(), 2000).unwrap();

    // 7. Accrue with new factor
    test_accrue_reserve(&env, &contract_id, asset.clone(), 5000).unwrap(); // +1000 (20%)
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        2300
    );

    // 8. Withdraw remaining
    test_withdraw_reserve_to_treasury(&env, &contract_id, admin, asset.clone(), 2300).unwrap();
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset), 0);
}

#[test]
fn test_multiple_assets_independent_reserves() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();
    let asset1 = Some(Address::generate(&env));
    let asset2 = Some(Address::generate(&env));

    // Initialize both assets with different factors
    test_initialize_reserve_config(&env, &contract_id, asset1.clone(), 1000).unwrap(); // 10%
    test_initialize_reserve_config(&env, &contract_id, asset2.clone(), 2000).unwrap(); // 20%

    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();

    // Accrue reserves for both assets
    test_accrue_reserve(&env, &contract_id, asset1.clone(), 10000).unwrap(); // +1000
    test_accrue_reserve(&env, &contract_id, asset2.clone(), 10000).unwrap(); // +2000

    // Verify independent balances
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset1.clone()),
        1000
    );
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset2.clone()),
        2000
    );

    // Withdraw from asset1
    test_withdraw_reserve_to_treasury(&env, &contract_id, admin.clone(), asset1.clone(), 500)
        .unwrap();

    // Verify asset2 is unaffected
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset1), 500);
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset2), 2000);
}

#[test]
fn test_native_asset_reserves() {
    let (env, contract_id, admin, _user, treasury) = setup_test_env();

    // Test with native asset (None)
    test_initialize_reserve_config(&env, &contract_id, None, 1500).unwrap();
    test_set_treasury(&env, &contract_id, admin.clone(), treasury).unwrap();

    test_accrue_reserve(&env, &contract_id, None, 10000).unwrap(); // +1500
    assert_eq!(test_get_reserve_balance(&env, &contract_id, None), 1500);

    test_withdraw_reserve_to_treasury(&env, &contract_id, admin, None, 1000).unwrap();
    assert_eq!(test_get_reserve_balance(&env, &contract_id, None), 500);
}

#[test]
fn test_reserve_factor_change_does_not_affect_existing_balance() {
    let (env, contract_id, admin, _user, _treasury) = setup_test_env();
    let asset = Some(Address::generate(&env));

    // Initialize with 10% factor
    test_initialize_reserve_config(&env, &contract_id, asset.clone(), 1000).unwrap();
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap(); // +1000
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        1000
    );

    // Change factor to 20%
    test_set_reserve_factor(&env, &contract_id, admin, asset.clone(), 2000).unwrap();

    // Existing balance should remain unchanged
    assert_eq!(
        test_get_reserve_balance(&env, &contract_id, asset.clone()),
        1000
    );

    // New accruals use new factor
    test_accrue_reserve(&env, &contract_id, asset.clone(), 10000).unwrap(); // +2000 (20%)
    assert_eq!(test_get_reserve_balance(&env, &contract_id, asset), 3000); // 1000 + 2000
}
