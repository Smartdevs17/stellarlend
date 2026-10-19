//! Gas benchmarks for contract tests.
//!
//! [`GasMeter`] runs each step under a fresh Soroban budget and records CPU
//! instructions, memory bytes and wall time. [`GasBudgets`] loads the
//! per-operation ceilings from `benchmarks/baseline.json` (`gas_budgets`),
//! and [`BenchReport`] writes the measurements as JSON for CI to publish.
//!
//! ```text
//! let budgets = GasBudgets::workspace_baseline();
//! let mut meter = GasMeter::new("deposit_cycle");
//! meter.measure(&env, "deposit", "lending::deposit", || client.deposit(&user, &asset, &1_000));
//! budgets.assert_within(&meter);
//! BenchReport::new(&[meter], &budgets).write(&report_dir("BENCH_REPORT_DIR", default), "deposit.json");
//! ```
//!
//! Budget keys are `<contract>::<function>`. Keys for read-only views
//! (`::get_...`) are measured and reported but not enforced, because clients
//! read views through RPC simulation rather than fee-paying transactions.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use soroban_sdk::Env;

/// Cost of one measured step.
#[derive(Clone, Debug)]
pub struct StepCost {
    pub step: String,
    pub budget_key: String,
    pub cpu: u64,
    pub mem: u64,
    pub wall_us: u128,
}

/// Records the cost of a named sequence of steps (a journey or benchmark).
#[derive(Clone, Debug, Default)]
pub struct GasMeter {
    pub name: String,
    pub steps: Vec<StepCost>,
}

impl GasMeter {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    /// Run `call` under a fresh default budget and record its cost.
    /// `budget_key` names the `gas_budgets` entry the step is checked against.
    /// The budget is left unlimited afterwards so setup code between measured
    /// steps is never metered.
    pub fn measure<T>(
        &mut self,
        env: &Env,
        step: &str,
        budget_key: &str,
        call: impl FnOnce() -> T,
    ) -> T {
        let mut budget = env.cost_estimate().budget();
        budget.reset_default();
        let start = Instant::now();
        let out = call();
        let wall_us = start.elapsed().as_micros();
        let cpu = budget.cpu_instruction_cost();
        let mem = budget.memory_bytes_cost();
        budget.reset_unlimited();
        self.steps.push(StepCost {
            step: step.into(),
            budget_key: budget_key.into(),
            cpu,
            mem,
            wall_us,
        });
        out
    }

    pub fn total_cpu(&self) -> u64 {
        self.steps.iter().map(|s| s.cpu).sum()
    }

    pub fn total_mem(&self) -> u64 {
        self.steps.iter().map(|s| s.mem).sum()
    }

    pub fn total_wall_us(&self) -> u128 {
        self.steps.iter().map(|s| s.wall_us).sum()
    }

    /// CPU cost of every recorded step named `step`.
    pub fn cpu_of(&self, step: &str) -> Vec<u64> {
        self.steps
            .iter()
            .filter(|s| s.step == step)
            .map(|s| s.cpu)
            .collect()
    }
}

/// Read-only entry points: measured and reported, not budget-enforced.
pub fn is_view(budget_key: &str) -> bool {
    budget_key.contains("::get_")
}

/// Per-operation CPU-instruction ceilings, keyed `<contract>::<function>`.
#[derive(Clone, Debug, Default)]
pub struct GasBudgets {
    budgets: BTreeMap<String, u64>,
}

impl GasBudgets {
    /// Path of the shared baseline, `stellar-lend/benchmarks/baseline.json`.
    pub fn workspace_baseline_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/baseline.json")
    }

    /// Budgets from the shared baseline file.
    pub fn workspace_baseline() -> Self {
        Self::load(Self::workspace_baseline_path())
    }

    /// Load the `gas_budgets` object of a baseline JSON file.
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        Self::from_baseline_json(&raw)
    }

    pub fn from_baseline_json(raw: &str) -> Self {
        let json: serde_json::Value = serde_json::from_str(raw).expect("baseline is valid JSON");
        let budgets = json["gas_budgets"]
            .as_object()
            .expect("baseline has gas_budgets")
            .iter()
            .filter_map(|(k, v)| v.as_u64().map(|b| (k.clone(), b)))
            .collect();
        Self { budgets }
    }

    pub fn from_pairs(pairs: &[(&str, u64)]) -> Self {
        Self {
            budgets: pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<u64> {
        self.budgets.get(key).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.budgets.is_empty()
    }

    /// Assert every state-changing step fits its budget. Panics when a step
    /// has no budget entry, so new operations must be given one.
    pub fn assert_within(&self, meter: &GasMeter) {
        for s in meter.steps.iter().filter(|s| !is_view(&s.budget_key)) {
            let budget = self
                .get(&s.budget_key)
                .unwrap_or_else(|| panic!("no gas budget for {}", s.budget_key));
            assert!(
                s.cpu <= budget,
                "{}: step '{}' used {} CPU instructions, over its {} budget of {}",
                meter.name,
                s.step,
                s.cpu,
                s.budget_key,
                budget
            );
        }
    }
}

/// Rule-based optimization hints derived from measured costs; kept
/// deterministic so they are reproducible in CI.
pub fn recommendations(meters: &[GasMeter], budgets: &GasBudgets) -> Vec<String> {
    let mut out = Vec::new();
    for m in meters {
        for s in &m.steps {
            let budget = budgets.get(&s.budget_key).unwrap_or(0);
            if budget > 0 && s.cpu > budget {
                out.push(format!(
                    "{}: step '{}' exceeds its {} budget ({} > {}) — either optimize it or revise the budget in benchmarks/baseline.json",
                    m.name, s.step, s.budget_key, s.cpu, budget
                ));
            } else if budget > 0 && s.cpu * 100 > budget * 80 {
                out.push(format!(
                    "{}: step '{}' uses {}% of its {} budget — profile storage access before adding logic",
                    m.name,
                    s.step,
                    s.cpu * 100 / budget,
                    s.budget_key
                ));
            }
        }
        let total = m.total_cpu().max(1);
        if let Some(top) = m.steps.iter().max_by_key(|s| s.cpu) {
            if top.cpu * 100 / total >= 40 {
                out.push(format!(
                    "{}: '{}' accounts for {}% of journey gas — the highest-leverage optimization target",
                    m.name,
                    top.step,
                    top.cpu * 100 / total
                ));
            }
        }
    }
    out
}

/// `$var` if set, otherwise `default`.
pub fn report_dir(var: &str, default: impl Into<PathBuf>) -> PathBuf {
    std::env::var(var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| default.into())
}

/// JSON report of one or more meters against a set of budgets.
pub struct BenchReport<'a> {
    meters: &'a [GasMeter],
    budgets: &'a GasBudgets,
    extra: Vec<String>,
}

