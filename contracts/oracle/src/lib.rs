#![no_std]

pub mod adapter;
pub mod cache;

pub use adapter::{
    AdapterError, AdapterTelemetry, ContractPriceSource, NoopTelemetry, OracleAdapter, PriceFeed,
    PriceFeedClient, PriceQuote, PriceSource, SorobanTelemetry, SourceKind,
};
pub use cache::{get_or_fetch, invalidate, CachedPrice};
