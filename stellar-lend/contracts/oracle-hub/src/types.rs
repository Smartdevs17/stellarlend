//! Shared data types, constants, and contract events for the Oracle Hub.

use soroban_sdk::{contractevent, contracttype, Address, Bytes};

/// Current protocol version baked into the initial deployment.
pub const VERSION: u32 = 1;

/// Upper bound on the number of feed slots an asset may register.
/// Each asset can register up to three priority slots, so this is purely a
/// safety bound for aggregation arrays.
pub const MAX_FEEDS_PER_ASSET: u32 = 5;

/// Maximum deviation from the median (in basis points) before a quote is
/// treated as an outlier during aggregation. Defaults to 20 %.
pub const OUTLIER_DEVIATION_BPS: i128 = 2_000;

/// Default staleness threshold used when a feed is registered without one.
pub const DEFAULT_STALE_THRESHOLD_SECONDS: u64 = 3600;

/// Default per-feed weight used by the weighted aggregation strategy.
pub const DEFAULT_FEED_WEIGHT_BPS: u32 = 10_000;

/// Priority of a feed slot. Primary is leading; Fallback is used last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum FeedPriority {
    /// Highest-priority feed slot for an asset.
    Primary = 0,
    /// Second feed slot; used when the primary is stale or disabled.
    Secondary = 1,
    /// Final feed slot; used when all higher-priority slots are unavailable.
    Fallback = 2,
}

/// How a feed obtains prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum FeedMode {
    /// Providers push prices via `report_price`.
    Push = 0,
    /// The hub pulls prices live from a `PriceProvider` contract.
    Pull = 1,
}

/// Aggregation strategy used to combine multiple feed quotes for an asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum AggregationStrategy {
    /// Robust, outlier-resistant median across active feeds.
    Median = 0,
    /// Confidence- and weight-adjusted mean across active feeds.
    Weighted = 1,
}

/// Registered configuration of a single feed slot for an asset.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PriceFeed {
    /// Asset this feed prices.
    pub asset: Bytes,
    /// Oracle address. For `Push` feeds this is the reporter that must
    /// authorize `report_price`; for `Pull` feeds it is the `PriceProvider`
    /// contract the hub queries.
    pub oracle_address: Address,
    /// Priority slot this feed occupies.
    pub priority: FeedPriority,
    /// Whether the feed participates in aggregation.
    pub enabled: bool,
    /// Maximum age (seconds) before a quote is considered stale.
    pub stale_threshold_seconds: u64,
    /// Ledger timestamp when the feed was registered.
    pub registered_at: u64,
    /// Push or pull mode.
    pub mode: FeedMode,
    /// Relative weight used by the `Weighted` strategy (0 = default weight).
    pub weight_bps: u32,
}

/// Latest reported (or last pulled) price point for a feed slot.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PricePoint {
    /// Asset this point prices.
    pub asset: Bytes,
    /// Price in the smallest relevant unit.
    pub price: i128,
    /// Ledger timestamp when the quote was recorded.
    pub timestamp: u64,
    /// Provider confidence for the quote (0 = unknown).
    pub confidence: u32,
}

/// A single candidate quote collected during aggregation.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct FeedQuote {
    pub price: i128,
    pub timestamp: u64,
    pub confidence: u32,
    /// Priority slot (0..=2) the quote came from.
    pub priority: u32,
    /// Feed-configured weight for the `Weighted` strategy.
    pub weight_bps: u32,
}

/// Result of aggregating all active feeds for an asset.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct AggregatedPrice {
    pub price: i128,
    pub timestamp: u64,
    pub confidence: u32,
    /// Number of quotes that participated in the aggregation.
    pub num_feeds: u32,
    /// Number of active (non-stale, enabled) feeds observed.
    pub num_active_feeds: u32,
    /// Strategy used for this asset.
    pub strategy: AggregationStrategy,
}

/// Health classification of a single feed slot.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum FeedStatusCode {
    Active = 0,
    Stale = 1,
    Disabled = 2,
    Frozen = 3,
}

/// Health snapshot of a single feed slot.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct FeedStatus {
    pub asset: Bytes,
    pub status: FeedStatusCode,
    pub last_update: u64,
    pub is_stale: bool,
}

/// Per-asset circuit-breaker state.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct BreakerState {
    /// Ledger timestamp before which pricing for the asset is halted.
    pub open_until: u64,
    /// True when the breaker was opened automatically by health monitoring.
    pub auto: bool,
}

/// Health summary for an asset used by monitoring infrastructure.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct OracleHealthStatus {
    pub asset: Bytes,
    pub consecutive_failures: u32,
    pub last_success_timestamp: u64,
    pub circuit_breaker_open: bool,
    /// True when `monitor_oracle_health` auto-opened the breaker.
    pub auto_triggered: bool,
    pub active_feeds: u32,
}

/// Price quote returned by an external `PriceProvider` contract.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ProviderPrice {
    pub price: i128,
    pub decimals: u32,
    pub timestamp: u64,
    pub confidence: u32,
}

// ── Contract events ────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeedRegisteredEvent {
    #[topic]
    pub asset: Bytes,
    pub oracle: Address,
    pub priority: u32,
    pub mode: FeedMode,
    pub weight_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeedUpdatedEvent {
    #[topic]
    pub asset: Bytes,
    pub priority: u32,
    pub mode: FeedMode,
    pub stale_threshold_seconds: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeedDisabledEvent {
    #[topic]
    pub asset: Bytes,
    pub priority: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeedEnabledEvent {
    #[topic]
    pub asset: Bytes,
    pub priority: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PriceReportedEvent {
    #[topic]
    pub asset: Bytes,
    pub priority: u32,
    pub price: i128,
    pub confidence: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PricePulledEvent {
    #[topic]
    pub asset: Bytes,
    pub provider: Address,
    pub price: i128,
    pub confidence: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct DefaultStrategyUpdatedEvent {
    pub strategy: AggregationStrategy,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AssetStrategyUpdatedEvent {
    #[topic]
    pub asset: Bytes,
    pub strategy: AggregationStrategy,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeedAutoDisabledEvent {
    #[topic]
    pub asset: Bytes,
    pub priority: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct BreakerOpenedEvent {
    #[topic]
    pub asset: Bytes,
    pub open_until: u64,
    pub auto: bool,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct BreakerUnfrozenEvent {
    #[topic]
    pub asset: Bytes,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct HealthFailureEvent {
    #[topic]
    pub asset: Bytes,
    pub consecutive_failures: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct HealthSuccessEvent {
    #[topic]
    pub asset: Bytes,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FrozenEvent {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct UnfrozenEvent {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct UpgradeStagedEvent {
    pub wasm_hash: soroban_sdk::BytesN<32>,
    pub staged_by: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct UpgradeExecutedEvent {
    pub old_version: u32,
    pub new_version: u32,
    pub wasm_hash: soroban_sdk::BytesN<32>,
    pub executed_by: Address,
}
