//! Collateral factor registry with risk-based parameters (issue #1020).
//!
//! Each collateral asset is registered under a [`RiskTier`]. The tier fixes the
//! ceiling for its collateral factor, liquidation threshold and liquidation
//! bonus, so a risky asset cannot be listed with the parameters of a blue chip.
//! Every registration and update is validated against the tier and against three
//! invariants:
//!
//! 1. `collateral_factor < liquidation_threshold` by at least the tier's
//!    `min_threshold_gap_bps`, so a fresh borrow is not instantly liquidatable.
//! 2. `liquidation_threshold * (1 + liquidation_bonus) <= 100%`, so paying the
//!    liquidator's bonus at the threshold still leaves the position covered.
//! 3. Nothing exceeds 100% (10_000 bps).
//!
//! Pure data structure, no storage or `Env`; a contract persists the entries it
//! holds. See `docs/COLLATERAL_FACTOR_REGISTRY.md`.

use lending_types::BPS_DIVISOR;
use stellarlend_safe_math::{safe_div, safe_mul, MathError};

/// Most assets one registry holds.
pub const MAX_REGISTRY_ASSETS: usize = 32;

/// Risk classification of a collateral asset, safest first.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RiskTier {
    /// Deep, liquid, low-volatility assets.
    Conservative,
    /// Established assets with moderate volatility.
    Moderate,
    /// Volatile or thinner-market assets.
    Aggressive,
    /// Newly listed or illiquid assets: the tightest limits.
    Isolated,
}

/// Ceilings a tier places on an asset's parameters (all basis points).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct TierBounds {
    pub max_collateral_factor_bps: i128,
    pub max_liquidation_threshold_bps: i128,
    pub max_liquidation_bonus_bps: i128,
    /// Smallest allowed `liquidation_threshold - collateral_factor`.
    pub min_threshold_gap_bps: i128,
}

impl RiskTier {
    pub const fn bounds(self) -> TierBounds {
        match self {
            RiskTier::Conservative => TierBounds {
                max_collateral_factor_bps: 8_000,
                max_liquidation_threshold_bps: 8_500,
                max_liquidation_bonus_bps: 500,
                min_threshold_gap_bps: 300,
            },
            RiskTier::Moderate => TierBounds {
                max_collateral_factor_bps: 7_000,
                max_liquidation_threshold_bps: 7_500,
                max_liquidation_bonus_bps: 800,
                min_threshold_gap_bps: 300,
            },
            RiskTier::Aggressive => TierBounds {
                max_collateral_factor_bps: 5_000,
                max_liquidation_threshold_bps: 6_000,
                max_liquidation_bonus_bps: 1_200,
                min_threshold_gap_bps: 500,
            },
            RiskTier::Isolated => TierBounds {
                max_collateral_factor_bps: 3_500,
                max_liquidation_threshold_bps: 4_500,
                max_liquidation_bonus_bps: 1_500,
                min_threshold_gap_bps: 500,
            },
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// A value is out of its basic range (non-positive factor, above 100%, negative cap).
    InvalidParameter,
    /// A value exceeds the ceiling of the asset's tier.
    ExceedsTierBound,
    /// `liquidation_threshold - collateral_factor` is below the tier's minimum gap.
    ThresholdGapTooSmall,
    /// The liquidation bonus would leave the position under-covered at the threshold.
    BonusUndercollateralizes,
    /// The asset is already registered.
    AssetExists,
    /// The asset is not registered.
    AssetNotFound,
    /// The registry holds `MAX_REGISTRY_ASSETS` assets.
    RegistryFull,
    /// The asset is frozen and cannot be changed or used to borrow.
    Frozen,
    /// Arithmetic overflow.
    Overflow,
}

impl From<MathError> for RegistryError {
    fn from(_: MathError) -> Self {
        RegistryError::Overflow
    }
}

/// Registered risk parameters for one collateral asset.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CollateralEntry {
    pub asset_id: u32,
    pub tier: RiskTier,
    /// Share of the collateral's value that can be borrowed against.
    pub collateral_factor_bps: i128,
    /// Debt-to-collateral ratio at which the position becomes liquidatable.
    pub liquidation_threshold_bps: i128,
    /// Extra collateral a liquidator receives.
    pub liquidation_bonus_bps: i128,
    /// Most of this asset the protocol accepts as collateral (0 = uncapped).
    pub supply_cap: i128,
    pub is_frozen: bool,
}

/// Checks `entry` against basic ranges, its tier and the registry invariants.
pub fn validate_entry(entry: &CollateralEntry) -> Result<(), RegistryError> {
    if entry.collateral_factor_bps <= 0
        || entry.liquidation_threshold_bps <= 0
        || entry.liquidation_threshold_bps > BPS_DIVISOR
        || entry.liquidation_bonus_bps < 0
        || entry.supply_cap < 0
    {
        return Err(RegistryError::InvalidParameter);
    }

    let bounds = entry.tier.bounds();
    if entry.collateral_factor_bps > bounds.max_collateral_factor_bps
        || entry.liquidation_threshold_bps > bounds.max_liquidation_threshold_bps
        || entry.liquidation_bonus_bps > bounds.max_liquidation_bonus_bps
    {
        return Err(RegistryError::ExceedsTierBound);
    }

    if entry.liquidation_threshold_bps - entry.collateral_factor_bps < bounds.min_threshold_gap_bps
    {
        return Err(RegistryError::ThresholdGapTooSmall);
    }

    // Both operands are now bounded by the tier, so this cannot overflow.
    let covered = entry.liquidation_threshold_bps * (BPS_DIVISOR + entry.liquidation_bonus_bps);
    if covered > BPS_DIVISOR * BPS_DIVISOR {
        return Err(RegistryError::BonusUndercollateralizes);
    }

    Ok(())
}

/// The set of registered collateral assets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralRegistry {
    entries: [Option<CollateralEntry>; MAX_REGISTRY_ASSETS],
}

