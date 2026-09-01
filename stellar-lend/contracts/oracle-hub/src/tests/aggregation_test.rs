//! Aggregation strategy tests: median, confidence-weighted, outlier handling,
//! and per-asset strategy overrides.

extern crate std;

use super::helpers::{allow_all, client, mk_asset, register_push_feed, report, setup};
use crate::types::{AggregatedPrice, AggregationStrategy, FeedMode, FeedPriority};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, IntoVal};

#[test]
fn test_single_feed_passthrough() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    let agg: AggregatedPrice = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);
    assert_eq!(agg.confidence, 100);
    assert_eq!(agg.num_feeds, 1);
    assert_eq!(agg.num_active_feeds, 1);
    assert_eq!(agg.strategy, AggregationStrategy::Median);
}

#[test]
fn test_median_aggregation_across_sources() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let o1 = Address::generate(&te.env);
    let o2 = Address::generate(&te.env);
    let o3 = Address::generate(&te.env);

    register_push_feed(&te, &asset, &o1, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset, &o2, &FeedPriority::Secondary, 3600);
    register_push_feed(&te, &asset, &o3, &FeedPriority::Fallback, 3600);

    report(&te, &asset, &o1, 98_000_000, &FeedPriority::Primary);
    report(&te, &asset, &o2, 100_000_000, &FeedPriority::Secondary);
    report(&te, &asset, &o3, 102_000_000, &FeedPriority::Fallback);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);
    assert_eq!(agg.num_feeds, 3);
    assert_eq!(agg.confidence, 100);
}

#[test]
fn test_outlier_rejection() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let o1 = Address::generate(&te.env);
    let o2 = Address::generate(&te.env);
    let o3 = Address::generate(&te.env);

    register_push_feed(&te, &asset, &o1, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset, &o2, &FeedPriority::Secondary, 3600);
    register_push_feed(&te, &asset, &o3, &FeedPriority::Fallback, 3600);

    // 200M deviates 100 % from the 100M median -> rejected.
    report(&te, &asset, &o1, 100_000_000, &FeedPriority::Primary);
    report(&te, &asset, &o2, 100_000_000, &FeedPriority::Secondary);
    report(&te, &asset, &o3, 200_000_000, &FeedPriority::Fallback);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);
}

#[test]
fn test_weighted_strategy_distinguishes_from_median() {
    let te = setup();
    let asset = mk_asset(&te.env, "ETH");
    let o1 = Address::generate(&te.env);
    let o2 = Address::generate(&te.env);

    register_push_feed(&te, &asset, &o1, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset, &o2, &FeedPriority::Secondary, 3600);

    report(&te, &asset, &o1, 100_000_000, &FeedPriority::Primary);
    report(&te, &asset, &o2, 120_000_000, &FeedPriority::Secondary);

    // Median -> upper median of [100M, 120M] = 120M.
    let median_agg = client(&te).get_price(&asset);
    assert_eq!(median_agg.price, 120_000_000);

    // Weighted (equal weights) -> mean = 110M.
    allow_all(&te);
    client(&te).set_aggregation_strategy(&None, &AggregationStrategy::Weighted);
    let weighted_agg = client(&te).get_price(&asset);
    assert_eq!(weighted_agg.price, 110_000_000);
    assert_eq!(weighted_agg.strategy, AggregationStrategy::Weighted);
}

#[test]
fn test_weighted_strategy_respects_feed_weights() {
    let te = setup();
    let asset = mk_asset(&te.env, "ETH");
    let o1 = Address::generate(&te.env);
    let o2 = Address::generate(&te.env);

    allow_all(&te);
    client(&te).register_feed(
        &asset,
        &o1,
        &FeedPriority::Primary,
        &3600,
        &FeedMode::Push,
        &10000,
    );
    client(&te).register_feed(
        &asset,
        &o2,
        &FeedPriority::Secondary,
        &3600,
        &FeedMode::Push,
        &30000,
    );

    report(&te, &asset, &o1, 100_000_000, &FeedPriority::Primary);
    report(&te, &asset, &o2, 120_000_000, &FeedPriority::Secondary);

    allow_all(&te);
    client(&te).set_aggregation_strategy(&None, &AggregationStrategy::Weighted);

    // Effective weights @ conf 100: 100M*(1*100) and 120M*(3*100) -> (100 + 360)/4 = 115M.
    let agg = client(&te).get_price(&asset);
    let expected = (100_000_000 + 120_000_000i128 * 3) / 4;
    assert_eq!(agg.price, expected);
    assert_eq!(expected, 115_000_000);
}

