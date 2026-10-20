//! Storage TTL management tests: policy defaults and validation, per-asset
//! overrides, idempotent extension, the permissionless bump path, and the
//! governance controls around them.

extern crate std;

use super::helpers::{allow_all, client, has_event, mk_asset, register_push_feed, report, setup};
use crate::types::{FeedPriority, TtlPolicy, DEFAULT_TTL_EXTEND_TO, MAX_TTL_EXTEND_TO};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Address;

#[test]
fn test_default_policy_is_present_and_enabled() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let config = client(&te).get_ttl_policy(&asset);

    assert!(config.policy.enabled);
    assert_eq!(config.policy.extend_to, DEFAULT_TTL_EXTEND_TO);
    assert!(config.policy.threshold < config.policy.extend_to);
}

#[test]
fn test_set_hub_wide_policy_applies_to_unconfigured_assets() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    allow_all(&te);

    let policy = TtlPolicy {
        enabled: true,
        threshold: 1_000,
        extend_to: 10_000,
    };
    client(&te).set_ttl_policy(&None, &policy);

    let config = client(&te).get_ttl_policy(&asset);
    assert_eq!(config.policy, policy);
}

#[test]
fn test_per_asset_override_wins_over_default() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let other = mk_asset(&te.env, "ETH");
    allow_all(&te);

    let default = TtlPolicy {
        enabled: true,
        threshold: 1_000,
        extend_to: 10_000,
    };
    let override_policy = TtlPolicy {
        enabled: true,
        threshold: 500,
        extend_to: 5_000,
    };
    client(&te).set_ttl_policy(&None, &default);
    client(&te).set_ttl_policy(&Some(asset.clone()), &override_policy);

    assert_eq!(client(&te).get_ttl_policy(&asset).policy, override_policy);
    // The override must not leak to a different asset.
    assert_eq!(client(&te).get_ttl_policy(&other).policy, default);
}

#[test]
fn test_incoherent_policy_is_rejected() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    allow_all(&te);

    // extend_to must exceed threshold.
    let bad = TtlPolicy {
        enabled: true,
        threshold: 10_000,
        extend_to: 5_000,
    };
    let err = client(&te).try_set_ttl_policy(&Some(asset.clone()), &bad);
    assert!(err.is_err());

    // extend_to must not exceed the ceiling.
    let too_big = TtlPolicy {
        enabled: true,
        threshold: 1_000,
        extend_to: MAX_TTL_EXTEND_TO + 1,
    };
    let err = client(&te).try_set_ttl_policy(&Some(asset.clone()), &too_big);
    assert!(err.is_err());
}

#[test]
fn test_disabled_policy_is_always_accepted() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    allow_all(&te);

    // Values are not validated when the policy is disabled, because it has no
    // effect; governance may park a policy and re-enable it later.
    let disabled = TtlPolicy {
        enabled: false,
        threshold: 0,
        extend_to: 0,
    };
    client(&te).set_ttl_policy(&Some(asset.clone()), &disabled);
    assert_eq!(client(&te).get_ttl_policy(&asset).policy, disabled);
}

#[test]
fn test_bump_extends_latest_price_and_emits_event() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);
    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);
    allow_all(&te);

    let bumped = client(&te).bump_storage(&asset);
    assert!(bumped >= 1, "a live price entry must be extended");
    assert!(has_event(&te, "ttl_extended", &asset));
}

#[test]
fn test_bump_is_idempotent_within_the_window() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);
    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);
    allow_all(&te);

    // First bump refreshes the live entry and stamps the ledger.
    let first = client(&te).bump_storage(&asset);
    assert!(first >= 1);
    let stamp = client(&te).get_ttl_policy(&asset).last_bump_ledger;

    // A second bump in the same ledger walks the same managed entries; the
    // storage-level extension is a no-op because the entry is inside the
    // window, so the stamped ledger does not advance.
    client(&te).bump_storage(&asset);
    assert_eq!(client(&te).get_ttl_policy(&asset).last_bump_ledger, stamp);
}

#[test]
fn test_bump_extends_again_near_the_threshold() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);
    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);
    allow_all(&te);

    // A tight policy so the threshold is crossed quickly.
    let policy = TtlPolicy {
        enabled: true,
        threshold: 100,
        extend_to: 1_000,
    };
    client(&te).set_ttl_policy(&Some(asset.clone()), &policy);

    assert!(client(&te).bump_storage(&asset) >= 1);
    let first_stamp = client(&te).get_ttl_policy(&asset).last_bump_ledger;

    // Advance the ledger past the remaining window so the entry is due again,
    // and confirm the manager runs once more.
    te.env.ledger().set_sequence_number(2_000);
    assert!(client(&te).bump_storage(&asset) >= 1);
    assert!(client(&te).get_ttl_policy(&asset).last_bump_ledger > first_stamp);
}

#[test]
fn test_disabled_asset_policy_makes_bump_a_noop() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);
    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);
    allow_all(&te);

    client(&te).set_ttl_policy(
        &Some(asset.clone()),
        &TtlPolicy {
            enabled: false,
            threshold: 1_000,
            extend_to: 10_000,
        },
    );

    assert_eq!(client(&te).bump_storage(&asset), 0);
    assert_eq!(client(&te).get_ttl_policy(&asset).last_bump_ledger, 0);
}

#[test]
fn test_policy_change_emits_event() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    allow_all(&te);

    client(&te).set_ttl_policy(
        &Some(asset.clone()),
        &TtlPolicy {
            enabled: true,
            threshold: 1_000,
            extend_to: 10_000,
        },
    );
    assert!(has_event(&te, "ttl_config_updated", &asset));
}
