//! # Oracle Hub
//!
//! A dedicated, governance-managed contract for price feed management.
//!
//! The hub decouples price aggregation from lending logic:
//! - **Pluggable providers**: feeds can be push-based (providers call
//!   `report_price`) or pull-based (external contracts implementing the
//!   [`interface::PriceProvider`] interface are queried by the hub).
//! - **Aggregation strategies**: median (default, outlier-resistant) or
//!   confidence-weighted mean, configurable globally or per asset.
//! - **Health monitoring**: per-feed staleness classification, consecutive
//!   failure tracking, and an auto-opening per-asset circuit breaker.
//! - **Upgrade mechanism**: governance stages a WASM hash and atomically swaps
//!   the contract code via `env.deployer().update_current_contract_wasm`.
//! - **Emergency controls**: global freeze plus per-asset governance freeze.
//!
//! See the `docs/` directory for the full architecture and integration guide.

#![no_std]

mod aggregation;
mod health;
mod interface;
mod provider;
mod storage;
mod types;
mod upgrade;

#[cfg(test)]
mod tests;

use crate::types::{
    AggregatedPrice, AggregationStrategy, FeedMode, FeedPriority, FeedQuote, FeedStatus,
    OracleHealthStatus, PriceFeed, PricePoint, ProviderPrice, VERSION,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, Address, Bytes, BytesN, Env, Vec,
};

/// Errors surfaced by the Oracle Hub.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleHubError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Frozen = 3,
    FeedNotFound = 4,
    FeedDisabled = 5,
    InvalidPrice = 6,
    NoActiveFeeds = 7,
    FetchFailed = 8,
    InvalidConfig = 9,
}

/// The Oracle Hub contract.
#[contract]
pub struct OracleHubContract;

#[contractimpl]
impl OracleHubContract {
    // ── Initialization ─────────────────────────────────────────────────────

    pub fn initialize(env: Env, governance: Address, admin: Address) {
        if env
            .storage()
            .instance()
            .get::<_, Address>(&storage::DataKey::Governance)
            .is_some()
        {
            panic_with_error!(&env, OracleHubError::AlreadyInitialized);
        }
        env.storage()
            .instance()
            .set(&storage::DataKey::Governance, &governance);
        env.storage()
            .instance()
            .set(&storage::DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&storage::DataKey::Version, &VERSION);
        env.storage()
            .instance()
            .set(&storage::DataKey::Frozen, &false);
        env.storage()
            .instance()
            .set(&storage::DataKey::FeedCount, &0u32);
        env.storage().instance().set(
            &storage::DataKey::DefaultStrategy,
            &AggregationStrategy::Median,
        );
    }