impl CollateralRegistry {
    pub fn new() -> Self {
        Self {
            entries: [None; MAX_REGISTRY_ASSETS],
        }
    }

    pub fn len(&self) -> usize {
        self.entries.iter().flatten().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, asset_id: u32) -> Option<&CollateralEntry> {
        self.entries.iter().flatten().find(|e| e.asset_id == asset_id)
    }

    /// Iterates registered assets in registration-slot order.
    pub fn iter(&self) -> impl Iterator<Item = &CollateralEntry> {
        self.entries.iter().flatten()
    }

    /// Adds a validated asset.
    pub fn register(&mut self, entry: CollateralEntry) -> Result<(), RegistryError> {
        validate_entry(&entry)?;
        if self.get(entry.asset_id).is_some() {
            return Err(RegistryError::AssetExists);
        }
        match self.entries.iter_mut().find(|slot| slot.is_none()) {
            Some(slot) => {
                *slot = Some(entry);
                Ok(())
            }
            None => Err(RegistryError::RegistryFull),
        }
    }

    /// Changes an asset's risk parameters; the result is validated as a whole
    /// and nothing changes on error.
    pub fn update_risk_params(
        &mut self,
        asset_id: u32,
        collateral_factor_bps: i128,
        liquidation_threshold_bps: i128,
        liquidation_bonus_bps: i128,
    ) -> Result<(), RegistryError> {
        let slot = self.slot_mut(asset_id)?;
        let current = slot.ok_or(RegistryError::AssetNotFound)?;
        if current.is_frozen {
            return Err(RegistryError::Frozen);
        }
        let updated = CollateralEntry {
            collateral_factor_bps,
            liquidation_threshold_bps,
            liquidation_bonus_bps,
            ..current
        };
        validate_entry(&updated)?;
        *slot = Some(updated);
        Ok(())
    }

    /// Moves an asset to another tier. The asset's existing parameters must fit
    /// the new tier's bounds; otherwise nothing changes and the caller must
    /// update the parameters first.
    pub fn set_tier(&mut self, asset_id: u32, tier: RiskTier) -> Result<(), RegistryError> {
        let slot = self.slot_mut(asset_id)?;
        let current = slot.ok_or(RegistryError::AssetNotFound)?;
        if current.is_frozen {
            return Err(RegistryError::Frozen);
        }
        let updated = CollateralEntry { tier, ..current };
        validate_entry(&updated)?;
        *slot = Some(updated);
        Ok(())
    }

