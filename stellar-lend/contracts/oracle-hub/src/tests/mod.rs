//! Oracle Hub test suite.
//!
//! Organized into focused suites covering feed management, multi-source
//! aggregation, decimal normalization, deviation-checked fallback, the price
//! cache, pull providers, health monitoring, emergency freeze controls, read
//! cost, and the upgrade mechanism. All suites share the harness in
//! `helpers`.

mod aggregation_test;
mod cache_test;
mod fallback_test;
mod feed_test;
mod freeze_test;
mod gas_test;
mod health_test;
mod helpers;
mod precision_test;
mod provider_test;
mod upgrade_test;
