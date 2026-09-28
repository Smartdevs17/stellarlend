//! Per-collateral leverage multipliers.
//!
//! Each collateral asset carries a collateral factor (LTV, in bps) that
//! reflects its risk profile. Looping a position at that LTV gives a
//! theoretical maximum leverage of `1 / (1 - LTV)`. Governance can cap it
//! lower per asset with `max_multiplier_bps`. Multipliers are in bps:
//! `10_000` = 1x, `25_000` = 2.5x.

use crate::checked::{checked_mul_div, BPS_DIVISOR};

/// 1x leverage expressed in bps.
pub const ONE_X_BPS: i128 = BPS_DIVISOR;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LeverageError {
    /// Collateral factor outside `0..BPS_DIVISOR` (100% LTV allows infinite leverage).
    InvalidCollateralFactor,
    /// Configured cap is below 1x.
    InvalidCap,
    /// Requested multiplier is below 1x.
    BelowOneX,
    /// Requested multiplier exceeds the asset's allowed maximum.
    ExceedsMax,
    Overflow,
}

/// Leverage configuration for one collateral asset.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CollateralLeverage {
    pub collateral_factor_bps: i128,
    pub max_multiplier_bps: i128,
}

impl CollateralLeverage {
    pub fn new(
        collateral_factor_bps: i128,
        max_multiplier_bps: i128,
    ) -> Result<Self, LeverageError> {
        if !(0..BPS_DIVISOR).contains(&collateral_factor_bps) {
            return Err(LeverageError::InvalidCollateralFactor);
        }
        if max_multiplier_bps < ONE_X_BPS {
            return Err(LeverageError::InvalidCap);
        }
        Ok(Self {
            collateral_factor_bps,
            max_multiplier_bps,
        })
    }

    /// `min(1 / (1 - LTV), cap)` in bps, rounded down.
    pub fn max_multiplier(&self) -> Result<i128, LeverageError> {
        let theoretical = checked_mul_div(
            BPS_DIVISOR,
            BPS_DIVISOR,
            BPS_DIVISOR - self.collateral_factor_bps,
        )
        .map_err(|_| LeverageError::Overflow)?;
        Ok(theoretical.min(self.max_multiplier_bps))
    }

    /// Checks a requested multiplier against this asset's limits.
    pub fn validate(&self, requested_bps: i128) -> Result<(), LeverageError> {
        if requested_bps < ONE_X_BPS {
            return Err(LeverageError::BelowOneX);
        }
        if requested_bps > self.max_multiplier()? {
            return Err(LeverageError::ExceedsMax);
        }
        Ok(())
    }

    /// Total position size for `equity` at `multiplier_bps`, after validation.
    pub fn position_size(&self, equity: i128, multiplier_bps: i128) -> Result<i128, LeverageError> {
        self.validate(multiplier_bps)?;
        checked_mul_div(equity, multiplier_bps, BPS_DIVISOR).map_err(|_| LeverageError::Overflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theoretical_max_follows_ltv() {
        // 75% LTV → 4x, 50% → 2x, 0% → 1x.
        assert_eq!(
            CollateralLeverage::new(7_500, 100_000)
                .unwrap()
                .max_multiplier(),
            Ok(40_000)
        );
        assert_eq!(
            CollateralLeverage::new(5_000, 100_000)
                .unwrap()
                .max_multiplier(),
            Ok(20_000)
        );
        assert_eq!(
            CollateralLeverage::new(0, 100_000)
                .unwrap()
                .max_multiplier(),
            Ok(ONE_X_BPS)
        );
    }

    #[test]
    fn configured_cap_wins_when_lower() {
        let volatile = CollateralLeverage::new(7_500, 25_000).unwrap();
        assert_eq!(volatile.max_multiplier(), Ok(25_000));
        assert_eq!(volatile.validate(25_000), Ok(()));
        assert_eq!(volatile.validate(25_001), Err(LeverageError::ExceedsMax));
    }

    #[test]
    fn rejects_bad_config_and_requests() {
        assert_eq!(
            CollateralLeverage::new(10_000, 20_000),
            Err(LeverageError::InvalidCollateralFactor)
        );
        assert_eq!(
            CollateralLeverage::new(-1, 20_000),
            Err(LeverageError::InvalidCollateralFactor)
        );
        assert_eq!(
            CollateralLeverage::new(5_000, 9_999),
            Err(LeverageError::InvalidCap)
        );
        let cfg = CollateralLeverage::new(5_000, 20_000).unwrap();
        assert_eq!(cfg.validate(9_999), Err(LeverageError::BelowOneX));
    }

    #[test]
    fn position_size_scales_equity() {
        let cfg = CollateralLeverage::new(8_000, 30_000).unwrap();
        assert_eq!(cfg.position_size(1_000, 30_000), Ok(3_000));
        assert_eq!(
            cfg.position_size(1_000, 30_001),
            Err(LeverageError::ExceedsMax)
        );
    }
}
