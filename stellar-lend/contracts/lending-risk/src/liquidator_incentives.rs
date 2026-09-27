//! Liquidator incentives and MEV protection for liquidations (issue #1021).
//!
//! **Incentives.** A liquidator earns the asset's base liquidation bonus plus a
//! loyalty bonus that grows with the volume it has already liquidated
//! ([`INCENTIVE_TIERS`]). The total is capped by both the asset's own maximum
//! bonus and [`MAX_TOTAL_BONUS_BPS`], so incentives can never push a position
//! below full coverage.
//!
//! **MEV protection.** [`MevGuardConfig::check_liquidation`] refuses a
//! liquidation that looks like ordering abuse:
//!
//! - a repeat on the same position within `min_ledgers_between_liquidations`
//!   (stops back-to-back partial liquidations bundled around a price move),
//! - more than `max_liquidations_per_ledger` in one ledger (bounds how much one
//!   ledger can be manipulated), and
//! - an observed price more than `max_price_deviation_bps` away from a
//!   reference price such as a TWAP (a sandwich or oracle-push signature).
//!
//! Pure functions and plain structs, no storage or `Env`; the contract supplies
//! ledger numbers and prices. See `docs/LIQUIDATOR_INCENTIVES_MEV.md`.

use lending_types::BPS_DIVISOR;
use stellarlend_safe_math::{safe_div, safe_mul, MathError};

/// Highest total liquidation bonus (base + loyalty) ever paid: 15%.
pub const MAX_TOTAL_BONUS_BPS: i128 = 1_500;

/// Loyalty tier: a liquidator that has liquidated at least `min_volume` earns
/// `bonus_bps` on top of the base bonus. Ordered by ascending `min_volume`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct IncentiveTier {
    pub min_volume: i128,
    pub bonus_bps: i128,
}

/// Loyalty schedule (volume in the protocol's base units).
pub const INCENTIVE_TIERS: [IncentiveTier; 4] = [
    IncentiveTier {
        min_volume: 0,
        bonus_bps: 0,
    },
    IncentiveTier {
        min_volume: 100_000_000,
        bonus_bps: 25,
    },
    IncentiveTier {
        min_volume: 1_000_000_000,
        bonus_bps: 50,
    },
    IncentiveTier {
        min_volume: 10_000_000_000,
        bonus_bps: 100,
    },
];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum IncentiveError {
    /// A bonus or volume is negative, or a cap is out of range.
    InvalidParameter,
    /// Arithmetic overflow.
    Overflow,
}

impl From<MathError> for IncentiveError {
    fn from(_: MathError) -> Self {
        IncentiveError::Overflow
    }
}

/// Loyalty bonus for a liquidator that has liquidated `cumulative_volume`:
/// the bonus of the highest tier reached (0 for a negative volume).
pub fn loyalty_bonus_bps(cumulative_volume: i128) -> i128 {
    let mut bonus = 0;
    for tier in INCENTIVE_TIERS.iter() {
        if cumulative_volume >= tier.min_volume {
            bonus = tier.bonus_bps;
        }
    }
    bonus
}

/// Total bonus in basis points: `base + loyalty`, capped at the asset's own
/// maximum and at [`MAX_TOTAL_BONUS_BPS`]. The base bonus is never reduced below
/// what the asset configured, unless that base already exceeds a cap.
pub fn effective_bonus_bps(
    base_bonus_bps: i128,
    cumulative_volume: i128,
    asset_max_bonus_bps: i128,
) -> Result<i128, IncentiveError> {
    if base_bonus_bps < 0 || asset_max_bonus_bps < 0 || asset_max_bonus_bps > MAX_TOTAL_BONUS_BPS {
        return Err(IncentiveError::InvalidParameter);
    }
    let total = base_bonus_bps
        .checked_add(loyalty_bonus_bps(cumulative_volume))
        .ok_or(IncentiveError::Overflow)?;
    let cap = if asset_max_bonus_bps < MAX_TOTAL_BONUS_BPS {
        asset_max_bonus_bps
    } else {
        MAX_TOTAL_BONUS_BPS
    };
    Ok(if total < cap { total } else { cap })
}

/// Bonus collateral value owed on `repaid_value`: `repaid * bonus / 10_000`.
pub fn liquidator_reward(repaid_value: i128, bonus_bps: i128) -> Result<i128, IncentiveError> {
    if repaid_value < 0 || bonus_bps < 0 {
        return Err(IncentiveError::InvalidParameter);
    }
    Ok(safe_mul(repaid_value, bonus_bps).and_then(|v| safe_div(v, BPS_DIVISOR))?)
}

/// Running record of one liquidator's activity.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct LiquidatorStats {
    pub total_volume: i128,
    pub liquidation_count: u64,
    pub total_rewards: i128,
}

