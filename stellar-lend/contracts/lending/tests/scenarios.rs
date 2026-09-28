//! Data-driven and seeded lending scenarios on the shared test framework.
//!
//! - `lending_journey_scenario` executes `test-utils/scenarios/lending-journey.json`
//!   step by step against the real contract through `ScenarioRunner`, so new
//!   journeys can be added as JSON without writing Rust.
//! - The seeded tests draw amounts and oracle price paths from `SeedRng`
//!   (`TEST_SEED` overrides the seed) and compare the contract's health factor
//!   with the `test_utils::reference` model.

mod common;

use std::collections::BTreeMap;

use common::*;
use soroban_sdk::{testutils::Address as _, Address};
use test_utils::scenario::ScenarioStep;
use test_utils::{health_factor_bps, Scenario, ScenarioRunner, SeedRng};

const THRESHOLD_BPS: i128 = 8_000;

fn setup_with_threshold<'a>() -> Fixture<'a> {
    let f = setup();
    f.client
        .set_liquidation_threshold_bps(&f.admin, &THRESHOLD_BPS);
    f
}

/// Maps scenario names (`alice`, `collateral`, ...) to addresses.
struct Actors<'f, 'a> {
    f: &'f Fixture<'a>,
    users: BTreeMap<String, Address>,
}

impl<'f, 'a> Actors<'f, 'a> {
    fn new(f: &'f Fixture<'a>) -> Self {
        Self {
            f,
            users: BTreeMap::new(),
        }
    }

    fn user(&mut self, name: &str) -> Address {
        let env = &self.f.env;
        self.users
            .entry(name.to_string())
            .or_insert_with(|| Address::generate(env))
            .clone()
    }

    fn asset(&self, name: &str) -> Address {
        match name {
            "collateral" => self.f.collateral_asset.clone(),
            "debt" => self.f.debt_asset.clone(),
            other => panic!("unknown asset '{other}'"),
        }
    }

    /// Execute one scenario step; contract errors become `Err` so the runner
    /// can match them against `expected_result`.
    fn apply(&mut self, step: &ScenarioStep) -> Result<(), String> {
        let c = &self.f.client;
        match step.action.as_str() {
            "deposit" => {
                let user = self.user(step.param("user"));
                let asset = self.asset(step.param("asset"));
                c.try_deposit(&user, &asset, &step.param_i128("amount"))
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}"))
            }
            "borrow" => {
                let user = self.user(step.param("user"));
                c.try_borrow(
                    &user,
                    &self.asset(step.param("asset")),
                    &step.param_i128("amount"),
                    &self.asset(step.param("collateral_asset")),
                    &step.param_i128("collateral_amount"),
                )
                .map(|_| ())
                .map_err(|e| format!("{e:?}"))
            }
            "repay" => {
                let user = self.user(step.param("user"));
                let asset = self.asset(step.param("asset"));
                c.try_repay(&user, &asset, &step.param_i128("amount"))
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}"))
            }
            "withdraw" => {
                let user = self.user(step.param("user"));
                let asset = self.asset(step.param("asset"));
                c.try_withdraw(&user, &asset, &step.param_i128("amount"))
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}"))
            }
            "set_price" => {
                let asset = self.asset(step.param("asset"));
                self.f
                    .oracle_client()
                    .set_price(&asset, &step.param_i128("price"));
                Ok(())
            }
            "pause_deposits" => c
                .try_set_deposit_paused(&(step.param("paused") == "true"))
                .map(|_| ())
                .map_err(|e| format!("{e:?}")),
            "expect_healthy" => {
                let user = self.user(step.param("user"));
                let hf = c.get_health_factor(&user);
                if hf >= HF_SCALE {
                    Ok(())
                } else {
                    Err(format!("liquidatable: health factor {hf}"))
                }
            }
            other => panic!("unsupported scenario action '{other}'"),
        }
    }
}

#[test]
fn lending_journey_scenario() {
    let f = setup_with_threshold();
    let scenario = Scenario::bundled("lending-journey.json");
    let mut actors = Actors::new(&f);

    let result = ScenarioRunner::new().run(&scenario, |step| actors.apply(step));
    result.assert_passed();
    assert_eq!(result.total_steps, scenario.steps.len());

    // The journey closes alice out and leaves bob's resumed deposit in place.
    let alice = actors.user("alice");
    let bob = actors.user("bob");
    let a = snapshot(&f, &alice);
    assert_eq!((a.deposit, a.principal, a.debt_balance), (0, 0, 0));
    assert_eq!(snapshot(&f, &bob).deposit, 500);
}

#[test]
fn seeded_multi_user_deposits_are_conserved() {
    let f = setup_with_threshold();
    let mut rng = SeedRng::from_env();
    let users: Vec<Address> = (0..8).map(|_| Address::generate(&f.env)).collect();
    let mut expected = Vec::new();

    for user in &users {
        let rounds = rng.range_u64(1, 4);
        let mut total = 0;
        for _ in 0..rounds {
            let amount = rng.range_i128(MIN_DEPOSIT, 50_000);
            total = f.client.deposit(user, &f.collateral_asset, &amount);
        }
        expected.push(total);
    }

    for (user, total) in users.iter().zip(&expected) {
        assert_eq!(snapshot(&f, user).deposit, *total);
    }
}

#[test]
fn seeded_price_path_matches_reference_health_factor() {
    let f = setup_with_threshold();
    let mut rng = SeedRng::from_env();
    let user = Address::generate(&f.env);
    let (collateral, debt) = (30_000, 20_000);
    f.client.deposit(&user, &f.collateral_asset, &50_000);
    f.client.borrow(
        &user,
        &f.debt_asset,
        &debt,
        &f.collateral_asset,
        &collateral,
    );

    let oracle = f.oracle_client();
    let mut price = PRICE_SCALE;
    for _ in 0..40 {
        let move_bps = rng.range_i128(-1_500, 1_500);
        price = (price + price * move_bps / 10_000).max(PRICE_SCALE / 100);
        oracle.set_price(&f.collateral_asset, &price);

        let collateral_value = collateral * price / PRICE_SCALE;
        let expected = health_factor_bps(collateral_value, debt, THRESHOLD_BPS);
        let actual = f.client.get_health_factor(&user);
        // The contract rounds the weighted collateral down before scaling,
        // so it may differ from the model by up to HF_SCALE / debt + 1.
        let tolerance = HF_SCALE / debt + 1;
        assert!(
            (expected - actual).abs() <= tolerance,
            "price {price}: health factor {actual}, reference {expected}"
        );
        if expected + tolerance < HF_SCALE {
            assert!(actual < HF_SCALE, "price {price}: should be liquidatable");
        }
        if expected - tolerance >= HF_SCALE {
            assert!(actual >= HF_SCALE, "price {price}: should be healthy");
        }
    }
}
