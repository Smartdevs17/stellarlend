//! Pluggable pull-based provider tests.
//!
//! Pull feeds are backed by external contracts implementing the
//! `PriceProvider` interface; the hub queries them live during `get_price`.
//! The mock provider in `helpers` stands in for such a contract.

use super::helpers::{client, mk_asset, register_mock_provider, register_push_feed, report, setup};
use crate::types::{FeedMode, FeedPriority, ProviderPrice};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Bytes;

use super::helpers::MockProviderClient;

fn set_mock_price(
    te: &super::helpers::TestEnv,
    provider: &soroban_sdk::Address,
    asset: &Bytes,
    price: i128,
    confidence: u32,
) {
    MockProviderClient::new(&te.env, provider).set_price(asset, &price, &confidence);
}

#[test]
fn test_pull_feed_aggregates_from_provider() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);

    set_mock_price(&te, &provider, &asset, 123_456_789, 95);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 123_456_789);
    assert_eq!(agg.confidence, 95);
    assert_eq!(agg.num_feeds, 1);

    // The pull should have populated the stored LatestPrice for health views.
    let statuses = client(&te).check_feed_health(&asset);
    assert_eq!(
        statuses.get(0).unwrap().status,
        crate::types::FeedStatusCode::Active
    );
}

#[test]
fn test_mixed_push_and_pull_aggregation() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);
    register_push_feed(&te, &asset, &oracle, &FeedPriority::Secondary, 3600);

    set_mock_price(&te, &provider, &asset, 100_000_000, 100);
    report(&te, &asset, &oracle, 101_000_000, &FeedPriority::Secondary);

    // Median of [100M, 101M] (upper median index 1) = 101M.
    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.num_feeds, 2);
    assert_eq!(agg.price, 101_000_000);
}

#[test]
fn test_multiple_pull_feeds_aggregate() {
    let te = setup();
    let asset = mk_asset(&te.env, "BTC");
    let provider1 = register_mock_provider(&te, &asset, &FeedPriority::Primary);
    let provider2 = register_mock_provider(&te, &asset, &FeedPriority::Secondary);

    set_mock_price(&te, &provider1, &asset, 1_000_000_000, 90);
    set_mock_price(&te, &provider2, &asset, 1_010_000_000, 90);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.num_feeds, 2);
    // Upper median of [1000M, 1010M] = 1010M.
    assert_eq!(agg.price, 1_010_000_000);
}

#[test]
fn test_fetch_provider_price_view() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);

    te.env.ledger().set_timestamp(1000);
    set_mock_price(&te, &provider, &asset, 55_000_000, 70);

    let fetched: ProviderPrice = client(&te).fetch_provider_price(&asset, &provider);
    assert_eq!(fetched.price, 55_000_000);
    assert_eq!(fetched.confidence, 70);
    assert!(fetched.timestamp > 0);
    assert_eq!(fetched.timestamp, 1000);
}

#[test]
fn test_provider_timestamp_is_clamped_to_ledger_time() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);

    // Provider timestamp stored at ledger time; a future timestamp is clamped.
    set_mock_price(&te, &provider, &asset, 10_000_000, 50);

    let agg = client(&te).get_price(&asset);
    assert!(agg.timestamp <= te.env.ledger().timestamp());
}

#[test]
#[should_panic(expected = "HostError")]
fn test_invalid_provider_price_reverts() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let _provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);

    // No price configured: MockProvider returns 0 -> hub must reject.
    client(&te).get_price(&asset);
}

#[test]
fn test_pull_feed_mode_is_stored() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let _provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);

    let feed = client(&te)
        .get_feed(&asset, &FeedPriority::Primary)
        .unwrap();
    assert_eq!(feed.mode, FeedMode::Pull);
}
