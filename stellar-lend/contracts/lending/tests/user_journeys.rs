//! Contract-level end-to-end user journeys with per-step gas metering.
//!
//! Complements the API-level journeys in `tests/e2e/scenarios/` by running the
//! same deposit → borrow → check → repay → withdraw flow against the real
//! contract and measuring each step with the Soroban cost model (CPU
//! instructions + memory bytes — the inputs to on-chain fees).
//!
//! Every state-changing step is checked against its per-operation budget in
//! `benchmarks/baseline.json` (`gas_budgets`), so a gas regression in any
//! transaction of a journey fails this suite. Read-only views are metered and
//! reported (with `over_budget` flagged) but not enforced, because clients
//! read them through RPC simulation rather than fee-paying transactions. A JSON report with per-step costs, journey
//! totals, timing and optimization recommendations is written to
//! `target/journey-reports/lending-journeys.json` (override with
//! `JOURNEY_REPORT_DIR`) for CI to publish.

mod common;

use common::*;
use soroban_sdk::{testutils::Address as _, Address};
use test_utils::bench::report_dir;
use test_utils::{BenchReport, GasBudgets, GasMeter};

/// Per-step metering, budgets and the JSON report come from
/// `test_utils::bench`; the lending journeys only add this suite-specific
/// recommendation on top of the generic ones.
fn lending_recommendations(journeys: &[GasMeter]) -> Vec<String> {
    journeys
        .iter()
        .flat_map(|j| {
            j.steps
                .iter()
                .filter(|s| s.budget_key.ends_with("get_user_position"))
                .map(move |s| {
                    format!(
                        "{}: '{}' costs {} instructions on-chain; clients should read it via RPC simulation instead of a submitted transaction",
                        j.name, s.step, s.cpu
                    )
                })
        })
        .collect()
}

fn write_report(journeys: &[GasMeter], budgets: &GasBudgets, file: &str) {
    let dir = report_dir(
        "JOURNEY_REPORT_DIR",
        format!("{}/target/journey-reports", env!("CARGO_MANIFEST_DIR")),
    );
    BenchReport::new(journeys, budgets)
        .with_recommendations(lending_recommendations(journeys))
        .write(&dir, file);
}

/// deposit → borrow → check position → repay → withdraw for one user.
fn run_complete_journey(f: &Fixture, user: &Address, name: &str) -> GasMeter {
    let mut j = GasMeter::new(name);
    let c = &f.client;
    let (ca, da) = (&f.collateral_asset, &f.debt_asset);

    let bal = j.measure(&f.env, "deposit", "lending::deposit", || {
        c.deposit(user, ca, &10_000)
    });
    assert_eq!(bal, 10_000);
    j.measure(&f.env, "borrow", "lending::borrow", || {
        c.borrow(user, da, &2_000, ca, &3_000)
    });
    let pos = j.measure(
        &f.env,
        "check_position",
        "lending::get_user_position",
        || c.get_user_position(user),
    );
    assert_eq!(pos.debt_balance, 2_000);
    assert!(
        pos.health_factor >= HF_SCALE,
        "fresh position must be healthy"
    );
    j.measure(&f.env, "repay", "lending::repay", || {
        c.repay(user, da, &2_000)
    });
    assert_eq!(c.get_debt_balance(user), 0);
    let left = j.measure(&f.env, "withdraw", "lending::withdraw", || {
        c.withdraw(user, ca, &10_000)
    });
    assert_eq!(left, 0);
    j
}

#[test]
fn complete_journey_within_gas_budgets() {
    let f = setup();
    let budgets = GasBudgets::workspace_baseline();
    let user = Address::generate(&f.env);

    let journey = run_complete_journey(&f, &user, "single_user_full_cycle");
    assert_eq!(journey.steps.len(), 5);
    assert!(
        journey.steps.iter().all(|s| s.cpu > 0),
        "every step must be metered"
    );
    budgets.assert_within(&journey);

    write_report(&[journey], &budgets, "lending-journeys.json");
}

