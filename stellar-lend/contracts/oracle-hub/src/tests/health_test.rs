//! Oracle health monitoring tests: per-feed classification, consecutive
//! failure tracking, auto-degradation/circuit breakers, and self-healing.

extern crate std;

use super::helpers::{
    allow_all, client, mk_asset, register_mock_provider, register_push_feed, report, setup,
};
use crate::types::{FeedPriority, FeedStatusCode, OracleHealthStatus};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Bytes;

use super::helpers::MockProviderClient;

/// Repeat `monitor_oracle_health` `n` times, advancing the ledger each round
/// so each call counts as a distinct epoch.
fn hammer_monitor(te: &super::helpers::TestEnv, asset: &Bytes, n: u32) {
    for i in 0..n {
        te.env
            .ledger()
            .set_timestamp(te.env.ledger().timestamp() + 10 + i as u64);
        client(te).monitor_oracle_health(asset);
    }
}

#[test]
fn test_check_feed_health_active() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    let statuses = client(&te).check_feed_health(&asset);
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses.get(0).unwrap().status, FeedStatusCode::Active);
}

#[test]
fn test_check_feed_health_without_reports() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);

    // Hung feed -> Stale (missing report), never reported -> treated stale.
    te.env.ledger().set_timestamp(7200);
    let statuses = client(&te).check_feed_health(&asset);
    assert_eq!(statuses.get(0).unwrap().status, FeedStatusCode::Stale);
}

#[test]
fn test_check_feed_health_stale_and_disabled() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 100);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    te.env.ledger().set_timestamp(200);
    let statuses = client(&te).check_feed_health(&asset);
    assert_eq!(statuses.get(0).unwrap().status, FeedStatusCode::Stale);

    // Governed disable is reported as Disabled, never Stale.
    allow_all(&te);
    client(&te).disable_feed(&asset, &FeedPriority::Primary);
    let statuses = client(&te).check_feed_health(&asset);
    assert_eq!(statuses.get(0).unwrap().status, FeedStatusCode::Disabled);
}

#[test]
fn test_check_feed_health_asset_frozen() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    allow_all(&te);
    client(&te).freeze_asset(&asset);

    let statuses = client(&te).check_feed_health(&asset);
    assert_eq!(statuses.get(0).unwrap().status, FeedStatusCode::Frozen);
}

#[test]
fn test_healthy_asset_without_failures_stays_healthy() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    // No failed fetch was ever signalled -> no recorded failures, no trip.
    let health: OracleHealthStatus = client(&te).get_health(&asset);
    assert_eq!(health.consecutive_failures, 0);
    assert!(!health.circuit_breaker_open);
    assert!(!te_is_asset_frozen(&te, &asset));
    assert_eq!(client(&te).price(&asset), 100_000_000);
}

fn te_is_asset_frozen(te: &super::helpers::TestEnv, asset: &Bytes) -> bool {
    client(te).is_asset_frozen(asset)
}

#[test]
fn test_monitor_auto_degrades_asset_then_circuit_breaker() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    // 1 failure -> degradation, no freeze.
    te.env.ledger().set_timestamp(10);
    client(&te).monitor_oracle_health(&asset);
    assert!(!te_is_asset_frozen(&te, &asset));
    assert_eq!(client(&te).get_health(&asset).consecutive_failures, 1);

    // 2 more failures -> auto circuit breaker trips and freezes the asset.
    hammer_monitor(&te, &asset, 2);
    assert!(te_is_asset_frozen(&te, &asset));
    assert!(client(&te).get_health(&asset).circuit_breaker_open);

    // Frozen asset refuses to serve prices.
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset);
    }));
    assert!(err.is_err());
}

#[test]
fn test_monitor_auto_breaker_opens_only_for_failing_asset() {
    let te = setup();
    let asset_a = mk_asset(&te.env, "AAA");
    let asset_b = mk_asset(&te.env, "BBB");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset_a, &oracle, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset_b, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset_a, &oracle, 100_000_000, &FeedPriority::Primary);
    report(&te, &asset_b, &oracle, 100_000_000, &FeedPriority::Primary);

    hammer_monitor(&te, &asset_a, 4);
    assert!(te_is_asset_frozen(&te, &asset_a));
    assert!(!te_is_asset_frozen(&te, &asset_b));

    // Healthy sibling still serves a live price.
    assert_eq!(client(&te).price(&asset_b), 100_000_000);
}

#[test]
fn test_record_oracle_success_resets_failures() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    hammer_monitor(&te, &asset, 2);
    assert_eq!(client(&te).get_health(&asset).consecutive_failures, 2);

    allow_all(&te);
    client(&te).record_oracle_success(&asset);
    assert_eq!(client(&te).get_health(&asset).consecutive_failures, 0);
    assert!(!te_is_asset_frozen(&te, &asset));
}

#[test]
fn test_healthy_pull_recovers_after_breaker_cooldown() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);
    MockProviderClient::new(&te.env, &provider).set_price(&asset, &100_000_000, &100);

    hammer_monitor(&te, &asset, 4);
    assert!(te_is_asset_frozen(&te, &asset));

    // Cooldown expires; hub may resume serving and records success.
    te.env
        .ledger()
        .set_timestamp(te.env.ledger().timestamp() + 600 + 1);
    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 100_000_000);

    let health = client(&te).get_health(&asset);
    assert!(!health.circuit_breaker_open);
    assert_eq!(health.consecutive_failures, 0);
}

#[test]
fn test_monitor_with_pull_feed_counts_pull_failure() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    // Registered pull feed whose provider returns 0 (invalid).
    let provider = register_mock_provider(&te, &asset, &FeedPriority::Primary);

    // First monitor classifies the pull failure; repeated monitors trip breaker.
    client(&te).monitor_oracle_health(&asset);
    assert!(!te_is_asset_frozen(&te, &asset));

    hammer_monitor(&te, &asset, 3);
    assert!(te_is_asset_frozen(&te, &asset));

    // Fix the provider; after cooldown the asset serves prices again.
    MockProviderClient::new(&te.env, &provider).set_price(&asset, &99_000_000, &80);
    te.env
        .ledger()
        .set_timestamp(te.env.ledger().timestamp() + 600 + 1);
    let agg = client(&te).get_price(&asset);
    assert_eq!(agg.price, 99_000_000);
}

#[test]
fn test_health_views_without_feed() {
    let te = setup();
    let asset = mk_asset(&te.env, "NONE");
    let statuses = client(&te).check_feed_health(&asset);
    assert_eq!(statuses.len(), 0);
    let health = client(&te).get_health(&asset);
    assert_eq!(health.consecutive_failures, 0);
    assert!(!health.circuit_breaker_open);
}
