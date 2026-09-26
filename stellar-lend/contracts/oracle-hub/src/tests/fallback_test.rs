//! Deviation-checked multi-source fallback tests (#1035).
//!
//! Covers demotion of a manipulated leading source, the quorum rule that
//! keeps the hub from publishing a thinner consensus than governance
//! configured, and the on-chain evidence every demotion publishes.

extern crate std;

use super::helpers::{
    allow_all, client, has_event, mk_asset, register_and_report, register_push_feed, report, setup,
    slot,
};
use crate::types::{AggregationStrategy, FeedPriority, PriceSource};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Address;

#[test]
fn test_manipulated_primary_is_demoted() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");

    // Primary reports 10x the honest consensus.
    register_and_report(&te, &asset, &[1_000_000_000, 100_000_000, 101_000_000]);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);
    assert!(agg.used_fallback);
    assert_eq!(agg.rejected_sources, 1);
    assert_eq!(agg.num_feeds, 2);
    assert_eq!(agg.num_active_feeds, 3);
    assert_eq!(agg.source, PriceSource::Consensus);
    // 1_000M against the 100M reference of the other two sources is 900 % off.
    assert_eq!(agg.deviation_bps, 90_000);
}

#[test]
fn test_demotion_publishes_evidence() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");

    register_and_report(&te, &asset, &[900_000_000, 100_000_000, 100_000_000]);
    client(&te).get_price(&asset);

    assert!(has_event(&te, "deviation_rejected_event", &asset));
    assert!(has_event(&te, "fallback_activated_event", &asset));
}

#[test]
fn test_honest_primary_is_not_demoted() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");

    // 1 % spread across all three sources: inside the default 20 % band.
    register_and_report(&te, &asset, &[100_000_000, 101_000_000, 99_000_000]);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);
    assert!(!agg.used_fallback);
    assert_eq!(agg.rejected_sources, 0);
    assert_eq!(agg.num_feeds, 3);
    assert_eq!(agg.deviation_bps, 100);
    assert!(!has_event(&te, "fallback_activated_event", &asset));
}

#[test]
fn test_deviation_check_is_skipped_without_quorum() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");

    // Two sources only: there is no independent reference, so the leading
    // source is never demoted here.
    register_and_report(&te, &asset, &[100_000_000, 105_000_000]);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.num_feeds, 2);
    assert!(!agg.used_fallback);
    assert!(!has_event(&te, "fallback_activated_event", &asset));
}

#[test]
fn test_sole_source_is_flagged_as_fallback() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);
    assert_eq!(agg.source, PriceSource::Sole);
    assert!(agg.used_fallback);
    assert_eq!(agg.deviation_bps, 0);
}

#[test]
fn test_governance_can_widen_the_deviation_band() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");

    // 30 % above the others: outside the 20 % default, inside a 40 % band.
    register_and_report(&te, &asset, &[130_000_000, 100_000_000, 100_000_000]);

    allow_all(&te);
    client(&te).set_aggregation_params(&asset, &4_000, &2);

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.num_feeds, 3);
    assert!(!agg.used_fallback);
    assert_eq!(agg.rejected_sources, 0);
    assert_eq!(agg.price, 100_000_000);
}

#[test]
fn test_deviation_check_can_be_disabled() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");

    register_and_report(&te, &asset, &[500_000_000, 100_000_000, 100_000_000]);

    allow_all(&te);
    // Band of 0 disables the deviation check; the median strategy still
    // rejects the outlier during aggregation.
    client(&te).set_aggregation_params(&asset, &0, &2);

    let agg = client(&te).get_price(&asset);
    assert!(!has_event(&te, "fallback_activated_event", &asset));
    assert_eq!(agg.rejected_sources, 1);
    assert_eq!(agg.price, 100_000_000);
}

#[test]
fn test_demotion_below_min_sources_fails_closed() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");

    register_and_report(&te, &asset, &[900_000_000, 100_000_000, 100_000_000]);

    allow_all(&te);
    // Demoting the primary would leave a single source; governance demands
    // three, so the read must fail instead of publishing a thin consensus.
    client(&te).set_aggregation_params(&asset, &2_000, &3);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset);
    }));
    assert!(result.is_err());
}

#[test]
fn test_min_sources_is_a_floor_on_the_consensus() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let primary = Address::generate(&te.env);
    let secondary = Address::generate(&te.env);

    register_push_feed(&te, &asset, &primary, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset, &secondary, &FeedPriority::Secondary, 3600);
    report(&te, &asset, &primary, 100_000_000, &FeedPriority::Primary);
    report(
        &te,
        &asset,
        &secondary,
        100_000_000,
        &FeedPriority::Secondary,
    );

    allow_all(&te);
    client(&te).set_aggregation_params(&asset, &2_000, &3);

    // Two agreeing sources are below the configured floor of three.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset);
    }));
    assert!(result.is_err());

    // A third source satisfies it.
    let third = Address::generate(&te.env);
    register_push_feed(&te, &asset, &third, &slot(2), 3600);
    report(&te, &asset, &third, 100_000_000, &slot(2));

    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);
    assert_eq!(agg.num_feeds, 3);
}

#[test]
fn test_demoted_source_recovers_once_it_reports_again() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracles = register_and_report(&te, &asset, &[900_000_000, 100_000_000, 100_000_000]);

    assert!(client(&te).get_price(&asset).used_fallback);

    // The manipulated source corrects itself: the deviation check no longer
    // fires and the primary is trusted again.
    te.env.ledger().set_timestamp(10);
    report(&te, &asset, &oracles.get(0).unwrap(), 100_000_000, &slot(0));

    let agg = client(&te).get_price(&asset);
    assert!(!agg.used_fallback);
    assert_eq!(agg.rejected_sources, 0);
    assert_eq!(agg.num_feeds, 3);
}

#[test]
fn test_stale_leader_falls_back_to_next_slot() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let primary = Address::generate(&te.env);
    let secondary = Address::generate(&te.env);

    register_push_feed(&te, &asset, &primary, &FeedPriority::Primary, 100);
    register_push_feed(&te, &asset, &secondary, &FeedPriority::Secondary, 3600);
    report(&te, &asset, &primary, 100_000_000, &FeedPriority::Primary);
    report(
        &te,
        &asset,
        &secondary,
        101_000_000,
        &FeedPriority::Secondary,
    );

    te.env.ledger().set_timestamp(500);
    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 101_000_000);
    // The stale leader was auto-disabled before selection, so the sole
    // remaining source is reported as such.
    assert_eq!(agg.source, PriceSource::Sole);
    assert!(agg.used_fallback);
    assert!(
        !client(&te)
            .get_feed(&asset, &FeedPriority::Primary)
            .unwrap()
            .enabled
    );
}

#[test]
fn test_trimmed_mean_ignores_both_tails() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");

    // A cheap source and an expensive one bracket the honest middle pair.
    register_and_report(
        &te,
        &asset,
        &[100_000_000, 101_000_000, 102_000_000, 103_000_000],
    );

    allow_all(&te);
    // Wide enough that no source is demoted or filtered out.
    client(&te).set_aggregation_params(&asset, &5_000, &2);
    client(&te).set_aggregation_strategy(&Some(asset.clone()), &AggregationStrategy::TrimmedMean);

    // Trimming 100M and 103M leaves [101M, 102M] to average.
    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 101_500_000);
    assert_eq!(agg.num_feeds, 4);
    assert_eq!(agg.strategy, AggregationStrategy::TrimmedMean);
    assert!(!agg.used_fallback);
}