#[test]
fn multi_user_interleaved_journeys() {
    let f = setup();
    let budgets = GasBudgets::workspace_baseline();
    let users: Vec<Address> = (0..5).map(|_| Address::generate(&f.env)).collect();
    let (ca, da) = (&f.collateral_asset, &f.debt_asset);
    let mut meters: Vec<GasMeter> = (0..users.len())
        .map(|i| GasMeter::new(&format!("interleaved_user_{i}")))
        .collect();

    // Phase-by-phase interleaving: all users deposit, then all borrow, etc.,
    // so every step runs against shared state touched by the other users.
    for (i, u) in users.iter().enumerate() {
        let amount = 5_000 + i as i128 * 1_000;
        meters[i].measure(&f.env, "deposit", "lending::deposit", || {
            f.client.deposit(u, ca, &amount)
        });
    }
    for (i, u) in users.iter().enumerate() {
        let amount = 1_000 + i as i128 * 100;
        meters[i].measure(&f.env, "borrow", "lending::borrow", || {
            f.client.borrow(u, da, &amount, ca, &(amount * 2))
        });
    }
    for (i, u) in users.iter().enumerate().rev() {
        let owed = f.client.get_debt_balance(u);
        meters[i].measure(&f.env, "repay", "lending::repay", || {
            f.client.repay(u, da, &owed)
        });
    }
    for (i, u) in users.iter().enumerate() {
        let held = f.client.get_user_collateral_deposit(u, ca).amount;
        meters[i].measure(&f.env, "withdraw", "lending::withdraw", || {
            f.client.withdraw(u, ca, &held)
        });
    }

    for (m, u) in meters.iter().zip(&users) {
        budgets.assert_within(m);
        let s = snapshot(&f, u);
        assert_eq!(
            (s.deposit, s.principal, s.debt_balance),
            (0, 0, 0),
            "{} did not close out",
            m.name
        );
    }

    // Isolation: the same operation should cost roughly the same for every
    // user; a large spread means cost depends on other users' state.
    for step in ["deposit", "borrow", "repay", "withdraw"] {
        let costs: Vec<u64> = meters.iter().flat_map(|m| m.cpu_of(step)).collect();
        let (min, max) = (*costs.iter().min().unwrap(), *costs.iter().max().unwrap());
        assert!(
            max <= min * 2,
            "{step} cost varies {min}..{max} across users — per-user cost should not scale with protocol size"
        );
    }

    write_report(&meters, &budgets, "lending-multi-user-journeys.json");
}

#[test]
fn journey_recovers_from_paused_withdrawals() {
    let f = setup();
    let user = Address::generate(&f.env);
    let (ca, da) = (&f.collateral_asset, &f.debt_asset);

    f.client.deposit(&user, ca, &10_000);
    f.client.borrow(&user, da, &1_000, ca, &1_500);
    f.client.repay(&user, da, &1_000);

    // Withdrawals halted mid-journey (e.g. incident response).
    f.client.set_withdraw_paused(&true);
    let before = snapshot(&f, &user);
    assert!(f.client.try_withdraw(&user, ca, &10_000).is_err());
    assert_eq!(
        snapshot(&f, &user),
        before,
        "failed withdraw must not change state"
    );

    // Protocol resumes; the journey completes from where it stopped.
    f.client.set_withdraw_paused(&false);
    assert_eq!(f.client.withdraw(&user, ca, &10_000), 0);
}

#[test]
fn journey_recovers_from_rejected_steps() {
    let f = setup();
    let user = Address::generate(&f.env);
    let (ca, da) = (&f.collateral_asset, &f.debt_asset);

    f.client.deposit(&user, ca, &10_000);

    // Under-collateralized borrow is rejected, then retried with enough collateral.
    let before = snapshot(&f, &user);
    assert!(f.client.try_borrow(&user, da, &1_000, ca, &1_000).is_err());
    assert_eq!(snapshot(&f, &user), before);
    f.client.borrow(&user, da, &1_000, ca, &1_500);

    // Over-repay is rejected, then the exact amount goes through.
    let before = snapshot(&f, &user);
    assert!(f.client.try_repay(&user, da, &5_000).is_err());
    assert_eq!(snapshot(&f, &user), before);
    f.client.repay(&user, da, &1_000);

    // Deposit paused → rejected → resumed → accepted.
    f.client.set_deposit_paused(&true);
    assert!(f.client.try_deposit(&user, ca, &500).is_err());
    f.client.set_deposit_paused(&false);
    assert_eq!(f.client.deposit(&user, ca, &500), 10_500);

    assert_eq!(f.client.withdraw(&user, ca, &10_500), 0);
    assert_eq!(snapshot(&f, &user).debt_balance, 0);
}
