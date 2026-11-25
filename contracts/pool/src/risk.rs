//! Memoized risk-weight reads for the pool (issue #1774).
//!
//! [`PoolRiskView`] answers the same questions as
//! [`PoolConfig::borrow_capacity`] and [`PoolConfig::liquidation_limit`], but
//! resolves each asset's parameters at most once per block height. A
//! liquidation sweep or multi-asset health check that touches the same asset
//! for many positions pays for one registry lookup per asset instead of one
//! per position.
//!
//! The view borrows the config immutably, so parameters cannot change while
//! it is alive. Lookups for a new block height discard everything resolved
//! earlier, so a long-lived keeper reusing one view never serves weights from
//! a previous ledger. Unknown assets are memoized too (as `None`), so repeated
//! probes for unlisted collateral stay cheap.
//!
//! Operators read [`PoolRiskView::stats`] to export hit rate and registry
//! round-trips.

use stellarlend_params::registry::{AssetParameters, MAX_ASSETS};
use stellarlend_risk::cache::{CacheStats, RiskWeightCache};

use crate::config::{ConfigError, PoolConfig};

/// Read-only, per-block memoized view over a [`PoolConfig`].
#[derive(Clone, Copy, Debug)]
pub struct PoolRiskView<'a> {
    config: &'a PoolConfig,
    cache: RiskWeightCache<Option<AssetParameters>, MAX_ASSETS>,
}

impl<'a> PoolRiskView<'a> {
    /// Create an empty view over `config`.
    pub fn new(config: &'a PoolConfig) -> Self {
        Self {
            config,
            cache: RiskWeightCache::new(),
        }
    }

