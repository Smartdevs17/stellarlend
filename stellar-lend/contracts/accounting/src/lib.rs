#![no_std]

pub mod revenue;
pub mod types;
pub mod storage;
pub mod errors;

#[cfg(test)]
mod tests;

use soroban_sdk::{contract, contractimpl, Address, Env};
use types::{Recognition, RevenueSchedule, RevenueAnalytics};
use errors::AccountingError;

#[contract]
pub struct AccountingContract;

#[contractimpl]
impl AccountingContract {
    /// Initialize the accounting contract
    pub fn initialize(env: Env, admin: Address) -> Result<(), AccountingError> {
        revenue::initialize(&env, admin)
    }

    /// Recognize revenue for a subscription
    pub fn recognize_revenue(
        env: Env,
        caller: Address,
        subscription_id: u64,
    ) -> Result<Recognition, AccountingError> {
        revenue::recognize_revenue(&env, caller, subscription_id)
    }

    /// Get deferred revenue for a merchant
    pub fn get_deferred_revenue(
        env: Env,
        merchant_id: Address,
    ) -> Result<i128, AccountingError> {
        revenue::get_deferred_revenue(&env, merchant_id)
    }

    /// Get revenue schedule for a subscription
    pub fn get_revenue_schedule(
        env: Env,
        subscription_id: u64,
    ) -> Result<RevenueSchedule, AccountingError> {
        revenue::get_revenue_schedule(&env, subscription_id)
    }

    /// Configure revenue recognition rule
    pub fn configure_recognition_rule(
        env: Env,
        caller: Address,
        subscription_id: u64,
        method: u32,
        recognition_period: u64,
    ) -> Result<(), AccountingError> {
        revenue::configure_recognition_rule(&env, caller, subscription_id, method, recognition_period)
    }

    /// Get revenue analytics by period
    pub fn get_revenue_analytics(
        env: Env,
        merchant_id: Address,
        start_time: u64,
        end_time: u64,
    ) -> Result<RevenueAnalytics, AccountingError> {
        revenue::get_revenue_analytics(&env, merchant_id, start_time, end_time)
    }

    /// Handle contract modification
    pub fn handle_contract_modification(
        env: Env,
        caller: Address,
        subscription_id: u64,
        new_amount: i128,
    ) -> Result<(), AccountingError> {
        revenue::handle_contract_modification(&env, caller, subscription_id, new_amount)
    }

    /// Handle mid-period cancellation
    pub fn handle_cancellation(
        env: Env,
        caller: Address,
        subscription_id: u64,
    ) -> Result<i128, AccountingError> {
        revenue::handle_cancellation(&env, caller, subscription_id)
    }

    /// Create a new subscription
    pub fn create_subscription(
        env: Env,
        merchant_id: Address,
        total_amount: i128,
        start_time: u64,
    ) -> Result<u64, AccountingError> {
        revenue::create_subscription(&env, merchant_id, total_amount, start_time)
    }
}
