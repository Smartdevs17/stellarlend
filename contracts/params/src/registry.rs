//! Typed per-asset parameter registry (issue #1760).
//!
//! This is a plain value type: no `Env`, no storage keys, and no governance
//! logic. A contract stores one `ParameterRegistry` value and routes mutations
//! through its admin/timelock checks. Keeping validation here means every
//! caller enforces the same invariants.

/// Basis-point divisor (`100% == 10_000 bps`).
pub const BPS_DIVISOR: i128 = 10_000;
/// Maximum reserve factor accepted by the protocol.
pub const MAX_RESERVE_FACTOR_BPS: i128 = 5_000;
/// Fixed capacity keeps the registry `no_std` and cheap to persist.
pub const MAX_ASSETS: usize = 32;

/// Errors returned by registry operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterError {
    /// A basis-point value was outside its allowed range.
    InvalidBps,
    /// A cap or value was negative.
    InvalidAmount,
    /// Collateral factor exceeded the liquidation threshold.
    ThresholdBelowFactor,
    /// Reserve factor exceeded [`MAX_RESERVE_FACTOR_BPS`].
    ReserveFactorTooHigh,
    /// The asset is not present in the registry.
    AssetNotFound,
    /// The asset is already registered.
    AssetExists,
    /// All [`MAX_ASSETS`] slots are occupied.
    RegistryFull,
    /// The asset is frozen and cannot be used for borrowing.
    Frozen,
    /// Arithmetic overflowed `i128`.
    Overflow,
}

/// All risk and limit parameters for one asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetParameters {
    /// Stable numeric asset identifier.
    pub asset_id: u64,
    /// Borrowing power per unit of collateral, in bps.
    pub collateral_factor_bps: i128,
    /// Collateral value used for liquidation, in bps.
    pub liquidation_threshold_bps: i128,
    /// Share of interest retained by the protocol, in bps.
    pub reserve_factor_bps: i128,
    /// Maximum borrow amount; `0` means unlimited.
    pub borrow_cap: i128,
    /// Stable numeric identifier of the asset's price feed.
    pub price_feed_id: u64,
    /// Whether deposits and borrowing are enabled.
    pub enabled: bool,
}

impl AssetParameters {
    /// Construct an enabled asset entry.
    pub fn new(
        asset_id: u64,
        collateral_factor_bps: i128,
        liquidation_threshold_bps: i128,
        reserve_factor_bps: i128,
        borrow_cap: i128,
        price_feed_id: u64,
    ) -> Result<Self, ParameterError> {
        let entry = Self {
            asset_id,
            collateral_factor_bps,
            liquidation_threshold_bps,
            reserve_factor_bps,
            borrow_cap,
            price_feed_id,
            enabled: true,
        };
        entry.validate()?;
        Ok(entry)
    }

    /// Validate all invariants. Frozen entries may keep historical factors but
    /// still have to stay within the static bps ranges.
    pub fn validate(&self) -> Result<(), ParameterError> {
        if self.collateral_factor_bps < 0
            || self.collateral_factor_bps > BPS_DIVISOR
            || self.liquidation_threshold_bps < 0
            || self.liquidation_threshold_bps > BPS_DIVISOR
        {
            return Err(ParameterError::InvalidBps);
        }
        if self.collateral_factor_bps > self.liquidation_threshold_bps {
            return Err(ParameterError::ThresholdBelowFactor);
        }
        if self.reserve_factor_bps < 0 || self.reserve_factor_bps > MAX_RESERVE_FACTOR_BPS {
            return Err(ParameterError::ReserveFactorTooHigh);
        }
        if self.borrow_cap < 0 {
            return Err(ParameterError::InvalidAmount);
        }
        if self.enabled
            && (self.collateral_factor_bps == 0 || self.liquidation_threshold_bps == 0)
        {
            return Err(ParameterError::InvalidBps);
        }
        Ok(())
    }

    /// Borrowing power for a collateral value, before existing debt.
    pub fn borrow_capacity(&self, collateral_value: i128) -> Result<i128, ParameterError> {
        if !self.enabled {
            return Ok(0);
        }
        if collateral_value < 0 {
            return Err(ParameterError::InvalidAmount);
        }
        let capacity = mul_bps(collateral_value, self.collateral_factor_bps)?;
        Ok(if self.borrow_cap > 0 && capacity > self.borrow_cap {
            self.borrow_cap
        } else {
            capacity
        })
    }

    /// Collateral value recognised during liquidation.
    pub fn liquidation_limit(&self, collateral_value: i128) -> Result<i128, ParameterError> {
        if !self.enabled {
            return Ok(0);
        }
        if collateral_value < 0 {
            return Err(ParameterError::InvalidAmount);
        }
        mul_bps(collateral_value, self.liquidation_threshold_bps)
    }
}

/// Partial update used by governance calls. `None` keeps the existing value.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AssetParametersUpdate {
    pub collateral_factor_bps: Option<i128>,
    pub liquidation_threshold_bps: Option<i128>,
    pub reserve_factor_bps: Option<i128>,
    pub borrow_cap: Option<i128>,
    pub price_feed_id: Option<u64>,
    pub enabled: Option<bool>,
}

