#![no_std]

pub mod adapter;

pub use adapter::{
    AdapterError, AdapterTelemetry, ContractPriceSource, NoopTelemetry, OracleAdapter, PriceFeed,
    PriceFeedClient, PriceQuote, PriceSource, SorobanTelemetry, SourceKind,
};