impl<'a> BenchReport<'a> {
    pub fn new(meters: &'a [GasMeter], budgets: &'a GasBudgets) -> Self {
        Self {
            meters,
            budgets,
            extra: Vec::new(),
        }
    }

    /// Add suite-specific recommendations to the generic ones.
    pub fn with_recommendations(mut self, extra: Vec<String>) -> Self {
        self.extra.extend(extra);
        self
    }

    pub fn to_json(&self) -> serde_json::Value {
        let budgets = self.budgets;
        let mut recs = recommendations(self.meters, budgets);
        recs.extend(self.extra.iter().cloned());
        recs.sort();
        recs.dedup();
        serde_json::json!({
            "journeys": self.meters.iter().map(|m| serde_json::json!({
                "name": m.name,
                "total_cpu_instructions": m.total_cpu(),
                "total_memory_bytes": m.total_mem(),
                "total_wall_us": m.total_wall_us() as u64,
                "steps": m.steps.iter().map(|s| serde_json::json!({
                    "step": s.step,
                    "budget_key": s.budget_key,
                    "budget": budgets.get(&s.budget_key).unwrap_or(0),
                    "cpu_instructions": s.cpu,
                    "memory_bytes": s.mem,
                    "wall_us": s.wall_us as u64,
                    "enforced": !is_view(&s.budget_key),
                    "over_budget": budgets.get(&s.budget_key).is_some_and(|b| s.cpu > b),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "recommendations": recs,
        })
    }

    /// Write the report to `dir/file`, creating `dir` if needed.
    pub fn write(&self, dir: &Path, file: &str) -> PathBuf {
        std::fs::create_dir_all(dir).expect("create report dir");
        let path = dir.join(file);
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&self.to_json()).unwrap(),
        )
        .expect("write bench report");
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meter(steps: &[(&str, &str, u64)]) -> GasMeter {
        GasMeter {
            name: "m".into(),
            steps: steps
                .iter()
                .map(|(step, key, cpu)| StepCost {
                    step: step.to_string(),
                    budget_key: key.to_string(),
                    cpu: *cpu,
                    mem: 1,
                    wall_us: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn parses_gas_budgets_from_baseline() {
        let budgets = GasBudgets::from_baseline_json(
            r#"{"gas_budgets": {"lending::deposit": 800000, "note": "x"}}"#,
        );
        assert_eq!(budgets.get("lending::deposit"), Some(800_000));
        assert_eq!(budgets.get("note"), None);
    }

    #[test]
    fn workspace_baseline_has_lending_budgets() {
        let budgets = GasBudgets::workspace_baseline();
        assert!(budgets.get("lending::deposit").is_some());
    }

    #[test]
    fn views_are_not_enforced() {
        let budgets = GasBudgets::from_pairs(&[("c::op", 100), ("c::get_x", 10)]);
        budgets.assert_within(&meter(&[("op", "c::op", 100), ("view", "c::get_x", 50)]));
    }

    #[test]
    #[should_panic(expected = "over its c::op budget")]
    fn over_budget_step_fails() {
        let budgets = GasBudgets::from_pairs(&[("c::op", 100)]);
        budgets.assert_within(&meter(&[("op", "c::op", 101)]));
    }

    #[test]
    #[should_panic(expected = "no gas budget for c::missing")]
    fn step_without_budget_fails() {
        GasBudgets::default().assert_within(&meter(&[("op", "c::missing", 1)]));
    }

    #[test]
    fn report_flags_over_budget_and_hot_steps() {
        let budgets = GasBudgets::from_pairs(&[("c::a", 100), ("c::b", 100)]);
        let meters = [meter(&[("a", "c::a", 150), ("b", "c::b", 10)])];
        let json = BenchReport::new(&meters, &budgets)
            .with_recommendations(vec!["extra".into()])
            .to_json();
        let steps = json["journeys"][0]["steps"].as_array().unwrap();
        assert_eq!(steps[0]["over_budget"], true);
        assert_eq!(steps[1]["over_budget"], false);
        let recs: Vec<&str> = json["recommendations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        assert!(recs.iter().any(|r| r.contains("exceeds its c::a budget")));
        assert!(recs.iter().any(|r| r.contains("highest-leverage")));
        assert!(recs.contains(&"extra"));
    }
}