impl AssetParametersUpdate {
    /// Build an update that only changes the collateral factor.
    pub fn collateral_factor(value: i128) -> Self {
        Self {
            collateral_factor_bps: Some(value),
            ..Self::default()
        }
    }

    /// Build an update that freezes or unfreezes an asset.
    pub fn enabled(value: bool) -> Self {
        Self {
            enabled: Some(value),
            ..Self::default()
        }
    }

    fn apply(self, current: AssetParameters) -> AssetParameters {
        AssetParameters {
            asset_id: current.asset_id,
            collateral_factor_bps: self
                .collateral_factor_bps
                .unwrap_or(current.collateral_factor_bps),
            liquidation_threshold_bps: self
                .liquidation_threshold_bps
                .unwrap_or(current.liquidation_threshold_bps),
            reserve_factor_bps: self.reserve_factor_bps.unwrap_or(current.reserve_factor_bps),
            borrow_cap: self.borrow_cap.unwrap_or(current.borrow_cap),
            price_feed_id: self.price_feed_id.unwrap_or(current.price_feed_id),
            enabled: self.enabled.unwrap_or(current.enabled),
        }
    }
}

/// Fixed-capacity, typed collection of all per-asset parameters.
#[derive(Clone, Copy, Debug)]
pub struct ParameterRegistry {
    entries: [Option<AssetParameters>; MAX_ASSETS],
    len: usize,
}

impl Default for ParameterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ParameterRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            entries: [None; MAX_ASSETS],
            len: 0,
        }
    }

    /// Number of registered assets.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the registry has no assets.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether all slots are occupied.
    pub fn is_full(&self) -> bool {
        self.len >= MAX_ASSETS
    }

    /// Look up one asset.
    pub fn get(&self, asset_id: u64) -> Option<&AssetParameters> {
        self.entries.iter().flatten().find(|entry| entry.asset_id == asset_id)
    }

    /// Iterate over all registered assets.
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a AssetParameters> + 'a {
        self.entries.iter().flatten()
    }

    /// Validate every entry and the registry's internal count.
    pub fn validate_all(&self) -> Result<(), ParameterError> {
        let mut count = 0;
        for entry in self.entries.iter().flatten() {
            entry.validate()?;
            count += 1;
        }
        if count != self.len {
            return Err(ParameterError::Overflow);
        }
        Ok(())
    }

    /// Add a new asset. Duplicates and a full registry are rejected.
    pub fn register(&mut self, entry: AssetParameters) -> Result<(), ParameterError> {
        entry.validate()?;
        if self.get(entry.asset_id).is_some() {
            return Err(ParameterError::AssetExists);
        }
        match self.entries.iter_mut().find(|slot| slot.is_none()) {
            Some(slot) => {
                *slot = Some(entry);
                self.len += 1;
                Ok(())
            }
            None => Err(ParameterError::RegistryFull),
        }
    }

    /// Replace an asset's full parameter set atomically.
    pub fn replace(&mut self, entry: AssetParameters) -> Result<(), ParameterError> {
        entry.validate()?;
        match self
            .entries
            .iter_mut()
            .find(|slot| matches!(slot, Some(current) if current.asset_id == entry.asset_id))
        {
            Some(slot) => {
                *slot = Some(entry);
                Ok(())
            }
            None => Err(ParameterError::AssetNotFound),
        }
    }

    /// Apply a partial update without mutating anything when it is invalid.
    pub fn update(
        &mut self,
        asset_id: u64,
        update: AssetParametersUpdate,
    ) -> Result<(), ParameterError> {
        let current = *self.get(asset_id).ok_or(ParameterError::AssetNotFound)?;
        self.replace(update.apply(current))
    }

    /// Freeze or unfreeze an asset.
    pub fn set_enabled(&mut self, asset_id: u64, enabled: bool) -> Result<(), ParameterError> {
        self.update(asset_id, AssetParametersUpdate::enabled(enabled))
    }

    /// Remove an asset and free its slot.
    pub fn remove(&mut self, asset_id: u64) -> Result<AssetParameters, ParameterError> {
        let slot = self
            .entries
            .iter_mut()
            .find(|slot| matches!(slot, Some(current) if current.asset_id == asset_id))
            .ok_or(ParameterError::AssetNotFound)?;
        let removed = slot.take().ok_or(ParameterError::AssetNotFound)?;
        self.len -= 1;
        Ok(removed)
    }

    /// Borrowing power for an asset; frozen and unknown assets return `0`.
    pub fn borrow_capacity(
        &self,
        asset_id: u64,
        collateral_value: i128,
    ) -> Result<i128, ParameterError> {
        match self.get(asset_id) {
            Some(entry) => entry.borrow_capacity(collateral_value),
            None => Ok(0),
        }
    }

    /// Liquidation threshold value for an asset.
    pub fn liquidation_limit(
        &self,
        asset_id: u64,
        collateral_value: i128,
    ) -> Result<i128, ParameterError> {
        match self.get(asset_id) {
            Some(entry) => entry.liquidation_limit(collateral_value),
            None => Ok(0),
        }
    }

    /// Borrowing power left after accounting for existing debt.
    pub fn available_borrow(
        &self,
        asset_id: u64,
        collateral_value: i128,
        current_debt: i128,
    ) -> Result<i128, ParameterError> {
        if current_debt < 0 {
            return Err(ParameterError::InvalidAmount);
        }
        let capacity = self.borrow_capacity(asset_id, collateral_value)?;
        Ok(if capacity > current_debt {
            capacity - current_debt
        } else {
            0
        })
    }
}

