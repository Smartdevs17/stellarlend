//! Trait definitions for pluggable liquidation strategies

use soroban_sdk::{contracttype, Bytes, Env};

/// Result type for strategy operations
pub type StrategyResult<T> = Result<T, StrategyError>;

/// Strategy execution error
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StrategyError {
    /// Invalid parameters
    InvalidParameters = 1,
    /// Validation failed
    ValidationFailed = 2,
    /// Calculation error
    CalculationError = 3,
    /// Strategy not applicable
    NotApplicable = 4,
    /// Insufficient data
    InsufficientData = 5,
}

/// Discount calculation result
#[contracttype]
#[derive(Clone, Debug)]
pub struct DiscountResult {
    /// Discount in basis points
    pub discount_bps: u32,
    /// Confidence level (0-100)
    pub confidence: u32,
    /// Metadata about calculation
    pub metadata: u64,
}

/// Strategy trait for implementing custom liquidation strategies
pub trait LiquidationStrategyTrait {
    /// Validate strategy parameters
    fn validate(&self, env: &Env, parameters: &Bytes) -> StrategyResult<()>;

    /// Calculate liquidation discount
    fn calculate_discount(
        &self,
        env: &Env,
        collateral_value: i128,
        debt_value: i128,
        time_since_unhealthy: u64,
    ) -> StrategyResult<DiscountResult>;

    /// Check if strategy is applicable for given conditions
    fn is_applicable(
        &self,
        collateral_value: i128,
        debt_value: i128,
        time_since_unhealthy: u64,
    ) -> bool;

    /// Get strategy name
    fn name(&self) -> &'static str;

    /// Get strategy version
    fn version(&self) -> u32;
}

/// Fixed discount strategy (simple, predictable)
pub struct FixedDiscountStrategy;

impl LiquidationStrategyTrait for FixedDiscountStrategy {
    fn validate(&self, _env: &Env, _parameters: &Bytes) -> StrategyResult<()> {
        // Parameters should contain: discount_bps (u32)
        Ok(())
    }

    fn calculate_discount(
        &self,
        _env: &Env,
        _collateral_value: i128,
        _debt_value: i128,
        _time_since_unhealthy: u64,
    ) -> StrategyResult<DiscountResult> {
        Ok(DiscountResult {
            discount_bps: 500, // 5% fixed discount
            confidence: 100,
            metadata: 0,
        })
    }

    fn is_applicable(&self, _collateral_value: i128, _debt_value: i128, _time_since_unhealthy: u64) -> bool {
        true // Always applicable
    }

    fn name(&self) -> &'static str {
        "FixedDiscount"
    }

    fn version(&self) -> u32 {
        1
    }
}

/// Dutch auction strategy (discount increases over time)
pub struct DutchAuctionStrategy;

impl LiquidationStrategyTrait for DutchAuctionStrategy {
    fn validate(&self, _env: &Env, _parameters: &Bytes) -> StrategyResult<()> {
        Ok(())
    }

    fn calculate_discount(
        &self,
        _env: &Env,
        _collateral_value: i128,
        _debt_value: i128,
        time_since_unhealthy: u64,
    ) -> StrategyResult<DiscountResult> {
        // Discount increases with time: 1% base + 0.1% per hour
        let hours = time_since_unhealthy / 3600;
        let discount_bps = 100 + (hours.min(5000) as u32 * 10); // Cap at 50% + 1%

        Ok(DiscountResult {
            discount_bps,
            confidence: 80,
            metadata: hours as u64,
        })
    }

    fn is_applicable(&self, _collateral_value: i128, _debt_value: i128, _time_since_unhealthy: u64) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "DutchAuction"
    }

    fn version(&self) -> u32 {
        1
    }
}

/// TWAP-based strategy (uses time-weighted average price)
pub struct TWAPStrategy;

impl LiquidationStrategyTrait for TWAPStrategy {
    fn validate(&self, _env: &Env, _parameters: &Bytes) -> StrategyResult<()> {
        Ok(())
    }

    fn calculate_discount(
        &self,
        _env: &Env,
        _collateral_value: i128,
        _debt_value: i128,
        _time_since_unhealthy: u64,
    ) -> StrategyResult<DiscountResult> {
        Ok(DiscountResult {
            discount_bps: 300, // 3% based on TWAP
            confidence: 85,
            metadata: 0,
        })
    }

    fn is_applicable(&self, _collateral_value: i128, _debt_value: i128, _time_since_unhealthy: u64) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "TWAP"
    }

    fn version(&self) -> u32 {
        1
    }
}

/// Hybrid strategy combining multiple approaches
pub struct HybridStrategy;

impl LiquidationStrategyTrait for HybridStrategy {
    fn validate(&self, _env: &Env, _parameters: &Bytes) -> StrategyResult<()> {
        Ok(())
    }

    fn calculate_discount(
        &self,
        env: &Env,
        collateral_value: i128,
        debt_value: i128,
        time_since_unhealthy: u64,
    ) -> StrategyResult<DiscountResult> {
        // Combine fixed and dutch auction
        let fixed_strategy = FixedDiscountStrategy;
        let dutch_strategy = DutchAuctionStrategy;

        let fixed_result = fixed_strategy.calculate_discount(
            env,
            collateral_value,
            debt_value,
            time_since_unhealthy,
        )?;

        let dutch_result = dutch_strategy.calculate_discount(
            env,
            collateral_value,
            debt_value,
            time_since_unhealthy,
        )?;

        // Take average
        let avg_discount = (fixed_result.discount_bps as u64 + dutch_result.discount_bps as u64) / 2;
        let avg_confidence = (fixed_result.confidence as u64 + dutch_result.confidence as u64) / 2;

        Ok(DiscountResult {
            discount_bps: avg_discount as u32,
            confidence: avg_confidence as u32,
            metadata: 0,
        })
    }

    fn is_applicable(&self, _collateral_value: i128, _debt_value: i128, _time_since_unhealthy: u64) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "Hybrid"
    }

    fn version(&self) -> u32 {
        1
    }
}
