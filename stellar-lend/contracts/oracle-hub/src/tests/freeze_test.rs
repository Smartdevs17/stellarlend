//! Emergency freeze controls: global halt, per-asset halt, and thawing.

extern crate std;

use super::helpers::{allow_all, client, mk_asset, register_push_feed, report, setup};
use crate::types::FeedPriority;
use soroban_sdk::testutils::Address as _;

#[test]
fn test_global_freeze_halts_all_prices() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);
    assert_eq!(client(&te).price(&asset), 100_000_000);

    allow_all(&te);
    client(&te).freeze();
    assert!(client(&te).is_frozen());

    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset)
    }));
    assert!(err.is_err());

    // Reporting is also blocked while the hub is halted.
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        report(&te, &asset, &oracle, 101_000_000, &FeedPriority::Primary);
    }));
    assert!(err.is_err());
}

#[test]
fn test_unfreeze_restores_service() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    allow_all(&te);
    client(&te).freeze();
    client(&te).unfreeze();
    assert!(!client(&te).is_frozen());
    assert_eq!(client(&te).price(&asset), 100_000_000);
}

#[test]
fn test_per_asset_freeze_isolated() {
    let te = setup();
    let asset_a = mk_asset(&te.env, "AAA");
    let asset_b = mk_asset(&te.env, "BBB");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset_a, &oracle, &FeedPriority::Primary, 3600);
    register_push_feed(&te, &asset_b, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset_a, &oracle, 10_000_000, &FeedPriority::Primary);
    report(&te, &asset_b, &oracle, 20_000_000, &FeedPriority::Primary);

    allow_all(&te);
    client(&te).freeze_asset(&asset_a);
    assert!(client(&te).is_asset_frozen(&asset_a));
    assert!(!client(&te).is_asset_frozen(&asset_b));

    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset_a)
    }));
    assert!(err.is_err());
    assert_eq!(client(&te).price(&asset_b), 20_000_000);

    client(&te).unfreeze_asset(&asset_a);
    assert!(!client(&te).is_asset_frozen(&asset_a));
    assert_eq!(client(&te).price(&asset_a), 10_000_000);
}

#[test]
fn test_global_freeze_takes_precedence_over_asset_thaw() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = soroban_sdk::Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    allow_all(&te);
    client(&te).freeze();
    client(&te).unfreeze_asset(&asset);
    // The global halt still applies: the asset is served as frozen.
    assert!(client(&te).is_asset_frozen(&asset));

    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).get_price(&asset)
    }));
    assert!(err.is_err());
}

#[test]
#[should_panic(expected = "HostError")]
fn test_freeze_requires_governance() {
    let te = setup();
    // No governance auth mocked -> default source account is not governance.
    client(&te).freeze();
}

#[test]
#[should_panic(expected = "HostError")]
fn test_unfreeze_requires_governance() {
    let te = setup();
    client(&te).unfreeze();
}
