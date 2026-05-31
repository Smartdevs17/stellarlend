#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Bytes, Env, Val, Vec};

mod borrow;
mod deposit;
mod events;
mod flash_loan;
mod pause;
mod token_receiver;
mod withdraw;

use borrow::{
    borrow as borrow_cmd, deposit as borrow_deposit, get_admin as get_borrow_admin,
    get_user_collateral as get_borrow_collateral, get_user_debt as get_borrow_debt,
    initialize_borrow_settings as initialize_borrow_logic, repay as borrow_repay,
    set_admin as set_borrow_admin,
    set_liquidation_threshold_bps as set_liquidation_threshold_logic,
    set_oracle as set_oracle_logic, BorrowCollateral, BorrowError, DebtPosition,
};
use deposit::{
    deposit as deposit_logic, get_user_collateral as get_deposit_collateral,
    initialize_deposit_settings as initialize_deposit_logic, DepositCollateral, DepositError,
};
use flash_loan::{
    flash_loan as flash_loan_logic, set_flash_loan_fee_bps as set_flash_loan_fee_logic,
    set_manipulation_config as set_flash_manipulation_config,
    record_price_sample as flash_record_price_sample,
    ManipulationConfig as FlashManipulationConfig,
    FlashLoanError,
};
use pause::{is_paused, set_pause as set_pause_logic, PauseType};
use token_receiver::receive as receive_logic;

mod views;
use views::{
    get_collateral_balance as view_collateral_balance,
    get_collateral_value as view_collateral_value, get_debt_balance as view_debt_balance,
    get_debt_value as view_debt_value, get_health_factor as view_health_factor,
    get_user_position as view_user_position, UserPositionSummary,
};

use withdraw::{
    initialize_withdraw_settings as initialize_withdraw_logic,
    set_withdraw_paused as set_withdraw_paused_logic, withdraw as withdraw_logic, WithdrawError,
};
mod data_store;
mod insurance;
mod upgrade;

pub mod interest_rate;
pub mod risk_monitor;

use insurance::{
    cancel_claim as insurance_cancel_claim, collect_premium as insurance_collect_premium,
    evaluate_claim as insurance_evaluate_claim, fund_pool as insurance_fund_pool,
    get_all_claim_ids as insurance_get_all_claim_ids, get_all_claims as insurance_get_all_claims,
    get_analytics as insurance_get_analytics, get_claim_by_id as insurance_get_claim,
    get_coverage_limit as insurance_get_coverage_limit,
    get_premium_rate as insurance_get_premium_rate, initialize as insurance_initialize,
    set_coverage_limit as insurance_set_coverage_limit,
    submit_claim as insurance_submit_claim, InsuranceAnalytics, InsuranceClaim, InsuranceError,
};

#[cfg(test)]
mod borrow_test;
#[cfg(test)]
mod data_store_test;
#[cfg(test)]
mod deposit_test;
#[cfg(test)]
mod flash_loan_test;
#[cfg(test)]
mod insurance_test;
#[cfg(test)]
mod math_safety_test;
#[cfg(test)]
mod pause_test;
#[cfg(test)]
mod token_receiver_test;
#[cfg(test)]
mod upgrade_test;
#[cfg(test)]
mod views_test;
#[cfg(test)]
mod withdraw_test;

// Property-based tests (issue #359)
#[cfg(test)]
mod proptest_helpers;
#[cfg(test)]
mod deposit_prop_test;
#[cfg(test)]
mod withdraw_prop_test;
#[cfg(test)]
mod borrow_prop_test;
#[cfg(test)]
mod interest_rate_prop_test;
#[cfg(test)]
mod invariant_prop_test;

#[contract]
pub struct LendingContract;

#[contractimpl]
impl LendingContract {
    /// Initialize the protocol with admin and settings
    pub fn initialize(
        env: Env,
        admin: Address,
        debt_ceiling: i128,
        min_borrow_amount: i128,
    ) -> Result<(), BorrowError> {
        if get_borrow_admin(&env).is_some() {
            return Err(BorrowError::Unauthorized);
        }
        set_borrow_admin(&env, &admin);
        initialize_borrow_logic(&env, debt_ceiling, min_borrow_amount)?;
        Ok(())
    }

    /// Borrow assets against deposited collateral
    pub fn borrow(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
        collateral_asset: Address,
        collateral_amount: i128,
    ) -> Result<(), BorrowError> {
        borrow_cmd(
            &env,
            user,
            asset,
            amount,
            collateral_asset,
            collateral_amount,
        )
    }

    /// Set protocol pause state for a specific operation (admin only)
    pub fn set_pause(
        env: Env,
        admin: Address,
        pause_type: PauseType,
        paused: bool,
    ) -> Result<(), BorrowError> {
        let current_admin = get_borrow_admin(&env).ok_or(BorrowError::Unauthorized)?;
        if admin != current_admin {
            return Err(BorrowError::Unauthorized);
        }
        admin.require_auth();
        set_pause_logic(&env, admin, pause_type, paused);
        Ok(())
    }

