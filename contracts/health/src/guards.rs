//! Composable health guards for lending positions (issue #1768).
//!
//! Every rule is a small `fn(&Position) -> GuardResult<()>` so callers can
//! compose exactly the checks a transaction needs (borrow, withdraw,
//! liquidate) without duplicating arithmetic or threshold comparisons.

use core::cmp::max;

pub const BPS_DENOMINATOR: u32 = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    /// Collateral value in price decimals.
    pub collateral: i128,
    /// Outstanding debt in price decimals.
    pub debt: i128,
    /// Risk-adjusted collateral weight, in basis points.
    pub collateral_factor_bps: u32,
    /// Liquidation trigger, in basis points.
    pub liquidation_threshold_bps: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardError {
    InvalidConfiguration,
    MissingOracle,
    ZeroDebt,
    Undercollateralized { health_bps: u32 },
    Liquidatable { health_bps: u32 },
}

pub type GuardResult<T> = Result<T, GuardError>;

/// Risk-weighted collateral / debt, expressed in basis points.
/// `None` means the position has no debt and therefore no health factor.
pub fn health_factor_bps(position: &Position) -> Option<u32> {
    if position.debt <= 0 {
        return None;
    }
    let weighted = (position.collateral as i128)
        .checked_mul(position.collateral_factor_bps as i128)?
        / (BPS_DENOMINATOR as i128);
    if weighted <= 0 {
        return Some(0);
    }
    let ratio = weighted.checked_div(position.debt)?;
    Some(ratio.clamp(0, u32::MAX as i128) as u32)
}

/// Configuration must be sane before any health math is trusted.
pub fn valid_configuration(position: &Position) -> GuardResult<()> {
    if position.collateral < 0
        || position.debt < 0
        || position.collateral_factor_bps == 0
        || position.collateral_factor_bps > BPS_DENOMINATOR
        || position.liquidation_threshold_bps == 0
        || position.liquidation_threshold_bps > BPS_DENOMINATOR
        || position.liquidation_threshold_bps < position.collateral_factor_bps
    {
        return Err(GuardError::InvalidConfiguration);
    }
    Ok(())
}

pub fn has_debt(position: &Position) -> GuardResult<()> {
    if position.debt <= 0 {
        Err(GuardError::ZeroDebt)
    } else {
        Ok(())
    }
}

/// Borrow/withdraw guard: health must stay above `min_health_bps`.
pub fn above_min_health(position: &Position, min_health_bps: u32) -> GuardResult<()> {
    let health = health_factor_bps(position).unwrap_or(u32::MAX);
    if health < min_health_bps {
        Err(GuardError::Undercollateralized { health_bps: health })
    } else {
        Ok(())
    }
}

/// Liquidation guard: health must be strictly above the threshold.
pub fn not_liquidatable(position: &Position) -> GuardResult<()> {
    let health = health_factor_bps(position).unwrap_or(u32::MAX);
    if health <= position.liquidation_threshold_bps {
        Err(GuardError::Liquidatable { health_bps: health })
    } else {
        Ok(())
    }
}

/// Compose the full pre-flight check used by mutating position instructions.
pub fn check_position(position: &Position, min_health_bps: u32) -> GuardResult<()> {
    valid_configuration(position)?;
    has_debt(position)?;
    let min = max(min_health_bps, position.liquidation_threshold_bps);
    above_min_health(position, min)?;
    not_liquidatable(position)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    fn healthy() -> Position {
        Position {
            collateral: 20_000,
            debt: 10_000,
            collateral_factor_bps: 8_000,
            liquidation_threshold_bps: 8_500,
        }
    }

    #[test]
    fn health_factor_uses_risk_weighted_collateral() {
        assert_eq!(health_factor_bps(&healthy()), Some(16_000));
        assert_eq!(
            health_factor_bps(&Position { debt: 0, ..healthy() }),
            None
        );
    }

    #[test]
    fn guards_are_composable() {
        assert_eq!(check_position(&healthy(), 12_000), Ok(()));
        assert_eq!(
            above_min_health(&healthy(), 17_000),
            Err(GuardError::Undercollateralized { health_bps: 16_000 })
        );
    }

    #[test]
    fn liquidatable_positions_are_rejected() {
        let risky = Position {
            collateral: 8_600,
            debt: 10_000,
            ..healthy()
        };
        assert_eq!(
            not_liquidatable(&risky),
            Err(GuardError::Liquidatable { health_bps: 6_880 })
        );
    }

    #[test]
    fn invalid_configuration_is_rejected_before_arithmetic() {
        let broken = Position {
            liquidation_threshold_bps: 7_000,
            ..healthy()
        };
        assert_eq!(
            check_position(&broken, 10_000),
            Err(GuardError::InvalidConfiguration)
        );
    }
}
