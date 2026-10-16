//! Per-update price-jump bound tests.
//!
//! Covers the default bound, the boundary itself, rejection in both
//! directions, per-asset and hub-wide configuration, disabling the check,
//! governance gating, and the `rebase_price` recovery path.

extern crate std;

use super::helpers::{
    allow_all, client, has_event, mk_asset, register_push_feed, report, setup, TestEnv,
};
use crate::jump::{exceeds_bound, DEFAULT_MAX_PRICE_JUMP_BPS};
use crate::types::FeedPriority;
use crate::OracleHubError;
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, Bytes, IntoVal};

const PRICE: i128 = 100_000_000;

/// One push feed on the primary slot, seeded with `PRICE`.
fn seeded(te: &TestEnv, asset: &Bytes) -> Address {
    let oracle = Address::generate(&te.env);
    register_push_feed(te, asset, &oracle, &FeedPriority::Primary, 3600);
    report(te, asset, &oracle, PRICE, &FeedPriority::Primary);
    oracle
}

/// The host-level error a panicking entry point surfaces for `e`.
fn err(e: OracleHubError) -> soroban_sdk::Error {
    e.into()
}

/// Report as `oracle` and return the contract error, if any.
fn try_report(
    te: &TestEnv,
    asset: &Bytes,
    oracle: &Address,
    price: i128,
) -> Option<soroban_sdk::Error> {
    te.env.mock_auths(&[MockAuth {
        address: oracle,
        invoke: &MockAuthInvoke {
            contract: &te.contract_id,
            fn_name: "report_price",
            args: (asset, &price, &100u32, &FeedPriority::Primary).into_val(&te.env),
            sub_invokes: &[],
        },
    }]);
    match client(te).try_report_price(asset, &price, &100, &FeedPriority::Primary) {
        Ok(_) => None,
        Err(Ok(err)) => Some(err),
        Err(Err(err)) => panic!("unexpected invoke error: {:?}", err),
    }
}

fn latest_price(te: &TestEnv, asset: &Bytes) -> i128 {
    allow_all(te);
    client(te).refresh_price(asset).price
}

// ── Default bound and enforcement ───────────────────────────────────────────

#[test]
fn test_default_bound_is_twenty_percent() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    assert_eq!(DEFAULT_MAX_PRICE_JUMP_BPS, 2_000);
    assert_eq!(client(&te).get_max_price_jump(&asset), 2_000);
}

#[test]
fn test_first_report_is_unbounded() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);
    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);

    // Nothing to measure against yet, so any positive price seeds the slot.
    assert_eq!(try_report(&te, &asset, &oracle, 123_456_789_000), None);
    assert_eq!(latest_price(&te, &asset), 123_456_789_000);
}

#[test]
fn test_move_at_the_bound_is_accepted() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = seeded(&te, &asset);

    // Exactly +20 %, then exactly -20 % of the new reference.
    assert_eq!(try_report(&te, &asset, &oracle, 120_000_000), None);
    assert_eq!(try_report(&te, &asset, &oracle, 96_000_000), None);
    assert_eq!(latest_price(&te, &asset), 96_000_000);
}

#[test]
fn test_upward_jump_beyond_bound_is_rejected() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = seeded(&te, &asset);

    assert_eq!(
        try_report(&te, &asset, &oracle, 120_000_001),
        Some(err(OracleHubError::PriceJumpExceeded))
    );
    // Single-block 10x manipulation.
    assert_eq!(
        try_report(&te, &asset, &oracle, 10 * PRICE),
        Some(err(OracleHubError::PriceJumpExceeded))
    );
    assert_eq!(
        latest_price(&te, &asset),
        PRICE,
        "slot keeps its last price"
    );
}

#[test]
fn test_downward_jump_beyond_bound_is_rejected() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = seeded(&te, &asset);

    assert_eq!(
        try_report(&te, &asset, &oracle, 79_999_999),
        Some(err(OracleHubError::PriceJumpExceeded))
    );
    assert_eq!(
        try_report(&te, &asset, &oracle, 1),
        Some(err(OracleHubError::PriceJumpExceeded))
    );
    assert_eq!(latest_price(&te, &asset), PRICE);
}