impl LiquidatorStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bonus this liquidator would earn today for an asset with `base_bonus_bps`.
    pub fn current_bonus_bps(
        &self,
        base_bonus_bps: i128,
        asset_max_bonus_bps: i128,
    ) -> Result<i128, IncentiveError> {
        effective_bonus_bps(base_bonus_bps, self.total_volume, asset_max_bonus_bps)
    }

    /// Records a completed liquidation. Nothing changes if any total would overflow.
    pub fn record(&mut self, repaid_value: i128, reward: i128) -> Result<(), IncentiveError> {
        if repaid_value < 0 || reward < 0 {
            return Err(IncentiveError::InvalidParameter);
        }
        let volume = self
            .total_volume
            .checked_add(repaid_value)
            .ok_or(IncentiveError::Overflow)?;
        let rewards = self
            .total_rewards
            .checked_add(reward)
            .ok_or(IncentiveError::Overflow)?;
        let count = self
            .liquidation_count
            .checked_add(1)
            .ok_or(IncentiveError::Overflow)?;
        self.total_volume = volume;
        self.total_rewards = rewards;
        self.liquidation_count = count;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MevGuardError {
    /// A configuration value is out of range.
    InvalidConfig,
    /// The position was liquidated too recently.
    CooldownActive,
    /// This ledger has already reached its liquidation limit.
    LedgerCapReached,
    /// A price is zero or negative.
    InvalidPrice,
    /// The observed price is too far from the reference price.
    PriceDeviationTooHigh,
    /// Arithmetic overflow.
    Overflow,
}

/// Limits applied to liquidations.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct MevGuardConfig {
    /// Ledgers that must pass before the same position can be liquidated again.
    pub min_ledgers_between_liquidations: u32,
    /// Most liquidations one ledger may contain.
    pub max_liquidations_per_ledger: u32,
    /// Largest allowed gap between observed and reference price (basis points).
    pub max_price_deviation_bps: i128,
}

