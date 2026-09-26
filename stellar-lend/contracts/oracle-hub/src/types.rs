//! Shared data types, constants, and contract events for the Oracle Hub.

use soroban_sdk::{contractevent, contracttype, Address, Bytes};

/// Current protocol version baked into the initial deployment.
pub const VERSION: u32 = 2;

/// Number of feed slots an asset may register (`FeedPriority` variants).
///
/// Aggregation is bounded by this value: a quote vector can never hold more
/// than [`MAX_FEEDS_PER_ASSET`] entries, which keeps the read path gas
/// predictable no matter how many sources governance wires up.
pub const MAX_FEEDS_PER_ASSET: u32 = 5;

/// Default maximum deviation from the median (in basis points) before a quote
/// is treated as an outlier during aggregation. Defaults to 20 %.
pub const OUTLIER_DEVIATION_BPS: i128 = 2_000;

/// Default number of sources that must agree before a price is published when
/// the asset has more than one registered source.
pub const DEFAULT_MIN_SOURCES: u32 = 2;

/// Default staleness threshold used when a feed is registered without one.
pub const DEFAULT_STALE_THRESHOLD_SECONDS: u64 = 3600;

/// Default per-feed weight used by the weighted aggregation strategy.
pub const DEFAULT_FEED_WEIGHT_BPS: u32 = 10_000;

/// Canonical number of decimals every aggregated price is expressed in.
///
/// Push reporters must submit canonical-scale prices; pull providers declare
/// their own scale in [`ProviderPrice::decimals`] and the hub rescales the
/// quote before it takes part in aggregation.
pub const CANONICAL_DECIMALS: u32 = 8;

/// Largest provider decimal count the hub will rescale from. Beyond this the
/// rescale can overflow `i128`, so the quote is rejected instead.
pub const MAX_PROVIDER_DECIMALS: u32 = 18;

/// Default price cache TTL in seconds. `0` disables caching, which is the
/// default because serving a memoized price to a lending market is a
/// security-relevant choice; governance opts in with `set_cache_ttl`.
pub const DEFAULT_CACHE_TTL_SECONDS: u64 = 0;

/// Largest cache TTL governance may configure (1 hour).
pub const MAX_CACHE_TTL_SECONDS: u64 = 3600;

/// Basis-point denominator used across the hub.
pub const BPS_DENOM: i128 = 10_000;

/// Priority of a feed slot. Lower slots are consulted first during fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum FeedPriority {
    /// Highest-priority feed slot for an asset.
    Primary = 0,
    /// Second feed slot; used when the primary is stale or disabled.
    Secondary = 1,
    /// Third feed slot. Historical slot of the `Fallback` tier; kept stable so
    /// previously registered deployments keep resolving the same way.
    Fallback = 2,
    /// Fourth feed slot; used only when every higher slot is unusable.
    Quaternary = 3,
    /// Fifth and final feed slot.
    Quinary = 4,
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
    /// Mean of the accepted quotes after dropping the highest and the lowest.
    /// Robust against a single corrupted source on either side of the median.
    TrimmedMean = 2,
}

/// Origin of the price returned by `get_price`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum PriceSource {
    /// Aggregated from every source that passed the deviation check.
    Consensus = 0,
    /// The asset's only usable source.
    Sole = 1,
    /// A memoized price served from the cache entry.
    Cached = 2,
}

