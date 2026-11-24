#![no_std]
//! Typed pool configuration and normalised events (#1760, #1761).

pub mod config;
pub mod events;

pub use config::{ConfigError, PoolConfig};
pub use events::{context, deposit, liquidation, repay};
