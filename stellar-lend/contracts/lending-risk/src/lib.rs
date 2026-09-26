#![no_std]

use lending_types::{calculate_health_factor, is_healthy, BPS_DIVISOR};
use stellarlend_safe_math::{safe_add, safe_div, safe_mul, safe_sub, MathError};

pub struct RiskManager;

impl RiskManager {
    pub fn check_liquidation_eligibility(
        collateral_value: i128,
        debt_value: i128,
        liquidation_threshold_bps: i128,
    ) -> bool {
        let health_factor =
            calculate_health_factor(collateral_value, debt_value, liquidation_threshold_bps);
        !is_healthy(health_factor)
    }

    pub fn calculate_liquidation_bonus(
        debt_amount: i128,
        bonus_bps: i128,
    ) -> Result<i128, MathError> {
        safe_mul(debt_amount, bonus_bps).and_then(|v| safe_div(v, BPS_DIVISOR))
    }

    pub fn calculate_max_liquidatable(
        debt_value: i128,
        close_factor_bps: i128,
    ) -> Result<i128, MathError> {
        safe_mul(debt_value, close_factor_bps).and_then(|v| safe_div(v, BPS_DIVISOR))
    }

    pub fn validate_borrow_capacity(
        collateral_value: i128,
        existing_debt: i128,
        new_borrow: i128,
        collateral_factor_bps: i128,
    ) -> Result<bool, MathError> {
        let max_borrow = safe_mul(collateral_value, collateral_factor_bps)
            .and_then(|v| safe_div(v, BPS_DIVISOR))?;
        let total_debt = safe_add(existing_debt, new_borrow)?;
        Ok(total_debt <= max_borrow)
    }

    pub fn calculate_ltv(debt_value: i128, collateral_value: i128) -> Result<i128, MathError> {
        if collateral_value == 0 {
            return Ok(BPS_DIVISOR);
        }
        safe_mul(debt_value, BPS_DIVISOR).and_then(|v| safe_div(v, collateral_value))
    }

    pub fn check_concentration_risk(
        asset_value: i128,
        total_pool_value: i128,
        max_concentration_bps: i128,
    ) -> Result<bool, MathError> {
        if total_pool_value == 0 {
            return Ok(true);
        }
        let concentration =
            safe_mul(asset_value, BPS_DIVISOR).and_then(|v| safe_div(v, total_pool_value))?;
        Ok(concentration <= max_concentration_bps)
    }

    /// Simulates a partial liquidation of an under-collateralized position.
    ///
    /// A liquidator repays `repay_amount` of the position's debt and, in
    /// exchange, seizes collateral equal to the repaid amount plus a
    /// liquidation bonus (the standard Aave/Compound-style liquidation
    /// incentive: `seized = repay_amount * (1 + bonus_bps / 10_000)`).
    ///
    /// Returns `Err` — rather than panicking or silently clamping — for any
    /// input that would violate protocol solvency: repaying more than the
    /// outstanding debt, or seizing more collateral than the position holds.
    pub fn apply_liquidation(
        collateral_value: i128,
        debt_value: i128,
        repay_amount: i128,
        liquidation_bonus_bps: i128,
    ) -> Result<LiquidationOutcome, MathError> {
        if repay_amount < 0 || repay_amount > debt_value {
            return Err(MathError::Underflow);
        }

        let bonus = Self::calculate_liquidation_bonus(repay_amount, liquidation_bonus_bps)?;
        let seized_collateral = safe_add(repay_amount, bonus)?;

        if seized_collateral > collateral_value {
            return Err(MathError::Underflow);
        }

        let new_collateral_value = safe_sub(collateral_value, seized_collateral)?;
        let new_debt_value = safe_sub(debt_value, repay_amount)?;

        Ok(LiquidationOutcome {
            new_collateral_value,
            new_debt_value,
            seized_collateral,
            liquidator_profit: bonus,
        })
    }

