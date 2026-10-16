//! Flash loan pool rules: configurable fee and limits (issue #1019).
//!
//! Pure arithmetic and validation with no storage or `Env`, so the rules can be
//! unit tested directly and reused by any pool contract. A contract stores a
//! [`FlashLoanConfig`], asks [`FlashLoanConfig::check_loan`] before sending funds,
//! and [`FlashLoanConfig::check_repayment`] after the receiver callback returns.
//!
//! ## Rules
//!
//! - `fee_bps` is at most [`MAX_FEE_BPS`] (10%). The fee is rounded **up**, so a
//!   non-zero fee rate never rounds a small loan's fee down to zero.
//! - A loan must be within `[min_amount, max_amount]`.
//! - A single loan may take at most `max_pool_share_bps` of the pool's available
//!   liquidity, so one loan cannot drain the pool.
//! - Repayment is checked against balances: the pool must end the transaction
//!   holding at least `balance_before + fee`.
//!
//! See `docs/FLASH_LOAN_MODULE.md`.

use crate::FlashLoanMetrics;

/// Basis points in 100%.
pub const BPS_DIVISOR: i128 = 10_000;
/// Highest fee a pool may configure: 10%.
pub const MAX_FEE_BPS: i128 = 1_000;
/// Default fee: 9 bps (0.09%).
pub const DEFAULT_FEE_BPS: i128 = 9;
/// Default share of available liquidity one loan may take: 90%.
pub const DEFAULT_MAX_POOL_SHARE_BPS: i128 = 9_000;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FlashLoanError {
    /// A configuration value is out of range.
    InvalidConfig,
    /// Flash loans are switched off for this pool.
    Disabled,
    /// The requested amount is zero or negative.
    InvalidAmount,
    /// The requested amount is below `min_amount`.
    BelowMinimum,
    /// The requested amount is above `max_amount`.
    AboveMaximum,
    /// The pool does not hold enough liquidity for the amount.
    InsufficientLiquidity,
    /// The amount exceeds `max_pool_share_bps` of the available liquidity.
    ExceedsPoolShare,
    /// The pool ended with less than `balance_before + fee`.
    RepaymentTooLow,
    /// Arithmetic overflow.
    Overflow,
}

/// Fee and limits for one flash loan pool.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FlashLoanConfig {
    /// Fee in basis points of the borrowed amount (0..=`MAX_FEE_BPS`).
    pub fee_bps: i128,
    /// Smallest loan accepted (>= 1).
    pub min_amount: i128,
    /// Largest loan accepted (>= `min_amount`).
    pub max_amount: i128,
    /// Largest share of available liquidity one loan may take (1..=10_000).
    pub max_pool_share_bps: i128,
    /// When false, every loan is refused.
    pub enabled: bool,
}

impl FlashLoanConfig {
    /// Builds a validated, enabled configuration.
    pub fn new(
        fee_bps: i128,
        min_amount: i128,
        max_amount: i128,
        max_pool_share_bps: i128,
    ) -> Result<Self, FlashLoanError> {
        let config = Self {
            fee_bps,
            min_amount,
            max_amount,
            max_pool_share_bps,
            enabled: true,
        };
        config.validate()?;
        Ok(config)
    }

    /// Checks every field is in range and internally consistent.
    pub fn validate(&self) -> Result<(), FlashLoanError> {
        if self.fee_bps < 0 || self.fee_bps > MAX_FEE_BPS {
            return Err(FlashLoanError::InvalidConfig);
        }
        if self.min_amount < 1 || self.max_amount < self.min_amount {
            return Err(FlashLoanError::InvalidConfig);
        }
        if self.max_pool_share_bps < 1 || self.max_pool_share_bps > BPS_DIVISOR {
            return Err(FlashLoanError::InvalidConfig);
        }
        Ok(())
    }

    /// Fee for `amount`, rounded up: `ceil(amount * fee_bps / 10_000)`.
    pub fn calculate_fee(&self, amount: i128) -> Result<i128, FlashLoanError> {
        if amount <= 0 {
            return Err(FlashLoanError::InvalidAmount);
        }
        let scaled = amount
            .checked_mul(self.fee_bps)
            .ok_or(FlashLoanError::Overflow)?;
        let quotient = scaled / BPS_DIVISOR;
        if scaled % BPS_DIVISOR == 0 {
            Ok(quotient)
        } else {
            quotient.checked_add(1).ok_or(FlashLoanError::Overflow)
        }
    }