    /// Sets the supply cap (0 = uncapped).
    pub fn set_supply_cap(&mut self, asset_id: u32, supply_cap: i128) -> Result<(), RegistryError> {
        if supply_cap < 0 {
            return Err(RegistryError::InvalidParameter);
        }
        let slot = self.slot_mut(asset_id)?;
        let current = slot.ok_or(RegistryError::AssetNotFound)?;
        *slot = Some(CollateralEntry {
            supply_cap,
            ..current
        });
        Ok(())
    }

    /// Freezes or unfreezes an asset. A frozen asset adds no borrowing power and
    /// its parameters cannot be changed until it is unfrozen.
    pub fn set_frozen(&mut self, asset_id: u32, is_frozen: bool) -> Result<(), RegistryError> {
        let slot = self.slot_mut(asset_id)?;
        let current = slot.ok_or(RegistryError::AssetNotFound)?;
        *slot = Some(CollateralEntry {
            is_frozen,
            ..current
        });
        Ok(())
    }

    /// Removes an asset from the registry.
    pub fn remove(&mut self, asset_id: u32) -> Result<(), RegistryError> {
        let slot = self.slot_mut(asset_id)?;
        *slot = None;
        Ok(())
    }

    /// Value that can be borrowed against `collateral_value` of this asset:
    /// `collateral_value * collateral_factor / 10_000`, or 0 when frozen.
    pub fn borrow_capacity(
        &self,
        asset_id: u32,
        collateral_value: i128,
    ) -> Result<i128, RegistryError> {
        let entry = self.get(asset_id).ok_or(RegistryError::AssetNotFound)?;
        if entry.is_frozen {
            return Ok(0);
        }
        Ok(safe_mul(collateral_value, entry.collateral_factor_bps)
            .and_then(|v| safe_div(v, BPS_DIVISOR))?)
    }

    /// Debt level at which `collateral_value` of this asset becomes liquidatable:
    /// `collateral_value * liquidation_threshold / 10_000`.
    pub fn liquidation_limit(
        &self,
        asset_id: u32,
        collateral_value: i128,
    ) -> Result<i128, RegistryError> {
        let entry = self.get(asset_id).ok_or(RegistryError::AssetNotFound)?;
        Ok(safe_mul(collateral_value, entry.liquidation_threshold_bps)
            .and_then(|v| safe_div(v, BPS_DIVISOR))?)
    }

    /// Finds the slot holding `asset_id`, or `AssetNotFound`.
    fn slot_mut(&mut self, asset_id: u32) -> Result<&mut Option<CollateralEntry>, RegistryError> {
        self.entries
            .iter_mut()
            .find(|slot| matches!(slot, Some(e) if e.asset_id == asset_id))
            .ok_or(RegistryError::AssetNotFound)
    }
}

