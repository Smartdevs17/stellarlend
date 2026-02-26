use soroban_sdk::{contract, contractimpl, Address, Env, Map, Symbol, Vec, contracttype, contracterror};

pub mod admin;
pub mod amm;
pub mod analytics;
pub mod borrow;
pub mod bridge;
pub mod config;
pub mod cross_asset;
pub mod deposit;
pub mod errors;
pub mod events;
pub mod flash_loan;
pub mod governance;
pub mod interest_rate;
pub mod liquidate;
pub mod multisig;
pub mod oracle;
pub mod recovery;
pub mod repay;
pub mod reserve;
pub mod risk_management;
pub mod risk_params;
pub mod storage;
pub mod types;
pub mod withdraw;

#[cfg(test)]
mod tests;

use crate::oracle::OracleConfig;
use crate::risk_management::{RiskConfig, RiskManagementError};

use deposit::deposit_collateral;
use repay::repay_debt;

use risk_management::{
    check_emergency_pause, initialize_risk_management, is_emergency_paused, is_operation_paused,
    require_admin, set_pause_switch, set_pause_switches,
};
use risk_params::{
    can_be_liquidated, get_liquidation_incentive_amount, get_max_liquidatable_amount,
    initialize_risk_params, require_min_collateral_ratio, RiskParamsError,
};
use withdraw::withdraw_collateral;

use analytics::{
    generate_protocol_report, generate_user_report, get_recent_activity, get_user_activity_feed,
    AnalyticsError, ProtocolReport, UserReport,
};

use cross_asset::{
    get_asset_config_by_address, get_asset_list, get_user_asset_position,
    get_user_position_summary, initialize_asset, update_asset_config, update_asset_price,
    AssetConfig, AssetKey, AssetPosition, CrossAssetError, UserPositionSummary,
};

use oracle::{
    configure_oracle, get_price, set_fallback_oracle, set_primary_oracle, update_price_feed,
};

use config::{config_backup, config_get, config_restore, config_set, ConfigError};

use flash_loan::{
    configure_flash_loan, execute_flash_loan, repay_flash_loan, set_flash_loan_fee, FlashLoanConfig,
};

#[allow(unused_imports)]
use bridge::{
    bridge_deposit, bridge_withdraw, get_bridge_config, list_bridges, register_bridge,
    set_bridge_fee, BridgeConfig, BridgeError,
};

use liquidate::liquidate;

// AMM types (temporary stubs until stellarlend_amm types are made public)
#[derive(Clone)]
#[contracttype]
pub struct AmmProtocolConfig {
    // Placeholder fields
}

#[derive(Clone)]
#[contracttype]
pub struct SwapParams {
    // Placeholder fields
}

#[derive(Clone, Debug)]
#[contracterror]
pub enum AmmError {
    InvalidParams = 1,
    InsufficientLiquidity = 2,
    SlippageExceeded = 3,
}

pub mod reentrancy;

#[allow(unused_imports)]
use interest_rate::{
    get_current_borrow_rate, get_current_supply_rate, get_current_utilization,
    initialize_interest_rate_config, set_emergency_rate_adjustment, update_interest_rate_config,
    InterestRateError,
};

use storage::{GuardianConfig, DepositDataKey};

// Governance module
use crate::types::{
    GovernanceConfig, MultisigConfig, Proposal, ProposalOutcome, ProposalType, RecoveryRequest,
    VoteInfo, VoteType,
};
// use crate::governance::self;

/// The StellarLend core contract.
///
/// Provides the public API for all lending protocol operations. Each method
/// delegates to the corresponding module implementation and converts internal
/// errors into panics for Soroban's contract-call semantics.

#[contract]
pub struct HelloContract;

