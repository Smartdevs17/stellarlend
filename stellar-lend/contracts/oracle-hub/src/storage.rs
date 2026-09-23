//! Storage keys for the Oracle Hub contract.

use soroban_sdk::{contracttype, Bytes};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Governance address authorized to manage feeds, strategies, and upgrades.
    Governance,
    /// Secondary administrator address (informational role).
    Admin,
    /// Current contract version.
    Version,
    /// Global emergency freeze flag.
    Frozen,
    /// Number of registered feeds across all assets.
    FeedCount,
    /// Per (asset, priority) feed configuration. Value: `crate::types::PriceFeed`.
    Feed(Bytes, u32),
    /// Latest price point per (asset, priority). Value: `crate::types::PricePoint`.
    LatestPrice(Bytes, u32),
    /// Per-asset aggregation strategy override. Value: `crate::types::AggregationStrategy`.
    Strategy(Bytes),
    /// Default aggregation strategy used when no per-asset override exists.
    DefaultStrategy,
    /// Per-asset circuit-breaker state. Value: `crate::types::BreakerState`.
    AssetBreaker(Bytes),
    /// Consecutive failed price fetches for an asset. Value: `u32`.
    ConsecutiveFailures(Bytes),
    /// Ledger timestamp of the last successful price fetch. Value: `u64`.
    LastSuccess(Bytes),
    /// Staged upgrade WASM hash. Value: `soroban_sdk::BytesN<32>`.
    ProposedWasm,
}