    /// The underlying configuration.
    pub fn config(&self) -> &'a PoolConfig {
        self.config
    }

    /// Risk parameters for `asset_id` at `height`; `None` when unlisted.
    pub fn risk_weights(&mut self, height: u32, asset_id: u64) -> Option<AssetParameters> {
        let config = self.config;
        self.cache
            .get_or_insert_with(height, asset_id, || config.registry.get(asset_id).copied())
    }

    /// Same result as [`PoolConfig::borrow_capacity`], memoized per height.
    pub fn borrow_capacity(
        &mut self,
        height: u32,
        asset_id: u64,
        collateral_value: i128,
    ) -> Result<i128, ConfigError> {
        let config = self.config;
        if config.paused || !config.borrowing_enabled || !config.collateral_enabled {
            return Ok(0);
        }
        match self.risk_weights(height, asset_id) {
            Some(entry) => Ok(entry.borrow_capacity(collateral_value)?),
            None => Ok(0),
        }
    }

    /// Same result as [`PoolConfig::liquidation_limit`], memoized per height.
    pub fn liquidation_limit(
        &mut self,
        height: u32,
        asset_id: u64,
        collateral_value: i128,
    ) -> Result<i128, ConfigError> {
        match self.risk_weights(height, asset_id) {
            Some(entry) => Ok(entry.liquidation_limit(collateral_value)?),
            None => Ok(0),
        }
    }

    /// Usage counters: hits, registry loads, height resets.
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellarlend_params::registry::{AssetParametersUpdate, ParameterError};

    fn config() -> PoolConfig {
        let mut pool =
            PoolConfig::new(1, false, 7_500, 8_300, 1_000, 5_000, 500, 1, 1_700_000_000)
                .expect("valid config");
        pool.register_asset(1, 7_500, 8_300, 1_000, 0, 1).unwrap();
        pool.register_asset(2, 6_000, 7_000, 1_000, 5_000, 2).unwrap();
        pool
    }

    #[test]
    fn matches_uncached_config_results() {
        let pool = config();
        let mut view = PoolRiskView::new(&pool);
        for asset in [1, 2, 3] {
            for value in [0, 1, 9_999, 20_000, 1_000_000] {
                assert_eq!(
                    view.borrow_capacity(100, asset, value),
                    pool.borrow_capacity(asset, value)
                );
                assert_eq!(
                    view.liquidation_limit(100, asset, value),
                    pool.liquidation_limit(asset, value)
                );
            }
        }
    }

    #[test]
    fn one_registry_load_per_asset_per_block() {
        let pool = config();
        let mut view = PoolRiskView::new(&pool);

        // A liquidation sweep over 50 positions spread across two assets.
        for position in 0..50u64 {
            let asset = 1 + position % 2;
            view.liquidation_limit(100, asset, 10_000).unwrap();
            view.borrow_capacity(100, asset, 10_000).unwrap();
        }

        let stats = view.stats();
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 98);
        assert_eq!(stats.resets, 0);
    }

    #[test]
    fn next_block_sees_updated_parameters() {
        let mut pool = config();
        {
            let mut view = PoolRiskView::new(&pool);
            assert_eq!(view.borrow_capacity(100, 1, 20_000), Ok(15_000));
        }
        pool.update_asset(1, AssetParametersUpdate::collateral_factor(7_000))
            .unwrap();
        let mut view = PoolRiskView::new(&pool);
        assert_eq!(view.borrow_capacity(101, 1, 20_000), Ok(14_000));
    }

    #[test]
    fn height_change_reloads_weights() {
        let pool = config();
        let mut view = PoolRiskView::new(&pool);
        view.risk_weights(100, 1);
        view.risk_weights(100, 1);
        view.risk_weights(101, 1);
        let stats = view.stats();
        assert_eq!((stats.misses, stats.hits, stats.resets), (2, 1, 1));
    }

    #[test]
    fn unknown_assets_are_memoized_as_absent() {
        let pool = config();
        let mut view = PoolRiskView::new(&pool);
        assert_eq!(view.risk_weights(5, 42), None);
        assert_eq!(view.borrow_capacity(5, 42, 10_000), Ok(0));
        assert_eq!(view.liquidation_limit(5, 42, 10_000), Ok(0));
        assert_eq!(view.stats().misses, 1);
    }

    #[test]
    fn paused_pool_grants_nothing_without_touching_the_cache() {
        let mut pool = config();
        pool.pause();
        let mut view = PoolRiskView::new(&pool);
        assert_eq!(view.borrow_capacity(5, 1, 20_000), Ok(0));
        assert_eq!(view.stats().lookups(), 0);
        // Liquidations still recognise collateral while paused.
        assert_eq!(view.liquidation_limit(5, 1, 20_000), Ok(16_600));
    }

    #[test]
    fn frozen_assets_and_caps_are_honoured() {
        let mut pool = config();
        pool.set_asset_enabled(1, false).unwrap();
        let mut view = PoolRiskView::new(&pool);
        assert_eq!(view.borrow_capacity(5, 1, 20_000), Ok(0));
        assert_eq!(view.liquidation_limit(5, 1, 20_000), Ok(0));
        // Asset 2 is capped at 5_000.
        assert_eq!(view.borrow_capacity(5, 2, 20_000), Ok(5_000));
    }

    #[test]
    fn invalid_amounts_surface_errors_and_keep_weights_cached() {
        let pool = config();
        let mut view = PoolRiskView::new(&pool);
        assert_eq!(
            view.borrow_capacity(5, 1, -1),
            Err(ConfigError::Registry(ParameterError::InvalidAmount))
        );
        assert_eq!(view.borrow_capacity(5, 1, 20_000), Ok(15_000));
        let stats = view.stats();
        assert_eq!((stats.misses, stats.hits), (1, 1));
    }

    #[test]
    fn many_unknown_assets_never_break_lookups() {
        let pool = config();
        let mut view = PoolRiskView::new(&pool);
        for asset in 1_000..1_000 + MAX_ASSETS as u64 + 8 {
            assert_eq!(view.borrow_capacity(5, asset, 10_000), Ok(0));
        }
        assert_eq!(view.stats().bypasses, 8);
        assert_eq!(view.borrow_capacity(5, 1, 20_000), Ok(15_000));
    }
}
