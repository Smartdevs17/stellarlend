//! Pull-based price provider integration.
//!
//! Feeds registered in [`crate::types::FeedMode::Pull`] mode point at an
//! external contract implementing the [`crate::interface::PriceProvider`]
//! interface. The hub fetches the live price through the generated
//! `PriceProviderClient`, validates it, and clamps the provider timestamp to
//! the current ledger time so that stale timestamps can never masquerade as
//! fresh quotes.

use crate::interface::PriceProviderClient;
use crate::types::{PricePoint, ProviderPrice};
use soroban_sdk::{Address, Bytes, Env};

/// Fetch and validate a price from a pull-based provider contract.
pub fn fetch_provider_price(env: &Env, asset: &Bytes, provider: &Address) -> ProviderPrice {
    let mut price = price_provider_client(env, provider).get_price(asset);

    assert!(price.price > 0, "Provider returned non-positive price");

    let now = env.ledger().timestamp();
    if price.timestamp > now {
        price.timestamp = now;
    }

    price
}

/// Initialize a `PriceProviderClient` for a provider address.
pub fn price_provider_client<'a>(env: &'a Env, provider: &Address) -> PriceProviderClient<'a> {
    PriceProviderClient::new(env, provider)
}

/// Build a storable `PricePoint` from a validated provider quote.
pub fn to_price_point(_env: &Env, asset: &Bytes, price: ProviderPrice) -> PricePoint {
    PricePoint {
        asset: asset.clone(),
        price: price.price,
        timestamp: price.timestamp,
        confidence: price.confidence,
    }
}
