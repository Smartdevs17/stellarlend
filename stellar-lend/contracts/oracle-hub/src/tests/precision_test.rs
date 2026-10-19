//! Decimal normalization tests (#1036).
//!
//! Aggregating quotes that use different precisions is meaningless unless
//! they are rescaled first. Pull providers declare their own precision; the
//! hub rescales every quote to the canonical precision before it compares,
//! weights, or averages anything.

extern crate std;

use super::helpers::{
    allow_all, client, mk_asset, register_mock_provider, register_push_feed, report, setup,
    MockProviderClient,
};
use crate::provider::rescale;
use crate::types::{FeedPriority, ProviderPrice};
use soroban_sdk::testutils::Address as _;

#[test]
fn test_rescale_up_and_down() {
    // 18 decimals -> 8 decimals.
    assert_eq!(rescale(1_500_000_000_000_000_000, 18, 8), Some(150_000_000));
    // 6 decimals -> 8 decimals.
    assert_eq!(rescale(1_500_000, 6, 8), Some(150_000_000));
    // Identity.
    assert_eq!(rescale(42, 8, 8), Some(42));
    // Beyond the supported precision.
    assert_eq!(rescale(1, 19, 8), None);
    // Overflow must not wrap.
    assert_eq!(rescale(i128::MAX, 0, 18), None);
}

#[test]
fn test_canonical_precision_default_and_change() {
    let te = setup();
    assert_eq!(client(&te).get_price_decimals(), 8);

    allow_all(&te);
    client(&te).set_price_decimals(&6);
    assert_eq!(client(&te).get_price_decimals(), 6);
}

#[test]
fn test_precision_is_bounded() {
    let te = setup();
    allow_all(&te);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).set_price_decimals(&19);
    }));
    assert!(result.is_err());
    assert_eq!(client(&te).get_price_decimals(), 8);
}

#[test]
fn test_pull_quote_is_rescaled_before_aggregation() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);

    // The provider quotes 6 decimals while the hub aggregates in 8.
    MockProviderClient::new(&te.env, &provider).set_scaled_price(&asset, &1_500_000, &6, &100);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 150_000_000);
}

#[test]
fn test_mixed_precision_sources_agree_after_rescaling() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);
    let oracle = soroban_sdk::Address::generate(&te.env);

    // The same economic price quoted at 6 and at 8 decimals.
    MockProviderClient::new(&te.env, &provider).set_scaled_price(&asset, &1_000_000, &6, &100);
    register_push_feed(&te, &asset, &oracle, &FeedPriority::Secondary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Secondary);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.num_feeds, 2);
    assert_eq!(agg.price, 100_000_000);
    assert!(!agg.used_fallback);
    assert_eq!(agg.deviation_bps, 0);
}

#[test]
fn test_rescale_is_published() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);
    MockProviderClient::new(&te.env, &provider).set_scaled_price(&asset, &1_500_000, &6, &100);

    client(&te).get_price(&asset);
    assert!(super::helpers::has_event(
        &te,
        "provider_price_rescaled_event",
        &asset
    ));
}

#[test]
fn test_unrescalable_provider_price_reverts() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);
    // 25 decimals is outside the supported range.
    MockProviderClient::new(&te.env, &provider).set_scaled_price(&asset, &1_000, &25, &100);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset);
    }));
    assert!(result.is_err());
}

#[test]
fn test_raw_provider_view_is_unscaled() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);
    MockProviderClient::new(&te.env, &provider).set_scaled_price(&asset, &1_500_000, &6, &100);

    // The diagnostic view reports exactly what the provider said.
    let fetched: ProviderPrice = client(&te).fetch_provider_price(&asset, &provider);
    assert_eq!(fetched.price, 1_500_000);
    assert_eq!(fetched.decimals, 6);
}

#[test]
fn test_canonical_precision_change_invalidates_the_cache() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);
    MockProviderClient::new(&te.env, &provider).set_price(&asset, &100_000_000, &100);

    allow_all(&te);
    client(&te).set_cache_ttl(&600);
    assert_eq!(client(&te).get_price(&asset).price, 100_000_000);
    assert!(client(&te).get_price(&asset).from_cache);

    // Aggregates taken at a different precision are not comparable, so the
    // precision change drops the entry.
    client(&te).set_price_decimals(&6);
    assert!(!client(&te).get_price(&asset).from_cache);
    assert_eq!(client(&te).get_price(&asset).price, 1_000_000);
}
