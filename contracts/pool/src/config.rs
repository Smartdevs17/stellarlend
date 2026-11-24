//! Typed pool configuration built around a single `ParameterRegistry` (#1760).

use stellarlend_params::registry::{
    AssetParameters, AssetParametersUpdate, ParameterError, ParameterRegistry, BPS_DIVISOR,
    MAX_RESERVE_FACTOR_BPS,
};

/// Configuration version. Bump only when a persisted field changes meaning.
pub const CONFIG_VERSION: u32 = 1;
/// Maximum liquidation incentive accepted by the pool.
pub const MAX_LIQUIDATION_INCENTIVE_BPS: i128 = 2_000;

/// Errors returned while constructing or updating a pool configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    /// A risk parameter was outside its allowed range.
    InvalidBps,
    /// The configuration timestamp was zero.
    InvalidTimestamp,
    /// The oracle identifier was missing.
    InvalidOracle,
    /// A per-asset parameter failed validation.
    Registry(ParameterError),
}

impl From<ParameterError> for ConfigError {
    fn from(error: ParameterError) -> Self {
        Self::Registry(error)
    }
}

/// Every pool parameter in one typed value.
///
/// Per-asset settings live in `registry`; changing one asset is a single
/// atomic mutation of this struct, which callers persist under one key.
#[derive(Clone, Copy, Debug)]
pub struct PoolConfig {
    pub version: u32,
    pub paused: bool,
    pub borrowing_enabled: bool,
    pub collateral_enabled: bool,
    pub ltv_bps: i128,
    pub liquidation_threshold_bps: i128,
    pub reserve_factor_bps: i128,
    pub close_factor_bps: i128,
    pub liquidation_incentive_bps: i128,
    pub oracle_id: u64,
    pub registry: ParameterRegistry,
    pub updated_at: u64,
}

#[allow(clippy::too_many_arguments)]
impl PoolConfig {
    /// Construct and validate a new pool configuration.
    pub fn new(
        version: u32,
        paused: bool,
        ltv_bps: i128,
        liquidation_threshold_bps: i128,
        reserve_factor_bps: i128,
        close_factor_bps: i128,
        liquidation_incentive_bps: i128,
        oracle_id: u64,
        updated_at: u64,
    ) -> Result<Self, ConfigError> {
        let config = Self {
            version,
            paused,
            borrowing_enabled: true,
            collateral_enabled: true,
            ltv_bps,
            liquidation_threshold_bps,
            reserve_factor_bps,
            close_factor_bps,
            liquidation_incentive_bps,
            oracle_id,
            registry: ParameterRegistry::new(),
            updated_at,
        };
        config.validate()?;
        Ok(config)
    }