    /// Enforces supply cap with graceful degradation.
    ///
    /// When a deposit amount would breach the pool supply cap, instead of
    /// hard-failing the transaction, this calculates the available headroom and
    /// gracefully degrades the deposit to the maximum allowable amount.
    /// If the cap is already 100% exhausted (0 headroom), returns an error.
    pub fn enforce_supply_cap_graceful(
        current_pool_supply: i128,
        pool_supply_cap: i128,
        deposit_amount: i128,
    ) -> Result<SupplyCapEnforcementResult, MathError> {
        if deposit_amount <= 0 {
            return Err(MathError::Underflow);
        }
        if pool_supply_cap <= 0 {
            // Cap <= 0 means unlimited
            return Ok(SupplyCapEnforcementResult {
                accepted_amount: deposit_amount,
                is_degraded: false,
                remaining_headroom: i128::MAX,
                tier: SupplyCapDegradationTier::Normal,
            });
        }
        if current_pool_supply >= pool_supply_cap {
            return Err(MathError::Underflow);
        }

        let available_headroom = safe_sub(pool_supply_cap, current_pool_supply)?;
        let (accepted_amount, is_degraded) = if deposit_amount > available_headroom {
            (available_headroom, true)
        } else {
            (deposit_amount, false)
        };

        let new_supply = safe_add(current_pool_supply, accepted_amount)?;
        let remaining_headroom = safe_sub(pool_supply_cap, new_supply)?;

        let utilization_bps =
            safe_mul(new_supply, BPS_DIVISOR).and_then(|v| safe_div(v, pool_supply_cap))?;

        let tier = if remaining_headroom == 0 {
            SupplyCapDegradationTier::Capped
        } else if utilization_bps >= 9500 {
            SupplyCapDegradationTier::Critical
        } else if utilization_bps >= 8000 {
            SupplyCapDegradationTier::Elevated
        } else {
            SupplyCapDegradationTier::Normal
        };

        Ok(SupplyCapEnforcementResult {
            accepted_amount,
            is_degraded,
            remaining_headroom,
            tier,
        })
    }

    /// Enforces borrow cap with graceful degradation per asset.
    ///
    /// When a borrow amount would breach the pool's borrow cap, instead of
    /// hard-failing the transaction, this calculates the available headroom and
    /// gracefully degrades the borrow to the maximum allowable amount.
    /// If the cap is already 100% exhausted (0 headroom), returns an error.
    pub fn enforce_borrow_cap_graceful(
        current_pool_borrow: i128,
        pool_borrow_cap: i128,
        requested_borrow: i128,
    ) -> Result<BorrowCapEnforcementResult, MathError> {
        if requested_borrow <= 0 {
            return Err(MathError::Underflow);
        }
        if pool_borrow_cap <= 0 {
            // Cap <= 0 means unlimited
            return Ok(BorrowCapEnforcementResult {
                accepted_amount: requested_borrow,
                is_degraded: false,
                remaining_headroom: i128::MAX,
                tier: BorrowCapDegradationTier::Normal,
            });
        }
        if current_pool_borrow >= pool_borrow_cap {
            return Err(MathError::Underflow);
        }

        let available_headroom = safe_sub(pool_borrow_cap, current_pool_borrow)?;
        let (accepted_amount, is_degraded) = if requested_borrow > available_headroom {
            (available_headroom, true)
        } else {
            (requested_borrow, false)
        };

        let new_borrow = safe_add(current_pool_borrow, accepted_amount)?;
        let remaining_headroom = safe_sub(pool_borrow_cap, new_borrow)?;

        let utilization_bps =
            safe_mul(new_borrow, BPS_DIVISOR).and_then(|v| safe_div(v, pool_borrow_cap))?;

        let tier = if remaining_headroom == 0 {
            BorrowCapDegradationTier::Capped
        } else if utilization_bps >= 9500 {
            BorrowCapDegradationTier::Critical
        } else if utilization_bps >= 8000 {
            BorrowCapDegradationTier::Elevated
        } else {
            BorrowCapDegradationTier::Normal
        };

        Ok(BorrowCapEnforcementResult {
            accepted_amount,
            is_degraded,
            remaining_headroom,
            tier,
        })
    }
}

