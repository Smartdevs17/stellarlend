#![allow(unused_imports)]

use soroban_sdk::{contract, contractimpl, Address, Env, Map, String, Symbol, Val, Vec};

// ─── Module declarations ─────────────────────────────────────────────────────

pub mod analytics;
pub mod borrow;
pub mod cross_asset;
pub mod deposit;
pub mod events;
pub mod flash_loan;
pub mod governance;
pub mod interest_rate;
pub mod liquidate;
pub mod oracle;
pub mod repay;
pub mod risk_management;
pub mod withdraw;
pub mod recovery;
pub mod multisig;
pub mod types;
pub mod storage;
pub mod reentrancy;

mod admin;
mod errors;
mod reserve;
mod risk_params;
mod config;
mod bridge;

#[cfg(test)]
mod tests;

// ─── Re-exports used by the impl block ───────────────────────────────────────

use crate::deposit::{AssetParams, DepositDataKey, ProtocolAnalytics};
use crate::oracle::OracleConfig;
use crate::risk_management::{RiskConfig, RiskManagementError};
use crate::types::{
    GovernanceConfig, GuardianConfig, InterestRateParams, MultisigConfig, Proposal, ProposalOutcome,
    ProposalStatus, ProposalType, RecoveryRequest, VoteInfo, VoteType,
};
use crate::errors::GovernanceError;

use analytics::{ProtocolReport, UserReport};
use bridge::{BridgeConfig, BridgeError};
use config::ConfigError;
use cross_asset::{AssetConfig, AssetKey, AssetPosition, CrossAssetError, UserPositionSummary};
use flash_loan::FlashLoanConfig;
use interest_rate::InterestRateError;
use risk_params::RiskParamsError;

// ─── Admin helper ─────────────────────────────────────────────────────────────

/// Require that `caller` is the stored admin; panics via `?` on failure.
fn require_admin(env: &Env, caller: &Address) -> Result<(), RiskManagementError> {
    caller.require_auth();
    let admin_key = DepositDataKey::Admin;
    let admin = env
        .storage()
        .persistent()
        .get::<DepositDataKey, Address>(&admin_key)
        .ok_or(RiskManagementError::Unauthorized)?;

    if caller != &admin {
        return Err(RiskManagementError::Unauthorized);
    }
    Ok(())
}

/// The StellarLend core contract.
///
/// Every public method delegates to the appropriate module and converts
/// internal errors to Soroban's contract-call semantics.

#[contract]
pub struct HelloContract;