    /// Validate all cross-field invariants.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != CONFIG_VERSION {
            return Err(ConfigError::InvalidBps);
        }
        if self.oracle_id == 0 {
            return Err(ConfigError::InvalidOracle);
        }
        if self.updated_at == 0 {
            return Err(ConfigError::InvalidTimestamp);
        }
        if self.ltv_bps < 0
            || self.ltv_bps > BPS_DIVISOR
            || self.liquidation_threshold_bps <= 0
            || self.liquidation_threshold_bps > BPS_DIVISOR
            || self.reserve_factor_bps < 0
            || self.reserve_factor_bps > MAX_RESERVE_FACTOR_BPS
            || self.close_factor_bps < 0
            || self.close_factor_bps > BPS_DIVISOR
            || self.liquidation_incentive_bps < 0
            || self.liquidation_incentive_bps > MAX_LIQUIDATION_INCENTIVE_BPS
        {
            return Err(ConfigError::InvalidBps);
        }
        if self.ltv_bps > self.liquidation_threshold_bps {
            return Err(ConfigError::InvalidBps);
        }
        self.registry.validate_all()?;
        Ok(())
    }

    /// Register a new asset. The whole config stays valid or is unchanged.
    pub fn register_asset(
        &mut self,
        asset_id: u64,
        collateral_factor_bps: i128,
        liquidation_threshold_bps: i128,
        reserve_factor_bps: i128,
        borrow_cap: i128,
        price_feed_id: u64,
    ) -> Result<(), ConfigError> {
        let entry = AssetParameters::new(
            asset_id,
            collateral_factor_bps,
            liquidation_threshold_bps,
            reserve_factor_bps,
            borrow_cap,
            price_feed_id,
        )?;
        self.registry.register(entry)?;
        Ok(())
    }

    /// Apply a partial per-asset update atomically.
    pub fn update_asset(
        &mut self,
        asset_id: u64,
        update: AssetParametersUpdate,
    ) -> Result<(), ConfigError> {
        self.registry.update(asset_id, update)?;
        Ok(())
    }

    /// Freeze or unfreeze an asset.
    pub fn set_asset_enabled(
        &mut self,
        asset_id: u64,
        enabled: bool,
    ) -> Result<(), ConfigError> {
        self.registry.set_enabled(asset_id, enabled)?;
        Ok(())
    }

    /// Remove an asset from the registry.
    pub fn remove_asset(&mut self, asset_id: u64) -> Result<AssetParameters, ConfigError> {
        Ok(self.registry.remove(asset_id)?)
    }

    /// Number of configured assets.
    pub fn asset_count(&self) -> usize {
        self.registry.len()
    }

    /// Borrowing power for an asset, or `0` when borrowing is disabled.
    pub fn borrow_capacity(
        &self,
        asset_id: u64,
        collateral_value: i128,
    ) -> Result<i128, ConfigError> {
        if self.paused || !self.borrowing_enabled || !self.collateral_enabled {
            return Ok(0);
        }
        Ok(self.registry.borrow_capacity(asset_id, collateral_value)?)
    }

    /// Collateral value recognised by liquidation for an asset.
    pub fn liquidation_limit(
        &self,
        asset_id: u64,
        collateral_value: i128,
    ) -> Result<i128, ConfigError> {
        Ok(self.registry.liquidation_limit(asset_id, collateral_value)?)
    }

    /// Pause all pool actions.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume pool actions.
    pub fn unpause(&mut self) {
        self.paused = false;
    }

    /// Update the audit timestamp after a successful mutation.
    pub fn touch(&mut self, timestamp: u64) -> Result<(), ConfigError> {
        if timestamp < self.updated_at || timestamp == 0 {
            return Err(ConfigError::InvalidTimestamp);
        }
        self.updated_at = timestamp;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PoolConfig {
        PoolConfig::new(1, false, 7_500, 8_300, 1_000, 5_000, 500, 1, 1_700_000_000)
            .expect("valid config")
    }

    #[test]
    fn config_rejects_out_of_range_risk_parameters() {
        assert_eq!(
            PoolConfig::new(1, false, 9_000, 8_000, 1_000, 5_000, 500, 1, 1),
            Err(ConfigError::InvalidBps)
        );
        assert_eq!(
            PoolConfig::new(1, false, 7_000, 8_000, 6_000, 5_000, 500, 1, 1),
            Err(ConfigError::InvalidBps)
        );
        assert_eq!(
            PoolConfig::new(1, false, 7_000, 8_000, 1_000, 5_000, 500, 0, 1),
            Err(ConfigError::InvalidOracle)
        );
    }

    #[test]
    fn assets_are_registered_and_updated_through_the_registry() {
        let mut pool = config();
        pool.register_asset(1, 7_500, 8_300, 1_000, 0, 1).unwrap();
        assert_eq!(pool.asset_count(), 1);
        assert_eq!(pool.borrow_capacity(1, 20_000), Ok(15_000));

        pool.update_asset(1, AssetParametersUpdate::collateral_factor(9_000))
            .unwrap_err();
        assert_eq!(pool.registry.get(1).unwrap().collateral_factor_bps, 7_500);

        pool.update_asset(1, AssetParametersUpdate::collateral_factor(7_000))
            .unwrap();
        assert_eq!(pool.borrow_capacity(1, 20_000), Ok(14_000));
    }

    #[test]
    fn paused_or_frozen_pools_grant_no_capacity() {
        let mut pool = config();
        pool.register_asset(1, 7_500, 8_300, 1_000, 0, 1).unwrap();
        pool.pause();
        assert_eq!(pool.borrow_capacity(1, 20_000), Ok(0));
        pool.unpause();
        pool.set_asset_enabled(1, false).unwrap();
        assert_eq!(pool.borrow_capacity(1, 20_000), Ok(0));
        assert_eq!(pool.liquidation_limit(1, 20_000), Ok(0));
    }

    #[test]
    fn removal_and_timestamps_are_guarded() {
        let mut pool = config();
        pool.register_asset(1, 7_500, 8_300, 1_000, 0, 1).unwrap();
        assert_eq!(pool.remove_asset(1).unwrap().asset_id, 1);
        assert_eq!(pool.asset_count(), 0);
        assert_eq!(pool.touch(1_700_000_000), Ok(()));
        assert_eq!(
            pool.touch(1_699_999_999),
            Err(ConfigError::InvalidTimestamp)
        );
    }
}
