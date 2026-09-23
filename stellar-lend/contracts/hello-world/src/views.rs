//! Read-only query operations for the lending pool
//!
//! This module contains all non-state-modifying query functions.
//! These functions are gas-optimized for parallel execution and can be safely cached.
//!
//! ## Gas Optimization
//! - No persistent storage writes
//! - Eligible for aggressive caching
//! - Can execute in parallel
//! - Suitable for aggregation queries

use soroban_sdk::{Address, Env};
use crate::types::*;
use crate::errors::LendingError;

/// Query operations namespace
pub struct ViewsModule;

impl ViewsModule {
    /// Get the health factor for a user
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user address
    ///
    /// # Returns
    /// Health factor as i128 (scaled by 1e18)
    /// Returns 0 if user has no positions
    pub fn get_health_factor(env: &Env, user: &Address) -> i128 {
        crate::health::calculate_health_factor(env, user)
    }

    /// Check if a user's position is liquidatable
    pub fn is_liquidatable(env: &Env, user: &Address) -> bool {
        crate::health::is_liquidatable(env, user)
    }

    /// Get user's current collateral value
    pub fn get_user_collateral_value(env: &Env, user: &Address) -> i128 {
        crate::risk_management::calculate_collateral_value(env, user)
    }

    /// Get user's current debt value
    pub fn get_user_debt_value(env: &Env, user: &Address) -> i128 {
        crate::risk_management::calculate_debt_value(env, user)
    }

    /// Get current price for an asset
    pub fn get_asset_price(env: &Env, asset: &Address) -> Result<i128, crate::oracle::OracleError> {
        crate::oracle::get_price(env, asset)
    }

    /// Get user's position details
    pub fn get_user_position(env: &Env, user: &Address) -> Option<crate::deposit::Position> {
        crate::deposit::get_position(env, user)
    }

    /// Get total protocol statistics
    pub fn get_protocol_stats(env: &Env) -> ProtocolStats {
        ProtocolStats {
            total_deposits: crate::analytics::get_total_deposits(env),
            total_borrows: crate::analytics::get_total_borrows(env),
            total_reserves: crate::analytics::get_total_reserves(env),
            average_health_factor: crate::analytics::get_average_health_factor(env),
        }
    }

    /// Get risk metrics for the protocol
    pub fn get_protocol_risk_metrics(env: &Env) -> Result<RiskMetrics, LendingError> {
        crate::risk_management::calculate_protocol_risk_metrics(env)
    }

    /// Get interest rates for an asset
    pub fn get_asset_interest_rates(env: &Env, asset: &Address) -> Result<InterestRates, LendingError> {
        crate::interest_rate::get_interest_rates(env, asset)
    }

    /// Check if contract is paused
    pub fn is_paused(env: &Env) -> bool {
        crate::circuit_breaker::is_paused(env)
    }

    /// Get circuit breaker state
    pub fn get_circuit_breaker_state(env: &Env) -> CircuitBreakerState {
        crate::circuit_breaker::get_state(env)
    }

    /// Get fee tier for a user
    pub fn get_user_fee_tier(env: &Env, user: &Address) -> Result<u32, LendingError> {
        crate::reserve_factor::get_user_fee_tier(env, user)
    }

    /// Get governance proposal
    pub fn get_proposal(env: &Env, proposal_id: u64) -> Option<Proposal> {
        crate::governance::get_proposal(env, proposal_id)
    }

    /// Get governance analytics
    pub fn get_governance_analytics(env: &Env) -> GovernanceAnalytics {
        crate::governance::get_governance_analytics(env)
    }

    /// Get monitoring data for risk dashboard
    pub fn get_risk_dashboard(env: &Env) -> Result<RiskDashboard, LendingError> {
        crate::monitor::get_risk_dashboard(env)
    }
}

/// Protocol-wide statistics
#[derive(Clone, Debug)]
pub struct ProtocolStats {
    pub total_deposits: i128,
    pub total_borrows: i128,
    pub total_reserves: i128,
    pub average_health_factor: i128,
}

/// Interest rate information for an asset
#[derive(Clone, Debug)]
pub struct InterestRates {
    pub supply_rate: i128,
    pub borrow_rate: i128,
    pub utilization_rate: i128,
}

/// Risk metrics snapshot
#[derive(Clone, Debug)]
pub struct RiskMetrics {
    pub total_collateral_value: i128,
    pub total_debt_value: i128,
    pub weighted_ltv: i128,
    pub concentration_risk: i128,
}

/// Circuit breaker state snapshot
#[derive(Clone, Debug)]
pub struct CircuitBreakerState {
    pub is_active: bool,
    pub triggered_at: Option<u64>,
    pub recovery_time: Option<u64>,
}

/// Risk dashboard data
#[derive(Clone, Debug)]
pub struct RiskDashboard {
    pub liquidatable_positions: u32,
    pub at_risk_positions: u32,
    pub total_active_users: u32,
    pub protocol_risk_level: u32, // 0-100
}