    /// Principal plus fee: what the receiver must return.
    pub fn required_repayment(&self, amount: i128) -> Result<i128, FlashLoanError> {
        let fee = self.calculate_fee(amount)?;
        amount.checked_add(fee).ok_or(FlashLoanError::Overflow)
    }

    /// Validates a loan request against the limits and the pool's liquidity.
    /// Returns the fee the borrower will owe.
    pub fn check_loan(
        &self,
        amount: i128,
        available_liquidity: i128,
    ) -> Result<i128, FlashLoanError> {
        if !self.enabled {
            return Err(FlashLoanError::Disabled);
        }
        if amount <= 0 {
            return Err(FlashLoanError::InvalidAmount);
        }
        if amount < self.min_amount {
            return Err(FlashLoanError::BelowMinimum);
        }
        if amount > self.max_amount {
            return Err(FlashLoanError::AboveMaximum);
        }
        if amount > available_liquidity {
            return Err(FlashLoanError::InsufficientLiquidity);
        }
        let cap = available_liquidity
            .checked_mul(self.max_pool_share_bps)
            .ok_or(FlashLoanError::Overflow)?
            / BPS_DIVISOR;
        if amount > cap {
            return Err(FlashLoanError::ExceedsPoolShare);
        }
        self.calculate_fee(amount)
    }

    /// After the receiver callback: the pool must hold at least
    /// `balance_before + fee`. Checking balances (not the callback's word) is
    /// what makes the loan safe against a receiver that lies.
    pub fn check_repayment(
        &self,
        amount: i128,
        balance_before: i128,
        balance_after: i128,
    ) -> Result<i128, FlashLoanError> {
        let fee = self.calculate_fee(amount)?;
        let required = balance_before
            .checked_add(fee)
            .ok_or(FlashLoanError::Overflow)?;
        if balance_after < required {
            return Err(FlashLoanError::RepaymentTooLow);
        }
        Ok(fee)
    }
}

impl Default for FlashLoanConfig {
    fn default() -> Self {
        Self {
            fee_bps: DEFAULT_FEE_BPS,
            min_amount: 1,
            max_amount: i128::MAX,
            max_pool_share_bps: DEFAULT_MAX_POOL_SHARE_BPS,
            enabled: true,
        }
    }
}

