//! Contract-focused wrapper crate for StellarLend shared test fixtures.
//!
//! Issue #1616 extracts shared Rust test fixtures into a dedicated
//! `contract-test-utils` crate. This crate provides that stable package name
//! while reusing the existing unified `test-utils` implementation.
//!
//! New contract tests should depend on `contract-test-utils`; existing suites
//! can migrate incrementally.

pub use test_utils::*;