impl Default for CollateralRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(asset_id: u32, tier: RiskTier, cf: i128, lt: i128, bonus: i128) -> CollateralEntry {
        CollateralEntry {
            asset_id,
            tier,
            collateral_factor_bps: cf,
            liquidation_threshold_bps: lt,
            liquidation_bonus_bps: bonus,
            supply_cap: 0,
            is_frozen: false,
        }
    }

    fn moderate() -> CollateralEntry {
        entry(1, RiskTier::Moderate, 6_500, 7_200, 500)
    }

    #[test]
    fn every_tier_has_a_consistent_bound_set() {
        for tier in [
            RiskTier::Conservative,
            RiskTier::Moderate,
            RiskTier::Aggressive,
            RiskTier::Isolated,
        ] {
            let b = tier.bounds();
            assert!(b.max_collateral_factor_bps > 0);
            assert!(b.max_liquidation_threshold_bps <= BPS_DIVISOR);
            assert!(
                b.max_liquidation_threshold_bps - b.max_collateral_factor_bps
                    >= b.min_threshold_gap_bps
            );
            // The tier's own maximums must be a legal entry.
            let at_max = entry(
                1,
                tier,
                b.max_collateral_factor_bps,
                b.max_liquidation_threshold_bps,
                b.max_liquidation_bonus_bps,
            );
            assert_eq!(validate_entry(&at_max), Ok(()), "tier maximums must be valid");
        }
    }

    #[test]
    fn riskier_tiers_are_stricter_on_collateral_factor() {
        let c = RiskTier::Conservative.bounds().max_collateral_factor_bps;
        let m = RiskTier::Moderate.bounds().max_collateral_factor_bps;
        let a = RiskTier::Aggressive.bounds().max_collateral_factor_bps;
        let i = RiskTier::Isolated.bounds().max_collateral_factor_bps;
        assert!(c > m && m > a && a > i);
    }

    #[test]
    fn valid_entry_registers_and_is_readable() {
        let mut registry = CollateralRegistry::new();
        assert!(registry.is_empty());
        registry.register(moderate()).unwrap();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get(1), Some(&moderate()));
        assert_eq!(registry.get(2), None);
    }

    #[test]
    fn duplicate_asset_is_rejected() {
        let mut registry = CollateralRegistry::new();
        registry.register(moderate()).unwrap();
        assert_eq!(registry.register(moderate()), Err(RegistryError::AssetExists));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn basic_range_errors_are_invalid_parameter() {
        for bad in [
            entry(1, RiskTier::Moderate, 0, 7_200, 500),
            entry(1, RiskTier::Moderate, -1, 7_200, 500),
            entry(1, RiskTier::Moderate, 6_500, 0, 500),
            entry(1, RiskTier::Moderate, 6_500, 10_001, 500),
            entry(1, RiskTier::Moderate, 6_500, 7_200, -1),
        ] {
            assert_eq!(validate_entry(&bad), Err(RegistryError::InvalidParameter));
        }
        let mut negative_cap = moderate();
        negative_cap.supply_cap = -1;
        assert_eq!(validate_entry(&negative_cap), Err(RegistryError::InvalidParameter));
    }

    #[test]
    fn values_above_the_tier_ceiling_are_rejected() {
        // Moderate: cf <= 7000, lt <= 7500, bonus <= 800.
        assert_eq!(
            validate_entry(&entry(1, RiskTier::Moderate, 7_001, 7_500, 500)),
            Err(RegistryError::ExceedsTierBound)
        );
        assert_eq!(
            validate_entry(&entry(1, RiskTier::Moderate, 6_500, 7_501, 500)),
            Err(RegistryError::ExceedsTierBound)
        );
        assert_eq!(
            validate_entry(&entry(1, RiskTier::Moderate, 6_500, 7_200, 801)),
            Err(RegistryError::ExceedsTierBound)
        );
        // The same parameters are fine for a safer tier.
        assert_eq!(
            validate_entry(&entry(1, RiskTier::Conservative, 7_001, 7_500, 500)),
            Ok(())
        );
    }

    #[test]
    fn threshold_must_sit_above_collateral_factor_by_the_tier_gap() {
        // Moderate gap is 300.
        assert_eq!(
            validate_entry(&entry(1, RiskTier::Moderate, 6_900, 7_199, 100)),
            Err(RegistryError::ThresholdGapTooSmall)
        );
        assert_eq!(
            validate_entry(&entry(1, RiskTier::Moderate, 6_900, 7_200, 100)),
            Ok(())
        );
        // Equal or inverted factors are caught by the same rule.
        assert_eq!(
            validate_entry(&entry(1, RiskTier::Moderate, 7_000, 7_000, 100)),
            Err(RegistryError::ThresholdGapTooSmall)
        );
    }

    #[test]
    fn tier_ceilings_satisfy_the_coverage_invariant_together() {
        // `liquidation_threshold * (1 + bonus) <= 100%` is checked for every entry.
        // With the shipped ceilings it cannot fail (the check guards future tier
        // edits), so assert the property of the ceilings themselves: an entry at a
        // tier's maximum threshold and maximum bonus is still fully covered.
        for tier in [
            RiskTier::Conservative,
            RiskTier::Moderate,
            RiskTier::Aggressive,
            RiskTier::Isolated,
        ] {
            let b = tier.bounds();
            let covered =
                b.max_liquidation_threshold_bps * (BPS_DIVISOR + b.max_liquidation_bonus_bps);
            assert!(covered <= BPS_DIVISOR * BPS_DIVISOR);
        }
    }

    #[test]
    fn update_changes_parameters_atomically() {
        let mut registry = CollateralRegistry::new();
        registry.register(moderate()).unwrap();

        registry.update_risk_params(1, 6_000, 6_800, 400).unwrap();
        let e = registry.get(1).unwrap();
        assert_eq!(
            (e.collateral_factor_bps, e.liquidation_threshold_bps, e.liquidation_bonus_bps),
            (6_000, 6_800, 400)
        );

        // A rejected update leaves the entry unchanged.
        assert_eq!(
            registry.update_risk_params(1, 7_500, 7_800, 400),
            Err(RegistryError::ExceedsTierBound)
        );
        assert_eq!(registry.get(1).unwrap().collateral_factor_bps, 6_000);
        assert_eq!(
            registry.update_risk_params(9, 6_000, 6_800, 400),
            Err(RegistryError::AssetNotFound)
        );
    }

    #[test]
    fn tier_change_requires_parameters_to_fit_the_new_tier() {
        let mut registry = CollateralRegistry::new();
        registry.register(moderate()).unwrap(); // cf 6500 lt 7200 bonus 500

        // Isolated caps cf at 3500: does not fit, nothing changes.
        assert_eq!(
            registry.set_tier(1, RiskTier::Isolated),
            Err(RegistryError::ExceedsTierBound)
        );
        assert_eq!(registry.get(1).unwrap().tier, RiskTier::Moderate);

        // Tighten parameters first, then the move succeeds.
        registry.update_risk_params(1, 3_000, 3_800, 500).unwrap();
        registry.set_tier(1, RiskTier::Isolated).unwrap();
        assert_eq!(registry.get(1).unwrap().tier, RiskTier::Isolated);

        // Moving to a safer tier that still fits is allowed.
        registry.set_tier(1, RiskTier::Conservative).unwrap();
    }

    #[test]
    fn frozen_asset_adds_no_borrowing_power_and_cannot_be_changed() {
        let mut registry = CollateralRegistry::new();
        registry.register(moderate()).unwrap();
        assert_eq!(registry.borrow_capacity(1, 100_000), Ok(65_000));

        registry.set_frozen(1, true).unwrap();
        assert_eq!(registry.borrow_capacity(1, 100_000), Ok(0));
        assert_eq!(
            registry.update_risk_params(1, 6_000, 6_800, 400),
            Err(RegistryError::Frozen)
        );
        assert_eq!(registry.set_tier(1, RiskTier::Conservative), Err(RegistryError::Frozen));

        registry.set_frozen(1, false).unwrap();
        assert_eq!(registry.borrow_capacity(1, 100_000), Ok(65_000));
    }

    #[test]
    fn borrow_capacity_and_liquidation_limit_use_the_asset_parameters() {
        let mut registry = CollateralRegistry::new();
        registry.register(moderate()).unwrap();
        assert_eq!(registry.borrow_capacity(1, 200_000), Ok(130_000)); // 65%
        assert_eq!(registry.liquidation_limit(1, 200_000), Ok(144_000)); // 72%
        assert_eq!(registry.borrow_capacity(2, 1), Err(RegistryError::AssetNotFound));
        assert_eq!(
            registry.borrow_capacity(1, i128::MAX),
            Err(RegistryError::Overflow)
        );
    }

    #[test]
    fn supply_cap_can_be_set_and_rejects_negative_values() {
        let mut registry = CollateralRegistry::new();
        registry.register(moderate()).unwrap();
        registry.set_supply_cap(1, 5_000_000).unwrap();
        assert_eq!(registry.get(1).unwrap().supply_cap, 5_000_000);
        assert_eq!(
            registry.set_supply_cap(1, -1),
            Err(RegistryError::InvalidParameter)
        );
        assert_eq!(registry.set_supply_cap(3, 1), Err(RegistryError::AssetNotFound));
    }

    #[test]
    fn remove_frees_the_slot_and_the_asset_id() {
        let mut registry = CollateralRegistry::new();
        registry.register(moderate()).unwrap();
        registry.remove(1).unwrap();
        assert!(registry.is_empty());
        assert_eq!(registry.remove(1), Err(RegistryError::AssetNotFound));
        registry.register(moderate()).unwrap();
    }

    #[test]
    fn registry_reports_full_at_capacity() {
        let mut registry = CollateralRegistry::new();
        for id in 0..MAX_REGISTRY_ASSETS as u32 {
            registry
                .register(entry(id, RiskTier::Moderate, 6_500, 7_200, 500))
                .unwrap();
        }
        assert_eq!(registry.len(), MAX_REGISTRY_ASSETS);
        assert_eq!(
            registry.register(entry(999, RiskTier::Moderate, 6_500, 7_200, 500)),
            Err(RegistryError::RegistryFull)
        );
        assert_eq!(registry.iter().count(), MAX_REGISTRY_ASSETS);
    }
}