/// Per-asset aggregation and deviation-check parameters.
///
/// A `0` on either numeric field means "inherit the hub default", which keeps
/// the stored struct small for the common case.
#[derive(Clone, Copy, Debug, PartialEq)]
#[contracttype]
pub struct AggregationParams {
    /// Strategy used for this asset.
    pub strategy: AggregationStrategy,
    /// Maximum deviation (bps) from the median before a source is rejected.
    /// Read only when `deviation_configured` is set; a value of `0` then
    /// disables the deviation check entirely.
    pub max_deviation_bps: i128,
    /// Whether governance pinned `max_deviation_bps` for this asset. Without
    /// it the hub default applies, which keeps `0` free to mean "no deviation
    /// check" instead of "unset".
    pub deviation_configured: bool,
    /// Sources that must agree before a price is published.
    /// `0` inherits [`DEFAULT_MIN_SOURCES`].
    pub min_sources: u32,
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
    /// Price already rescaled to the canonical decimal precision.
    pub price: i128,
    pub timestamp: u64,
    pub confidence: u32,
    /// Feed slot (0..[`MAX_FEEDS_PER_ASSET`]) the quote came from.
    pub priority: u32,
    /// Feed-configured weight for the `Weighted` strategy.
    pub weight_bps: u32,
    /// Freshness budget of the source that produced the quote. A memoized
    /// price may never outlive the shortest budget that fed into it.
    pub stale_threshold_seconds: u64,
}

/// Result of aggregating the accepted quotes of an asset.
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
    /// Number of candidate sources the deviation check threw away.
    pub rejected_sources: u32,
    /// True when a lower-priority source replaced the leading one.
    pub used_fallback: bool,
    /// True when the price was served from the cache instead of being recomputed.
    pub from_cache: bool,
    /// Where the returned price came from.
    pub source: PriceSource,
    /// Largest deviation (in basis points) observed inside the accepted set.
    pub deviation_bps: i128,
}

/// Memoized aggregate plus the bookkeeping the cache needs on read.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct CachedPrice {
    /// The memoized aggregate.
    pub price: AggregatedPrice,
    /// Ledger timestamp the entry was written at.
    pub cached_at: u64,
    /// Effective TTL: the smaller of the configured TTL and the freshness
    /// budget of every source that fed the aggregate.
    pub ttl_seconds: u64,
    /// Configuration epoch the entry was produced under. Any governance
    /// change bumps the epoch, which invalidates every entry at once.
    pub epoch: u32,
}

/// Cache observability counters, exposed through `get_cache_stats`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[contracttype]
pub struct CacheStats {
    /// Reads answered from the cache.
    pub hits: u32,
    /// Reads that had to recompute.
    pub misses: u32,
    /// Entries written after a recompute.
    pub writes: u32,
    /// Reads served straight from a pull provider.
    pub pull_reads: u32,
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
pub struct ProviderPriceRescaledEvent {
    #[topic]
    pub asset: Bytes,
    pub provider: Address,
    pub raw_price: i128,
    pub price: i128,
    pub from_decimals: u32,
    pub to_decimals: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct DeviationRejectedEvent {
    #[topic]
    pub asset: Bytes,
    pub priority: u32,
    pub price: i128,
    pub reference: i128,
    pub deviation_bps: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FallbackActivatedEvent {
    #[topic]
    pub asset: Bytes,
    pub rejected_priority: u32,
    pub serving_priority: u32,
    pub remaining_sources: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PriceCachedEvent {
    #[topic]
    pub asset: Bytes,
    pub price: i128,
    pub ttl_seconds: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CacheServedEvent {
    #[topic]
    pub asset: Bytes,
    pub price: i128,
    pub age_seconds: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CacheConfigUpdatedEvent {
    pub cache_ttl_seconds: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PriceDecimalsUpdatedEvent {
    pub decimals: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AggregationParamsUpdatedEvent {
    #[topic]
    pub asset: Bytes,
    pub strategy: AggregationStrategy,
    pub max_deviation_bps: i128,
    pub min_sources: u32,
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

// The explicit topic overrides the derived snake-case name, which would
// exceed the 32-character symbol limit.
#[contractevent(topics = ["upgrade_multisig_configured"])]
#[derive(Clone, Debug)]
pub struct UpgradeMultisigConfiguredEvent {
    pub threshold: u32,
}

#[contractevent(topics = ["upgrade_approved"])]
#[derive(Clone, Debug)]
pub struct UpgradeApprovedEvent {
    #[topic]
    pub approver: Address,
    pub approval_count: u32,
    pub timelock_until: u64,
}