#[test]
fn test_rejected_report_does_not_refresh_the_slot() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = seeded(&te, &asset);

    te.env.ledger().set_timestamp(500);
    assert!(try_report(&te, &asset, &oracle, 5 * PRICE).is_some());

    allow_all(&te);
    let point = client(&te).refresh_price(&asset);
    assert_eq!(
        point.timestamp, 0,
        "rejected report must not bump the timestamp"
    );
}

#[test]
fn test_bound_is_per_slot() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let primary = seeded(&te, &asset);
    let secondary = Address::generate(&te.env);
    register_push_feed(&te, &asset, &secondary, &FeedPriority::Secondary, 3600);

    // The secondary's first report is measured against nothing, not against
    // the primary's price.
    report(&te, &asset, &secondary, 3 * PRICE, &FeedPriority::Secondary);
    assert_eq!(try_report(&te, &asset, &primary, 110_000_000), None);
}

// ── Configuration ───────────────────────────────────────────────────────────

#[test]
fn test_hub_default_is_configurable() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = seeded(&te, &asset);

    allow_all(&te);
    client(&te).set_max_price_jump(&None, &500);
    assert!(has_event_named(&te, "max_price_jump_updated_event"));
    assert_eq!(client(&te).get_max_price_jump(&asset), 500);

    assert_eq!(
        try_report(&te, &asset, &oracle, 106_000_000),
        Some(err(OracleHubError::PriceJumpExceeded))
    );
    assert_eq!(try_report(&te, &asset, &oracle, 105_000_000), None);
}

#[test]
fn test_asset_override_takes_precedence() {
    let te = setup();
    let tight = mk_asset(&te.env, "USDC");
    let loose = mk_asset(&te.env, "XLM");
    let tight_oracle = seeded(&te, &tight);
    let loose_oracle = seeded(&te, &loose);

    allow_all(&te);
    client(&te).set_max_price_jump(&None, &5_000);
    client(&te).set_max_price_jump(&Some(tight.clone()), &100);
    assert_eq!(client(&te).get_max_price_jump(&tight), 100);
    assert_eq!(client(&te).get_max_price_jump(&loose), 5_000);

    assert_eq!(
        try_report(&te, &tight, &tight_oracle, 102_000_000),
        Some(err(OracleHubError::PriceJumpExceeded))
    );
    assert_eq!(try_report(&te, &loose, &loose_oracle, 140_000_000), None);
}

#[test]
fn test_zero_disables_the_check() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = seeded(&te, &asset);

    allow_all(&te);
    client(&te).set_max_price_jump(&Some(asset.clone()), &0);
    assert_eq!(try_report(&te, &asset, &oracle, 50 * PRICE), None);
    assert_eq!(latest_price(&te, &asset), 50 * PRICE);
}

#[test]
fn test_out_of_range_bound_is_rejected() {
    let te = setup();
    allow_all(&te);
    assert_eq!(
        client(&te).try_set_max_price_jump(&None, &-1),
        Err(Ok(err(OracleHubError::InvalidConfig)))
    );
    assert_eq!(
        client(&te).try_set_max_price_jump(&None, &10_001),
        Err(Ok(err(OracleHubError::InvalidConfig)))
    );
    // A full 100 % band is the ceiling and is allowed.
    client(&te).set_max_price_jump(&None, &10_000);
}

#[test]
fn test_setting_the_bound_is_governance_gated() {
    let te = setup();
    te.env.mock_auths(&[]);
    assert!(client(&te).try_set_max_price_jump(&None, &500).is_err());
}

// ── Rebase ──────────────────────────────────────────────────────────────────

