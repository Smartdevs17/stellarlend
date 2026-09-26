//! Read-cost tests for the price path (#1037).
//!
//! These tests turn the gas story into an assertion: a memoized read must be
//! materially cheaper than a recomputed one, the marginal cost of an extra
//! source must stay small, and the cached path must stay inside a fixed
//! instruction budget so a future change to the read path cannot quietly
//! double its cost.

use super::helpers::{allow_all, client, mk_asset, register_and_report, setup};
use soroban_sdk::testutils::Ledger;
use soroban_sdk::Env;

/// Instruction budget for a fully cached `get_price`. The measured value is
/// an order of magnitude below this; the ceiling exists to catch a
/// regression that turns the cache back into a full recompute.
const CACHED_READ_INSTRUCTION_BUDGET: i64 = 500_000;

fn instructions(env: &Env) -> i64 {
    env.cost_estimate().resources().instructions
}

#[test]
fn test_cached_read_is_cheaper_than_a_recompute() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    register_and_report(&te, &asset, &[100_000_000, 101_000_000, 99_000_000]);

    allow_all(&te);
    client(&te).set_cache_ttl(&600);

    // First read: full aggregation over three push feeds.
    client(&te).get_price(&asset);
    let recompute_cost = instructions(&te.env);

    te.env.ledger().set_timestamp(10);
    assert!(client(&te).get_price(&asset).from_cache);
    let cached_cost = instructions(&te.env);

    assert!(
        cached_cost < recompute_cost,
        "cached read ({cached_cost}) must be cheaper than a recompute ({recompute_cost})"
    );
    // The saving has to be material, not a rounding difference.
    assert!(
        cached_cost * 5 < recompute_cost * 4,
        "cached read ({cached_cost}) must cost at most 80% of a recompute ({recompute_cost})"
    );
    assert!(
        cached_cost < CACHED_READ_INSTRUCTION_BUDGET,
        "cached read ({cached_cost}) exceeded the {CACHED_READ_INSTRUCTION_BUDGET} instruction budget"
    );
}

#[test]
fn test_cached_pull_read_skips_the_provider_call() {
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
    client(&te).set_cache_ttl(&600);

    client(&te).get_price(&asset);
    let pull_cost = instructions(&te.env);

    te.env.ledger().set_timestamp(10);
    client(&te).get_price(&asset);
    let cached_cost = instructions(&te.env);

    assert!(
        cached_cost < pull_cost,
        "a cached pull read ({cached_cost}) must be cheaper than calling the provider ({pull_cost})"
    );
}

#[test]
fn test_extra_sources_cost_boundedly() {
    let one = setup();
    let asset_one = mk_asset(&one.env, "XLM");
    register_and_report(&one, &asset_one, &[100_000_000, 101_000_000]);
    client(&one).get_price(&asset_one);
    let two_source_cost = instructions(&one.env);

    let three = setup();
    let asset_three = mk_asset(&three.env, "XLM");
    register_and_report(
        &three,
        &asset_three,
        &[
            100_000_000,
            101_000_000,
            99_000_000,
            100_500_000,
            99_500_000,
        ],
    );
    client(&three).get_price(&asset_three);
    let five_source_cost = instructions(&three.env);

    // Three extra slots may cost more, but nowhere near three times as much:
    // the read is dominated by fixed per-invocation overhead.
    assert!(
        five_source_cost * 2 < two_source_cost * 5,
        "five sources ({five_source_cost}) must stay under 2.5x the two-source cost ({two_source_cost})"
    );
}

#[test]
fn test_disabled_cache_never_reports_a_hit() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    register_and_report(&te, &asset, &[100_000_000, 101_000_000]);

    client(&te).get_price(&asset);
    te.env.ledger().set_timestamp(5);
    let second = client(&te).get_price(&asset);

    assert!(!second.from_cache);
    assert_eq!(client(&te).get_cache_stats().hits, 0);
}
