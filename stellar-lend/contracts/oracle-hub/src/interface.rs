//! Pluggable price provider interface.
//!
//! Any external contract that implements `get_price(Env, Bytes) -> ProviderPrice`
//! can be plugged into the Oracle Hub as a pull-based feed. The hub talks to the
//! provider through the generated `PriceProviderClient`, so providers can be
//! swapped without changing the hub.

use crate::types::ProviderPrice;
use soroban_sdk::{contractclient, Bytes, Env};

/// Standard interface implemented by external price providers.
///
/// A compliant provider is expected to:
/// - return a positive `price`,
/// - return a `timestamp` no later than the current ledger time,
/// - be callable by any address (read-only, no authorization required), and
/// - not panic for supported assets.
///
/// The trait is the provider spec; the generated `PriceProviderClient` below
/// is the on-chain channel the hub uses to call arbitrary provider contracts.
#[allow(dead_code)]
#[contractclient(name = "PriceProviderClient")]
pub trait PriceProvider {
    /// Fetch the latest price for `asset`.
    ///
    /// The returned `ProviderPrice` carries the price, the number of decimals
    /// of the price representation, the provider's timestamp, and an optional
    /// confidence score (0 = unknown).
    fn get_price(env: Env, asset: Bytes) -> ProviderPrice;
}