/// Apply basis points with an overflow-safe product, truncating remainder.
fn mul_bps(value: i128, bps: i128) -> Result<i128, ParameterError> {
    if value < 0 || bps < 0 {
        return Err(ParameterError::InvalidAmount);
    }
    match value.checked_mul(bps) {
        Some(product) => Ok(product / BPS_DIVISOR),
        None => {
            let quotient = value / BPS_DIVISOR;
            let remainder = value % BPS_DIVISOR;
            let high = quotient.checked_mul(bps).ok_or(ParameterError::Overflow)?;
            let low = remainder.checked_mul(bps).ok_or(ParameterError::Overflow)? / BPS_DIVISOR;
            high.checked_add(low).ok_or(ParameterError::Overflow)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, cf: i128, lt: i128) -> AssetParameters {
        AssetParameters::new(id, cf, lt, 1_000, 0, id).expect("valid entry")
    }

    #[test]
    fn registration_and_lookup() {
        let mut registry = ParameterRegistry::new();
        assert!(registry.is_empty());
        registry.register(entry(1, 7_500, 8_300)).unwrap();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get(1).map(|item| item.collateral_factor_bps), Some(7_500));
        assert_eq!(registry.register(entry(1, 7_000, 8_000)), Err(ParameterError::AssetExists));
        assert_eq!(registry.get(2), None);
    }

    #[test]
    fn invalid_entries_are_rejected() {
        assert_eq!(
            AssetParameters::new(1, 9_000, 8_000, 0, 0, 1),
            Err(ParameterError::ThresholdBelowFactor)
        );
        assert_eq!(
            AssetParameters::new(1, 7_000, 8_000, 6_000, 0, 1),
            Err(ParameterError::ReserveFactorTooHigh)
        );
        assert_eq!(
            AssetParameters::new(1, 7_000, 8_000, 0, -1, 1),
            Err(ParameterError::InvalidAmount)
        );
    }

    #[test]
    fn updates_are_atomic() {
        let mut registry = ParameterRegistry::new();
        registry.register(entry(1, 7_500, 8_300)).unwrap();

        assert_eq!(
            registry.update(1, AssetParametersUpdate::collateral_factor(9_000)),
            Err(ParameterError::ThresholdBelowFactor)
        );
        assert_eq!(registry.get(1).unwrap().collateral_factor_bps, 7_500);

        registry
            .update(1, AssetParametersUpdate::collateral_factor(7_000))
            .unwrap();
        assert_eq!(registry.get(1).unwrap().collateral_factor_bps, 7_000);
    }

    #[test]
    fn frozen_assets_have_no_borrowing_power() {
        let mut registry = ParameterRegistry::new();
        registry.register(entry(1, 7_500, 8_300)).unwrap();
        assert_eq!(registry.borrow_capacity(1, 20_000), Ok(15_000));
        assert_eq!(registry.liquidation_limit(1, 20_000), Ok(16_600));

        registry.set_enabled(1, false).unwrap();
        assert_eq!(registry.borrow_capacity(1, 20_000), Ok(0));
        assert_eq!(registry.liquidation_limit(1, 20_000), Ok(0));
        assert_eq!(registry.available_borrow(1, 20_000, 0), Ok(0));
    }

    #[test]
    fn caps_and_available_borrow_are_honoured() {
        let mut capped = entry(1, 7_500, 8_300);
        capped.borrow_cap = 5_000;
        let mut registry = ParameterRegistry::new();
        registry.register(capped).unwrap();

        assert_eq!(registry.borrow_capacity(1, 20_000), Ok(5_000));
        assert_eq!(registry.available_borrow(1, 20_000, 2_000), Ok(3_000));
        assert_eq!(registry.available_borrow(1, 20_000, 9_000), Ok(0));
    }

    #[test]
    fn removal_frees_the_slot() {
        let mut registry = ParameterRegistry::new();
        registry.register(entry(1, 7_500, 8_300)).unwrap();
        assert_eq!(registry.remove(1).unwrap().asset_id, 1);
        assert!(registry.is_empty());
        assert_eq!(registry.remove(1), Err(ParameterError::AssetNotFound));
    }

    #[test]
    fn registry_reports_full_at_capacity() {
        let mut registry = ParameterRegistry::new();
        for id in 0..MAX_ASSETS as u64 {
            registry.register(entry(id, 7_000, 8_000)).unwrap();
        }
        assert!(registry.is_full());
        assert_eq!(
            registry.register(entry(MAX_ASSETS as u64, 7_000, 8_000)),
            Err(ParameterError::RegistryFull)
        );
        assert_eq!(registry.validate_all(), Ok(()));
    }
}