    pub fn version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&storage::DataKey::Version)
            .unwrap_or(0)
    }

    // ── Upgrade mechanism ──────────────────────────────────────────────────

    /// Governance stages the next contract code.
    pub fn stage_upgrade(env: Env, new_wasm: BytesN<32>) {
        let governance = require_governance(&env);
        governance.require_auth();
        require_not_frozen(&env);
        upgrade::stage_upgrade(&env, new_wasm, &governance);
    }

    /// Governance applies the staged upgrade, atomically swapping contract code.
    pub fn upgrade(env: Env) -> BytesN<32> {
        let governance = require_governance(&env);
        governance.require_auth();
        require_not_frozen(&env);
        upgrade::apply_upgrade(&env, &governance)
    }

    /// Staged upgrade WASM hash, if any.
    pub fn pending_wasm_hash(env: Env) -> Option<BytesN<32>> {
        upgrade::pending_wasm(&env)
    }

    // ── Feed management ────────────────────────────────────────────────────

    /// Governance registers a feed slot for an asset.
    pub fn register_feed(
        env: Env,
        asset: Bytes,
        oracle_address: Address,
        priority: FeedPriority,
        stale_threshold_seconds: u64,
        mode: FeedMode,
        weight_bps: u32,
    ) {
        let governance = require_governance(&env);
        governance.require_auth();
        require_not_frozen(&env);

        assert!(!asset.is_empty(), "Asset must not be empty");
        assert!(
            oracle_address != env.current_contract_address(),
            "Oracle must not be the hub contract itself"
        );

        let threshold = if stale_threshold_seconds == 0 {
            types::DEFAULT_STALE_THRESHOLD_SECONDS
        } else {
            stale_threshold_seconds
        };
        let weight = if weight_bps == 0 {
            types::DEFAULT_FEED_WEIGHT_BPS
        } else {
            weight_bps
        };

        let feed = PriceFeed {
            asset: asset.clone(),
            oracle_address: oracle_address.clone(),
            priority,
            enabled: true,
            stale_threshold_seconds: threshold,
            registered_at: env.ledger().timestamp(),
            mode,
            weight_bps: weight,
        };

        let feed_key = storage::DataKey::Feed(asset.clone(), priority as u32);
        env.storage().instance().set(&feed_key, &feed);

        let count: u32 = env
            .storage()
            .instance()
            .get(&storage::DataKey::FeedCount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&storage::DataKey::FeedCount, &(count + 1));

        types::FeedRegisteredEvent {
            asset: asset.clone(),
            oracle: oracle_address,
            priority: priority as u32,
            mode,
            weight_bps: weight,
        }
        .publish(&env);
    }

    /// Governance updates an existing feed's staleness, mode, and weight.
    pub fn update_feed(
        env: Env,
        asset: Bytes,
        priority: FeedPriority,
        stale_threshold_seconds: u64,
        mode: FeedMode,
        weight_bps: u32,
    ) {
        require_governance(&env).require_auth();
        require_not_frozen(&env);

        let feed_key = storage::DataKey::Feed(asset.clone(), priority as u32);
        let mut feed: PriceFeed = env
            .storage()
            .instance()
            .get(&feed_key)
            .unwrap_or_else(|| panic_with_error!(&env, OracleHubError::FeedNotFound));
        feed.stale_threshold_seconds = if stale_threshold_seconds == 0 {
            types::DEFAULT_STALE_THRESHOLD_SECONDS
        } else {
            stale_threshold_seconds
        };
        feed.mode = mode;
        feed.weight_bps = if weight_bps == 0 {
            types::DEFAULT_FEED_WEIGHT_BPS
        } else {
            weight_bps
        };
        env.storage().instance().set(&feed_key, &feed);

        types::FeedUpdatedEvent {
            asset: asset.clone(),
            priority: priority as u32,
            mode,
            stale_threshold_seconds: feed.stale_threshold_seconds,
        }
        .publish(&env);
    }

    /// View an asset's feed configuration for a priority slot.
    pub fn get_feed(env: Env, asset: Bytes, priority: FeedPriority) -> Option<PriceFeed> {
        env.storage()
            .instance()
            .get(&storage::DataKey::Feed(asset, priority as u32))
    }

    pub fn disable_feed(env: Env, asset: Bytes, priority: FeedPriority) {
        require_governance(&env).require_auth();
        let feed_key = storage::DataKey::Feed(asset.clone(), priority as u32);
        let mut feed: PriceFeed = env
            .storage()
            .instance()
            .get(&feed_key)
            .unwrap_or_else(|| panic_with_error!(&env, OracleHubError::FeedNotFound));
        feed.enabled = false;
        env.storage().instance().set(&feed_key, &feed);
        types::FeedDisabledEvent {
            asset: asset.clone(),
            priority: priority as u32,
        }
        .publish(&env);
    }

    pub fn enable_feed(env: Env, asset: Bytes, priority: FeedPriority) {
        require_governance(&env).require_auth();
        let feed_key = storage::DataKey::Feed(asset.clone(), priority as u32);
        let mut feed: PriceFeed = env
            .storage()
            .instance()
            .get(&feed_key)
            .unwrap_or_else(|| panic_with_error!(&env, OracleHubError::FeedNotFound));
        feed.enabled = true;
        env.storage().instance().set(&feed_key, &feed);
        types::FeedEnabledEvent {
            asset: asset.clone(),
            priority: priority as u32,
        }
        .publish(&env);
    }

    // ── Aggregation strategy ───────────────────────────────────────────────

    /// Governance sets the default strategy or a per-asset override.
    pub fn set_aggregation_strategy(env: Env, asset: Option<Bytes>, strategy: AggregationStrategy) {
        require_governance(&env).require_auth();
        match asset {
            Some(asset) => {
                env.storage()
                    .instance()
                    .set(&storage::DataKey::Strategy(asset.clone()), &strategy);
                types::AssetStrategyUpdatedEvent { asset, strategy }.publish(&env);
            }
            None => {
                env.storage()
                    .instance()
                    .set(&storage::DataKey::DefaultStrategy, &strategy);
                types::DefaultStrategyUpdatedEvent { strategy }.publish(&env);
            }
        }
    }

    /// Effective aggregation strategy for an asset (per-asset override or default).
    pub fn get_aggregation_strategy(env: Env, asset: Bytes) -> AggregationStrategy {
        env.storage()
            .instance()
            .get::<_, AggregationStrategy>(&storage::DataKey::Strategy(asset))
            .or_else(|| {
                env.storage()
                    .instance()
                    .get::<_, AggregationStrategy>(&storage::DataKey::DefaultStrategy)
            })
            .unwrap_or(AggregationStrategy::Median)
    }

    // ── Emergency controls ─────────────────────────────────────────────────

    pub fn freeze(env: Env) {
        let governance = require_governance(&env);
        governance.require_auth();
        env.storage()
            .instance()
            .set(&storage::DataKey::Frozen, &true);
        types::FrozenEvent { admin: governance }.publish(&env);
    }

    pub fn unfreeze(env: Env) {
        let governance = require_governance(&env);
        governance.require_auth();
        env.storage()
            .instance()
            .set(&storage::DataKey::Frozen, &false);
        types::UnfrozenEvent { admin: governance }.publish(&env);
    }

    pub fn is_frozen(env: Env) -> bool {
        is_globally_frozen(&env)
    }

    /// Governance freezes a single asset's pricing for the default cooldown.
    pub fn freeze_asset(env: Env, asset: Bytes) {
        require_governance(&env).require_auth();
        health::freeze_asset(&env, &asset);
    }

    pub fn unfreeze_asset(env: Env, asset: Bytes) {
        require_governance(&env).require_auth();
        health::unfreeze_asset(&env, &asset);
    }

    pub fn is_asset_frozen(env: Env, asset: Bytes) -> bool {
        health::is_frozen(&env, &asset)
    }

    // ── Price reporting (push) ─────────────────────────────────────────────

    /// A registered push provider reports a new price for its feed slot.
    pub fn report_price(
        env: Env,
        asset: Bytes,
        price: i128,
        confidence: u32,
        priority: FeedPriority,
    ) {
        let feed_key = storage::DataKey::Feed(asset.clone(), priority as u32);
        let feed: PriceFeed = env
            .storage()
            .instance()
            .get(&feed_key)
            .unwrap_or_else(|| panic_with_error!(&env, OracleHubError::FeedNotFound));

        feed.oracle_address.require_auth();

        require_not_frozen(&env);
        if !feed.enabled {
            panic_with_error!(&env, OracleHubError::FeedDisabled);
        }
        if price <= 0 {
            panic_with_error!(&env, OracleHubError::InvalidPrice);
        }

        let price_point = PricePoint {
            asset: asset.clone(),
            price,
            timestamp: env.ledger().timestamp(),
            confidence,
        };
        let latest_key = storage::DataKey::LatestPrice(asset.clone(), priority as u32);
        env.storage().instance().set(&latest_key, &price_point);

        types::PriceReportedEvent {
            asset: asset.clone(),
            priority: priority as u32,
            price,
            confidence,
        }
        .publish(&env);
    }

    // ── Price pulls (provider interface) ───────────────────────────────────

    /// Pull a live price from any registered `PriceProvider` contract.
    pub fn fetch_provider_price(env: Env, asset: Bytes, provider: Address) -> ProviderPrice {
        require_not_frozen(&env);
        provider::fetch_provider_price(&env, &asset, &provider)
    }

    // ── Price queries ──────────────────────────────────────────────────────

    /// Raw price (i128) for an asset using its effective strategy.
    pub fn price(env: Env, asset: Bytes) -> i128 {
        Self::get_price(env, asset).price
    }

    /// Aggregate all active feeds for an asset into a single price.
    ///
    /// Pull-mode feeds are fetched live from their provider; push-mode feeds
    /// use the latest reported point. Stale feeds are auto-disabled. A
    /// successful read self-heals any auto-opened asset breaker.
    pub fn get_price(env: Env, asset: Bytes) -> AggregatedPrice {
        require_not_frozen(&env);
        if health::is_frozen(&env, &asset) {
            panic_with_error!(&env, OracleHubError::Frozen);
        }

        let current_time = env.ledger().timestamp();
        let mut quotes: Vec<FeedQuote> = Vec::new(&env);

        for priority in 0u32..=2 {
            let feed_key = storage::DataKey::Feed(asset.clone(), priority);
            if let Some(feed) = env.storage().instance().get::<_, PriceFeed>(&feed_key) {
                if !feed.enabled {
                    continue;
                }

                let point =
                    if feed.mode == FeedMode::Pull {
                        // Live pull from the provider. Failures revert the read,
                        // which is the safe behavior: never return a price when a
                        // registered provider is unavailable.
                        let fetched =
                            provider::fetch_provider_price(&env, &asset, &feed.oracle_address);
                        types::PricePulledEvent {
                            asset: asset.clone(),
                            provider: feed.oracle_address.clone(),
                            price: fetched.price,
                            confidence: fetched.confidence,
                        }
                        .publish(&env);
                        let point = provider::to_price_point(&env, &asset, fetched);
                        env.storage().instance().set(
                            &storage::DataKey::LatestPrice(asset.clone(), priority),
                            &point,
                        );
                        Some(point)
                    } else {
                        env.storage().instance().get::<_, PricePoint>(
                            &storage::DataKey::LatestPrice(asset.clone(), priority),
                        )
                    };

                if let Some(point) = point {
                    let stale =
                        current_time.saturating_sub(point.timestamp) > feed.stale_threshold_seconds;
                    if stale {
                        auto_disable_feed(&env, &asset, priority);
                        continue;
                    }
                    quotes.push_back(FeedQuote {
                        price: point.price,
                        timestamp: point.timestamp,
                        confidence: point.confidence,
                        priority,
                        weight_bps: feed.weight_bps,
                    });
                }
            }
        }

        if quotes.is_empty() {
            panic_with_error!(&env, OracleHubError::NoActiveFeeds);
        }

        let strategy = Self::get_aggregation_strategy(env.clone(), asset.clone());
        let (price, confidence, timestamp) = aggregation::aggregate(&env, quotes.clone(), strategy)
            .unwrap_or_else(|_| panic_with_error!(&env, OracleHubError::NoActiveFeeds));

        // A successful read proves the asset recovered; clear any auto-opened breaker.
        health::recover_breaker_if_healthy(&env, &asset);

        AggregatedPrice {
            price,
            timestamp,
            confidence,
            num_feeds: quotes.len(),
            num_active_feeds: quotes.len(),
            strategy,
        }
    }

    // ── Health monitoring ──────────────────────────────────────────────────

    /// Per-feed health classification for all registered slots of an asset.
    pub fn check_feed_health(env: Env, asset: Bytes) -> Vec<FeedStatus> {
        health::check_feeds(&env, &asset)
    }

    /// Record a failed fetch; auto-opens the per-asset breaker on threshold.
    pub fn monitor_oracle_health(env: Env, asset: Bytes) -> OracleHealthStatus {
        health::monitor_oracle_health(&env, &asset)
    }

    /// Record a successful fetch, resetting the failure counter and last-success.
    pub fn record_oracle_success(env: Env, asset: Bytes) {
        health::record_oracle_success(&env, &asset);
    }

    /// Read-only health snapshot for an asset.
    pub fn get_health(env: Env, asset: Bytes) -> OracleHealthStatus {
        health::get_health(&env, &asset)
    }
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn is_globally_frozen(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<_, bool>(&storage::DataKey::Frozen)
        .unwrap_or(false)
}

fn require_not_frozen(env: &Env) {
    if is_globally_frozen(env) {
        panic_with_error!(env, OracleHubError::Frozen);
    }
}

fn require_governance(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&storage::DataKey::Governance)
        .unwrap_or_else(|| panic_with_error!(env, OracleHubError::NotInitialized))
}

fn auto_disable_feed(env: &Env, asset: &Bytes, priority: u32) {
    let feed_key = storage::DataKey::Feed(asset.clone(), priority);
    let mut feed: PriceFeed = env
        .storage()
        .instance()
        .get(&feed_key)
        .expect("Feed not found");
    if feed.enabled {
        feed.enabled = false;
        env.storage().instance().set(&feed_key, &feed);
        types::FeedAutoDisabledEvent {
            asset: asset.clone(),
            priority,
        }
        .publish(env);
    }
}