    /// Repay borrowed assets
    pub fn repay(env: Env, user: Address, asset: Address, amount: i128) -> Result<(), BorrowError> {
        user.require_auth();
        if is_paused(&env, PauseType::Repay) {
            return Err(BorrowError::ProtocolPaused);
        }
        borrow_repay(&env, user, asset, amount)
    }

    /// Deposit collateral into the protocol
    pub fn deposit(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
    ) -> Result<i128, DepositError> {
        if is_paused(&env, PauseType::Deposit) {
            return Err(DepositError::DepositPaused);
        }
        deposit_logic(&env, user, asset, amount)
    }

    /// Deposit collateral for a borrow position
    pub fn deposit_collateral(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
    ) -> Result<(), BorrowError> {
        user.require_auth();
        if is_paused(&env, PauseType::Deposit) {
            return Err(BorrowError::ProtocolPaused);
        }
        borrow_deposit(&env, user, asset, amount)
    }

    /// Liquidate a position
    pub fn liquidate(
        env: Env,
        liquidator: Address,
        _borrower: Address,
        _debt_asset: Address,
        _collateral_asset: Address,
        _amount: i128,
    ) -> Result<(), BorrowError> {
        liquidator.require_auth();
        if is_paused(&env, PauseType::Liquidation) {
            return Err(BorrowError::ProtocolPaused);
        }
        Ok(())
    }

    /// Get user's debt position
    pub fn get_user_debt(env: Env, user: Address) -> DebtPosition {
        get_borrow_debt(&env, &user)
    }

    /// Get user's collateral position (borrow module)
    pub fn get_user_collateral(env: Env, user: Address) -> BorrowCollateral {
        get_borrow_collateral(&env, &user)
    }

    /// Returns the user's collateral balance (raw amount).
    pub fn get_collateral_balance(env: Env, user: Address) -> i128 {
        view_collateral_balance(&env, &user)
    }

    /// Returns the user's debt balance (principal + accrued interest).
    pub fn get_debt_balance(env: Env, user: Address) -> i128 {
        view_debt_balance(&env, &user)
    }

    /// Returns the user's collateral value in common unit. 0 if oracle not set.
    pub fn get_collateral_value(env: Env, user: Address) -> i128 {
        view_collateral_value(&env, &user)
    }

    /// Returns the user's debt value in common unit. 0 if oracle not set.
    pub fn get_debt_value(env: Env, user: Address) -> i128 {
        view_debt_value(&env, &user)
    }

    /// Returns health factor (scaled 10000 = 1.0).
    pub fn get_health_factor(env: Env, user: Address) -> i128 {
        view_health_factor(&env, &user)
    }

    /// Returns full position summary.
    pub fn get_user_position(env: Env, user: Address) -> UserPositionSummary {
        view_user_position(&env, &user)
    }

    /// Set oracle address for price feeds (admin only).
    pub fn set_oracle(env: Env, admin: Address, oracle: Address) -> Result<(), BorrowError> {
        set_oracle_logic(&env, &admin, oracle)
    }

    /// Set liquidation threshold in basis points (admin only).
    pub fn set_liquidation_threshold_bps(
        env: Env,
        admin: Address,
        bps: i128,
    ) -> Result<(), BorrowError> {
        set_liquidation_threshold_logic(&env, &admin, bps)
    }

    /// Initialize deposit settings (admin only)
    pub fn initialize_deposit_settings(
        env: Env,
        deposit_cap: i128,
        min_deposit_amount: i128,
    ) -> Result<(), DepositError> {
        initialize_deposit_logic(&env, deposit_cap, min_deposit_amount)
    }

    /// Set deposit pause state (admin only)
    pub fn set_deposit_paused(env: Env, paused: bool) -> Result<(), DepositError> {
        env.storage()
            .persistent()
            .set(&pause::PauseDataKey::State(PauseType::Deposit), &paused);
        Ok(())
    }

    /// Get user's deposit collateral position
    pub fn get_user_collateral_deposit(
        env: Env,
        user: Address,
        asset: Address,
    ) -> DepositCollateral {
        get_deposit_collateral(&env, &user, &asset)
    }

    /// Get protocol admin
    pub fn get_admin(env: Env) -> Option<Address> {
        get_borrow_admin(&env)
    }

    /// Execute a flash loan with attack-prevention guards.
    ///
    /// `spot_price` is the current oracle price of `asset` and is used for
    /// TWAP-deviation detection. Pass 0 to skip the TWAP check (not recommended
    /// in production).
    pub fn flash_loan(
        env: Env,
        receiver: Address,
        asset: Address,
        amount: i128,
        spot_price: i128,
        params: Bytes,
    ) -> Result<(), FlashLoanError> {
        flash_loan_logic(&env, receiver, asset, amount, spot_price, params)
    }

