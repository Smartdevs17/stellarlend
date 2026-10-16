//! Data-driven scenarios.
//!
//! A [`Scenario`] is a list of actions with string parameters and an expected
//! outcome, loadable from JSON (see `scenarios/*.json`). [`ScenarioRunner`]
//! hands each step to a suite-provided handler that drives the real contract,
//! and checks the handler's outcome against `expected_result`:
//!
//! | `expected_result` | passes when the handler returns |
//! |---|---|
//! | `"success"` | `Ok(())` |
//! | `"error"` | any `Err(_)` |
//! | `"error:<text>"` | an `Err` whose message contains `<text>` |
//!
//! Moved here from the former `packages/test-framework` crate, whose runner
//! only validated the scenario shape without executing it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<ScenarioStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioStep {
    pub action: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    pub expected_result: String,
}

impl Scenario {
    pub fn from_json(raw: &str) -> Result<Self, String> {
        serde_json::from_str(raw).map_err(|e| format!("invalid scenario JSON: {e}"))
    }

    pub fn from_file(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        Self::from_json(&raw).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// A scenario from this crate's `scenarios/` directory.
    pub fn bundled(file: &str) -> Self {
        Self::from_file(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scenarios")
                .join(file),
        )
    }
}

impl ScenarioStep {
    pub fn param(&self, key: &str) -> &str {
        self.params
            .get(key)
            .map(String::as_str)
            .unwrap_or_else(|| panic!("step '{}' is missing param '{}'", self.action, key))
    }

    pub fn param_i128(&self, key: &str) -> i128 {
        let raw = self.param(key);
        raw.parse().unwrap_or_else(|_| {
            panic!(
                "step '{}': param '{}' = '{}' is not an integer",
                self.action, key, raw
            )
        })
    }

    fn expectation(&self) -> Expectation<'_> {
        let expected = self.expected_result.trim();
        if expected == "success" {
            Expectation::Success
        } else if expected == "error" {
            Expectation::Error(None)
        } else if let Some(text) = expected.strip_prefix("error:") {
            Expectation::Error(Some(text.trim()))
        } else {
            Expectation::Invalid
        }
    }
}

enum Expectation<'a> {
    Success,
    Error(Option<&'a str>),
    Invalid,
}

#[derive(Clone, Debug)]
pub struct ScenarioResult {
    pub scenario_id: String,
    pub passed: bool,
    pub failed_step: Option<usize>,
    pub total_steps: usize,
    pub failures: Vec<String>,
}

impl ScenarioResult {
    pub fn assert_passed(&self) {
        assert!(
            self.passed,
            "scenario {} failed at step {:?}: {:?}",
            self.scenario_id, self.failed_step, self.failures
        );
    }
}

#[derive(Default)]
pub struct ScenarioRunner;

impl ScenarioRunner {
    pub fn new() -> Self {
        ScenarioRunner
    }

    /// Check the scenario shape without running it: every step needs an
    /// action and a recognised `expected_result`.
    pub fn validate(&self, scenario: &Scenario) -> ScenarioResult {
        let failures: Vec<(usize, String)> = scenario
            .steps
            .iter()
            .enumerate()
            .filter_map(|(i, step)| {
                if step.action.trim().is_empty() {
                    Some((i, format!("step {i}: empty action")))
                } else if matches!(step.expectation(), Expectation::Invalid) {
                    Some((
                        i,
                        format!(
                            "step {i} ({}): unknown expected_result '{}'",
                            step.action, step.expected_result
                        ),
                    ))
                } else {
                    None
                }
            })
            .collect();
        Self::result(scenario, failures)
    }

    /// Run every step through `handler` and compare its outcome with the
    /// step's `expected_result`. Stops at the first failing step, since later
    /// steps depend on the state earlier ones set up.
    pub fn run<F>(&self, scenario: &Scenario, mut handler: F) -> ScenarioResult
    where
        F: FnMut(&ScenarioStep) -> Result<(), String>,
    {
        let shape = self.validate(scenario);
        if !shape.passed {
            return shape;
        }
        for (i, step) in scenario.steps.iter().enumerate() {
            let outcome = handler(step);
            let failure = match (step.expectation(), outcome) {
                (Expectation::Success, Ok(())) => None,
                (Expectation::Success, Err(e)) => Some(format!("expected success, got error: {e}")),
                (Expectation::Error(_), Ok(())) => Some("expected an error, got success".into()),
                (Expectation::Error(None), Err(_)) => None,
                (Expectation::Error(Some(text)), Err(e)) if e.contains(text) => None,
                (Expectation::Error(Some(text)), Err(e)) => {
                    Some(format!("expected error containing '{text}', got: {e}"))
                }
                (Expectation::Invalid, _) => unreachable!("rejected by validate"),
            };
            if let Some(msg) = failure {
                return Self::result(
                    scenario,
                    vec![(i, format!("step {i} ({}): {msg}", step.action))],
                );
            }
        }
        Self::result(scenario, Vec::new())
    }

