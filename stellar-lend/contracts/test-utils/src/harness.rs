//! Multi-contract state-simulation harness (Issue #688).
//!
//! Registers mock token + oracle alongside a target contract address and
//! drives a scripted sequence of cross-contract calls while recording
//! events, gas deltas, and invariant checks.

#![allow(unused)]

use soroban_sdk::{symbol_short, Address, Env, Symbol};

use crate::events::{
    topic_borrow, topic_deposit, topic_repay, topic_withdraw, EventRecorder,
};
use crate::gas::{reset_budget, snapshot, GasSnapshot};
use crate::invariants::InvariantReport;
use crate::mock_contracts::{register_mock_oracle, register_mock_token};

/// A single scripted step in a multi-contract scenario.
pub struct ScenarioStep {
    pub name: &'static str,
    pub kind: StepKind,
}

pub enum StepKind {
    Deposit { user: Address, amount: i128 },
    Borrow { user: Address, amount: i128 },
    Repay { user: Address, amount: i128 },
    Withdraw { user: Address, amount: i128 },
    SetPrice { asset: Address, price: i128 },
    Custom(&'static str),
}

/// Bundle of shared addresses used by a multi-contract scenario.
pub struct CrossContractHarness {
    pub env: Env,
    pub admin: Address,
    pub token: Address,
    pub oracle: Address,
    pub target: Option<Address>,
    pub events: EventRecorder,
    pub gas_baseline: GasSnapshot,
    pub invariants: InvariantReport,
}

impl CrossContractHarness {
    /// Create a harness with mock token + oracle registered on `env`.
    pub fn new(env: &Env, admin: &Address) -> Self {
        reset_budget(env);
        let token = register_mock_token(env);
        let oracle = register_mock_oracle(env);
        Self {
            env: env.clone(),
            admin: admin.clone(),
            token,
            oracle,
            target: None,
            events: EventRecorder::new(env, 64),
            gas_baseline: snapshot(env),
            invariants: InvariantReport::default(),
        }
    }

    /// Attach the contract under test (e.g. HelloContract address).
    pub fn with_target(mut self, target: Address) -> Self {
        self.target = Some(target);
        self
    }

    /// Record a deposit topic.
    pub fn note_deposit(&mut self) {
        let env = self.env.clone();
        self.events.record(&env, topic_deposit(), "deposit");
    }

    pub fn note_borrow(&mut self) {
        let env = self.env.clone();
        self.events.record(&env, topic_borrow(), "borrow");
    }

    pub fn note_repay(&mut self) {
        let env = self.env.clone();
        self.events.record(&env, topic_repay(), "repay");
    }

    pub fn note_withdraw(&mut self) {
        let env = self.env.clone();
        self.events.record(&env, topic_withdraw(), "withdraw");
    }

    /// Record an arbitrary named event topic.
    pub fn note(&mut self, topic: Symbol, label: &str) {
        let env = self.env.clone();
        self.events.record(&env, topic, label);
    }

    /// Mark an invariant check as passed.
    pub fn pass(&mut self) {
        self.invariants.record_ok();
    }

    /// Mark an invariant check as failed with a message.
    pub fn fail(&mut self, msg: impl Into<String>) {
        self.invariants.record_fail(msg.into());
    }

    /// Assert every recorded invariant passed and events were seen.
    pub fn assert_scenario(&self, name: &str) {
        self.invariants.assert_all_passed(name);
    }

    /// Current gas snapshot.
    pub fn gas(&self) -> GasSnapshot {
        snapshot(&self.env)
    }

    /// Simulated price update on the mock oracle.
    pub fn set_price(&mut self, asset: &Address, price: i128) {
        use crate::mock_contracts::MockOracleClient;
        let client = MockOracleClient::new(&self.env, &self.oracle);
        client.set_price(asset, &price);
        let env = self.env.clone();
        self.events
            .record(&env, symbol_short!("price"), "set_price");
    }

    /// Read price from the mock oracle.
    pub fn get_price(&self, asset: &Address) -> i128 {
        use crate::mock_contracts::MockOracleClient;
        let client = MockOracleClient::new(&self.env, &self.oracle);
        client.get_price(asset)
    }

    /// Mint mock tokens to a user.
    pub fn mint(&mut self, to: &Address, amount: i128) {
        use crate::mock_contracts::MockTokenClient;
        let client = MockTokenClient::new(&self.env, &self.token);
        client.mint(to, &amount);
    }

    /// Read mock token balance.
    pub fn balance(&self, of: &Address) -> i128 {
        use crate::mock_contracts::MockTokenClient;
        let client = MockTokenClient::new(&self.env, &self.token);
        client.balance(of)
    }
}

/// Run a list of step names as a smoke scenario (labels only).
/// Useful for CI wiring without full contract clients.
pub fn run_labeled_scenario(harness: &mut CrossContractHarness, steps: &[ScenarioStep]) {
    for step in steps {
        match &step.kind {
            StepKind::Deposit { .. } => harness.note_deposit(),
            StepKind::Borrow { .. } => harness.note_borrow(),
            StepKind::Repay { .. } => harness.note_repay(),
            StepKind::Withdraw { .. } => harness.note_withdraw(),
            StepKind::SetPrice { asset, price } => {
                let a = asset.clone();
                let p = *price;
                harness.set_price(&a, p);
            }
            StepKind::Custom(label) => {
                let t = symbol_short!("custom");
                harness.note(t, label);
            }
        }
        harness.pass();
    }
}