/// Resulting position state and liquidator payout from
/// [`RiskManager::apply_liquidation`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationOutcome {
    pub new_collateral_value: i128,
    pub new_debt_value: i128,
    pub seized_collateral: i128,
    pub liquidator_profit: i128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupplyCapDegradationTier {
    Normal,
    Elevated,
    Critical,
    Capped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupplyCapEnforcementResult {
    /// The amount permitted to be deposited (full or gracefully clamped to remaining headroom)
    pub accepted_amount: i128,
    /// Whether the deposit had to be gracefully degraded (partially filled) due to the cap
    pub is_degraded: bool,
    /// Headroom remaining after this deposit
    pub remaining_headroom: i128,
    /// Degradation tier after this deposit
    pub tier: SupplyCapDegradationTier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowCapDegradationTier {
    Normal,
    Elevated,
    Critical,
    Capped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowCapEnforcementResult {
    /// The amount permitted to be borrowed (full or gracefully clamped to remaining headroom)
    pub accepted_amount: i128,
    /// Whether the borrow had to be gracefully degraded (partially filled) due to the cap
    pub is_degraded: bool,
    /// Headroom remaining after this borrow
    pub remaining_headroom: i128,
    /// Degradation tier after this borrow
    pub tier: BorrowCapDegradationTier,
}

pub struct RiskMetrics {
    pub health_factor: i128,
    pub ltv_ratio: i128,
    pub liquidation_price: i128,
    pub borrow_capacity: i128,
}

impl RiskMetrics {
    pub fn calculate(
        collateral_value: i128,
        debt_value: i128,
        collateral_factor_bps: i128,
        liquidation_threshold_bps: i128,
    ) -> Result<Self, MathError> {
        let health_factor =
            calculate_health_factor(collateral_value, debt_value, liquidation_threshold_bps);
        let ltv_ratio = RiskManager::calculate_ltv(debt_value, collateral_value)?;
        let borrow_capacity = safe_mul(collateral_value, collateral_factor_bps)
            .and_then(|v| safe_div(v, BPS_DIVISOR))?;

        let liquidation_price = if collateral_value > 0 {
            let effective_collateral = safe_mul(collateral_value, liquidation_threshold_bps)
                .and_then(|v| safe_div(v, BPS_DIVISOR))?;
            if effective_collateral == 0 {
                0
            } else {
                safe_mul(debt_value, BPS_DIVISOR).and_then(|v| safe_div(v, effective_collateral))?
            }
        } else {
            0
        };

        Ok(Self {
            health_factor,
            ltv_ratio,
            liquidation_price,
            borrow_capacity,
        })
    }
}

// ── Borrow Cap Management Per Asset (#1026) ───────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetBorrowCapConfig {
    pub asset_id: u32,
    pub max_borrow_cap: i128,
    pub current_total_borrow: i128,
    pub is_frozen: bool,
}

impl AssetBorrowCapConfig {
    pub fn new(asset_id: u32, max_borrow_cap: i128) -> Self {
        Self {
            asset_id,
            max_borrow_cap,
            current_total_borrow: 0,
            is_frozen: false,
        }
    }

    pub fn set_cap(&mut self, new_cap: i128) -> Result<(), MathError> {
        if new_cap < 0 {
            return Err(MathError::Underflow);
        }
        self.max_borrow_cap = new_cap;
        Ok(())
    }

    pub fn execute_borrow_graceful(
        &mut self,
        amount: i128,
    ) -> Result<BorrowCapEnforcementResult, MathError> {
        if self.is_frozen {
            return Err(MathError::Underflow);
        }
        let result = RiskManager::enforce_borrow_cap_graceful(
            self.current_total_borrow,
            self.max_borrow_cap,
            amount,
        )?;
        self.current_total_borrow = safe_add(self.current_total_borrow, result.accepted_amount)?;
        Ok(result)
    }

    pub fn execute_repay(&mut self, amount: i128) -> Result<(), MathError> {
        if amount <= 0 {
            return Err(MathError::Underflow);
        }
        self.current_total_borrow = safe_sub(self.current_total_borrow, amount).unwrap_or(0);
        Ok(())
    }

    pub fn available_headroom(&self) -> i128 {
        if self.max_borrow_cap <= 0 {
            i128::MAX
        } else {
            safe_sub(self.max_borrow_cap, self.current_total_borrow).unwrap_or(0)
        }
    }
}

// ── Health Factor Calculator with Real-time Updates (#1025) ───────────────────

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Moderate,
    Warning,
    Liquidatable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RealTimeHealthReport {
    /// Real-time health factor (scaled by 10,000; 10,000 = 1.0; i128::MAX when debt is zero)
    pub health_factor: i128,
    /// Categorical health assessment
    pub status: HealthStatus,
    /// Total debt value including interest accrued in real time
    pub total_debt_value: i128,
    /// Real-time interest accrued since last checkpoint
    pub real_time_accrued_interest: i128,
    /// Liquidation margin before position becomes liquidatable
    pub liquidation_margin: i128,
    /// True if position can be liquidated right now
    pub is_liquidatable: bool,
    /// Maximum additional borrow allowed before breaching collateral capacity
    pub available_borrow_capacity: i128,
}

pub struct RealTimeHealthCalculator;

impl RealTimeHealthCalculator {
    /// Calculates real-time position health accounting for time-elapsed interest accrual.
    pub fn calculate(
        collateral_value: i128,
        debt_principal: i128,
        stored_accrued_interest: i128,
        borrow_rate_bps: i128,
        elapsed_secs: u64,
        collateral_factor_bps: i128,
        liquidation_threshold_bps: i128,
    ) -> Result<RealTimeHealthReport, MathError> {
        if collateral_value < 0 || debt_principal < 0 {
            return Err(MathError::Underflow);
        }

        let seconds_per_year: u64 = 31_536_000;
        let real_time_accrued_interest =
            if debt_principal > 0 && borrow_rate_bps > 0 && elapsed_secs > 0 {
                let numerator = safe_mul(debt_principal, borrow_rate_bps)
                    .and_then(|v| safe_mul(v, elapsed_secs as i128))?;
                let denominator = safe_mul(BPS_DIVISOR, seconds_per_year as i128)?;
                safe_div(numerator, denominator)?
            } else {
                0
            };

        let total_accrued_interest = safe_add(stored_accrued_interest, real_time_accrued_interest)?;
        let total_debt_value = safe_add(debt_principal, total_accrued_interest)?;

        let weighted_liquidation_collateral = safe_mul(collateral_value, liquidation_threshold_bps)
            .and_then(|v| safe_div(v, BPS_DIVISOR))?;

        let health_factor = if total_debt_value == 0 {
            i128::MAX
        } else {
            safe_mul(weighted_liquidation_collateral, BPS_DIVISOR)
                .and_then(|v| safe_div(v, total_debt_value))?
        };

        let is_liquidatable = health_factor < BPS_DIVISOR && total_debt_value > 0;

        let status = if total_debt_value == 0 || health_factor >= 15_000 {
            HealthStatus::Healthy
        } else if health_factor >= 12_000 {
            HealthStatus::Moderate
        } else if health_factor >= BPS_DIVISOR {
            HealthStatus::Warning
        } else {
            HealthStatus::Liquidatable
        };

        let liquidation_margin = core::cmp::max(
            0,
            safe_sub(weighted_liquidation_collateral, total_debt_value)?,
        );

        let max_borrow_power = safe_mul(collateral_value, collateral_factor_bps)
            .and_then(|v| safe_div(v, BPS_DIVISOR))?;
        let available_borrow_capacity =
            core::cmp::max(0, safe_sub(max_borrow_power, total_debt_value)?);

        Ok(RealTimeHealthReport {
            health_factor,
            status,
            total_debt_value,
            real_time_accrued_interest,
            liquidation_margin,
            is_liquidatable,
            available_borrow_capacity,
        })
    }

    /// Evaluates impact of an instantaneous price shock on health factor in real time.
    pub fn stress_test_price_drop(
        current_report: &RealTimeHealthReport,
        current_collateral_value: i128,
        price_drop_bps: i128,
        collateral_factor_bps: i128,
        liquidation_threshold_bps: i128,
    ) -> Result<RealTimeHealthReport, MathError> {
        if !(0..=BPS_DIVISOR).contains(&price_drop_bps) {
            return Err(MathError::Underflow);
        }
        let dropped_fraction = safe_mul(current_collateral_value, price_drop_bps)
            .and_then(|v| safe_div(v, BPS_DIVISOR))?;
        let stressed_collateral = safe_sub(current_collateral_value, dropped_fraction)?;

        Self::calculate(
            stressed_collateral,
            current_report.total_debt_value,
            0,
            0,
            0,
            collateral_factor_bps,
            liquidation_threshold_bps,
        )
    }
}

// ── Reserve Factor Management and Protocol Revenue Tracking (#1024) ───────────

pub const MAX_RESERVE_FACTOR_BPS: i128 = 5_000;
pub const DEFAULT_RESERVE_FACTOR_BPS: i128 = 1_000;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RevenueCategory {
    Interest,
    LiquidationBonus,
    FlashLoan,
    ProtocolFee,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReserveSplit {
    pub reserve_amount: i128,
    pub lender_amount: i128,
    pub reserve_factor_bps: i128,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProtocolRevenueTracker {
    pub interest_revenue: i128,
    pub liquidation_revenue: i128,
    pub flash_loan_revenue: i128,
    pub protocol_fee_revenue: i128,
    pub total_revenue: i128,
    pub total_claimed: i128,
    pub available_reserves: i128,
}

impl ProtocolRevenueTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_revenue(
        &mut self,
        category: RevenueCategory,
        amount: i128,
    ) -> Result<(), MathError> {
        if amount <= 0 {
            return Err(MathError::Underflow);
        }
        match category {
            RevenueCategory::Interest => {
                self.interest_revenue = safe_add(self.interest_revenue, amount)?;
            }
            RevenueCategory::LiquidationBonus => {
                self.liquidation_revenue = safe_add(self.liquidation_revenue, amount)?;
            }
            RevenueCategory::FlashLoan => {
                self.flash_loan_revenue = safe_add(self.flash_loan_revenue, amount)?;
            }
            RevenueCategory::ProtocolFee => {
                self.protocol_fee_revenue = safe_add(self.protocol_fee_revenue, amount)?;
            }
        }
        self.total_revenue = safe_add(self.total_revenue, amount)?;
        self.available_reserves = safe_add(self.available_reserves, amount)?;
        Ok(())
    }

    pub fn claim_reserves(&mut self, amount: i128) -> Result<(), MathError> {
        if amount <= 0 || amount > self.available_reserves {
            return Err(MathError::Underflow);
        }
        self.available_reserves = safe_sub(self.available_reserves, amount)?;
        self.total_claimed = safe_add(self.total_claimed, amount)?;
        Ok(())
    }
}

pub struct ReserveFactorManager;

impl ReserveFactorManager {
    pub fn validate_reserve_factor(reserve_factor_bps: i128) -> Result<(), MathError> {
        if !(0..=MAX_RESERVE_FACTOR_BPS).contains(&reserve_factor_bps) {
            return Err(MathError::Underflow);
        }
        Ok(())
    }

    pub fn calculate_split(
        interest_amount: i128,
        reserve_factor_bps: i128,
    ) -> Result<ReserveSplit, MathError> {
        Self::validate_reserve_factor(reserve_factor_bps)?;
        if interest_amount <= 0 {
            return Ok(ReserveSplit {
                reserve_amount: 0,
                lender_amount: 0,
                reserve_factor_bps,
            });
        }
        let reserve_amount =
            safe_mul(interest_amount, reserve_factor_bps).and_then(|v| safe_div(v, BPS_DIVISOR))?;
        let lender_amount = safe_sub(interest_amount, reserve_amount)?;
        Ok(ReserveSplit {
            reserve_amount,
            lender_amount,
            reserve_factor_bps,
        })
    }

    /// Projects annual protocol revenue: `total_borrow * borrow_rate * reserve_factor`.
    pub fn project_annual_revenue(
        total_borrow: i128,
        borrow_rate_bps: i128,
        reserve_factor_bps: i128,
    ) -> Result<i128, MathError> {
        Self::validate_reserve_factor(reserve_factor_bps)?;
        let annual_interest =
            safe_mul(total_borrow, borrow_rate_bps).and_then(|v| safe_div(v, BPS_DIVISOR))?;
        safe_mul(annual_interest, reserve_factor_bps).and_then(|v| safe_div(v, BPS_DIVISOR))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liquidation_eligibility() {
        assert!(RiskManager::check_liquidation_eligibility(
            100_000, 90_000, 8_000
        ));
        assert!(!RiskManager::check_liquidation_eligibility(
            100_000, 50_000, 8_000
        ));
    }

    #[test]
    fn test_liquidation_bonus() {
        let bonus = RiskManager::calculate_liquidation_bonus(100_000, 500).unwrap();
        assert_eq!(bonus, 5_000);
    }

    #[test]
    fn test_ltv_calculation() {
        assert_eq!(RiskManager::calculate_ltv(50_000, 100_000).unwrap(), 5_000);
        assert_eq!(RiskManager::calculate_ltv(80_000, 100_000).unwrap(), 8_000);
    }

    #[test]
    fn test_borrow_capacity() {
        assert!(RiskManager::validate_borrow_capacity(100_000, 50_000, 10_000, 7_500).unwrap());
        assert!(!RiskManager::validate_borrow_capacity(100_000, 70_000, 10_000, 7_500).unwrap());
    }

    #[test]
    fn test_concentration_risk() {
        assert!(RiskManager::check_concentration_risk(30_000, 100_000, 5_000).unwrap());
        assert!(!RiskManager::check_concentration_risk(60_000, 100_000, 5_000).unwrap());
    }

    #[test]
    fn test_risk_metrics() {
        let metrics = RiskMetrics::calculate(100_000, 50_000, 7_500, 8_000).unwrap();
        assert_eq!(metrics.ltv_ratio, 5_000);
        assert!(metrics.health_factor > 10_000);
        assert_eq!(metrics.borrow_capacity, 75_000);
    }

    #[test]
    fn test_apply_liquidation_improves_health_factor() {
        // Unhealthy position: collateral 100_000, debt 90_000, threshold 80%.
        let threshold_bps = 8_000;
        let before = calculate_health_factor(100_000, 90_000, threshold_bps);
        assert!(before < BPS_DIVISOR);

        let outcome = RiskManager::apply_liquidation(100_000, 90_000, 40_000, 500).unwrap();
        let after = calculate_health_factor(
            outcome.new_collateral_value,
            outcome.new_debt_value,
            threshold_bps,
        );
        assert!(after > before, "liquidation must improve health factor");
    }

    #[test]
    fn test_apply_liquidation_profit_matches_bonus() {
        let outcome = RiskManager::apply_liquidation(100_000, 90_000, 40_000, 500).unwrap();
        assert_eq!(outcome.liquidator_profit, 2_000); // 40_000 * 500 / 10_000
        assert_eq!(outcome.seized_collateral, 42_000);
        assert_eq!(outcome.new_debt_value, 50_000);
        assert_eq!(outcome.new_collateral_value, 58_000);
    }

    #[test]
    fn test_apply_liquidation_rejects_overpayment() {
        assert!(RiskManager::apply_liquidation(100_000, 90_000, 90_001, 500).is_err());
    }

    #[test]
    fn test_apply_liquidation_rejects_insufficient_collateral() {
        // Seizing repay + bonus would exceed available collateral.
        assert!(RiskManager::apply_liquidation(40_000, 90_000, 39_000, 500).is_err());
    }

    #[test]
    fn test_liquidation_bonus_overflow_is_err() {
        assert!(RiskManager::calculate_liquidation_bonus(i128::MAX, i128::MAX).is_err());
    }

    #[test]
    fn test_supply_cap_graceful_normal_deposit() {
        // Pool cap 10,000, current supply 2,000, deposit 3,000 -> Accepted in full (50% utilization, Normal)
        let res = RiskManager::enforce_supply_cap_graceful(2_000, 10_000, 3_000).unwrap();
        assert_eq!(res.accepted_amount, 3_000);
        assert!(!res.is_degraded);
        assert_eq!(res.remaining_headroom, 5_000);
        assert_eq!(res.tier, SupplyCapDegradationTier::Normal);
    }

    #[test]
    fn test_supply_cap_graceful_elevated_and_critical_tiers() {
        // Elevated: new supply 8,500 / 10,000 = 85%
        let res1 = RiskManager::enforce_supply_cap_graceful(5_000, 10_000, 3_500).unwrap();
        assert_eq!(res1.accepted_amount, 3_500);
        assert_eq!(res1.tier, SupplyCapDegradationTier::Elevated);

        // Critical: new supply 9,700 / 10,000 = 97%
        let res2 = RiskManager::enforce_supply_cap_graceful(9_000, 10_000, 700).unwrap();
        assert_eq!(res2.accepted_amount, 700);
        assert_eq!(res2.tier, SupplyCapDegradationTier::Critical);
    }

    #[test]
    fn test_supply_cap_graceful_degradation_partial_fill() {
        // Pool cap 10,000, current supply 8,000. User requests to deposit 5,000.
        // Instead of hard-failing, gracefully accepts remaining 2,000 headroom!
        let res = RiskManager::enforce_supply_cap_graceful(8_000, 10_000, 5_000).unwrap();
        assert_eq!(res.accepted_amount, 2_000);
        assert!(res.is_degraded);
        assert_eq!(res.remaining_headroom, 0);
        assert_eq!(res.tier, SupplyCapDegradationTier::Capped);
    }

    #[test]
    fn test_supply_cap_graceful_rejection_when_fully_capped() {
        // Cap is already 100% full -> Cannot accept any further deposits
        assert!(RiskManager::enforce_supply_cap_graceful(10_000, 10_000, 100).is_err());
        assert!(RiskManager::enforce_supply_cap_graceful(11_000, 10_000, 100).is_err());
    }

    #[test]
    fn test_supply_cap_graceful_unlimited_cap() {
        // Unlimited cap (0 or negative)
        let res = RiskManager::enforce_supply_cap_graceful(5_000, 0, 10_000).unwrap();
        assert_eq!(res.accepted_amount, 10_000);
        assert!(!res.is_degraded);
        assert_eq!(res.tier, SupplyCapDegradationTier::Normal);
    }

    #[test]
    fn test_supply_cap_graceful_invalid_amount() {
        assert!(RiskManager::enforce_supply_cap_graceful(1_000, 10_000, 0).is_err());
        assert!(RiskManager::enforce_supply_cap_graceful(1_000, 10_000, -50).is_err());
    }

    // ── Borrow Cap Tests (#1026) ──────────────────────────────────────────────

    #[test]
    fn test_borrow_cap_graceful_normal_and_tiers() {
        // Pool borrow cap 10_000. Current borrow 2_000, borrow 3_000 -> Accepted in full (50%, Normal)
        let res = RiskManager::enforce_borrow_cap_graceful(2_000, 10_000, 3_000).unwrap();
        assert_eq!(res.accepted_amount, 3_000);
        assert!(!res.is_degraded);
        assert_eq!(res.remaining_headroom, 5_000);
        assert_eq!(res.tier, BorrowCapDegradationTier::Normal);

        // Elevated: 85%
        let res_elevated = RiskManager::enforce_borrow_cap_graceful(5_000, 10_000, 3_500).unwrap();
        assert_eq!(res_elevated.tier, BorrowCapDegradationTier::Elevated);

        // Critical: 96%
        let res_critical = RiskManager::enforce_borrow_cap_graceful(9_000, 10_000, 600).unwrap();
        assert_eq!(res_critical.tier, BorrowCapDegradationTier::Critical);
    }

    #[test]
    fn test_borrow_cap_graceful_degradation_partial_fill() {
        // Pool borrow cap 10_000, current borrow 8_500, request 3_000 -> Graces to 1_500 remaining
        let res = RiskManager::enforce_borrow_cap_graceful(8_500, 10_000, 3_000).unwrap();
        assert_eq!(res.accepted_amount, 1_500);
        assert!(res.is_degraded);
        assert_eq!(res.remaining_headroom, 0);
        assert_eq!(res.tier, BorrowCapDegradationTier::Capped);
    }

    #[test]
    fn test_borrow_cap_rejection_when_exhausted_or_invalid() {
        // Already 100% capped
        assert!(RiskManager::enforce_borrow_cap_graceful(10_000, 10_000, 100).is_err());
        // Invalid amount
        assert!(RiskManager::enforce_borrow_cap_graceful(2_000, 10_000, 0).is_err());
        assert!(RiskManager::enforce_borrow_cap_graceful(2_000, 10_000, -10).is_err());
    }

    #[test]
    fn test_asset_borrow_cap_config_lifecycle() {
        let mut config = AssetBorrowCapConfig::new(1, 5_000);
        assert_eq!(config.available_headroom(), 5_000);

        // Borrow 3_000
        let b1 = config.execute_borrow_graceful(3_000).unwrap();
        assert_eq!(b1.accepted_amount, 3_000);
        assert_eq!(config.current_total_borrow, 3_000);
        assert_eq!(config.available_headroom(), 2_000);

        // Partial degrade borrow 4_000 -> fills 2_000
        let b2 = config.execute_borrow_graceful(4_000).unwrap();
        assert_eq!(b2.accepted_amount, 2_000);
        assert!(b2.is_degraded);
        assert_eq!(config.available_headroom(), 0);

        // Repay 2_500
        config.execute_repay(2_500).unwrap();
        assert_eq!(config.current_total_borrow, 2_500);
        assert_eq!(config.available_headroom(), 2_500);

        // Update cap
        config.set_cap(10_000).unwrap();
        assert_eq!(config.available_headroom(), 7_500);
        assert!(config.set_cap(-1).is_err());
    }

    // ── Real-Time Health Factor Calculator Tests (#1025) ──────────────────────

    #[test]
    fn test_real_time_health_zero_debt_is_healthy() {
        let report =
            RealTimeHealthCalculator::calculate(100_000, 0, 0, 500, 31_536_000, 7_500, 8_000)
                .unwrap();
        assert_eq!(report.health_factor, i128::MAX);
        assert_eq!(report.status, HealthStatus::Healthy);
        assert!(!report.is_liquidatable);
        assert_eq!(report.real_time_accrued_interest, 0);
        assert_eq!(report.available_borrow_capacity, 75_000);
    }

    #[test]
    fn test_real_time_health_interest_accrual_degrades_health() {
        // Collateral: 100_000, Principal debt: 70_000, Threshold: 80% (weighted collateral = 80_000)
        // At t=0, health factor = 80_000 * 10_000 / 70_000 = 11_428 (Warning)
        let initial =
            RealTimeHealthCalculator::calculate(100_000, 70_000, 0, 1_000, 0, 7_500, 8_000)
                .unwrap();
        assert_eq!(initial.health_factor, 11_428);
        assert_eq!(initial.status, HealthStatus::Warning);
        assert!(!initial.is_liquidatable);

        // After 2 years at 10% (1_000 bps), interest = 70_000 * 10% * 2 = 14_000
        // Total debt = 84_000 > 80_000 weighted collateral -> Health factor drops below 10_000!
        let elapsed_2_years = 31_536_000 * 2;
        let later = RealTimeHealthCalculator::calculate(
            100_000,
            70_000,
            0,
            1_000,
            elapsed_2_years,
            7_500,
            8_000,
        )
        .unwrap();
        assert!(later.real_time_accrued_interest >= 14_000);
        assert!(later.health_factor < BPS_DIVISOR);
        assert_eq!(later.status, HealthStatus::Liquidatable);
        assert!(later.is_liquidatable);
        assert_eq!(later.liquidation_margin, 0);
    }

    #[test]
    fn test_real_time_health_price_shock() {
        // Healthy position
        let report =
            RealTimeHealthCalculator::calculate(100_000, 50_000, 0, 500, 0, 7_500, 8_000).unwrap();
        assert_eq!(report.status, HealthStatus::Healthy);

        // 50% price drop
        let stressed =
            RealTimeHealthCalculator::stress_test_price_drop(&report, 100_000, 5_000, 7_500, 8_000)
                .unwrap();
        // Collateral becomes 50_000, weighted = 40_000 < debt 50_000 -> Liquidatable!
        assert_eq!(stressed.status, HealthStatus::Liquidatable);
        assert!(stressed.is_liquidatable);
    }

    // ── Reserve Factor & Protocol Revenue Tests (#1024) ───────────────────────

    #[test]
    fn test_reserve_factor_validation_and_split() {
        // Valid bounds 0..=5000 bps (0% to 50%)
        assert!(ReserveFactorManager::validate_reserve_factor(0).is_ok());
        assert!(ReserveFactorManager::validate_reserve_factor(1_000).is_ok());
        assert!(ReserveFactorManager::validate_reserve_factor(5_000).is_ok());
        assert!(ReserveFactorManager::validate_reserve_factor(5_001).is_err());
        assert!(ReserveFactorManager::validate_reserve_factor(-1).is_err());

        // 10% reserve factor on 50_000 interest: 5_000 to reserve, 45_000 to lenders
        let split = ReserveFactorManager::calculate_split(50_000, 1_000).unwrap();
        assert_eq!(split.reserve_amount, 5_000);
        assert_eq!(split.lender_amount, 45_000);
        assert_eq!(split.reserve_factor_bps, 1_000);

        // 0 interest returns 0
        let zero_split = ReserveFactorManager::calculate_split(0, 1_000).unwrap();
        assert_eq!(zero_split.reserve_amount, 0);
        assert_eq!(zero_split.lender_amount, 0);
    }

    #[test]
    fn test_protocol_revenue_tracker_lifecycle() {
        let mut tracker = ProtocolRevenueTracker::new();
        assert_eq!(tracker.total_revenue, 0);
        assert_eq!(tracker.available_reserves, 0);

        // Record interest revenue
        tracker
            .record_revenue(RevenueCategory::Interest, 10_000)
            .unwrap();
        assert_eq!(tracker.interest_revenue, 10_000);
        assert_eq!(tracker.available_reserves, 10_000);

        // Record liquidation bonus
        tracker
            .record_revenue(RevenueCategory::LiquidationBonus, 2_500)
            .unwrap();
        assert_eq!(tracker.liquidation_revenue, 2_500);

        // Record flash loan fee
        tracker
            .record_revenue(RevenueCategory::FlashLoan, 1_500)
            .unwrap();
        assert_eq!(tracker.flash_loan_revenue, 1_500);

        assert_eq!(tracker.total_revenue, 14_000);
        assert_eq!(tracker.available_reserves, 14_000);

        // Claim 9_000 to treasury
        tracker.claim_reserves(9_000).unwrap();
        assert_eq!(tracker.available_reserves, 5_000);
        assert_eq!(tracker.total_claimed, 9_000);

        // Claiming more than available fails
        assert!(tracker.claim_reserves(6_000).is_err());
    }

    #[test]
    fn test_project_annual_revenue() {
        // Total borrows 1_000_000, borrow rate 8% (800 bps), reserve factor 10% (1_000 bps)
        // Annual interest = 80_000. Protocol revenue = 8_000.
        let projected =
            ReserveFactorManager::project_annual_revenue(1_000_000, 800, 1_000).unwrap();
        assert_eq!(projected, 8_000);
    }
}