/// Adds one completed loan to the running metrics. Nothing changes on error.
pub fn record_loan(
    metrics: &mut FlashLoanMetrics,
    amount: i128,
    fee: i128,
) -> Result<(), FlashLoanError> {
    let loans = metrics
        .total_flash_loans
        .checked_add(1)
        .ok_or(FlashLoanError::Overflow)?;
    let volume = metrics
        .total_volume
        .checked_add(amount)
        .ok_or(FlashLoanError::Overflow)?;
    let fees = metrics
        .total_fees_collected
        .checked_add(fee)
        .ok_or(FlashLoanError::Overflow)?;
    metrics.total_flash_loans = loans;
    metrics.total_volume = volume;
    metrics.total_fees_collected = fees;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> FlashLoanConfig {
        FlashLoanConfig::new(9, 100, 1_000_000, 5_000).unwrap()
    }

    #[test]
    fn valid_config_is_accepted() {
        assert!(FlashLoanConfig::new(0, 1, 1, 1).is_ok());
        assert!(FlashLoanConfig::new(MAX_FEE_BPS, 1, 10, BPS_DIVISOR).is_ok());
        assert!(FlashLoanConfig::default().validate().is_ok());
    }

    #[test]
    fn invalid_config_is_rejected() {
        assert_eq!(
            FlashLoanConfig::new(-1, 1, 10, 5_000),
            Err(FlashLoanError::InvalidConfig)
        );
        assert_eq!(
            FlashLoanConfig::new(MAX_FEE_BPS + 1, 1, 10, 5_000),
            Err(FlashLoanError::InvalidConfig)
        );
        assert_eq!(
            FlashLoanConfig::new(9, 0, 10, 5_000),
            Err(FlashLoanError::InvalidConfig)
        );
        assert_eq!(
            FlashLoanConfig::new(9, 10, 9, 5_000),
            Err(FlashLoanError::InvalidConfig)
        );
        assert_eq!(
            FlashLoanConfig::new(9, 1, 10, 0),
            Err(FlashLoanError::InvalidConfig)
        );
        assert_eq!(
            FlashLoanConfig::new(9, 1, 10, BPS_DIVISOR + 1),
            Err(FlashLoanError::InvalidConfig)
        );
    }

    #[test]
    fn fee_is_exact_when_it_divides_evenly() {
        // 10_000_000 * 9 / 10_000 = 9_000
        assert_eq!(config().calculate_fee(10_000_000), Ok(9_000));
    }

    #[test]
    fn fee_rounds_up_and_is_never_zero_for_a_non_zero_rate() {
        // 100 * 9 / 10_000 = 0.09 -> rounds up to 1, not 0.
        assert_eq!(config().calculate_fee(100), Ok(1));
        assert_eq!(config().calculate_fee(1), Ok(1));
    }

    #[test]
    fn zero_rate_charges_no_fee() {
        let free = FlashLoanConfig::new(0, 1, 1_000, 10_000).unwrap();
        assert_eq!(free.calculate_fee(500), Ok(0));
    }

    #[test]
    fn fee_rejects_non_positive_amounts_and_overflow() {
        assert_eq!(config().calculate_fee(0), Err(FlashLoanError::InvalidAmount));
        assert_eq!(config().calculate_fee(-5), Err(FlashLoanError::InvalidAmount));
        assert_eq!(
            config().calculate_fee(i128::MAX),
            Err(FlashLoanError::Overflow)
        );
    }

    #[test]
    fn required_repayment_is_principal_plus_fee() {
        assert_eq!(config().required_repayment(10_000_000), Ok(10_009_000));
    }

    #[test]
    fn loan_within_limits_returns_the_fee() {
        // 10_000 * 9 / 10_000 = 9 exactly.
        assert_eq!(config().check_loan(10_000, 100_000), Ok(9));
    }

    #[test]
    fn disabled_pool_refuses_every_loan() {
        let mut c = config();
        c.enabled = false;
        assert_eq!(c.check_loan(10_000, 100_000), Err(FlashLoanError::Disabled));
    }

    #[test]
    fn amount_limits_are_enforced_at_the_boundaries() {
        let c = config();
        assert_eq!(c.check_loan(0, 1_000_000), Err(FlashLoanError::InvalidAmount));
        assert_eq!(c.check_loan(99, 1_000_000), Err(FlashLoanError::BelowMinimum));
        assert!(c.check_loan(100, 1_000_000).is_ok());
        // 1_000_000 is the max and 50% of 2_000_000 liquidity.
        assert!(c.check_loan(1_000_000, 2_000_000).is_ok());
        assert_eq!(
            c.check_loan(1_000_001, 10_000_000),
            Err(FlashLoanError::AboveMaximum)
        );
    }

    #[test]
    fn loan_cannot_exceed_available_liquidity() {
        assert_eq!(
            config().check_loan(5_000, 4_999),
            Err(FlashLoanError::InsufficientLiquidity)
        );
    }

    #[test]
    fn loan_cannot_take_more_than_the_pool_share() {
        // Cap is 50% of 10_000 = 5_000.
        let c = config();
        assert!(c.check_loan(5_000, 10_000).is_ok());
        assert_eq!(
            c.check_loan(5_001, 10_000),
            Err(FlashLoanError::ExceedsPoolShare)
        );
    }

    #[test]
    fn repayment_requires_principal_plus_fee_in_the_pool() {
        let c = config();
        // amount 10_000_000 -> fee 9_000.
        assert_eq!(
            c.check_repayment(10_000_000, 50_000_000, 50_009_000),
            Ok(9_000)
        );
        assert_eq!(
            c.check_repayment(10_000_000, 50_000_000, 50_008_999),
            Err(FlashLoanError::RepaymentTooLow)
        );
        // Principal returned but no fee.
        assert_eq!(
            c.check_repayment(10_000_000, 50_000_000, 50_000_000),
            Err(FlashLoanError::RepaymentTooLow)
        );
    }

    #[test]
    fn metrics_accumulate_and_report_overflow() {
        let mut metrics = FlashLoanMetrics {
            total_flash_loans: 0,
            total_volume: 0,
            total_fees_collected: 0,
        };
        record_loan(&mut metrics, 10_000, 9).unwrap();
        record_loan(&mut metrics, 5_000, 5).unwrap();
        assert_eq!(metrics.total_flash_loans, 2);
        assert_eq!(metrics.total_volume, 15_000);
        assert_eq!(metrics.total_fees_collected, 14);

        let mut full = FlashLoanMetrics {
            total_flash_loans: 0,
            total_volume: i128::MAX,
            total_fees_collected: 0,
        };
        assert_eq!(record_loan(&mut full, 1, 0), Err(FlashLoanError::Overflow));
        // A failed update leaves the metrics untouched.
        assert_eq!(full.total_flash_loans, 0);
    }
}
