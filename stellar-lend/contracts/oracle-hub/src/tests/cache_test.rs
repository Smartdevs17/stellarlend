//! Price cache tests (#1037).
//!
//! Covers the TTL window, the epoch invalidation that governance mutations
//! trigger, the freshness clamp that keeps a memoized price inside the
//! stalest source behind it, and the observable counters.

extern crate std;

use super::helpers::{allow_all, client, mk_asset, register_and_report, setup};
use crate::types::PriceSource;
use soroban_sdk::testutils::{Address as _, Ledger};

#[test]
fn test_cache_is_disabled_by_default() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    register_and_report(&te, &asset, &[100_000_000]);

    assert_eq!(client(&te).get_cache_ttl(), 0);

    let first = client(&te).get_price(&asset);
    assert!(!first.from_cache);
    assert!(client(&te).get_cached_price(&asset).is_none());
    assert_eq!(client(&te).get_cache_stats().misses, 1);
    assert_eq!(client(&te).get_cache_stats().writes, 0);
}

#[test]
fn test_cache_serves_repeat_reads() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    register_and_report(&te, &asset, &[100_000_000, 101_000_000]);

    allow_all(&te);
    client(&te).set_cache_ttl(&60);

    let first = client(&te).get_price(&asset);
    assert!(!first.from_cache);
    assert_eq!(first.source, PriceSource::Consensus);

    te.env.ledger().set_timestamp(30);
    let second = client(&te).get_price(&asset);
    assert!(second.from_cache);
    assert_eq!(second.source, PriceSource::Cached);
    assert_eq!(second.price, first.price);

    let stats = client(&te).get_cache_stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.writes, 1);
}

#[test]
fn test_cache_entry_expires_with_its_ttl() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    register_and_report(&te, &asset, &[100_000_000, 101_000_000]);

    allow_all(&te);
    client(&te).set_cache_ttl(&60);

    client(&te).get_price(&asset);
    te.env.ledger().set_timestamp(60);
    assert!(client(&te).get_price(&asset).from_cache);

    te.env.ledger().set_timestamp(61);
    assert!(!client(&te).get_price(&asset).from_cache);
    assert_eq!(client(&te).get_cache_stats().hits, 1);
    assert_eq!(client(&te).get_cache_stats().misses, 2);
}

#[test]
fn test_cache_ttl_is_clamped_to_the_stalest_source() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let primary = soroban_sdk::Address::generate(&te.env);
    let secondary = soroban_sdk::Address::generate(&te.env);

    // A 600 s cache TTL can never outlive a 100 s freshness budget.
    super::helpers::register_push_feed(
        &te,
        &asset,
        &primary,
        &crate::types::FeedPriority::Primary,
        100,
    );
    super::helpers::register_push_feed(
        &te,
        &asset,
        &secondary,
        &crate::types::FeedPriority::Secondary,
        100,
    );
    super::helpers::report(
        &te,
        &asset,
        &primary,
        100_000_000,
        &crate::types::FeedPriority::Primary,
    );
    super::helpers::report(
        &te,
        &asset,
        &secondary,
        101_000_000,
        &crate::types::FeedPriority::Secondary,
    );

    allow_all(&te);
    client(&te).set_cache_ttl(&600);

    client(&te).get_price(&asset);
    let cached = client(&te).get_cached_price(&asset).unwrap();
    assert_eq!(cached.ttl_seconds, 100);

    te.env.ledger().set_timestamp(99);
    assert!(client(&te).get_price(&asset).from_cache);

    // One second later every source is stale, so the read fails instead of
    // serving a memoized price the sources can no longer vouch for.
    te.env.ledger().set_timestamp(101);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset);
    }));
    assert!(result.is_err());
}

