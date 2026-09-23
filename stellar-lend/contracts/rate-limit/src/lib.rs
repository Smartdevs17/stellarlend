//! # StellarLend Rate-Limiting Library (issue #704)
//!
//! Policy-based, shared rate-limiting for StellarLend contracts. Historically each
//! contract implemented its own rate limiting with bespoke token-bucket math; this
//! crate centralizes:
//!
//! * the pure **token-bucket** algorithm in [`token_bucket`] (fixed-point, `1e6`
//!   scale) independent of any `Env` so it can be unit-tested exhaustively and shared;
//! * a **policy engine** ([`policy`]) replacing ad-hoc `default_config` dispatch with
//!   explicit, layered [`RateLimitPolicy`] resolution (default → per-operation →
//!   per-op+pool);
//! * **analytics** ([`analytics`]) producing operator-facing `RateLimitAnalytics`
//!   snapshots, mirroring the contract's existing monitoring hook;
//! * a **configuration API** ([`config`]) for encoding/decoding policies.
//!
//! This crate is `#![no_std]` so it compiles into Soroban WASM. The `Env`-dependent
//! storage/key/admin glue stays in each contract; all decisioning math lives here.

#![no_std]

extern crate alloc;

pub mod analytics;
pub mod config;
pub mod policy;
pub mod token_bucket;

pub use analytics::{RateLimitAnalytics, RateLimitStatus};
pub use policy::{
    resolve, resolve_config, DefaultPolicy, PolicyLayer, PolicySelector, RateLimitConfig,
    ResolvedLimit,
};
pub use token_bucket::{token_bucket_consume, BucketOutcome, BucketState, TokenBucketError};
