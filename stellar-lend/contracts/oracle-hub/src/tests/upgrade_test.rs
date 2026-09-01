//! Upgradable oracle mechanism: staged two-phase upgrade and governance gating.
//!
//! The swap step calls Soroban's `update_current_contract_wasm`, which the
//! unit-test env rejects (`upload_wasm` is unavailable off-chain; the target
//! must be a compiled soroban contract). As documented in
//! `contracts/hello-world/DIFFERENTIAL_TEST_REPORT.md`, the real code swap is
//! only verifiable with a pre-built `.wasm` artifact. The suite therefore
//! covers the deterministic governance lifecycle: staging, pending visibility,
//! frozen/unauthorized gating, and the failure of an un-provisioned apply.

extern crate std;

use super::helpers::{allow_all, client, mk_asset, register_push_feed, report, setup};
use crate::types::{FeedPriority, VERSION};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, IntoVal};

/// An arbitrary 32-byte wasm hash used to test the staging state machine.
/// It is never backed by an uploaded wasm, so a swap attempt always reverts.
fn wasm_hash(env: &Env, seed: u8) -> BytesN<32> {
    let mut h = [0u8; 32];
    h[0] = seed;
    BytesN::from_array(env, &h)
}

#[test]
fn test_upgrade_stage_sets_pending_wasm_hash() {
    let te = setup();
    let hash = wasm_hash(&te.env, 7);

    allow_all(&te);
    client(&te).stage_upgrade(&hash);

    assert_eq!(client(&te).pending_wasm_hash(), Some(hash));
    // Staging alone does not bump the version.
    assert_eq!(client(&te).version(), VERSION);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_upgrade_without_staging_reverts() {
    let te = setup();
    allow_all(&te);
    client(&te).upgrade();
}

#[test]
#[should_panic(expected = "HostError")]
fn test_stage_upgrade_while_frozen_reverts() {
    let te = setup();
    let hash = wasm_hash(&te.env, 1);

    allow_all(&te);
    client(&te).freeze();
    client(&te).stage_upgrade(&hash);
}

#[test]
fn test_pending_wasm_hash_empty_without_stage() {
    let te = setup();
    assert!(client(&te).pending_wasm_hash().is_none());
}

#[test]
#[should_panic(expected = "HostError")]
fn test_stage_upgrade_requires_governance() {
    let te = setup();
    let hash = wasm_hash(&te.env, 1);
    // No governance auth mocked -> default source account is not governance.
    client(&te).stage_upgrade(&hash);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_apply_upgrade_requires_governance() {
    let te = setup();
    let hash = wasm_hash(&te.env, 1);

    // Authorize ONLY the governance staging invocation.
    te.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &te.governance,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &te.contract_id,
            fn_name: "stage_upgrade",
            args: (&hash,).into_val(&te.env),
            sub_invokes: &[],
        },
    }]);
    client(&te).stage_upgrade(&hash);

    // Applying is a separate governance action and is not authorized.
    client(&te).upgrade();
}

#[test]
fn test_stage_upgrade_survives_freeze_thaw() {
    let te = setup();
    let hash = wasm_hash(&te.env, 3);

    allow_all(&te);
    client(&te).freeze();
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client(&te).stage_upgrade(&hash);
    }));
    assert!(err.is_err());
    assert!(client(&te).pending_wasm_hash().is_none());

    client(&te).unfreeze();
    client(&te).stage_upgrade(&hash);
    assert_eq!(client(&te).pending_wasm_hash(), Some(hash));
}

#[test]
fn test_staging_preserves_services_until_apply() {
    let te = setup();
    let asset = mk_asset(&te.env, "XLM");
    let oracle = Address::generate(&te.env);

    register_push_feed(&te, &asset, &oracle, &FeedPriority::Primary, 3600);
    report(&te, &asset, &oracle, 100_000_000, &FeedPriority::Primary);

    allow_all(&te);
    let hash = wasm_hash(&te.env, 4);
    client(&te).stage_upgrade(&hash);

    // Staging alone must not disturb the feed: version unchanged, price serves.
    assert_eq!(client(&te).version(), VERSION);
    assert!(client(&te).pending_wasm_hash().is_some());
    assert_eq!(client(&te).price(&asset), 100_000_000);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_apply_without_provisioned_wasm_reverts() {
    let te = setup();
    let hash = wasm_hash(&te.env, 9);

    allow_all(&te);
    client(&te).stage_upgrade(&hash);
    // No wasm is uploaded for this hash in the test env -> swap reverts.
    client(&te).upgrade();
}
