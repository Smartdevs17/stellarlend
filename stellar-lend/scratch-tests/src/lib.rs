//! Test harness for the `hello-world` feature modules that landed in
//! issues #1586 (per-pool withdrawal queue) and #1580 (strategy registry).
//!
//! # Why a separate crate?
//!
//! `cargo test -p hello-world` is currently blocked: `hello-world` declares
//! `stellarlend-lending` as a dev-dependency, and that crate fails to compile
//! against the workspace's pinned `soroban-sdk` (a pre-existing, unrelated
//! breakage). Any inline `#[cfg(test)]` module in `hello-world` therefore
//! cannot be built.
//!
//! This crate depends on `hello-world` as a *normal* library dependency (so
//! `hello-world`'s dev-dependencies are never compiled) and on
//! `soroban-sdk`'s `testutils` feature, which lets the tests exercise the
//! feature modules' public API in a sandboxed `Env` without the broken
//! dev-dependency. Once the `stellarlend-lending` breakage is resolved
//! upstream, these tests can move back inline next to their modules.
//!
//! # Storage context
//!
//! On `soroban-sdk` 28, `env.storage()` is only reachable inside a contract
//! frame. The feature modules read/write instance & persistent storage, so
//! every test registers the throwaway [`harness::Ctx`] contract and wraps
//! each auth-bearing call in `env.as_contract(&contract, || { ... })` to give
//! it a contract address to key storage against. Each auth-bearing call gets
//! its own frame because `mock_all_auths` (recording mode) rejects a second
//! `require_auth` for the same address within one frame.

#![allow(dead_code)]
#![allow(clippy::all)]

#[cfg(test)]
mod harness;

#[cfg(test)]
mod withdraw_queue_tests;

#[cfg(test)]
mod strategy_tests;

#[cfg(test)]
mod vault_tests;