#[test]
fn test_feed_change_invalidates_the_cache() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracles = register_and_report(&te, &asset, &[100_000_000]);

    allow_all(&te);
    client(&te).set_cache_ttl(&600);
    client(&te).get_price(&asset);

    te.env.ledger().set_timestamp(10);
    super::helpers::report(
        &te,
        &asset,
        &oracles.get(0).unwrap(),
        105_000_000,
        &super::helpers::slot(0),
    );

    // The memoized price is still inside its TTL, so it is served.
    assert!(client(&te).get_price(&asset).from_cache);

    // Registering a new source bumps the epoch and drops the entry.
    allow_all(&te);
    let extra = soroban_sdk::Address::generate(&te.env);
    super::helpers::register_push_feed(
        &te,
        &asset,
        &extra,
        &crate::types::FeedPriority::Secondary,
        3600,
    );
    super::helpers::report(
        &te,
        &asset,
        &extra,
        105_000_000,
        &crate::types::FeedPriority::Secondary,
    );

    let refreshed = client(&te).get_price(&asset);
    assert!(!refreshed.from_cache);
    assert_eq!(refreshed.num_feeds, 2);
}

#[test]
fn test_strategy_change_invalidates_the_cache() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    register_and_report(&te, &asset, &[100_000_000, 120_000_000]);

    allow_all(&te);
    client(&te).set_cache_ttl(&600);
    assert_eq!(client(&te).get_price(&asset).price, 100_000_000);

    client(&te).set_aggregation_strategy(
        &Some(asset.clone()),
        &crate::types::AggregationStrategy::Weighted,
    );
    assert_eq!(client(&te).get_price(&asset).price, 110_000_000);
}

#[test]
fn test_freeze_invalidates_the_cache() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    register_and_report(&te, &asset, &[100_000_000]);

    allow_all(&te);
    client(&te).set_cache_ttl(&600);
    client(&te).get_price(&asset);

    client(&te).freeze();
    client(&te).unfreeze();

    // The freeze bumped the epoch twice, so the next read recomputes.
    assert!(!client(&te).get_price(&asset).from_cache);
}

#[test]
fn test_refresh_price_bypasses_the_cache() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracles = register_and_report(&te, &asset, &[100_000_000]);

    allow_all(&te);
    client(&te).set_cache_ttl(&600);
    client(&te).get_price(&asset);

    te.env.ledger().set_timestamp(5);
    super::helpers::report(
        &te,
        &asset,
        &oracles.get(0).unwrap(),
        110_000_000,
        &super::helpers::slot(0),
    );

    // A keeper can force the next read to hit the providers.
    let refreshed = client(&te).refresh_price(&asset);
    assert!(!refreshed.from_cache);
    assert_eq!(refreshed.price, 110_000_000);
    assert!(client(&te).get_price(&asset).from_cache);
}

#[test]
fn test_governance_can_drop_a_single_entry() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    register_and_report(&te, &asset, &[100_000_000]);

    allow_all(&te);
    client(&te).set_cache_ttl(&600);
    client(&te).get_price(&asset);
    assert!(client(&te).get_cached_price(&asset).is_some());

    client(&te).invalidate_cache(&asset);
    assert!(client(&te).get_cached_price(&asset).is_none());
}

#[test]
fn test_cache_ttl_is_bounded() {
    let te = setup();
    allow_all(&te);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).set_cache_ttl(&86_401);
    }));
    assert!(result.is_err());
    assert_eq!(client(&te).get_cache_ttl(), 0);
}

#[test]
fn test_pull_reads_are_counted() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider =
        super::helpers::register_mock_provider(&te, &asset, &crate::types::FeedPriority::Primary);
    super::helpers::MockProviderClient::new(&te.env, &provider).set_price(
        &asset,
        &100_000_000,
        &100,
    );

    allow_all(&te);
    client(&te).set_cache_ttl(&60);

    client(&te).get_price(&asset);
    client(&te).get_price(&asset);

    let stats = client(&te).get_cache_stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.pull_reads, 1);
    assert_eq!(stats.hits, 1);
}
