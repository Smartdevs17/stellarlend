//! State-modifying operations for the lending pool
//!
//! This module contains all functions that modify contract state.
//! All state changes are protected by:
//! - Reentrancy guards
//! - Authorization checks
//! - Circuit breaker checks
//! - Input validation
//!
//! ## State Changes
//! - User positions and balances
//! - Reserve management
//! - Governance state
//! - Protocol parameters

use soroban_sdk::{Address, Env, String, Vec};
use crate::errors::LendingError;
use crate::types::*;

/// State-modifying operations namespace
pub struct OperationsModule;

impl OperationsModule {
    /// Deposit collateral to the protocol
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The depositing user
    /// * `asset` - Asset being deposited
    /// * `amount` - Amount to deposit
    pub fn deposit(
        env: &Env,
        user: &Address,
        asset: &Address,
        amount: i128,
    ) -> Result<(), LendingError> {
        // Check circuit breaker
        crate::circuit_breaker::check_deposit_allowed(env)?;

        // Check reentrancy
        crate::reentrancy::check_and_set_guard(env)?;

        // Delegate to deposit module
        let result = crate::deposit::execute_deposit(env, user, asset, amount);

        // Clear reentrancy guard
        crate::reentrancy::clear_guard(env);

        result
    }

    /// Borrow from the protocol
    pub fn borrow(
        env: &Env,
        user: &Address,
        asset: &Address,
        amount: i128,
    ) -> Result<(), LendingError> {
        crate::circuit_breaker::check_borrow_allowed(env)?;
        crate::reentrancy::check_and_set_guard(env)?;

        let result = crate::borrow::execute_borrow(env, user, asset, amount);

        crate::reentrancy::clear_guard(env);
        result
    }

    /// Repay a borrow position
    pub fn repay(
        env: &Env,
        user: &Address,
        asset: &Address,
        amount: i128,
    ) -> Result<i128, LendingError> {
        crate::circuit_breaker::check_repay_allowed(env)?;
        crate::reentrancy::check_and_set_guard(env)?;

        let result = crate::repay::execute_repay(env, user, asset, amount);

        crate::reentrancy::clear_guard(env);
        result
    }

    /// Withdraw collateral from the protocol
    pub fn withdraw(
        env: &Env,
        user: &Address,
        asset: &Address,
        amount: i128,
    ) -> Result<i128, LendingError> {
        crate::circuit_breaker::check_withdrawal_allowed(env)?;
        crate::reentrancy::check_and_set_guard(env)?;

        let result = crate::withdraw::execute_withdraw(env, user, asset, amount);

        crate::reentrancy::clear_guard(env);
        result
    }

    /// Liquidate an underwater position
    pub fn liquidate(
        env: &Env,
        liquidator: &Address,
        borrower: &Address,
        collateral_asset: &Address,
        debt_asset: &Address,
        max_debt_to_cover: i128,
    ) -> Result<i128, LendingError> {
        crate::circuit_breaker::check_liquidation_allowed(env)?;
        crate::reentrancy::check_and_set_guard(env)?;

        let result = crate::liquidate::execute_liquidation(
            env,
            liquidator,
            borrower,
            collateral_asset,
            debt_asset,
            max_debt_to_cover,
        );

        crate::reentrancy::clear_guard(env);
        result
    }

    /// Flash loan borrow and repay
    pub fn flash_loan(
        env: &Env,
        receiver: &Address,
        asset: &Address,
        amount: i128,
        params: Vec<u8>,
    ) -> Result<(), LendingError> {
        crate::reentrancy::check_and_set_guard(env)?;

        let result = crate::flash_loan::execute_flash_loan(env, receiver, asset, amount, params);

        crate::reentrancy::clear_guard(env);
        result
    }

