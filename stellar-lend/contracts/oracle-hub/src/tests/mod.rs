//! Oracle Hub test suite.
//!
//! Organized into focused suites covering feed management, aggregation
//! strategies, pull providers, health monitoring, emergency freeze controls,
//! and the upgrade mechanism. All suites share the harness in `helpers`.

mod aggregation_test;
mod feed_test;
mod freeze_test;
mod health_test;
mod helpers;
mod provider_test;
mod upgrade_test;