    fn result(scenario: &Scenario, failures: Vec<(usize, String)>) -> ScenarioResult {
        ScenarioResult {
            scenario_id: scenario.id.clone(),
            passed: failures.is_empty(),
            failed_step: failures.first().map(|(i, _)| *i),
            total_steps: scenario.steps.len(),
            failures: failures.into_iter().map(|(_, msg)| msg).collect(),
        }
    }
}

/// Built-in scenarios, kept for suites that do not load JSON.
pub mod library {
    use super::*;

    fn step(action: &str, params: &[(&str, &str)]) -> ScenarioStep {
        ScenarioStep {
            action: action.to_string(),
            params: params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            expected_result: "success".to_string(),
        }
    }

    pub fn deposit_borrow_liquidate_repay() -> Scenario {
        Scenario {
            id: "scenario_001".to_string(),
            name: "Deposit, Borrow, Liquidate, Repay".to_string(),
            description: "Full user journey: deposit collateral, borrow assets, trigger liquidation, repay debt"
                .to_string(),
            steps: vec![
                step("deposit", &[("amount", "1000")]),
                step("borrow", &[("amount", "500")]),
                step("liquidate", &[("borrower", "user1")]),
                step("repay", &[("amount", "500")]),
            ],
        }
    }

    pub fn multi_collateral_liquidation() -> Scenario {
        Scenario {
            id: "scenario_002".to_string(),
            name: "Multi-Collateral Liquidation".to_string(),
            description: "User deposits multiple collateral types, borrows, and gets liquidated"
                .to_string(),
            steps: vec![
                step(
                    "deposit_multi",
                    &[("collateral_1", "500"), ("collateral_2", "500")],
                ),
                step("borrow", &[("amount", "800")]),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario(steps: &[(&str, &str)]) -> Scenario {
        Scenario {
            id: "t".into(),
            name: "t".into(),
            description: String::new(),
            steps: steps
                .iter()
                .map(|(action, expected)| ScenarioStep {
                    action: action.to_string(),
                    params: BTreeMap::new(),
                    expected_result: expected.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn bundled_scenarios_parse_and_validate() {
        for file in [
            "deposit-borrow-liquidate-repay.json",
            "lending-journey.json",
        ] {
            let s = Scenario::bundled(file);
            assert!(!s.steps.is_empty(), "{file} has no steps");
            ScenarioRunner::new().validate(&s).assert_passed();
        }
    }

    #[test]
    fn library_scenarios_validate() {
        let runner = ScenarioRunner::new();
        runner
            .validate(&library::deposit_borrow_liquidate_repay())
            .assert_passed();
        runner
            .validate(&library::multi_collateral_liquidation())
            .assert_passed();
    }

    #[test]
    fn runner_matches_outcomes_to_expectations() {
        let s = scenario(&[("ok", "success"), ("bad", "error"), ("cap", "error: cap")]);
        let result = ScenarioRunner::new().run(&s, |step| match step.action.as_str() {
            "ok" => Ok(()),
            "bad" => Err("boom".into()),
            _ => Err("over the cap".into()),
        });
        result.assert_passed();
        assert_eq!(result.total_steps, 3);
    }

    #[test]
    fn runner_stops_at_first_mismatch() {
        let s = scenario(&[("a", "success"), ("b", "success"), ("c", "success")]);
        let mut calls = 0;
        let result = ScenarioRunner::new().run(&s, |step| {
            calls += 1;
            if step.action == "b" {
                Err("rejected".into())
            } else {
                Ok(())
            }
        });
        assert!(!result.passed);
        assert_eq!(result.failed_step, Some(1));
        assert_eq!(calls, 2);
    }

    #[test]
    fn unknown_expectation_is_rejected_before_running() {
        let s = scenario(&[("a", "maybe")]);
        let result = ScenarioRunner::new().run(&s, |_| panic!("must not run"));
        assert!(!result.passed);
        assert!(result.failures[0].contains("unknown expected_result"));
    }

    #[test]
    fn error_text_must_match() {
        let s = scenario(&[("a", "error:paused")]);
        let result = ScenarioRunner::new().run(&s, |_| Err("cap reached".into()));
        assert!(!result.passed);
    }
}
