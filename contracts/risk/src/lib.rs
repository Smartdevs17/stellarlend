#![no_std]
//! Risk helpers shared by the pool contracts.
//!
//! * [`haircut`] — collateral haircuts by volatility bucket (#1742).
//! * [`cache`] — per-block memoization of risk-weight lookups (#1774).

pub mod cache;
pub mod haircut;

pub use cache::{CacheStats, RiskWeightCache, DEFAULT_CAPACITY};