    /// Update oracle price feed
    pub fn update_oracle_price(
        env: &Env,
        caller: &Address,
        asset: &Address,
        price: i128,
        decimals: u32,
        oracle_address: &Address,
    ) -> Result<i128, crate::oracle::OracleError> {
        crate::oracle::update_price_feed(env, caller.clone(), asset.clone(), price, decimals, oracle_address.clone())
    }

    /// Create a governance proposal
    pub fn create_proposal(
        env: &Env,
        proposer: &Address,
        proposal_type: ProposalType,
        description: String,
        voting_threshold: Option<i128>,
    ) -> Result<u64, LendingError> {
        crate::governance::create_proposal(env, proposer.clone(), proposal_type, description, voting_threshold)
    }

    /// Vote on a proposal
    pub fn vote(
        env: &Env,
        voter: &Address,
        proposal_id: u64,
        vote_type: VoteType,
    ) -> Result<(), LendingError> {
        crate::governance::vote(env, voter.clone(), proposal_id, vote_type)
    }

    /// Queue a proposal for execution
    pub fn queue_proposal(
        env: &Env,
        caller: &Address,
        proposal_id: u64,
    ) -> Result<ProposalOutcome, LendingError> {
        crate::governance::queue_proposal(env, caller.clone(), proposal_id)
    }

    /// Execute a queued proposal
    pub fn execute_proposal(
        env: &Env,
        executor: &Address,
        proposal_id: u64,
    ) -> Result<(), LendingError> {
        crate::governance::execute_proposal(env, executor.clone(), proposal_id)
    }

    /// Cancel a proposal
    pub fn cancel_proposal(
        env: &Env,
        caller: &Address,
        proposal_id: u64,
    ) -> Result<(), LendingError> {
        crate::governance::cancel_proposal(env, caller.clone(), proposal_id)
    }

    /// Update risk parameters
    pub fn update_risk_parameters(
        env: &Env,
        caller: &Address,
        asset: &Address,
        ltv: u32,
        liquidation_threshold: u32,
    ) -> Result<(), LendingError> {
        crate::risk_params::update_parameters(env, caller, asset, ltv, liquidation_threshold)
    }

    /// Pause the protocol
    pub fn pause_protocol(
        env: &Env,
        caller: &Address,
        pause_duration: u64,
    ) -> Result<(), LendingError> {
        crate::circuit_breaker::pause(env, caller, pause_duration)
    }

    /// Resume the protocol
    pub fn resume_protocol(
        env: &Env,
        caller: &Address,
    ) -> Result<(), LendingError> {
        crate::circuit_breaker::resume(env, caller)
    }

    /// Trigger emergency shutdown
    pub fn emergency_shutdown(
        env: &Env,
        caller: &Address,
    ) -> Result<(), LendingError> {
        crate::recovery::trigger_emergency_shutdown(env, caller)
    }

    /// Migrate user to new contract version
    pub fn migrate_user_position(
        env: &Env,
        user: &Address,
        new_contract: &Address,
    ) -> Result<(), LendingError> {
        crate::recovery::migrate_position(env, user, new_contract)
    }

    /// Set interest rate model
    pub fn set_interest_rate_model(
        env: &Env,
        caller: &Address,
        asset: &Address,
        base_rate: i128,
        slope1: i128,
        slope2: i128,
        optimal_utilization: i128,
    ) -> Result<(), LendingError> {
        crate::interest_rate::set_model(
            env,
            caller,
            asset,
            base_rate,
            slope1,
            slope2,
            optimal_utilization,
        )
    }

    /// Rebalance reserves
    pub fn rebalance_reserves(
        env: &Env,
        caller: &Address,
    ) -> Result<(), LendingError> {
        crate::rebalancing::execute_rebalancing(env, caller)
    }

    /// Initiate multi-asset rebalancing
    pub fn rebalance_multi_collateral(
        env: &Env,
        caller: &Address,
        assets: Vec<Address>,
    ) -> Result<(), LendingError> {
        crate::multi_collateral::rebalance(env, caller, assets)
    }
}
