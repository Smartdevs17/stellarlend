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
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Clone, Debug)]
struct StepCost {
    step: String,
    budget_key: &'static str,
    cpu: u64,
    mem: u64,
    wall_us: u128,
}

#[derive(Default)]
struct JourneyMeter {
    name: String,
    steps: Vec<StepCost>,
}

impl JourneyMeter {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    /// Run `f` under a fresh budget and record its cost. `budget_key` names the
    /// `gas_budgets` entry the step is checked against.
    fn measure<T>(
        &mut self,
        f: &Fixture,
        step: &str,
        budget_key: &'static str,
        call: impl FnOnce() -> T,
    ) -> T {
        let mut budget = f.env.cost_estimate().budget();
        budget.reset_default();
        let start = Instant::now();
        let out = call();
        let wall_us = start.elapsed().as_micros();
        let cpu = budget.cpu_instruction_cost();
        let mem = budget.memory_bytes_cost();
        budget.reset_unlimited();
        self.steps.push(StepCost {
            step: step.into(),
            budget_key,
            cpu,
            mem,
            wall_us,
        });
        out
    }

    fn total_cpu(&self) -> u64 {
        self.steps.iter().map(|s| s.cpu).sum()
    }
}

fn load_budgets() -> BTreeMap<String, u64> {
    let path = format!(
        "{}/../../benchmarks/baseline.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let json: serde_json::Value = serde_json::from_str(&raw).expect("baseline.json is valid JSON");
    json["gas_budgets"]
        .as_object()
        .expect("baseline.json has gas_budgets")
        .iter()
        .filter_map(|(k, v)| v.as_u64().map(|b| (k.clone(), b)))
        .collect()
}

/// Recommendations derived from measured costs; kept deliberately rule-based
/// so they are reproducible in CI.
fn recommendations(journeys: &[JourneyMeter], budgets: &BTreeMap<String, u64>) -> Vec<String> {
    let mut out = Vec::new();
    for j in journeys {
        for s in &j.steps {
            let budget = budgets.get(s.budget_key).copied().unwrap_or(0);
            if budget > 0 && s.cpu > budget {
                out.push(format!(
                    "{}: step '{}' exceeds its {} budget ({} > {}) — either optimize it or revise the budget in benchmarks/baseline.json",
                    j.name, s.step, s.budget_key, s.cpu, budget
                ));
            } else if budget > 0 && s.cpu * 100 > budget * 80 {
                out.push(format!(
                    "{}: step '{}' uses {}% of its {} budget — profile storage access before adding logic",
                    j.name,
                    s.step,
                    s.cpu * 100 / budget,
                    s.budget_key
                ));
            }
            if s.budget_key.ends_with("get_user_position") {
                out.push(format!(
                    "{}: '{}' costs {} instructions on-chain; clients should read it via RPC simulation instead of a submitted transaction",
                    j.name, s.step, s.cpu
                ));
            }
        }
        let total = j.total_cpu().max(1);
        if let Some(top) = j.steps.iter().max_by_key(|s| s.cpu) {
            if top.cpu * 100 / total >= 40 {
                out.push(format!(
                    "{}: '{}' accounts for {}% of journey gas — the highest-leverage optimization target",
                    j.name,
                    top.step,
                    top.cpu * 100 / total
                ));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn write_report(journeys: &[JourneyMeter], budgets: &BTreeMap<String, u64>, file: &str) {
    let dir = std::env::var("JOURNEY_REPORT_DIR")
        .unwrap_or_else(|_| format!("{}/target/journey-reports", env!("CARGO_MANIFEST_DIR")));
    std::fs::create_dir_all(&dir).expect("create report dir");
    let report = serde_json::json!({
        "journeys": journeys.iter().map(|j| serde_json::json!({
            "name": j.name,
            "total_cpu_instructions": j.total_cpu(),
            "total_memory_bytes": j.steps.iter().map(|s| s.mem).sum::<u64>(),
            "total_wall_us": j.steps.iter().map(|s| s.wall_us).sum::<u128>() as u64,
            "steps": j.steps.iter().map(|s| serde_json::json!({
                "step": s.step,
                "budget_key": s.budget_key,
                "budget": budgets.get(s.budget_key).copied().unwrap_or(0),
                "cpu_instructions": s.cpu,
                "memory_bytes": s.mem,
                "wall_us": s.wall_us as u64,
                "enforced": !is_view(s.budget_key),
                "over_budget": budgets.get(s.budget_key).is_some_and(|b| s.cpu > *b),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "recommendations": recommendations(journeys, budgets),
    });
    std::fs::write(
        format!("{dir}/{file}"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .expect("write journey report");
}

/// Read-only entry points: metered and reported, not budget-enforced.
fn is_view(budget_key: &str) -> bool {
    budget_key.contains("::get_")
}

fn assert_within_budgets(j: &JourneyMeter, budgets: &BTreeMap<String, u64>) {
    for s in j.steps.iter().filter(|s| !is_view(s.budget_key)) {
        let budget = *budgets
            .get(s.budget_key)
            .unwrap_or_else(|| panic!("no gas budget for {}", s.budget_key));
        assert!(
            s.cpu <= budget,
            "{}: step '{}' used {} CPU instructions, over its {} budget of {}",
            j.name,
            s.step,
            s.cpu,
            s.budget_key,
            budget
        );
    }
}

/// deposit → borrow → check position → repay → withdraw for one user.
fn run_complete_journey(f: &Fixture, user: &Address, name: &str) -> JourneyMeter {
    let mut j = JourneyMeter::new(name);
    let c = &f.client;
    let (ca, da) = (&f.collateral_asset, &f.debt_asset);

    let bal = j.measure(f, "deposit", "lending::deposit", || {
        c.deposit(user, ca, &10_000)
    });
    assert_eq!(bal, 10_000);
    j.measure(f, "borrow", "lending::borrow", || {
        c.borrow(user, da, &2_000, ca, &3_000)
    });
    let pos = j.measure(f, "check_position", "lending::get_user_position", || {
        c.get_user_position(user)
    });
    assert_eq!(pos.debt_balance, 2_000);
    assert!(
        pos.health_factor >= HF_SCALE,
        "fresh position must be healthy"
    );
    j.measure(f, "repay", "lending::repay", || c.repay(user, da, &2_000));
    assert_eq!(c.get_debt_balance(user), 0);
    let left = j.measure(f, "withdraw", "lending::withdraw", || {
        c.withdraw(user, ca, &10_000)
    });
    assert_eq!(left, 0);
    j
}

#[test]
fn complete_journey_within_gas_budgets() {
    let f = setup();
    let budgets = load_budgets();
    let user = Address::generate(&f.env);

    let journey = run_complete_journey(&f, &user, "single_user_full_cycle");
    assert_eq!(journey.steps.len(), 5);
    assert!(
        journey.steps.iter().all(|s| s.cpu > 0),
        "every step must be metered"
    );
    assert_within_budgets(&journey, &budgets);

    write_report(&[journey], &budgets, "lending-journeys.json");
}

#[test]
fn multi_user_interleaved_journeys() {
    let f = setup();
    let budgets = load_budgets();
    let users: Vec<Address> = (0..5).map(|_| Address::generate(&f.env)).collect();
    let (ca, da) = (&f.collateral_asset, &f.debt_asset);
    let mut meters: Vec<JourneyMeter> = (0..users.len())
        .map(|i| JourneyMeter::new(&format!("interleaved_user_{i}")))
        .collect();

    // Phase-by-phase interleaving: all users deposit, then all borrow, etc.,
    // so every step runs against shared state touched by the other users.
    for (i, u) in users.iter().enumerate() {
        let amount = 5_000 + i as i128 * 1_000;
        meters[i].measure(&f, "deposit", "lending::deposit", || {
            f.client.deposit(u, ca, &amount)
        });
    }
    for (i, u) in users.iter().enumerate() {
        let amount = 1_000 + i as i128 * 100;
        meters[i].measure(&f, "borrow", "lending::borrow", || {
            f.client.borrow(u, da, &amount, ca, &(amount * 2))
        });
    }
    for (i, u) in users.iter().enumerate().rev() {
        let owed = f.client.get_debt_balance(u);
        meters[i].measure(&f, "repay", "lending::repay", || {
            f.client.repay(u, da, &owed)
        });
    }
    for (i, u) in users.iter().enumerate() {
        let held = f.client.get_user_collateral_deposit(u, ca).amount;
        meters[i].measure(&f, "withdraw", "lending::withdraw", || {
            f.client.withdraw(u, ca, &held)
        });
    }

    for (m, u) in meters.iter().zip(&users) {
        assert_within_budgets(m, &budgets);
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
        let costs: Vec<u64> = meters
            .iter()
            .flat_map(|m| m.steps.iter().filter(|s| s.step == step).map(|s| s.cpu))
            .collect();
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