#[test]
fn test_weighted_strategy_downweights_low_confidence() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let o1 = Address::generate(&te.env);
    let o2 = Address::generate(&te.env);

    register_push_feed(&te, &asset, &o1, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset, &o2, &FeedPriority::Secondary, 3600);

    // Low-confidence (10) vs high-confidence (100) same price -> mean still 100M.
    report(&te, &asset, &o1, 100_000_000, &FeedPriority::Primary);
    te.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &o2,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &te.contract_id,
            fn_name: "report_price",
            args: (&asset, &100_000_000i128, &10u32, &FeedPriority::Secondary).into_val(&te.env),
            sub_invokes: &[],
        },
    }]);
    client(&te).report_price(&asset, &100_000_000, &10, &FeedPriority::Secondary);

    allow_all(&te);
    client(&te).set_aggregation_strategy(&None, &AggregationStrategy::Weighted);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);
}

#[test]
fn test_per_asset_strategy_override() {
    let te = setup();
    let asset_a = mk_asset(&te.env, "AAA");
    let asset_b = mk_asset(&te.env, "BBB");
    let oa = Address::generate(&te.env);
    let ob = Address::generate(&te.env);

    register_push_feed(&te, &asset_a, &oa, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset_b, &ob, &FeedPriority::Primary, 3600);

    allow_all(&te);
    client(&te).set_aggregation_strategy(&Some(asset_a.clone()), &AggregationStrategy::Weighted);

    assert_eq!(
        client(&te).get_aggregation_strategy(&asset_a),
        AggregationStrategy::Weighted
    );
    assert_eq!(
        client(&te).get_aggregation_strategy(&asset_b),
        AggregationStrategy::Median
    );

    // Per-asset override must not leak to the default.
    client(&te).set_aggregation_strategy(&None, &AggregationStrategy::Weighted);
    assert_eq!(
        client(&te).get_aggregation_strategy(&asset_b),
        AggregationStrategy::Weighted
    );
    // Override still wins over the new default for asset_a.
    assert_eq!(
        client(&te).get_aggregation_strategy(&asset_a),
        AggregationStrategy::Weighted
    );
}

#[test]
fn test_fallback_on_stale_primary() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let primary = Address::generate(&te.env);
    let secondary = Address::generate(&te.env);

    register_push_feed(&te, &asset, &primary, &FeedPriority::Primary, 100);
    register_push_feed(&te, &asset, &secondary, &FeedPriority::Secondary, 1000);

    report(&te, &asset, &primary, 100_000_000, &FeedPriority::Primary);
    report(
        &te,
        &asset,
        &secondary,
        101_000_000,
        &FeedPriority::Secondary,
    );

    // Primary goes stale by t=500 (stale threshold 100).
    te.env.ledger().set_timestamp(500);

    let statuses = client(&te).check_feed_health(&asset);
    assert_eq!(
        statuses.get(0).unwrap().status,
        crate::types::FeedStatusCode::Stale
    );

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 101_000_000);
    assert_eq!(agg.num_feeds, 1);

    // The stale primary feed is now auto-disabled.
    let feed = client(&te)
        .get_feed(&asset, &FeedPriority::Primary)
        .unwrap();
    assert!(!feed.enabled);
}

#[test]
fn test_all_feeds_stale_reverts() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 100);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    te.env.ledger().set_timestamp(500);

    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset);
    }));
    assert!(err.is_err());
}