#[test]
fn test_rebase_unlocks_a_legitimate_large_move() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = seeded(&te, &asset);

    // The market really did halve; the honest provider is locked out.
    assert_eq!(
        try_report(&te, &asset, &oracle, PRICE / 2),
        Some(err(OracleHubError::PriceJumpExceeded))
    );

    te.env.ledger().set_timestamp(60);
    allow_all(&te);
    client(&te).rebase_price(&asset, &FeedPriority::Primary, &(PRICE / 2), &90);
    assert!(has_event(&te, "price_rebased_event", &asset));
    assert_eq!(latest_price(&te, &asset), PRICE / 2);

    // Reports are now bounded against the rebased reference.
    assert_eq!(try_report(&te, &asset, &oracle, 55_000_000), None);
    assert_eq!(
        try_report(&te, &asset, &oracle, PRICE),
        Some(err(OracleHubError::PriceJumpExceeded))
    );
}

#[test]
fn test_rebase_invalidates_the_cache() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    seeded(&te, &asset);

    allow_all(&te);
    client(&te).set_cache_ttl(&600);
    assert_eq!(client(&te).get_price(&asset).price, PRICE);

    client(&te).rebase_price(&asset, &FeedPriority::Primary, &(3 * PRICE), &100);
    let agg = client(&te).get_price(&asset);
    assert!(!agg.from_cache);
    assert_eq!(agg.price, 3 * PRICE);
}

#[test]
fn test_rebase_of_unregistered_slot_is_rejected() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    allow_all(&te);
    assert_eq!(
        client(&te).try_rebase_price(&asset, &FeedPriority::Primary, &PRICE, &100),
        Err(Ok(err(OracleHubError::FeedNotFound)))
    );
}

#[test]
fn test_rebase_rejects_non_positive_price() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    seeded(&te, &asset);
    allow_all(&te);
    assert_eq!(
        client(&te).try_rebase_price(&asset, &FeedPriority::Primary, &0, &100),
        Err(Ok(err(OracleHubError::InvalidPrice)))
    );
}

#[test]
fn test_rebase_is_governance_gated() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = seeded(&te, &asset);

    // Not even the slot's own reporter may bypass the bound.
    te.env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &te.contract_id,
            fn_name: "rebase_price",
            args: (&asset, &FeedPriority::Primary, &(10 * PRICE), &100u32).into_val(&te.env),
            sub_invokes: &[],
        },
    }]);
    assert!(client(&te)
        .try_rebase_price(&asset, &FeedPriority::Primary, &(10 * PRICE), &100)
        .is_err());
    assert_eq!(latest_price(&te, &asset), PRICE);
}

#[test]
fn test_rebase_while_frozen_is_rejected() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    seeded(&te, &asset);
    allow_all(&te);
    client(&te).freeze();
    assert_eq!(
        client(&te).try_rebase_price(&asset, &FeedPriority::Primary, &PRICE, &100),
        Err(Ok(err(OracleHubError::Frozen)))
    );
}

// ── Arithmetic ──────────────────────────────────────────────────────────────

#[test]
fn test_bound_comparison_is_exact() {
    // At the bound, both directions.
    assert!(!exceeds_bound(120, 100, 2_000));
    assert!(!exceeds_bound(80, 100, 2_000));
    // A fraction of a basis point past it is not rounded back onto it.
    assert!(exceeds_bound(120_000_001, 100_000_000, 2_000));
    assert!(exceeds_bound(79_999_999, 100_000_000, 2_000));
    assert!(!exceeds_bound(100, 100, 0) && exceeds_bound(101, 100, 0));
    // Overflow saturates towards "more deviant", never wraps to small.
    assert!(exceeds_bound(i128::MAX, 1, 10_000));
    assert!(exceeds_bound(i128::MIN + 1, 1, 10_000));
}

/// Whether an event with the given name was published, regardless of topics.
fn has_event_named(te: &TestEnv, name: &str) -> bool {
    use soroban_sdk::testutils::Events;
    use soroban_sdk::xdr::{ContractEventBody, ScVal};
    use soroban_sdk::{Symbol, TryFromVal, Val};
    let name_val: Val = Symbol::new(&te.env, name).into_val(&te.env);
    let expected = ScVal::try_from_val(&te.env, &name_val).unwrap();
    te.env.events().all().events().iter().any(|event| {
        let ContractEventBody::V0(body) = &event.body;
        body.topics.first() == Some(&expected)
    })
}