impl MevGuardConfig {
    pub fn new(
        min_ledgers_between_liquidations: u32,
        max_liquidations_per_ledger: u32,
        max_price_deviation_bps: i128,
    ) -> Result<Self, MevGuardError> {
        let config = Self {
            min_ledgers_between_liquidations,
            max_liquidations_per_ledger,
            max_price_deviation_bps,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), MevGuardError> {
        if self.max_liquidations_per_ledger == 0
            || self.max_price_deviation_bps < 0
            || self.max_price_deviation_bps > BPS_DIVISOR
        {
            return Err(MevGuardError::InvalidConfig);
        }
        Ok(())
    }

    /// Absolute gap between `observed_price` and `reference_price` in basis
    /// points of the reference price.
    pub fn price_deviation_bps(
        reference_price: i128,
        observed_price: i128,
    ) -> Result<i128, MevGuardError> {
        if reference_price <= 0 || observed_price <= 0 {
            return Err(MevGuardError::InvalidPrice);
        }
        let gap = if observed_price > reference_price {
            observed_price - reference_price
        } else {
            reference_price - observed_price
        };
        let scaled = gap
            .checked_mul(BPS_DIVISOR)
            .ok_or(MevGuardError::Overflow)?;
        Ok(scaled / reference_price)
    }

    /// Decides whether a liquidation may proceed.
    ///
    /// * `last_liquidated_ledger` - ledger of the previous liquidation of this position, if any
    /// * `current_ledger` - ledger of this transaction
    /// * `liquidations_this_ledger` - liquidations already accepted in `current_ledger`
    /// * `reference_price` / `observed_price` - e.g. TWAP and the spot price used now
    pub fn check_liquidation(
        &self,
        last_liquidated_ledger: Option<u32>,
        current_ledger: u32,
        liquidations_this_ledger: u32,
        reference_price: i128,
        observed_price: i128,
    ) -> Result<(), MevGuardError> {
        self.validate()?;

        if let Some(last) = last_liquidated_ledger {
            // A `last` in the future can only be stale or hostile data: treat as too recent.
            let elapsed = current_ledger.checked_sub(last);
            match elapsed {
                Some(gap) if gap >= self.min_ledgers_between_liquidations => {}
                _ => return Err(MevGuardError::CooldownActive),
            }
        }

        if liquidations_this_ledger >= self.max_liquidations_per_ledger {
            return Err(MevGuardError::LedgerCapReached);
        }

        let deviation = Self::price_deviation_bps(reference_price, observed_price)?;
        if deviation > self.max_price_deviation_bps {
            return Err(MevGuardError::PriceDeviationTooHigh);
        }

        Ok(())
    }
}

impl Default for MevGuardConfig {
    /// 2 ledgers between liquidations of one position, 20 per ledger, 5% price band.
    fn default() -> Self {
        Self {
            min_ledgers_between_liquidations: 2,
            max_liquidations_per_ledger: 20,
            max_price_deviation_bps: 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loyalty_tiers_are_ordered_and_start_at_zero() {
        assert_eq!(INCENTIVE_TIERS[0].min_volume, 0);
        assert_eq!(INCENTIVE_TIERS[0].bonus_bps, 0);
        for pair in INCENTIVE_TIERS.windows(2) {
            assert!(pair[1].min_volume > pair[0].min_volume);
            assert!(pair[1].bonus_bps > pair[0].bonus_bps);
        }
    }

    #[test]
    fn loyalty_bonus_uses_the_highest_tier_reached() {
        assert_eq!(loyalty_bonus_bps(-5), 0);
        assert_eq!(loyalty_bonus_bps(0), 0);
        assert_eq!(loyalty_bonus_bps(99_999_999), 0);
        assert_eq!(loyalty_bonus_bps(100_000_000), 25);
        assert_eq!(loyalty_bonus_bps(999_999_999), 25);
        assert_eq!(loyalty_bonus_bps(1_000_000_000), 50);
        assert_eq!(loyalty_bonus_bps(10_000_000_000), 100);
        assert_eq!(loyalty_bonus_bps(i128::MAX), 100);
    }

    #[test]
    fn effective_bonus_adds_loyalty_to_the_base() {
        assert_eq!(effective_bonus_bps(500, 0, 1_500), Ok(500));
        assert_eq!(effective_bonus_bps(500, 1_000_000_000, 1_500), Ok(550));
    }

    #[test]
    fn effective_bonus_is_capped_by_the_asset_maximum() {
        // Base 500 + loyalty 100 = 600, but this asset allows at most 550.
        assert_eq!(effective_bonus_bps(500, 10_000_000_000, 550), Ok(550));
    }

    #[test]
    fn effective_bonus_is_capped_globally() {
        assert_eq!(
            effective_bonus_bps(1_450, 10_000_000_000, MAX_TOTAL_BONUS_BPS),
            Ok(MAX_TOTAL_BONUS_BPS)
        );
    }

    #[test]
    fn effective_bonus_rejects_bad_inputs() {
        assert_eq!(
            effective_bonus_bps(-1, 0, 500),
            Err(IncentiveError::InvalidParameter)
        );
        assert_eq!(
            effective_bonus_bps(100, 0, -1),
            Err(IncentiveError::InvalidParameter)
        );
        assert_eq!(
            effective_bonus_bps(100, 0, MAX_TOTAL_BONUS_BPS + 1),
            Err(IncentiveError::InvalidParameter)
        );
        assert_eq!(
            effective_bonus_bps(i128::MAX, 10_000_000_000, MAX_TOTAL_BONUS_BPS),
            Err(IncentiveError::Overflow)
        );
    }

    #[test]
    fn reward_is_the_bonus_share_of_the_repaid_value() {
        assert_eq!(liquidator_reward(100_000, 500), Ok(5_000));
        assert_eq!(liquidator_reward(0, 500), Ok(0));
        assert_eq!(liquidator_reward(100_000, 0), Ok(0));
        assert_eq!(
            liquidator_reward(-1, 500),
            Err(IncentiveError::InvalidParameter)
        );
        assert_eq!(
            liquidator_reward(i128::MAX, 500),
            Err(IncentiveError::Overflow)
        );
    }

    #[test]
    fn stats_accumulate_and_raise_the_bonus_tier() {
        let mut stats = LiquidatorStats::new();
        assert_eq!(stats.current_bonus_bps(500, 1_500), Ok(500));

        stats.record(600_000_000, 30_000_000).unwrap();
        stats.record(600_000_000, 30_000_000).unwrap();
        assert_eq!(stats.total_volume, 1_200_000_000);
        assert_eq!(stats.total_rewards, 60_000_000);
        assert_eq!(stats.liquidation_count, 2);
        // 1.2e9 reaches the 1e9 tier: +50.
        assert_eq!(stats.current_bonus_bps(500, 1_500), Ok(550));
    }

    #[test]
    fn stats_reject_negative_values_and_stay_unchanged_on_overflow() {
        let mut stats = LiquidatorStats::new();
        assert_eq!(stats.record(-1, 0), Err(IncentiveError::InvalidParameter));
        assert_eq!(stats.record(0, -1), Err(IncentiveError::InvalidParameter));

        stats.record(10, 1).unwrap();
        let mut near_limit = LiquidatorStats {
            total_volume: i128::MAX,
            liquidation_count: 3,
            total_rewards: 0,
        };
        assert_eq!(near_limit.record(1, 0), Err(IncentiveError::Overflow));
        assert_eq!(near_limit.liquidation_count, 3);
        assert_eq!(near_limit.total_volume, i128::MAX);
    }

    fn guard() -> MevGuardConfig {
        MevGuardConfig::new(2, 3, 500).unwrap()
    }

    #[test]
    fn guard_config_validation() {
        assert!(MevGuardConfig::new(0, 1, 0).is_ok());
        assert!(MevGuardConfig::new(2, 20, 10_000).is_ok());
        assert_eq!(MevGuardConfig::new(2, 0, 500), Err(MevGuardError::InvalidConfig));
        assert_eq!(MevGuardConfig::new(2, 3, -1), Err(MevGuardError::InvalidConfig));
        assert_eq!(
            MevGuardConfig::new(2, 3, 10_001),
            Err(MevGuardError::InvalidConfig)
        );
        assert!(MevGuardConfig::default().validate().is_ok());
    }

    #[test]
    fn a_normal_liquidation_passes() {
        assert_eq!(guard().check_liquidation(None, 100, 0, 1_000, 1_000), Ok(()));
        assert_eq!(guard().check_liquidation(Some(90), 100, 2, 1_000, 1_040), Ok(()));
    }

    #[test]
    fn repeat_liquidation_of_a_position_needs_the_cooldown() {
        let g = guard(); // 2 ledgers
        assert_eq!(
            g.check_liquidation(Some(99), 100, 0, 1_000, 1_000),
            Err(MevGuardError::CooldownActive)
        );
        assert_eq!(
            g.check_liquidation(Some(100), 100, 0, 1_000, 1_000),
            Err(MevGuardError::CooldownActive)
        );
        assert_eq!(g.check_liquidation(Some(98), 100, 0, 1_000, 1_000), Ok(()));
    }

    #[test]
    fn a_last_liquidation_in_the_future_is_treated_as_too_recent() {
        assert_eq!(
            guard().check_liquidation(Some(500), 100, 0, 1_000, 1_000),
            Err(MevGuardError::CooldownActive)
        );
    }

    #[test]
    fn ledger_liquidation_cap_is_enforced() {
        let g = guard(); // 3 per ledger
        assert_eq!(g.check_liquidation(None, 100, 2, 1_000, 1_000), Ok(()));
        assert_eq!(
            g.check_liquidation(None, 100, 3, 1_000, 1_000),
            Err(MevGuardError::LedgerCapReached)
        );
    }

    #[test]
    fn price_deviation_beyond_the_band_is_refused() {
        let g = guard(); // 5%
        assert_eq!(g.check_liquidation(None, 100, 0, 1_000, 1_050), Ok(())); // +5.00%
        assert_eq!(
            g.check_liquidation(None, 100, 0, 1_000, 1_051),
            Err(MevGuardError::PriceDeviationTooHigh)
        );
        assert_eq!(g.check_liquidation(None, 100, 0, 1_000, 950), Ok(())); // -5.00%
        assert_eq!(
            g.check_liquidation(None, 100, 0, 1_000, 949),
            Err(MevGuardError::PriceDeviationTooHigh)
        );
    }

    #[test]
    fn invalid_prices_are_refused() {
        let g = guard();
        assert_eq!(
            g.check_liquidation(None, 100, 0, 0, 1_000),
            Err(MevGuardError::InvalidPrice)
        );
        assert_eq!(
            g.check_liquidation(None, 100, 0, 1_000, 0),
            Err(MevGuardError::InvalidPrice)
        );
        assert_eq!(
            g.check_liquidation(None, 100, 0, -1, 1_000),
            Err(MevGuardError::InvalidPrice)
        );
        assert_eq!(
            MevGuardConfig::price_deviation_bps(i128::MAX, 1),
            Err(MevGuardError::Overflow)
        );
    }

    #[test]
    fn price_deviation_is_measured_against_the_reference() {
        assert_eq!(MevGuardConfig::price_deviation_bps(1_000, 1_000), Ok(0));
        assert_eq!(MevGuardConfig::price_deviation_bps(1_000, 1_100), Ok(1_000));
        assert_eq!(MevGuardConfig::price_deviation_bps(1_000, 500), Ok(5_000));
    }

    #[test]
    fn checks_run_in_a_fixed_order() {
        // Cooldown is reported before the ledger cap, and both before price.
        let g = guard();
        assert_eq!(
            g.check_liquidation(Some(99), 100, 99, 0, 0),
            Err(MevGuardError::CooldownActive)
        );
        assert_eq!(
            g.check_liquidation(None, 100, 99, 0, 0),
            Err(MevGuardError::LedgerCapReached)
        );
    }

    #[test]
    fn an_invalid_config_is_refused_at_check_time() {
        let broken = MevGuardConfig {
            min_ledgers_between_liquidations: 0,
            max_liquidations_per_ledger: 0,
            max_price_deviation_bps: 500,
        };
        assert_eq!(
            broken.check_liquidation(None, 100, 0, 1_000, 1_000),
            Err(MevGuardError::InvalidConfig)
        );
    }
}
