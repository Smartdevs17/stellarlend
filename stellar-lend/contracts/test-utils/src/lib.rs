//! Unified test framework for StellarLend contracts.
//!
//! | Module | Provides |
//! |---|---|
//! | [`environment`] | `TestEnv`, snapshot-free envs, ledger time helpers |
//! | [`fixtures`] | value fixtures and `ProtocolFixture` (users, tokens, oracle) |
//! | [`seeding`] | deterministic `SeedRng` and `Seeder` for test data |
//! | [`mock_contracts`] | mock token, mock oracle and the lending `PriceOracle` |
//! | [`scenario`] | JSON-driven scenarios executed against real contracts |
//! | [`bench`] | per-step gas metering, budgets and JSON reports |
//! | [`gas`] | low-level budget snapshots |
//! | [`assertions`], [`invariants`], [`reference`] | domain assertions and reference math |
//! | [`events`], [`harness`] | event recording and the cross-contract harness |
//! | [`suite`], [`edge_cases`] | test registry and the edge-case catalog |
//!
//! See `stellar-lend/TESTING.md` for how suites are organised and run.

pub mod assertions;
pub mod bench;
pub mod edge_cases;
pub mod environment;
pub mod events;
pub mod fixtures;
pub mod gas;
pub mod harness;
pub mod invariants;
pub mod mock_contracts;
pub mod reference;
pub mod scenario;
pub mod seeding;
pub mod suite;

pub use assertions::*;
pub use bench::{BenchReport, GasBudgets, GasMeter, StepCost};
pub use edge_cases::{EdgeCase, EdgeCaseCatalog};
pub use environment::*;
pub use events::*;
pub use fixtures::*;
pub use gas::*;
pub use harness::*;
pub use invariants::*;
pub use mock_contracts::*;
pub use reference::*;
pub use scenario::{Scenario, ScenarioResult, ScenarioRunner};
pub use seeding::*;
pub use suite::*;