#[contractimpl]
impl HelloContract {
    pub fn hello(env: Env) -> String {
        String::from_str(env, "Hello")
    }

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
    ) -> Result<(), GovernanceError> {
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

    pub fn initialize(env: Env, admin: Address) -> Result<(), RiskManagementError> {
        // Prevent double initialization
        if crate::admin::has_admin(&env) {
            return Err(RiskManagementError::AlreadyInitialized);
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

    // --- Admin & Roles ---

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

    // --- Lending Operations ---

    pub fn deposit_collateral(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, crate::deposit::DepositError> {
        crate::deposit::deposit_collateral(&env, user, asset, amount)
    }

    pub fn withdraw_collateral(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, crate::withdraw::WithdrawError> {
        crate::withdraw::withdraw_collateral(&env, user, asset, amount)
    }

    pub fn borrow_asset(
        env: Env,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, crate::borrow::BorrowError> {
        crate::borrow::borrow_asset(&env, user, asset, amount)
    }

    pub fn repay_debt(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<(i128, i128, i128), crate::repay::RepayError> {
        crate::repay::repay_debt(&env, user, asset, amount)
    }

    pub fn liquidate(
        env: Env,
        liquidator: Address,
        user: Address,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, crate::liquidate::LiquidationError> {
        crate::liquidate::liquidate(&env, liquidator, user, asset, amount)
    }

    // --- Governance ---

    pub fn gov_create_proposal(env: Env, creator: Address, proposal_type: ProposalType, description: String) -> Result<u64, GovernanceError> {
        governance::create_proposal(&env, creator, proposal_type, description)
    }

    pub fn gov_approve_proposal(env: Env, approver: Address, proposal_id: u64) -> Result<(), GovernanceError> {
        governance::approve_proposal(&env, approver, proposal_id)
    }

    pub fn gov_queue_proposal(env: Env, initiator: Address, proposal_id: u64) -> Result<(), GovernanceError> {
        governance::queue_proposal(&env, initiator, proposal_id)
    }

    pub fn gov_execute_proposal(env: Env, executor: Address, proposal_id: u64) -> Result<(), GovernanceError> {
        governance::execute_proposal(&env, executor, proposal_id)
    }

    pub fn gov_cancel_proposal(env: Env, initiator: Address, proposal_id: u64) -> Result<(), GovernanceError> {
        governance::cancel_proposal(&env, initiator, proposal_id)
    }

    pub fn gov_get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        governance::get_proposal(&env, proposal_id)
    }

    pub fn gov_get_admin(env: Env) -> Option<Address> {
        governance::get_admin(&env)
    }

    pub fn gov_get_config(env: Env) -> Option<GovernanceConfig> {
        governance::get_config(&env)
    }

    pub fn gov_add_guardian(env: Env, caller: Address, guardian: Address) -> Result<(), GovernanceError> {
        governance::add_guardian(&env, caller, guardian)
    }

    pub fn gov_remove_guardian(env: Env, caller: Address, guardian: Address) -> Result<(), GovernanceError> {
        governance::remove_guardian(&env, caller, guardian)
    }

    pub fn gov_set_guardian_threshold(env: Env, caller: Address, threshold: u32) -> Result<(), GovernanceError> {
        governance::set_guardian_threshold(&env, caller, threshold)
    }

    pub fn gov_set_multisig_config(env: Env, caller: Address, admins: Vec<Address>, threshold: u32) -> Result<(), GovernanceError> {
        governance::set_multisig_config(&env, caller, admins, threshold)
    }

    pub fn gov_get_multisig_config(env: Env) -> Option<MultisigConfig> {
        governance::get_multisig_config(&env)
    }

    pub fn gov_get_guardian_config(env: Env) -> Option<GuardianConfig> {
        governance::get_guardian_config(&env)
    }

    pub fn gov_get_proposal_approvals(env: Env, proposal_id: u64) -> Option<Vec<Address>> {
        governance::get_proposal_approvals(&env, proposal_id)
    }

    pub fn gov_get_recovery_request(env: Env) -> Option<RecoveryRequest> {
        governance::get_recovery_request(&env)
    }

    pub fn gov_get_recovery_approvals(env: Env) -> Option<Vec<Address>> {
        governance::get_recovery_approvals(&env)
    }

    pub fn gov_get_proposals(env: Env, start_id: u64, limit: u32) -> Vec<Proposal> {
        governance::get_proposals(&env, start_id, limit)
    }

    pub fn gov_can_vote(env: Env, voter: Address, proposal_id: u64) -> bool {
        governance::can_vote(&env, voter, proposal_id)
    }

    pub fn gov_vote(env: Env, voter: Address, proposal_id: u64, vote_type: VoteType) -> Result<(), GovernanceError> {
        governance::vote(&env, voter, proposal_id, vote_type)
    }

    pub fn gov_get_vote(env: Env, voter: Address, proposal_id: u64) -> Option<VoteType> {
        governance::get_vote(&env, voter, proposal_id)
    }

    pub fn gov_get_admin_address(env: Env) -> Option<Address> {
        governance::get_admin(&env)
    }

    // --- Recovery ---

    pub fn gov_start_recovery(env: Env, initiator: Address, old_admin: Address, new_admin: Address) -> Result<(), GovernanceError> {
        governance::start_recovery(&env, initiator, old_admin, new_admin)
    }

    pub fn gov_approve_recovery(env: Env, approver: Address) -> Result<(), GovernanceError> {
        governance::approve_recovery(&env, approver)
    }

    pub fn gov_execute_recovery(env: Env, executor: Address) -> Result<(), GovernanceError> {
        governance::execute_recovery(&env, executor)
    }

    // --- Multisig & Emergency Helpers ---

    pub fn ms_set_admins(env: Env, caller: Address, admins: Vec<Address>) -> Result<(), GovernanceError> {
        governance::ms_set_admins(&env, caller, admins)
    }

    pub fn ms_approve(env: Env, caller: Address, proposal_id: u64) -> Result<(), GovernanceError> {
        governance::ms_approve(&env, caller, proposal_id)
    }

    pub fn ms_execute(env: Env, caller: Address, proposal_id: u64) -> Result<(), GovernanceError> {
        governance::ms_execute(&env, caller, proposal_id)
    }

    // --- Asset Management ---

    pub fn initialize_asset(env: Env, caller: Address, asset: Address, config: AssetConfig) -> Result<(), CrossAssetError> {
        cross_asset::initialize_asset(&env, caller, asset, config)
    }

    pub fn get_asset_config(env: Env, asset: Option<Address>) -> Result<AssetConfig, CrossAssetError> {
        cross_asset::get_asset_config_by_address(&env, asset)
    }

    pub fn get_asset_list(env: Env) -> Vec<AssetKey> {
        cross_asset::get_asset_list(&env)
    }

    pub fn cross_asset_deposit(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<AssetPosition, CrossAssetError> {
        cross_asset::cross_asset_deposit(&env, user, asset, amount)
    }

    pub fn cross_asset_withdraw(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<AssetPosition, CrossAssetError> {
        cross_asset::cross_asset_withdraw(&env, user, asset, amount)
    }

    pub fn cross_asset_borrow(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<AssetPosition, CrossAssetError> {
        cross_asset::cross_asset_borrow(&env, user, asset, amount)
    }

    pub fn cross_asset_repay(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<AssetPosition, CrossAssetError> {
        cross_asset::cross_asset_repay(&env, user, asset, amount)
    }

    pub fn get_user_asset_position(env: Env, user: Address, asset: Option<Address>) -> AssetPosition {
        cross_asset::get_user_asset_position(&env, &user, asset)
    }

    pub fn get_user_position_summary(env: Env, user: Address) -> Result<UserPositionSummary, CrossAssetError> {
        cross_asset::get_user_position_summary(&env, &user)
    }

    pub fn update_asset_price(env: Env, caller: Address, asset: Address, price: i128) -> Result<(), CrossAssetError> {
        cross_asset::update_asset_price(&env, caller, asset, price)
    }

    pub fn update_asset_config(env: Env, caller: Address, asset: Address, config: AssetConfig) -> Result<(), CrossAssetError> {
        cross_asset::update_asset_config(&env, caller, asset, config)
    }

    // --- Oracle ---

    pub fn update_price_feed(
        env: Env,
        caller: Address,
        asset: Address,
        price: i128,
        decimals: u32,
        oracle: Address,
    ) -> Result<i128, oracle::OracleError> {
        oracle::update_price_feed(&env, caller, asset, price, decimals, oracle)
    }

    pub fn get_price(env: Env, asset: Address) -> Result<i128, oracle::OracleError> {
        oracle::get_price(&env, &asset)
    }

    pub fn set_primary_oracle(
        env: Env,
        caller: Address,
        asset: Address,
        primary_oracle: Address,
    ) -> Result<(), oracle::OracleError> {
        oracle::set_primary_oracle(&env, caller, asset, primary_oracle)
    }

    pub fn set_fallback_oracle(
        env: Env,
        caller: Address,
        asset: Address,
        fallback_oracle: Address,
    ) -> Result<(), oracle::OracleError> {
        oracle::set_fallback_oracle(&env, caller, asset, fallback_oracle)
    }

    pub fn configure_oracle(
        env: Env,
        caller: Address,
        config: oracle::OracleConfig,
    ) -> Result<(), oracle::OracleError> {
        oracle::configure_oracle(&env, caller, config)
    }

    pub fn get_oracle_config(env: Env) -> oracle::OracleConfig {
        oracle::get_oracle_config(&env)
    }

    // --- Bridge ---

    pub fn register_bridge(
        env: Env,
        caller: Address,
        network_id: u32,
        bridge: Address,
        fee_bps: i128,
    ) -> Result<(), BridgeError> {
        bridge::register_bridge(&env, caller, network_id, bridge, fee_bps)
    }

    pub fn set_bridge_fee(
        env: Env,
        caller: Address,
        network_id: u32,
        fee_bps: i128,
    ) -> Result<(), BridgeError> {
        bridge::set_bridge_fee(&env, caller, network_id, fee_bps)
    }

    pub fn list_bridges(env: Env) -> Map<u32, BridgeConfig> {
        bridge::list_bridges(&env)
    }

    pub fn get_bridge_config(env: Env, network_id: u32) -> Result<BridgeConfig, BridgeError> {
        bridge::get_bridge_config(&env, network_id)
    }

    pub fn bridge_deposit(
        env: Env,
        user: Address,
        network_id: u32,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, BridgeError> {
        bridge::bridge_deposit(&env, user, network_id, asset, amount)
    }

    pub fn bridge_withdraw(
        env: Env,
        user: Address,
        network_id: u32,
        asset: Option<Address>,
        amount: i128,
    ) -> Result<i128, BridgeError> {
        bridge::bridge_withdraw(&env, user, network_id, asset, amount)
    }

    // --- Configuration ---

    pub fn config_set(env: Env, caller: Address, key: Symbol, value: soroban_sdk::Val) -> Result<(), ConfigError> {
        config::config_set(&env, caller, key, value)
    }

    pub fn config_get(env: Env, key: Symbol) -> Option<soroban_sdk::Val> {
        config::config_get(&env, key)
    }

    pub fn config_backup(env: Env, caller: Address, keys: Vec<Symbol>) -> Result<Vec<(Symbol, Val)>, ConfigError> {
        config::config_backup(&env, caller, keys)
    }

    pub fn config_restore(env: Env, caller: Address, backup: Vec<(Symbol, Val)>) -> Result<(), ConfigError> {
        config::config_restore(&env, caller, backup)
    }

    pub fn set_native_asset_address(env: Env, admin: Address, asset: Address) -> Result<(), RiskManagementError> {
        deposit::set_native_asset_address(&env, admin, asset).map_err(|_| RiskManagementError::Unauthorized)
    }

    // --- Analytics ---

    pub fn get_protocol_report(env: Env) -> ProtocolReport {
        analytics::generate_protocol_report(&env).expect("Protocol report generation failed")
    }

    pub fn get_user_report(env: Env, user: Address) -> UserReport {
        analytics::generate_user_report(&env, &user).expect("User report generation failed")
    }

    pub fn get_recent_activity(env: Env, limit: u32) -> Vec<crate::analytics::ActivityEntry> {
        crate::analytics::get_recent_activity(&env, limit)
    }

    pub fn get_user_activity_feed(env: Env, user: Address, limit: u32) -> Vec<crate::analytics::ActivityEntry> {
        crate::analytics::get_user_activity_feed(&env, user, limit)
    }

    // --- Risk Management ---

    pub fn get_risk_config(env: Env) -> Option<RiskConfig> {
        risk_management::get_risk_config(&env)
    }

    pub fn get_min_collateral_ratio(env: Env) -> i128 {
        risk_params::get_min_collateral_ratio(&env).unwrap_or(0)
    }

    pub fn get_liquidation_threshold(env: Env) -> i128 {
        risk_params::get_liquidation_threshold(&env).unwrap_or(0)
    }

    pub fn get_close_factor(env: Env) -> i128 {
        risk_params::get_close_factor(&env).unwrap_or(0)
    }

    pub fn get_liquidation_incentive(env: Env) -> i128 {
        risk_params::get_liquidation_incentive(&env).unwrap_or(0)
    }

    pub fn require_min_collateral_ratio(env: Env, collateral_value: i128, debt_value: i128) {
        risk_params::require_min_collateral_ratio(&env, collateral_value, debt_value).expect("Insufficient collateral ratio")
    }

    pub fn can_be_liquidated(env: Env, collateral_value: i128, debt_value: i128) -> bool {
        risk_params::can_be_liquidated(&env, collateral_value, debt_value)
    }

    pub fn get_max_liquidatable_amount(env: Env, debt_value: i128) -> i128 {
        risk_params::get_max_liquidatable_amount(&env, debt_value).unwrap_or(0)
    }

    pub fn get_liquidation_incentive_amount(env: Env, liquidated_amount: i128) -> i128 {
        risk_params::get_liquidation_incentive_amount(&env, liquidated_amount).unwrap_or(0)
    }

    pub fn set_risk_params(
        env: Env,
        caller: Address,
        min_cr: Option<i128>,
        liq_threshold: Option<i128>,
        close_factor: Option<i128>,
        liq_incentive: Option<i128>,
    ) -> Result<u64, RiskManagementError> {
        require_admin(&env, &caller)?;
        let proposal_type = ProposalType::RiskParams(
            min_cr,
            liq_threshold,
            close_factor,
            liq_incentive,
        );
        let description = String::from_str(&env, "Update risk parameters via timelock");

        governance::create_admin_proposal(&env, caller, proposal_type, description)
            .map_err(|_| RiskManagementError::GovernanceRequired)
    }

    pub fn set_pause_switches(env: Env, admin: Address, switches: Map<Symbol, bool>) -> Result<(), RiskManagementError> {
        risk_management::set_pause_switches(&env, admin, switches)
    }

    pub fn is_operation_paused(env: Env, operation: Symbol) -> bool {
        risk_management::is_operation_paused(&env, operation)
    }

    pub fn is_emergency_paused(env: Env) -> bool {
        risk_management::is_emergency_paused(&env)
    }

    pub fn set_emergency_pause(env: Env, admin: Address, paused: bool) -> Result<(), RiskManagementError> {
        risk_management::set_emergency_pause(&env, admin, paused)
    }

    // --- Flash Loan ---

    pub fn set_flash_loan_fee(env: Env, admin: Address, fee_bps: i128) -> Result<(), flash_loan::FlashLoanError> {
        flash_loan::set_flash_loan_fee(&env, admin, fee_bps)
    }

    pub fn configure_flash_loan(env: Env, admin: Address, config: flash_loan::FlashLoanConfig) -> Result<(), flash_loan::FlashLoanError> {
        flash_loan::configure_flash_loan(&env, admin, config)
    }

    pub fn execute_flash_loan(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
        callback: Address,
    ) -> Result<i128, flash_loan::FlashLoanError> {
        flash_loan::execute_flash_loan(&env, user, asset, amount, callback)
    }

    pub fn repay_flash_loan(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
    ) -> Result<(), flash_loan::FlashLoanError> {
        flash_loan::repay_flash_loan(&env, user, asset, amount)
    }

    // --- Interest Rate ---

    pub fn get_borrow_rate(env: Env) -> i128 {
        interest_rate::get_current_borrow_rate(&env).unwrap_or(0)
    }

    pub fn get_supply_rate(env: Env) -> i128 {
        interest_rate::get_current_supply_rate(&env).unwrap_or(0)
    }

    pub fn get_utilization(env: Env) -> i128 {
        interest_rate::get_current_utilization(&env).unwrap_or(0)
    }

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
    ) -> Result<u64, RiskManagementError> {
        require_admin(&env, &admin)?;

        let params = crate::types::InterestRateParams {
            base_rate_bps: base_rate,
            kink_utilization_bps: kink,
            multiplier_bps: multiplier,
            jump_multiplier_bps: jump_multiplier,
            rate_floor_bps: rate_floor,
            rate_ceiling_bps: rate_ceiling,
            spread_bps: spread,
        };
        let proposal_type = ProposalType::InterestRateConfig(params);
        let description = String::from_str(&env, "Update interest rate config via timelock");

        governance::create_admin_proposal(&env, admin, proposal_type, description)
            .map_err(|_| RiskManagementError::GovernanceRequired)
    }

    pub fn set_pause_switch(env: Env, admin: Address, op: Symbol, paused: bool) -> Result<(), RiskManagementError> {
        let mut map = Map::new(&env);
        map.set(op, paused);
        risk_management::set_pause_switches(&env, admin, map)
    }

    pub fn set_emergency_rate_adjustment(env: Env, admin: Address, adjustment: i128) -> Result<(), RiskManagementError> {
        interest_rate::set_emergency_rate_adjustment(&env, admin, adjustment).map_err(|_| RiskManagementError::Unauthorized)
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_reentrancy;

#[cfg(test)]
mod test_zero_amount;

#[cfg(test)]
mod flash_loan_test;