#[contractimpl]
impl HelloContract {
    /// Deposit assets into the protocol
    /// Health-check endpoint.
    ///
    /// Returns the string `"Hello"` to verify the contract is deployed and callable.
    pub fn hello(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "Hello")
    }

    /// Initialize the contract with admin address.
    ///
    /// Sets up the risk management system and interest rate model with default parameters.
    /// Must be called before any other operations.
    ///
    /// # Arguments
    /// * `admin` - The admin address
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn initialize(env: Env, admin: Address) -> Result<(), RiskManagementError> {
        // Prevent double initialization
        if crate::admin::has_admin(&env) {
            return Err(RiskManagementError::Unauthorized); // or a specific error
        }

        crate::admin::set_admin(&env, admin.clone(), None)
            .map_err(|_| RiskManagementError::Unauthorized)?;
        initialize_risk_management(&env, admin.clone())?;
        initialize_risk_params(&env).map_err(|_| RiskManagementError::InvalidParameter)?;
        // Initialize interest rate config with default parameters
        initialize_interest_rate_config(&env, admin.clone()).map_err(|e| {
            if e == InterestRateError::AlreadyInitialized {
                RiskManagementError::AlreadyInitialized
            } else {
                RiskManagementError::Unauthorized
            }
        })?;
        Ok(())
    }

    /// Transfer super admin rights
    ///
    /// # Arguments
    /// * `caller` - The current admin
    /// * `new_admin` - The new admin
    pub fn transfer_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), crate::admin::AdminError> {
        crate::admin::set_admin(&env, new_admin, Some(caller))
    }

    /// Grant a role to an address (admin only)
    pub fn grant_role(
        env: Env,
        caller: Address,
        role: Symbol,
        account: Address,
    ) -> Result<(), crate::admin::AdminError> {
        crate::admin::grant_role(&env, caller, role, account)
    }

    /// Revoke a role from an address (admin only)
    pub fn revoke_role(
        env: Env,
        caller: Address,
        role: Symbol,
        account: Address,
    ) -> Result<(), crate::admin::AdminError> {
        crate::admin::revoke_role(&env, caller, role, account)
    }

    /// Deposit collateral into the protocol
    ///
    /// Allows users to deposit assets as collateral in the protocol.
    /// Supports multiple asset types including XLM (native) and token contracts (USDC, etc.).
    ///
    /// # Arguments
    /// * `user` - The address of the user depositing collateral
    /// * `asset` - The address of the asset contract to deposit (None for native XLM)
    /// * `amount` - The amount to deposit
    ///
    /// # Returns
    /// Returns the updated collateral balance for the user
    ///
    /// # Events
    /// Emits the following events:
    /// - `deposit`: Deposit transaction event
    /// - `position_updated`: User position update event
    /// - `analytics_updated`: Analytics update event
    /// - `user_activity_tracked`: User activity tracking event
    pub fn deposit_collateral(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, crate::deposit::DepositError> {
        deposit::deposit_collateral(&env, user, asset, amount)
    }

    /// Set native asset address (admin only). Required before using asset = None for deposit/borrow/repay.
    pub fn set_native_asset_address(
        env: Env,
        caller: Address,
        native_asset: Address,
    ) -> Result<(), deposit::DepositError> {
        deposit::set_native_asset_address(&env, caller, native_asset)
    }

    /// Set risk parameters (admin only)
    ///
    /// Updates risk parameters with validation and change limits.
    ///
    /// # Arguments
    /// * `caller` - The caller address (must be admin)
    /// * `min_collateral_ratio` - Optional new minimum collateral ratio (in basis points)
    /// * `liquidation_threshold` - Optional new liquidation threshold (in basis points)
    /// * `close_factor` - Optional new close factor (in basis points)
    /// * `liquidation_incentive` - Optional new liquidation incentive (in basis points)
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn set_risk_params(
        env: Env,
        caller: Address,
        min_collateral_ratio: Option<i128>,
        liquidation_threshold: Option<i128>,
        close_factor: Option<i128>,
        liquidation_incentive: Option<i128>,
    ) -> Result<(), RiskManagementError> {
        require_admin(&env, &caller)?;
        check_emergency_pause(&env)?;
        risk_params::set_risk_params(
            &env,
            min_collateral_ratio,
            liquidation_threshold,
            close_factor,
            liquidation_incentive,
        )
        .map_err(|e| match e {
            RiskParamsError::ParameterChangeTooLarge => {
                RiskManagementError::ParameterChangeTooLarge
            }
            RiskParamsError::InvalidCollateralRatio => RiskManagementError::InvalidCollateralRatio,
            RiskParamsError::InvalidLiquidationThreshold => {
                RiskManagementError::InvalidLiquidationThreshold
            }
            RiskParamsError::InvalidCloseFactor => RiskManagementError::InvalidCloseFactor,
            RiskParamsError::InvalidLiquidationIncentive => {
                RiskManagementError::InvalidLiquidationIncentive
            }
            _ => RiskManagementError::InvalidParameter,
        })
    }

    pub fn set_guardians(
        env: Env,
        caller: Address,
        guardians: soroban_sdk::Vec<Address>,
        threshold: u32,
    ) -> Result<(), errors::GovernanceError> {
        recovery::set_guardians(&env, caller, guardians, threshold)
    }

    pub fn start_recovery(
        env: Env,
        initiator: Address,
        old_admin: Address,
        new_admin: Address,
    ) -> Result<(), errors::GovernanceError> {
        recovery::start_recovery(&env, initiator, old_admin, new_admin)
    }

    pub fn approve_recovery(
        env: Env,
        approver: Address,
    ) -> Result<(), errors::GovernanceError> {
        recovery::approve_recovery(&env, approver)
    }

    pub fn execute_recovery(
        env: Env,
        executor: Address,
    ) -> Result<(), errors::GovernanceError> {
        recovery::execute_recovery(&env, executor)
    }

    pub fn ms_set_admins(
        env: Env,
        caller: Address,
        admins: soroban_sdk::Vec<Address>,
        threshold: u32,
    ) -> Result<(), errors::GovernanceError> {
        multisig::ms_set_admins(&env, caller, admins, threshold)
    }

    pub fn ms_propose_set_min_cr(
        env: Env,
        proposer: Address,
        new_ratio: i128,
    ) -> Result<u64, errors::GovernanceError> {
        multisig::ms_propose_set_min_cr(&env, proposer, new_ratio)
    }

    pub fn ms_approve(
        env: Env,
        approver: Address,
        proposal_id: u64,
    ) -> Result<(), errors::GovernanceError> {
        multisig::ms_approve(&env, approver, proposal_id)
    }

    pub fn ms_execute(
        env: Env,
        executor: Address,
        proposal_id: u64,
    ) -> Result<(), errors::GovernanceError> {
        multisig::ms_execute(&env, executor, proposal_id)
    }

    /// Set pause switch for an operation (admin only)
    ///
    /// # Arguments
    /// * `caller` - The caller address (must be admin)
    /// * `operation` - The operation symbol (e.g., "pause_deposit", "pause_borrow")
    /// * `paused` - Whether to pause (true) or unpause (false)
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn set_pause_switch(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, crate::borrow::BorrowError> {
        borrow::borrow_asset(&env, user, asset, amount)
    }

    /// Repay borrowed assets
    pub fn repay_debt(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<(i128, i128, i128), crate::repay::RepayError> {
        repay::repay_debt(&env, user, asset, amount)
    }

    /// Liquidate an undercollateralized position
    pub fn liquidate(env: Env, caller: Address, paused: bool) -> Result<(), RiskManagementError> {
        risk_management::set_emergency_pause(&env, caller, paused)
    }

    /// Get current risk configuration
    ///
    /// # Returns
    /// Returns the current risk configuration or None if not initialized
    pub fn get_risk_config(env: Env) -> Option<RiskConfig> {
        risk_management::get_risk_config(&env)
    }

    /// Get minimum collateral ratio
    ///
    /// # Returns
    /// Returns the minimum collateral ratio in basis points
    pub fn get_min_collateral_ratio(env: Env) -> Result<i128, RiskManagementError> {
        risk_params::get_min_collateral_ratio(&env)
            .map_err(|_| RiskManagementError::InvalidParameter)
    }

    /// Get liquidation threshold
    ///
    /// # Returns
    /// Returns the liquidation threshold in basis points
    pub fn get_liquidation_threshold(env: Env) -> Result<i128, RiskManagementError> {
        risk_params::get_liquidation_threshold(&env)
            .map_err(|_| RiskManagementError::InvalidParameter)
    }

    /// Get close factor
    ///
    /// # Returns
    /// Returns the close factor in basis points
    pub fn get_close_factor(env: Env) -> Result<i128, RiskManagementError> {
        risk_params::get_close_factor(&env).map_err(|_| RiskManagementError::InvalidParameter)
    }

    /// Get liquidation incentive
    ///
    /// # Returns
    /// Returns the liquidation incentive in basis points
    pub fn get_liquidation_incentive(env: Env) -> Result<i128, RiskManagementError> {
        risk_params::get_liquidation_incentive(&env)
            .map_err(|_| RiskManagementError::InvalidParameter)
    }

    /// Get current borrow rate (in basis points)
    pub fn get_borrow_rate(env: Env) -> i128 {
        interest_rate::calculate_borrow_rate(&env).unwrap_or(0)
    }

    /// Get current supply rate (in basis points)
    pub fn get_supply_rate(env: Env) -> i128 {
        interest_rate::calculate_supply_rate(&env).unwrap_or(0)
    }

    /// Update interest rate model configuration (admin only)
    #[allow(clippy::too_many_arguments)]
    pub fn update_interest_rate_config(
        env: Env,
        admin: Address,
        base_rate: Option<i128>,
        kink: Option<i128>,
        multiplier: Option<i128>,
        jump_multiplier: Option<i128>,
        rate_floor: Option<i128>,
        rate_ceiling: Option<i128>,
        spread: Option<i128>,
    ) -> Result<(), RiskManagementError> {
        require_admin(&env, &admin)?;
        // Stub implementation - would update interest rate config in real implementation
        Ok(())
    }

    /// Check if position can be liquidated
    ///
    /// # Arguments
    /// * `collateral_value` - Total collateral value (in base units)
    /// * `debt_value` - Total debt value (in base units)
    ///
    /// # Returns
    /// Returns true if position can be liquidated
    pub fn can_be_liquidated(
        env: Env,
        collateral_value: i128,
        debt_value: i128,
    ) -> Result<bool, RiskManagementError> {
        can_be_liquidated(&env, collateral_value, debt_value)
            .map_err(|_| RiskManagementError::InvalidParameter)
    }

    /// Manual emergency interest rate adjustment (admin only)
    pub fn set_emergency_rate_adjustment(
        env: Env,
        debt_value: i128,
    ) -> Result<i128, RiskManagementError> {
        get_max_liquidatable_amount(&env, debt_value).map_err(|_| RiskManagementError::Overflow)
    }

    /// Calculate liquidation incentive amount
    ///
    /// # Arguments
    /// * `liquidated_amount` - Amount being liquidated (in base units)
    ///
    /// # Returns
    /// Liquidation incentive amount
    pub fn get_liquidation_incentive_amount(
        env: Env,
        liquidated_amount: i128,
    ) -> Result<i128, RiskManagementError> {
        get_liquidation_incentive_amount(&env, liquidated_amount)
            .map_err(|_| RiskManagementError::Overflow)
    }

    /// Refresh analytics for a user
    pub fn refresh_user_analytics(_env: Env, _user: Address) -> Result<(), RiskManagementError> {
        Ok(())
    }

    /// Claim accumulated protocol reserves (admin only)
    pub fn claim_reserves(
        env: Env,
        caller: Address,
        asset: Option<Address>,
        to: Address,
        amount: i128,
    ) -> Result<(), RiskManagementError> {
        require_admin(&env, &caller)?;

        let reserve_key = DepositDataKey::ProtocolReserve(asset.clone());
        let mut reserve_balance = env
            .storage()
            .persistent()
            .get::<DepositDataKey, i128>(&reserve_key)
            .unwrap_or(0);

        if amount > reserve_balance {
            return Err(RiskManagementError::InvalidParameter);
        }

        if let Some(_asset_addr) = asset {
            #[cfg(not(test))]
            {
                let token_client = soroban_sdk::token::Client::new(&env, &_asset_addr);
                token_client.transfer(&env.current_contract_address(), &to, &amount);
            }
        }

        reserve_balance -= amount;
        env.storage()
            .persistent()
            .set(&reserve_key, &reserve_balance);
        Ok(())
    }

    /// Get current protocol reserve balance for an asset
    pub fn get_reserve_balance(env: Env, asset: Option<Address>) -> i128 {
        let reserve_key = DepositDataKey::ProtocolReserve(asset);
        env.storage()
            .persistent()
            .get::<DepositDataKey, i128>(&reserve_key)
            .unwrap_or(0)
    }

    /// Generate a comprehensive protocol report.
    ///
    /// Aggregates TVL, utilization, average borrow rate, and user/transaction counts
    /// into a single [`ProtocolReport`] snapshot.
    ///
    /// # Returns
    /// A `ProtocolReport` containing current protocol metrics and timestamp.
    ///
    /// # Errors
    /// Returns `AnalyticsError` if protocol data is not initialized or computation overflows.
    pub fn get_protocol_report(env: Env) -> Result<ProtocolReport, AnalyticsError> {
        generate_protocol_report(&env)
    }

    /// Generate a comprehensive report for a specific user.
    ///
    /// Includes the user's position, health factor, risk level, activity history,
    /// and cumulative transaction metrics.
    ///
    /// # Arguments
    /// * `user` - The address of the user to report on
    ///
    /// # Returns
    /// A `UserReport` with the user's metrics, position, and recent activities.
    ///
    /// # Errors
    /// Returns `AnalyticsError::DataNotFound` if the user has no recorded activity.
    pub fn get_user_report(env: Env, user: Address) -> Result<UserReport, AnalyticsError> {
        generate_user_report(&env, &user)
    }

    /// Retrieve recent protocol activity entries.
    ///
    /// Returns a paginated list of the most recent protocol activities in
    /// reverse chronological order.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of entries to return
    /// * `offset` - Number of entries to skip from the most recent
    ///
    /// # Returns
    /// A vector of `ActivityEntry` records.
    pub fn get_recent_activity(
        env: Env,
        limit: u32,
        offset: u32,
    ) -> Result<soroban_sdk::Vec<analytics::ActivityEntry>, AnalyticsError> {
        get_recent_activity(&env, limit, offset)
    }

    /// Retrieve activity entries for a specific user.
    ///
    /// Returns a paginated list of the user's activities in reverse
    /// chronological order.
    ///
    /// # Arguments
    /// * `user` - The address of the user
    /// * `limit` - Maximum number of entries to return
    /// * `offset` - Number of entries to skip from the most recent
    ///
    /// # Returns
    /// A vector of `ActivityEntry` records for the specified user.
    pub fn get_user_activity(
        env: Env,
        user: Address,
        limit: u32,
        offset: u32,
    ) -> Result<soroban_sdk::Vec<analytics::ActivityEntry>, AnalyticsError> {
        get_user_activity_feed(&env, &user, limit, offset)
    }

    /// Update price feed from oracle
    pub fn update_price_feed(
        env: Env,
        caller: Address,
        asset: Address,
        price: i128,
        decimals: u32,
        oracle: Address,
    ) -> i128 {
        oracle::update_price_feed(&env, caller, asset, price, decimals, oracle)
            .expect("Oracle error")
    }

    /// Get current price for an asset
    pub fn get_price(env: Env, asset: Address) -> i128 {
        oracle::get_price(&env, &asset).expect("Oracle error")
    }

    /// Configure oracle parameters (admin only)
    pub fn configure_oracle(env: Env, caller: Address, config: OracleConfig) {
        oracle::configure_oracle(&env, caller, config).expect("Oracle error")
    }

    /// Set primary oracle for an asset (admin only)
    ///
    /// # Arguments
    /// * `caller` - The caller address (must be admin)
    /// * `asset` - The asset address
    /// * `primary_oracle` - The primary oracle address
    pub fn set_primary_oracle(env: Env, caller: Address, asset: Address, primary_oracle: Address) {
        set_primary_oracle(&env, caller, asset, primary_oracle)
            .unwrap_or_else(|e| panic!("Oracle error: {:?}", e))
    }

    /// Set fallback oracle for an asset (admin only)
    pub fn set_fallback_oracle(
        env: Env,
        caller: Address,
        asset: Address,
        fallback_oracle: Address,
    ) {
        oracle::set_fallback_oracle(&env, caller, asset, fallback_oracle).expect("Oracle error")
    }

    /// Initialize AMM settings (admin only)
    pub fn initialize_amm(
        env: Env,
        admin: Address,
        default_slippage: i128,
        max_slippage: i128,
        auto_swap_threshold: i128,
    ) -> Result<(), AmmError> {
        // Stub implementation
        require_admin(&env, &admin).map_err(|_| AmmError::InvalidParams)?;
        Ok(())
    }

    /// Set AMM pool configuration (admin only)
    pub fn set_amm_pool(
        env: Env,
        admin: Address,
        protocol_config: AmmProtocolConfig,
    ) -> Result<(), AmmError> {
        // Stub implementation
        require_admin(&env, &admin).map_err(|_| AmmError::InvalidParams)?;
        Ok(())
    }

    /// Execute swap through AMM
    pub fn amm_swap(env: Env, user: Address, params: SwapParams) -> Result<i128, AmmError> {
        // Stub implementation
        Ok(0)
    }

    // ============================================================================
    // Governance Entrypoints
    // ============================================================================

    /// Initialize governance module
    ///
    /// Sets up the governance system with voting token and parameters.
    /// Must be called after contract initialization.
    ///
    /// # Arguments
    /// * `admin` - The admin address
    /// * `vote_token` - Token contract address used for voting
    /// * `voting_period` - Optional voting duration in seconds
    /// * `execution_delay` - Optional delay before execution in seconds
    /// * `quorum_bps` - Optional quorum requirement in basis points
    /// * `proposal_threshold` - Optional minimum tokens to create proposal
    /// * `timelock_duration` - Optional timelock duration in seconds
    /// * `default_voting_threshold` - Optional default voting threshold in basis points
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_initialize(
        env: Env,
        admin: Address,
        vote_token: Address,
        voting_period: Option<u64>,
        execution_delay: Option<u64>,
        quorum_bps: Option<u32>,
        proposal_threshold: Option<i128>,
        timelock_duration: Option<u64>,
        default_voting_threshold: Option<i128>,
    ) -> Result<(), errors::GovernanceError> {
        governance::initialize(
            &env,
            admin,
            vote_token,
            voting_period,
            execution_delay,
            quorum_bps,
            proposal_threshold,
            timelock_duration,
            default_voting_threshold,
        )
    }

    /// Create a new governance proposal
    ///
    /// # Arguments
    /// * `proposer` - Address creating the proposal
    /// * `proposal_type` - Type of proposal (parameter change, pause, etc.)
    /// * `description` - Description of the proposal
    /// * `voting_threshold` - Optional custom voting threshold
    ///
    /// # Returns
    /// Returns the new proposal ID
    pub fn gov_create_proposal(
        env: Env,
        proposer: Address,
        proposal_type: ProposalType,
        description: String,
        voting_threshold: Option<i128>,
    ) -> Result<u64, errors::GovernanceError> {
        let soroban_desc = soroban_sdk::String::from_str(&env, &description.to_string());
        governance::create_proposal(&env, proposer, proposal_type, soroban_desc, voting_threshold)
    }

    /// Cast a vote on a proposal
    ///
    /// # Arguments
    /// * `voter` - Address casting the vote
    /// * `proposal_id` - ID of the proposal to vote on
    /// * `vote_type` - Vote choice (For, Against, Abstain)
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_vote(
        env: Env,
        voter: Address,
        proposal_id: u64,
        vote_type: VoteType,
    ) -> Result<(), errors::GovernanceError> {
        governance::vote(&env, voter, proposal_id, vote_type)
    }

    /// Queue a successful proposal for execution
    ///
    /// # Arguments
    /// * `caller` - Address queuing the proposal
    /// * `proposal_id` - ID of the proposal to queue
    ///
    /// # Returns
    /// Returns the proposal outcome
    pub fn gov_queue_proposal(
        env: Env,
        caller: Address,
        proposal_id: u64,
    ) -> Result<ProposalOutcome, errors::GovernanceError> {
        governance::queue_proposal(&env, caller, proposal_id)
    }

    /// Execute a queued proposal
    ///
    /// # Arguments
    /// * `executor` - Address executing the proposal
    /// * `proposal_id` - ID of the proposal to execute
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_execute_proposal(
        env: Env,
        executor: Address,
        proposal_id: u64,
    ) -> Result<(), errors::GovernanceError> {
        governance::execute_proposal(&env, executor, proposal_id)
    }

    /// Cancel a proposal
    ///
    /// Only proposer or admin can cancel.
    ///
    /// # Arguments
    /// * `caller` - Address cancelling the proposal
    /// * `proposal_id` - ID of the proposal to cancel
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_cancel_proposal(
        env: Env,
        caller: Address,
        proposal_id: u64,
    ) -> Result<(), errors::GovernanceError> {
        governance::cancel_proposal(&env, caller, proposal_id)
    }

    /// Approve a proposal as multisig admin
    ///
    /// # Arguments
    /// * `approver` - Admin address approving the proposal
    /// * `proposal_id` - ID of the proposal to approve
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_approve_proposal(
        env: Env,
        approver: Address,
        proposal_id: u64,
    ) -> Result<(), errors::GovernanceError> {
        governance::approve_proposal(&env, approver, proposal_id)
    }

    /// Set multisig configuration
    ///
    /// # Arguments
    /// * `caller` - Caller address (must be admin)
    /// * `admins` - Vector of admin addresses
    /// * `threshold` - Number of approvals required
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_set_multisig_config(
        env: Env,
        caller: Address,
        admins: Vec<Address>,
        threshold: u32,
    ) -> Result<(), errors::GovernanceError> {
        governance::set_multisig_config(&env, caller, admins, threshold)
    }

    /// Add a guardian
    ///
    /// # Arguments
    /// * `caller` - Caller address (must be admin)
    /// * `guardian` - Guardian address to add
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_add_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), errors::GovernanceError> {
        governance::add_guardian(&env, caller, guardian)
    }

    /// Remove a guardian
    ///
    /// # Arguments
    /// * `caller` - Caller address (must be admin)
    /// * `guardian` - Guardian address to remove
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_remove_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), errors::GovernanceError> {
        governance::remove_guardian(&env, caller, guardian)
    }

    /// Set guardian threshold
    ///
    /// # Arguments
    /// * `caller` - Caller address (must be admin)
    /// * `threshold` - Number of guardian approvals required for recovery
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_set_guardian_threshold(
        env: Env,
        caller: Address,
        threshold: u32,
    ) -> Result<(), errors::GovernanceError> {
        governance::set_guardian_threshold(&env, caller, threshold)
    }

    /// Start recovery process
    ///
    /// # Arguments
    /// * `initiator` - Guardian initiating recovery
    /// * `old_admin` - Current admin to replace
    /// * `new_admin` - New admin address
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_start_recovery(
        env: Env,
        initiator: Address,
        old_admin: Address,
        new_admin: Address,
    ) -> Result<(), errors::GovernanceError> {
        governance::start_recovery(&env, initiator, old_admin, new_admin)
    }

    /// Approve recovery
    ///
    /// # Arguments
    /// * `approver` - Guardian approving recovery
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_approve_recovery(
        env: Env,
        approver: Address,
    ) -> Result<(), errors::GovernanceError> {
        governance::approve_recovery(&env, approver)
    }

    /// Execute recovery
    ///
    /// # Arguments
    /// * `executor` - Address executing recovery
    ///
    /// # Returns
    /// Returns Ok(()) on success
    pub fn gov_execute_recovery(
        env: Env,
        user: Address,
    ) -> Result<UserPositionSummary, CrossAssetError> {
        get_user_position_summary(&env, &user)
    }

    // ============================================================================
    pub fn initialize_ca(env: Env, admin: Address) -> Result<(), CrossAssetError> {
        cross_asset::initialize(&env, admin)
    }

    /// Initialize asset configuration
    pub fn initialize_asset(
        env: Env,
        asset: Option<Address>,
        config: AssetConfig,
    ) -> Result<(), CrossAssetError> {
        cross_asset::initialize_asset(&env, asset, config)
    }

    /// Update asset configuration
    #[allow(clippy::too_many_arguments)]
    pub fn update_asset_config(
        env: Env,
        asset: Address,
        collateral_factor: Option<i128>,
        liquidation_threshold: Option<i128>,
        max_supply: Option<i128>,
        max_borrow: Option<i128>,
        can_collateralize: Option<bool>,
        can_borrow: Option<bool>,
    ) -> Result<(), CrossAssetError> {
        cross_asset::update_asset_config(
            &env,
            Some(asset),
            collateral_factor,
            liquidation_threshold,
            max_supply,
            max_borrow,
            can_collateralize,
            can_borrow,
        )
    }

    /// Update asset price
    pub fn update_asset_price(
        env: Env,
        asset: Option<Address>,
        price: i128,
    ) -> Result<(), CrossAssetError> {
        cross_asset::update_asset_price(&env, asset, price)
    }

    /// Deposit collateral for specific asset
    pub fn ca_deposit_collateral(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<AssetPosition, CrossAssetError> {
        cross_asset::cross_asset_deposit(&env, user, asset, amount)
    }

    /// Withdraw collateral for specific asset
    pub fn ca_withdraw_collateral(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<AssetPosition, CrossAssetError> {
        cross_asset::cross_asset_withdraw(&env, user, asset, amount)
    }

    /// Borrow specific asset
    pub fn ca_borrow_asset(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<AssetPosition, CrossAssetError> {
        cross_asset::cross_asset_borrow(&env, user, asset, amount)
    }

    /// Repay debt for specific asset
    pub fn ca_repay_debt(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<AssetPosition, CrossAssetError> {
        cross_asset::cross_asset_repay(&env, user, asset, amount)
    }

    /// Get user's position for specific asset
    pub fn get_user_asset_position(
        env: Env,
        user: Address,
        asset: Option<Address>,
    ) -> AssetPosition {
        cross_asset::get_user_asset_position(&env, &user, asset)
    }

    /// Get user's unified position summary across all assets
    pub fn get_user_position_summary(
        env: Env,
        user: Address,
    ) -> Result<UserPositionSummary, CrossAssetError> {
        cross_asset::get_user_position_summary(&env, &user)
    }

    /// Get list of all configured assets
    pub fn get_asset_list(env: Env) -> soroban_sdk::Vec<AssetKey> {
        cross_asset::get_asset_list(&env)
    }

    /// Get configuration for specific asset
    pub fn get_asset_config(
        env: Env,
        asset: Option<Address>,
    ) -> Result<AssetConfig, CrossAssetError> {
        cross_asset::get_asset_config_by_address(&env, asset)
    }

    // ============================================================================
    // Governance Query Functions
    // ============================================================================

    /// Get proposal by ID
    pub fn gov_get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        governance::get_proposal(&env, proposal_id)
    }

    /// Get vote information
    pub fn gov_get_vote(env: Env, proposal_id: u64, voter: Address) -> Option<VoteInfo> {
        governance::get_vote(&env, proposal_id, voter)
    }

    /// Get governance configuration
    pub fn gov_get_config(env: Env) -> Option<GovernanceConfig> {
        governance::get_config(&env)
    }

    /// Get governance admin
    pub fn gov_get_admin(env: Env) -> Option<Address> {
        governance::get_admin(&env)
    }

    /// Get multisig configuration
    pub fn gov_get_multisig_config(env: Env) -> Option<MultisigConfig> {
        governance::get_multisig_config(&env)
    }

    /// Get guardian configuration
    pub fn gov_get_guardian_config(env: Env) -> Option<GuardianConfig> {
        governance::get_guardian_config(&env)
    }

    /// Get proposal approvals
    pub fn gov_get_proposal_approvals(env: Env, proposal_id: u64) -> Option<Vec<Address>> {
        governance::get_proposal_approvals(&env, proposal_id)
    }

    /// Get current recovery request
    pub fn gov_get_recovery_request(env: Env) -> Option<RecoveryRequest> {
        governance::get_recovery_request(&env)
    }

    /// Get recovery approvals
    pub fn gov_get_recovery_approvals(env: Env) -> Option<Vec<Address>> {
        governance::get_recovery_approvals(&env)
    }

    /// Get paginated list of proposals
    pub fn gov_get_proposals(env: Env, start_id: u64, limit: u32) -> Vec<Proposal> {
        governance::get_proposals(&env, start_id, limit)
    }

    /// Check if an address can vote on a proposal
    pub fn gov_can_vote(env: Env, voter: Address, proposal_id: u64) -> bool {
        governance::can_vote(&env, voter, proposal_id)
    }

    // --- Bridge ---

    /// Register a new bridge (admin only)
    pub fn register_bridge(
        env: Env,
        caller: Address,
        network_id: u32,
        bridge: Address,
        fee_bps: i128,
    ) -> Result<(), BridgeError> {
        bridge::register_bridge(&env, caller, network_id, bridge, fee_bps)
    }

    /// Set fee for a bridge (admin only)
    pub fn set_bridge_fee(
        env: Env,
        caller: Address,
        network_id: u32,
        fee_bps: i128,
    ) -> Result<(), BridgeError> {
        bridge::set_bridge_fee(&env, caller, network_id, fee_bps)
    }

    /// List all registered bridges
    pub fn list_bridges(env: Env) -> Map<u32, BridgeConfig> {
        bridge::list_bridges(&env)
    }

    /// Get configuration for a bridge by network id
    pub fn get_bridge_config(env: Env, network_id: u32) -> Result<BridgeConfig, BridgeError> {
        bridge::get_bridge_config(&env, network_id)
    }

    /// Deposit into protocol via a bridge
    pub fn bridge_deposit(
        env: Env,
        user: Address,
        network_id: u32,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, BridgeError> {
        bridge::bridge_deposit(&env, user, network_id, asset, amount)
    }

    /// Withdraw from protocol via a bridge
    pub fn bridge_withdraw(
        env: Env,
        user: Address,
        network_id: u32,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, BridgeError> {
        bridge::bridge_withdraw(&env, user, network_id, asset, amount)
    }
}

#[cfg(test)]
mod test_reentrancy;

#[cfg(test)]
mod test_zero_amount;

#[cfg(test)]
mod flash_loan_test;
