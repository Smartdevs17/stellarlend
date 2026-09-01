//! Feed registration, configuration, views, and lifecycle tests.

extern crate std;

use super::helpers::{allow_all, client, mk_asset, register_push_feed, report, setup};
use crate::types::{AggregatedPrice, FeedMode, FeedPriority, VERSION};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Address;

#[test]
fn test_initialize() {
    let te = setup();
    assert_eq!(client(&te).version(), VERSION);
    assert!(!client(&te).is_frozen());
    assert!(!client(&te).is_asset_frozen(&mk_asset(&te.env, "XLM")));
}

#[test]
#[should_panic(expected = "HostError")]
fn test_double_initialize_reverts() {
    let te = setup();
    // Re-initializing the already-initialized hub must fail.
    client(&te).initialize(&te.governance, &te.admin);
}

#[test]
fn test_register_feed_defaults() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);

    allow_all(&te);
    client(&te).register_feed(
        &asset,
        &oracle,
        &FeedPriority::Primary,
        &0,
        &FeedMode::Push,
        &0,
    );

    let feed = client(&te)
        .get_feed(&asset, &FeedPriority::Primary)
        .expect("feed registered");
    assert!(feed.enabled);
    assert_eq!(feed.mode, FeedMode::Push);
    assert_eq!(feed.stale_threshold_seconds, 3600);
    assert_eq!(feed.weight_bps, 10000);
    assert_eq!(feed.registered_at, te.env.ledger().timestamp());
    assert_eq!(feed.oracle_address, oracle);
    assert_eq!(feed.asset, asset);
}

#[test]
fn test_register_feed_custom_staleness_and_weight() {
    let te = setup();
    let asset = mk_asset(&te.env, "ETH");
    let oracle = Address::generate(&te.env);

    allow_all(&te);
    client(&te).register_feed(
        &asset,
        &oracle,
        &FeedPriority::Primary,
        &120,
        &FeedMode::Push,
        &5000,
    );

    let feed = client(&te)
        .get_feed(&asset, &FeedPriority::Primary)
        .expect("feed registered");
    assert_eq!(feed.stale_threshold_seconds, 120);
    assert_eq!(feed.weight_bps, 5000);
    assert_eq!(feed.priority, FeedPriority::Primary);
}

#[test]
fn test_register_feed_pull_mode() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = Address::generate(&te.env);

    allow_all(&te);
    client(&te).register_feed(
        &asset,
        &provider,
        &FeedPriority::Secondary,
        &3600,
        &FeedMode::Pull,
        &10000,
    );

    let feed = client(&te)
        .get_feed(&asset, &FeedPriority::Secondary)
        .expect("feed registered");
    assert_eq!(feed.mode, FeedMode::Pull);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_register_feed_rejects_hub_self_oracle() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    allow_all(&te);
    client(&te).register_feed(
        &asset,
        &te.contract_id,
        &FeedPriority::Primary,
        &3600,
        &FeedMode::Push,
        &10000,
    );
}

#[test]
#[should_panic(expected = "HostError")]
fn test_register_feed_requires_governance() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    // No governance auth mocked -> the default source account is not governance.
    client(&te).register_feed(
        &asset,
        &Address::generate(&te.env),
        &FeedPriority::Primary,
        &3600,
        &FeedMode::Push,
        &10000,
    );
}

#[test]
fn test_update_feed() {
    let te = setup();
    let asset = mk_asset(&te.env, "BTC");
    let oracle = Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);

    allow_all(&te);
    client(&te).update_feed(
        &asset,
        &FeedPriority::Primary,
        &7200,
        &FeedMode::Pull,
        &20000,
    );

    let feed = client(&te)
        .get_feed(&asset, &FeedPriority::Primary)
        .unwrap();
    assert_eq!(feed.stale_threshold_seconds, 7200);
    assert_eq!(feed.mode, FeedMode::Pull);
    assert_eq!(feed.weight_bps, 20000);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_update_missing_feed_reverts() {
    let te = setup();
    let asset = mk_asset(&te.env, "UNKNOWN");
    allow_all(&te);
    client(&te).update_feed(
        &asset,
        &FeedPriority::Primary,
        &3600,
        &FeedMode::Push,
        &10000,
    );
}

#[test]
fn test_disable_and_enable_feed() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    allow_all(&te);
    client(&te).disable_feed(&asset, &FeedPriority::Primary);
    let feed = client(&te)
        .get_feed(&asset, &FeedPriority::Primary)
        .unwrap();
    assert!(!feed.enabled);

    // A disabled feed must not participate in aggregation.
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset);
    }));
    assert!(err.is_err());

    client(&te).enable_feed(&asset, &FeedPriority::Primary);
    let feed = client(&te)
        .get_feed(&asset, &FeedPriority::Primary)
        .unwrap();
    assert!(feed.enabled);
}

#[test]
fn test_multiple_assets_are_independent() {
    let te = setup();
    let asset1 = mk_asset(&te.env, "XLM");
    let asset2 = mk_asset(&te.env, "BTC");
    let oracle = Address::generate(&te.env);

    register_push_feed(&te, &asset1, &oracle, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset2, &oracle, &FeedPriority::Primary, 3600);

    report(&te, &asset1, &oracle, 50_000_000, &FeedPriority::Primary);
    report(&te, &asset2, &oracle, 1_000_000_000, &FeedPriority::Primary);

    let agg1: AggregatedPrice = client(&te).get_price(&asset1);
    let agg2: AggregatedPrice = client(&te).get_price(&asset2);
    assert_eq!(agg1.price, 50_000_000);
    assert_eq!(agg2.price, 1_000_000_000);
    assert_eq!(client(&te).price(&asset1), 50_000_000);
    assert_eq!(client(&te).price(&asset2), 1_000_000_000);
}

#[test]
fn test_feed_view_returns_missing() {
    let te = setup();
    let asset = mk_asset(&te.env, "AAA");
    assert!(client(&te)
        .get_feed(&asset, &FeedPriority::Primary)
        .is_none());
}

#[test]
fn test_no_feeds_returns_error() {
    let te = setup();
    let asset = mk_asset(&te.env, "UNKNOWN");
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset);
    }));
    assert!(err.is_err());
}
