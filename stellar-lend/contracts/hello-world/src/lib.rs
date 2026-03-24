use soroban_sdk::{contract, contractimpl, Address, Env, Map, Symbol, Vec, String};

pub mod admin;
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
pub mod multi_collateral;
pub mod multisig;
pub mod oracle;
pub mod recovery;
pub mod reentrancy;
pub mod repay;
pub mod reserve;
pub mod risk_management;
pub mod risk_params;
pub mod storage;
pub mod treasury;
pub mod types;
pub mod withdraw;

use crate::deposit::DepositDataKey;
use crate::deposit::Position;
use crate::risk_management::RiskManagementError;
use crate::interest_rate::InterestRateError;
use crate::analytics::AnalyticsError;

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
#[contract]
pub struct HelloContract;

#[contractimpl]
impl HelloContract {
    pub fn hello(env: Env) -> String {
        String::from_str(&env, "Hello")
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
    ) -> Result<(), governance::GovernanceError> {
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
        if crate::admin::has_admin(&env) {
            return Err(RiskManagementError::Unauthorized);
        }
        crate::admin::set_admin(&env, admin.clone(), None)
            .map_err(|_| RiskManagementError::Unauthorized)?;
        risk_management::initialize_risk_management(&env, admin.clone())?;
        risk_params::initialize_risk_params(&env).map_err(|_| RiskManagementError::InvalidParameter)?;
        interest_rate::initialize_interest_rate_config(&env, admin).map_err(|e| {
            if e == InterestRateError::AlreadyInitialized {
                RiskManagementError::AlreadyInitialized
            } else {
                RiskManagementError::Unauthorized
            }
        })?;
        Ok(())
    }

    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), admin::AdminError> {
        admin::set_admin(&env, new_admin, Some(caller))
    }

    pub fn deposit_collateral(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<i128, deposit::DepositError> {
        deposit::deposit_collateral(&env, user, asset, amount)
    }

    pub fn set_risk_params(
        env: Env,
        caller: Address,
        min_collateral_ratio: Option<i128>,
        liquidation_threshold: Option<i128>,
        close_factor: Option<i128>,
        liquidation_incentive: Option<i128>,
    ) -> Result<(), RiskManagementError> {
        require_admin(&env, &caller)?;
        risk_params::set_risk_params(&env, min_collateral_ratio, liquidation_threshold, close_factor, liquidation_incentive)
            .map_err(|_| RiskManagementError::InvalidParameter)
    }

    pub fn borrow_asset(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<i128, borrow::BorrowError> {
        borrow::borrow_asset(&env, user, asset, amount)
    }

    pub fn repay_debt(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<(i128, i128, i128), repay::RepayError> {
        repay::repay_debt(&env, user, asset, amount)
    }

    pub fn withdraw_collateral(env: Env, user: Address, asset: Option<Address>, amount: i128) -> Result<i128, withdraw::WithdrawError> {
        withdraw::withdraw_collateral(&env, user, asset, amount)
    }

    pub fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        debt_asset: Option<Address>,
        collateral_asset: Option<Address>,
        debt_amount: i128,
    ) -> Result<(i128, i128, i128), liquidate::LiquidationError> {
        liquidator.require_auth();
        liquidate::liquidate(
            &env,
            liquidator,
            borrower,
            debt_asset,
            collateral_asset,
            debt_amount,
        )
    }

    pub fn set_emergency_pause(env: Env, caller: Address, paused: bool) -> Result<(), RiskManagementError> {
        require_admin(&env, &caller)?;
        risk_management::set_emergency_pause(&env, caller, paused)
    }

    pub fn execute_flash_loan(env: Env, user: Address, asset: Address, amount: i128, callback: Address) -> Result<i128, flash_loan::FlashLoanError> {
        flash_loan::execute_flash_loan(&env, user, asset, amount, callback)
    }

    pub fn repay_flash_loan(env: Env, user: Address, asset: Address, amount: i128) -> Result<(), flash_loan::FlashLoanError> {
        flash_loan::repay_flash_loan(&env, user, asset, amount)
    }

    pub fn can_be_liquidated(env: Env, collateral_value: i128, debt_value: i128) -> Result<bool, risk_params::RiskParamsError> {
        risk_params::can_be_liquidated(&env, collateral_value, debt_value)
    }

    pub fn get_max_liquidatable_amount(env: Env, debt_value: i128) -> Result<i128, risk_params::RiskParamsError> {
        risk_params::get_max_liquidatable_amount(&env, debt_value)
    }

    pub fn get_liquidation_incentive_amount(env: Env, liquidated_amount: i128) -> Result<i128, risk_params::RiskParamsError> {
        risk_params::get_liquidation_incentive_amount(&env, liquidated_amount)
    }

    pub fn require_min_collateral_ratio(env: Env, collateral_value: i128, debt_value: i128) -> Result<(), risk_params::RiskParamsError> {
        risk_params::require_min_collateral_ratio(&env, collateral_value, debt_value)
    }

    // -------------------------------------------------------------------------
    // Treasury & Fee Management
    // -------------------------------------------------------------------------

    /// Set the protocol treasury address (admin-only)
    pub fn set_treasury(env: Env, caller: Address, treasury: Address) -> Result<(), treasury::TreasuryError> {
        treasury::set_treasury(&env, caller, treasury)
    }

    /// Return the configured treasury address
    pub fn get_treasury(env: Env) -> Option<Address> {
        treasury::get_treasury(&env)
    }

    /// Return accumulated protocol reserves for the given asset
    pub fn get_reserve_balance(env: Env, asset: Option<Address>) -> i128 {
        treasury::get_reserve_balance(&env, asset)
    }

    /// Withdraw protocol reserves to a recipient (admin-only)
    pub fn claim_reserves(
        env: Env,
        caller: Address,
        asset: Option<Address>,
        recipient: Address,
        amount: i128,
    ) -> Result<(), treasury::TreasuryError> {
        treasury::claim_reserves(&env, caller, asset, recipient, amount)
    }

    /// Update protocol fee percentages (admin-only)
    pub fn set_fee_config(
        env: Env,
        caller: Address,
        interest_fee_bps: i128,
        liquidation_fee_bps: i128,
    ) -> Result<(), treasury::TreasuryError> {
        treasury::set_fee_config(
            &env,
            caller,
            treasury::TreasuryFeeConfig {
                interest_fee_bps,
                liquidation_fee_bps,
            },
        )
    }

    /// Return the current fee configuration
    pub fn get_fee_config(env: Env) -> treasury::TreasuryFeeConfig {
        treasury::get_fee_config(&env)
    }

    // -------------------------------------------------------------------------
    // Multi-Asset Collateral
    // -------------------------------------------------------------------------

    /// Return the collateral balance for a specific (user, asset) pair
    pub fn get_user_asset_collateral(env: Env, user: Address, asset: Address) -> i128 {
        multi_collateral::get_user_asset_collateral(&env, &user, &asset)
    }

    /// Return the list of assets in which the user currently holds collateral
    pub fn get_user_asset_list(env: Env, user: Address) -> Vec<Address> {
        multi_collateral::get_user_asset_list(&env, &user)
    }

    /// Return the oracle-weighted total collateral value across all of the
    /// user's deposited assets (collateral factors applied per asset).
    /// Returns 0 for legacy single-asset users.
    pub fn get_user_total_collateral_value(env: Env, user: Address) -> i128 {
        multi_collateral::calculate_total_collateral_value(&env, &user).unwrap_or(0)
    }

    // -------------------------------------------------------------------------
    // Analytics
    // -------------------------------------------------------------------------

    /// Read-only user health factor query (collateral/debt in basis points).
    pub fn get_health_factor(env: Env, user: Address) -> Result<i128, AnalyticsError> {
        analytics::calculate_health_factor(&env, &user)
    }

    /// Read-only user position query.
    pub fn get_user_position(env: Env, user: Address) -> Result<Position, AnalyticsError> {
        analytics::get_user_position_summary(&env, &user)
    }
}

#[cfg(test)]
mod test_reentrancy;
#[cfg(test)]
mod test_zero_amount;
#[cfg(test)]
mod flash_loan_test;
#[cfg(test)]
mod multi_collateral_test;
#[cfg(test)]
mod treasury_test;
