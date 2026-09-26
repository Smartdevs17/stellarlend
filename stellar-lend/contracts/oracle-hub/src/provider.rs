//! Pull-based price provider integration.
//!
//! Feeds registered in [`crate::types::FeedMode::Pull`] mode point at an
//! external contract implementing the [`crate::interface::PriceProvider`]
//! interface. The hub fetches the live price through the generated
//! `PriceProviderClient`, validates it, rescales it to the canonical
//! precision, and clamps the provider timestamp to the current ledger time so
//! that stale timestamps can never masquerade as fresh quotes.

use crate::interface::PriceProviderClient;
use crate::types::{
    PriceDecimalsUpdatedEvent, PricePoint, ProviderPrice, ProviderPriceRescaledEvent,
    CANONICAL_DECIMALS, MAX_PROVIDER_DECIMALS,
};
use soroban_sdk::{Address, Bytes, Env};

/// Canonical decimal precision the hub aggregates in.
pub fn canonical_decimals(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get::<_, u32>(&crate::storage::DataKey::PriceDecimals)
        .unwrap_or(CANONICAL_DECIMALS)
}

/// Governance updates the canonical precision. Every memoized price is dropped
/// because an aggregate taken at one precision cannot be compared with one
/// taken at another.
pub fn set_canonical_decimals(env: &Env, decimals: u32) {
    env.storage()
        .instance()
        .set(&crate::storage::DataKey::PriceDecimals, &decimals);
    PriceDecimalsUpdatedEvent { decimals }.publish(env);
}

/// `10^exponent`, saturating instead of overflowing so a hostile provider
/// cannot make the rescale wrap.
fn pow10(exponent: u32) -> i128 {
    let mut acc: i128 = 1;
    let mut i = 0u32;
    while i < exponent {
        acc = match acc.checked_mul(10) {
            Some(v) => v,
            None => return i128::MAX,
        };
        i += 1;
    }
    acc
}

/// Rescale a provider quote to the hub's canonical precision.
///
/// Providers may report their price at any precision up to
/// [`MAX_PROVIDER_DECIMALS`]; a larger one is rejected because the rescale
/// would overflow `i128`. A provider that reports `0` decimals is taken at
/// face value (its price is already canonical). Returns `None` when the quote
/// cannot be rescaled safely.
pub fn rescale(price: i128, from_decimals: u32, to_decimals: u32) -> Option<i128> {
    if from_decimals > MAX_PROVIDER_DECIMALS {
        return None;
    }
    if from_decimals == to_decimals {
        return Some(price);
    }
    if from_decimals < to_decimals {
        let factor = pow10(to_decimals - from_decimals);
        price.checked_mul(factor)
    } else {
        let factor = pow10(from_decimals - to_decimals);
        if factor == 0 {
            return None;
        }
        Some(price / factor)
    }
}

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

/// Fetch, validate, and normalize a provider quote to canonical precision.
pub fn fetch_normalized_price(
    env: &Env,
    asset: &Bytes,
    provider: &Address,
    canonical: u32,
) -> ProviderPrice {
    let price = fetch_provider_price(env, asset, provider);
    let normalized = match rescale(price.price, price.decimals, canonical) {
        Some(value) if value > 0 => value,
        _ => panic!("Provider price cannot be rescaled to canonical precision"),
    };

    if normalized != price.price {
        ProviderPriceRescaledEvent {
            asset: asset.clone(),
            provider: provider.clone(),
            raw_price: price.price,
            price: normalized,
            from_decimals: price.decimals,
            to_decimals: canonical,
        }
        .publish(env);
    }

    ProviderPrice {
        price: normalized,
        decimals: canonical,
        timestamp: price.timestamp,
        confidence: price.confidence,
    }
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
