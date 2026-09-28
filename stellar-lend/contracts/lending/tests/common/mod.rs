//! Shared fixture for the lending integration suites (`fuzz_invariants.rs`,
//! `user_journeys.rs`, `scenarios.rs`, ...).
//!
//! These suites live under `tests/` so they compile against the public
//! contract surface only — the same surface wallets, the API and the
//! benchmark harness use — rather than crate internals. Environment setup,
//! the price oracle and gas metering come from the shared `test-utils` crate;
//! this module only adds what is specific to the lending contract.

#![allow(dead_code)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use stellarlend_lending::{LendingContract, LendingContractClient};
use test_utils::{register_price_oracle, snapshotless_env, PriceOracleClient};

/// Oracle price scale used by the lending views (8 decimals).
pub const PRICE_SCALE: i128 = test_utils::ORACLE_PRICE_SCALE;
/// Health factor scale (10_000 = 1.0).
pub const HF_SCALE: i128 = 10_000;
/// Sentinel returned by `get_health_factor` for debt-free positions.
pub const HF_NO_DEBT: i128 = 100_000_000;
/// Minimum collateral ratio enforced by `borrow` (150%).
pub const COLLATERAL_RATIO_BPS: i128 = 15_000;

pub const DEBT_CEILING: i128 = 1_000_000_000;
pub const MIN_BORROW: i128 = 100;
pub const DEPOSIT_CAP: i128 = 1_000_000_000;
pub const MIN_DEPOSIT: i128 = 100;
pub const MIN_WITHDRAW: i128 = 100;

pub struct Fixture<'a> {
    pub env: Env,
    pub client: LendingContractClient<'a>,
    pub admin: Address,
    pub oracle: Address,
    pub collateral_asset: Address,
    pub debt_asset: Address,
}

impl Fixture<'_> {
    pub fn oracle_client(&self) -> PriceOracleClient<'_> {
        PriceOracleClient::new(&self.env, &self.oracle)
    }
}

/// Register a lending contract and call `initialize` with the suite
/// defaults. Auths are left as the caller configured them, so authorization
/// suites can deploy without mocking.
pub fn deploy<'a>(env: &Env) -> (LendingContractClient<'a>, Address) {
    let contract_id = env.register(LendingContract, ());
    let client = LendingContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin, &DEBT_CEILING, &MIN_BORROW);
    (client, admin)
}

/// Deploy and fully initialize a lending contract with a flat 1.0 oracle.
/// Keeping prices flat isolates protocol accounting from market moves, so any
/// health factor drop observed by the suites comes from the contract itself;
/// tests that need a price move set it through `oracle_client()`.
pub fn setup<'a>() -> Fixture<'a> {
    // Snapshot capture is off: property runs create thousands of envs and
    // would otherwise write a snapshot file per env into test_snapshots/.
    let env = snapshotless_env();
    env.mock_all_auths();
    // Property runs execute hundreds of calls per case; metering limits are
    // exercised separately by the journey budget tests.
    env.cost_estimate().budget().reset_unlimited();

    let (client, admin) = deploy(&env);
    client.initialize_deposit_settings(&DEPOSIT_CAP, &MIN_DEPOSIT);
    client.initialize_withdraw_settings(&MIN_WITHDRAW);

    let oracle = register_price_oracle(&env, PRICE_SCALE);
    client.set_oracle(&admin, &oracle);

    Fixture {
        collateral_asset: Address::generate(&env),
        debt_asset: Address::generate(&env),
        env,
        client,
        admin,
        oracle,
    }
}

/// Observable per-user state used for invariant and atomicity checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserSnapshot {
    pub deposit: i128,
    pub borrow_collateral: i128,
    pub principal: i128,
    pub debt_balance: i128,
}

pub fn snapshot(f: &Fixture, user: &Address) -> UserSnapshot {
    let debt = f.client.get_user_debt(user);
    UserSnapshot {
        deposit: f
            .client
            .get_user_collateral_deposit(user, &f.collateral_asset)
            .amount,
        borrow_collateral: f.client.get_user_collateral(user).amount,
        principal: debt.borrowed_amount,
        debt_balance: f.client.get_debt_balance(user),
    }
}
