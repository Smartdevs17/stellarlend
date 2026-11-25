//! Position admission checks built from the shared health guards (issue #1768).
//!
//! This module intentionally contains no arithmetic of its own: it composes
//! `health_guards` so borrow, withdraw and liquidation paths all agree on what
//! "healthy" means.

use health_guards::{
    check_position, not_liquidatable, valid_configuration, GuardError, GuardResult, Position,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PositionInput {
    pub position: Position,
    pub oracle_available: bool,
    pub min_health_bps: u32,
}

/// Pre-flight check for any instruction that mutates a position.
pub fn validate(input: &PositionInput) -> GuardResult<()> {
    if !input.oracle_available {
        return Err(GuardError::MissingOracle);
    }
    check_position(&input.position, input.min_health_bps)
}

/// Borrow guard: the post-borrow position must still clear the minimum.
pub fn can_borrow(input: &PositionInput, additional_debt: i128) -> GuardResult<()> {
    if !input.oracle_available {
        return Err(GuardError::MissingOracle);
    }
    let debt = input.position.debt.checked_add(additional_debt)
        .ok_or(GuardError::InvalidConfiguration)?;
    let next = Position { debt, ..input.position };
    valid_configuration(&next)?;
    check_position(&next, input.min_health_bps)
}

/// Withdraw guard: the post-withdraw position must not be liquidatable.
pub fn can_withdraw(input: &PositionInput, collateral_reduction: i128) -> GuardResult<()> {
    if !input.oracle_available {
        return Err(GuardError::MissingOracle);
    }
    let collateral = input.position.collateral.checked_sub(collateral_reduction)
        .ok_or(GuardError::InvalidConfiguration)?;
    let next = Position { collateral, ..input.position };
    valid_configuration(&next)?;
    not_liquidatable(&next)
}

/// Liquidation guard: the position must actually be unhealthy.
pub fn can_liquidate(input: &PositionInput) -> GuardResult<()> {
    if !input.oracle_available {
        return Err(GuardError::MissingOracle);
    }
    valid_configuration(&input.position)?;
    match not_liquidatable(&input.position) {
        Ok(()) => Err(GuardError::Undercollateralized {
            health_bps: health_guards::health_factor_bps(&input.position).unwrap_or(u32::MAX),
        }),
        Err(error) => allow_liquidation(error),
    }
}

fn allow_liquidation(error: GuardError) -> GuardResult<()> {
    match error {
        GuardError::Liquidatable { .. } => Ok(()),
        other => Err(other),
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    fn input() -> PositionInput {
        PositionInput {
            position: Position {
                collateral: 20_000,
                debt: 10_000,
                collateral_factor_bps: 8_000,
                liquidation_threshold_bps: 8_500,
            },
            oracle_available: true,
            min_health_bps: 12_000,
        }
    }

    #[test]
    fn validate_accepts_a_healthy_position() {
        assert_eq!(validate(&input()), Ok(()));
    }

    #[test]
    fn missing_oracle_blocks_every_action() {
        let mut case = input();
        case.oracle_available = false;
        assert_eq!(validate(&case), Err(GuardError::MissingOracle));
        assert_eq!(can_borrow(&case, 1), Err(GuardError::MissingOracle));
        assert_eq!(can_withdraw(&case, 1), Err(GuardError::MissingOracle));
        assert_eq!(can_liquidate(&case), Err(GuardError::MissingOracle));
    }

    #[test]
    fn borrow_must_keep_health_above_the_minimum() {
        assert_eq!(can_borrow(&input(), 2_000), Ok(()));
        assert!(can_borrow(&input(), 10_000).is_err());
    }

    #[test]
    fn only_unhealthy_positions_can_be_liquidated() {
        assert_eq!(can_liquidate(&input()), Err(GuardError::Undercollateralized { health_bps: 16_000 }));
        let mut risky = input();
        risky.position.collateral = 8_600;
        assert_eq!(can_liquidate(&risky), Ok(()));
    }
}
