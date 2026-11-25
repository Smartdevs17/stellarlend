#![no_std]
//! Typed pool configuration, normalised events, and memoized risk reads
//! (#1760, #1761, #1774).

pub mod config;
pub mod events;
pub mod math;
pub mod risk;

pub use config::{ConfigError, PoolConfig};
pub use events::{context, deposit, liquidation, repay};
pub use risk::PoolRiskView;
