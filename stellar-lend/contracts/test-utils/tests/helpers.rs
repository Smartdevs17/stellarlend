//! Assertions, invariants, event recording, gas snapshots and the
//! cross-contract harness.

use soroban_sdk::{symbol_short, testutils::Address as _, Address};
use test_utils::{
    assert_accounting_identity, assert_approximately_equal, assert_cheaper_or_equal,
    assert_collateral_conservation, assert_frozen_while_paused,
    assert_health_direction_after_borrow, assert_health_direction_after_deposit, assert_in_range,
    assert_index_monotonic, assert_no_free_value, assert_percentage_in_range, assert_price_sane,
    assert_within_cpu, measure_delta, register_mock_token, reset_budget, run_labeled_scenario,
    snapshot, timed, topic_borrow, topic_deposit, CrossContractHarness, EventRecorder,
    InvariantReport, MockTokenClient, ScenarioStep, StepKind, TestEnv,
};

#[test]
fn numeric_assertions_accept_valid_values() {
    test_utils::assert_non_negative(0, "zero");
    test_utils::assert_positive(1, "one");
    assert_in_range(5, 0, 10, "range");
    test_utils::assert_balance_non_negative(10, "alice");
    test_utils::assert_balances_equal(7, 7, "equal");
    assert_approximately_equal(1_005, 1_000, 5, "approx");
    assert_percentage_in_range(7_500, 7_000, 8_000, 10_000, "pct");
    test_utils::assert_greater_than(2, 1, "gt");
    test_utils::assert_less_than(1, 2, "lt");
    test_utils::assert_zero(0, "zero");
    test_utils::assert_not_zero(3, "non-zero");
}

#[test]
#[should_panic(expected = "must be between")]
fn range_assertion_rejects_out_of_range() {
    assert_in_range(11, 0, 10, "range");
}

#[test]
fn invariants_accept_consistent_state() {
    assert_accounting_identity(1_000, 600, 50, 350, 0, "identity");
    assert_collateral_conservation(&[100, 200, 300], 600, "conservation");
    assert_health_direction_after_deposit(10_000, 12_000, "deposit");
    assert_health_direction_after_borrow(12_000, 10_000, "borrow");
    assert_price_sane(1, "price");
    assert_no_free_value(1_000, 900, 100, "repay");
    assert_index_monotonic(100, 101, "index");
    assert_frozen_while_paused(50, 50, "paused");
}

#[test]
#[should_panic(expected = "accounting identity violated")]
fn accounting_identity_detects_leaks() {
    assert_accounting_identity(1_000, 600, 50, 300, 10, "identity");
}

#[test]
fn invariant_report_tallies_checks() {
    let mut report = InvariantReport::default();
    report.record_ok();
    report.record_ok();
    assert_eq!(report.total(), 2);
    report.assert_all_passed("clean");

    report.record_fail("broken".into());
    assert_eq!(report.checks_failed, 1);
    assert_eq!(report.failures, vec!["broken".to_string()]);
}

#[test]
fn event_recorder_is_a_bounded_ring() {
    let t = TestEnv::snapshotless();
    let mut rec = EventRecorder::new(&t.env, 2);
    assert!(rec.is_empty());
    rec.record(&t.env, topic_deposit(), "first");
    rec.record(&t.env, topic_borrow(), "second");
    rec.record(&t.env, symbol_short!("repay"), "third");

    rec.assert_count(2);
    assert_eq!(rec.events()[0].label, "second");
    rec.assert_topic_seen(symbol_short!("repay"));
    rec.clear();
    assert_eq!(rec.len(), 0);
}

#[test]
fn gas_snapshots_measure_contract_calls() {
    let t = TestEnv::snapshotless();
    reset_budget(&t.env);
    let token = MockTokenClient::new(&t.env, &register_mock_token(&t.env));
    let user = Address::generate(&t.env);

    let (_, mint) = timed(&t.env, || token.mint(&user, &100));
    assert!(mint.cpu_instructions > 0 && mint.memory_bytes > 0);

    // Metering resets per top-level invocation, so an earlier expensive call
    // must not leak into the next measurement.
    let read = measure_delta(&t.env, &snapshot(&t.env), || {
        assert_eq!(token.balance(&user), 100);
    });
    assert!(read.cpu_instructions > 0);
    assert_cheaper_or_equal("balance read vs mint", &read, &mint);
    assert_within_cpu(&t.env, "balance", 10_000_000, || {
        assert_eq!(token.balance(&user), 100);
    });
}

#[test]
fn cross_contract_harness_runs_labeled_scenarios() {
    let t = TestEnv::snapshotless();
    let mut h = CrossContractHarness::new(&t.env, &t.admin);
    let user = Address::generate(&t.env);
    let asset = Address::generate(&t.env);

    h.mint(&user, 500);
    assert_eq!(h.balance(&user), 500);

    let steps = [
        ScenarioStep {
            name: "deposit",
            kind: StepKind::Deposit {
                user: user.clone(),
                amount: 100,
            },
        },
        ScenarioStep {
            name: "price",
            kind: StepKind::SetPrice {
                asset: asset.clone(),
                price: 2_000_000,
            },
        },
        ScenarioStep {
            name: "custom",
            kind: StepKind::Custom("rebalance"),
        },
    ];
    run_labeled_scenario(&mut h, &steps);

    assert_eq!(h.get_price(&asset), 2_000_000);
    h.events.assert_count(3);
    h.events.assert_topic_seen(topic_deposit());
    assert_eq!(h.invariants.checks_passed, 3);
    h.assert_scenario("labeled");
}
