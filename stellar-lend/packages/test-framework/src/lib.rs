use serde::{Deserialize, Serialize};
use soroban_sdk::Env;

pub mod fixtures;
pub mod helpers;
pub mod scenarios;
pub mod gas_benchmark;
pub mod edge_cases;

pub use fixtures::{ContractFixture, FixtureBuilder};
pub use helpers::*;
pub use scenarios::{Scenario, ScenarioRunner};
pub use gas_benchmark::{GasBenchmark, GasReport};
pub use edge_cases::{EdgeCase, EdgeCaseCatalog};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestConfig {
    pub network: String,
    pub admin: String,
    pub governance: String,
    pub oracle_addresses: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub gas_used: Option<u64>,
}

pub trait TestCase {
    fn setup(&mut self, env: &Env);
    fn run(&mut self, env: &Env) -> Result<(), String>;
    fn teardown(&mut self, env: &Env);
}

pub struct TestSuite {
    pub name: String,
    pub tests: Vec<Box<dyn TestCase>>,
}

impl TestSuite {
    pub fn new(name: &str) -> Self {
        TestSuite {
            name: name.to_string(),
            tests: Vec::new(),
        }
    }

    pub fn add_test(&mut self, test: Box<dyn TestCase>) {
        self.tests.push(test);
    }

    pub fn run(&self, env: &Env) -> Vec<TestResult> {
        let mut results = Vec::new();

        for test in &self.tests {
            let mut test_case = test;
            test_case.setup(env);

            let result = match test_case.run(env) {
                Ok(()) => TestResult {
                    name: "test".to_string(),
                    passed: true,
                    message: "Passed".to_string(),
                    gas_used: None,
                },
                Err(e) => TestResult {
                    name: "test".to_string(),
                    passed: false,
                    message: e,
                    gas_used: None,
                },
            };

            test_case.teardown(env);
            results.push(result);
        }

        results
    }
}