    /// Record an oracle price sample for a given asset (used to maintain the TWAP).
    /// Should be called by the oracle feed on each price update.
    pub fn flash_record_price(env: Env, asset: Address, price: i128) {
        flash_record_price_sample(&env, &asset, price);
    }

    /// Update the flash loan attack-prevention config (admin only).
    pub fn set_flash_manipulation_config(
        env: Env,
        admin: Address,
        config: FlashManipulationConfig,
    ) -> Result<(), FlashLoanError> {
        let current_admin = get_borrow_admin(&env).ok_or(FlashLoanError::Unauthorized)?;
        if admin != current_admin {
            return Err(FlashLoanError::Unauthorized);
        }
        admin.require_auth();
        set_flash_manipulation_config(&env, config)
    }

    /// Set the flash loan fee in basis points (admin only)
    pub fn set_flash_loan_fee_bps(env: Env, fee_bps: i128) -> Result<(), FlashLoanError> {
        let current_admin = get_borrow_admin(&env).ok_or(FlashLoanError::Unauthorized)?;
        current_admin.require_auth();
        set_flash_loan_fee_logic(&env, fee_bps)
    }

    /// Withdraw collateral from the protocol
    pub fn withdraw(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
    ) -> Result<i128, WithdrawError> {
        if is_paused(&env, PauseType::Withdraw) {
            return Err(WithdrawError::WithdrawPaused);
        }
        withdraw_logic(&env, user, asset, amount)
    }

    /// Initialize withdraw settings (admin only)
    pub fn initialize_withdraw_settings(
        env: Env,
        min_withdraw_amount: i128,
    ) -> Result<(), WithdrawError> {
        initialize_withdraw_logic(&env, min_withdraw_amount)
    }

    /// Set withdraw pause state (admin only)
    pub fn set_withdraw_paused(env: Env, paused: bool) -> Result<(), WithdrawError> {
        set_withdraw_paused_logic(&env, paused)
    }

    /// Token receiver hook
    pub fn receive(
        env: Env,
        token_asset: Address,
        from: Address,
        amount: i128,
        payload: Vec<Val>,
    ) -> Result<(), BorrowError> {
        receive_logic(env, token_asset, from, amount, payload)
    }

    /// Initialize borrow settings (admin only)
    pub fn initialize_borrow_settings(
        env: Env,
        debt_ceiling: i128,
        min_borrow_amount: i128,
    ) -> Result<(), BorrowError> {
        initialize_borrow_logic(&env, debt_ceiling, min_borrow_amount)
    }

    // ═══════════════════════════════════════════════ Insurance pool ═══

    pub fn insurance_initialize(env: Env, admin: Address) -> Result<(), InsuranceError> {
        insurance_initialize(&env, &admin)
    }

    pub fn insurance_fund_pool(env: Env, amount: i128) -> Result<(), InsuranceError> {
        insurance_fund_pool(&env, amount)
    }

    pub fn insurance_collect_premium(
        env: Env,
        payer: Address,
        asset: Address,
        coverage_amount: i128,
    ) -> Result<i128, InsuranceError> {
        insurance_collect_premium(&env, payer, asset, coverage_amount)
    }

    pub fn insurance_submit_claim(
        env: Env,
        claimant: Address,
        asset: Address,
        amount: i128,
    ) -> Result<u64, InsuranceError> {
        insurance_submit_claim(&env, claimant, asset, amount)
    }

    pub fn insurance_evaluate_claim(
        env: Env,
        admin: Address,
        claim_id: u64,
        approve: bool,
    ) -> Result<(), InsuranceError> {
        insurance_evaluate_claim(&env, admin, claim_id, approve)
    }

    pub fn insurance_set_coverage_limit(
        env: Env,
        admin: Address,
        asset: Address,
        limit_bps: i128,
    ) -> Result<(), InsuranceError> {
        insurance_set_coverage_limit(&env, admin, asset, limit_bps)
    }

    pub fn insurance_get_claim(env: Env, claim_id: u64) -> Option<InsuranceClaim> {
        insurance_get_claim(&env, claim_id)
    }

    pub fn insurance_get_premium_rate(env: Env, asset: Address) -> i128 {
        insurance_get_premium_rate(&env, &asset)
    }

    pub fn insurance_get_coverage_limit(env: Env, asset: Address) -> i128 {
        insurance_get_coverage_limit(&env, &asset)
    }

    pub fn insurance_get_analytics(env: Env) -> InsuranceAnalytics {
        insurance_get_analytics(&env)
    }

    pub fn insurance_cancel_claim(
        env: Env,
        claimant: Address,
        claim_id: u64,
    ) -> Result<(), InsuranceError> {
        insurance_cancel_claim(&env, claimant, claim_id)
    }

    pub fn insurance_get_all_claim_ids(env: Env) -> Vec<u64> {
        insurance_get_all_claim_ids(&env)
    }

    pub fn insurance_get_all_claims(env: Env) -> Vec<InsuranceClaim> {
        insurance_get_all_claims(&env)
    }
}
