//! Shared fixture for the lending integration suites (`fuzz_invariants.rs`,
//! `user_journeys.rs`).
//!
//! These suites live under `tests/` so they compile against the public
//! contract surface only — the same surface wallets, the API and the
//! benchmark harness use — rather than crate internals.

#![allow(dead_code)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, EnvTestConfig},
    Address, Env,
};
use stellarlend_lending::{LendingContract, LendingContractClient};

/// Oracle price scale used by the lending views (8 decimals).
pub const PRICE_SCALE: i128 = 100_000_000;
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

/// Flat 1.0 price oracle: every asset is worth exactly one unit. Keeping the
/// price flat isolates protocol accounting from market moves, so any health
/// factor drop observed by the suites comes from the contract itself.
#[contract]
pub struct FlatOracle;

#[contractimpl]
impl FlatOracle {
    pub fn price(_env: Env, _asset: Address) -> i128 {
        PRICE_SCALE
    }
}

pub struct Fixture<'a> {
    pub env: Env,
    pub client: LendingContractClient<'a>,
    pub admin: Address,
    pub collateral_asset: Address,
    pub debt_asset: Address,
}

/// Deploy and fully initialize a lending contract with a flat oracle.
pub fn setup<'a>() -> Fixture<'a> {
    // Snapshot capture is off: property runs create thousands of envs and
    // would otherwise write a snapshot file per env into test_snapshots/.
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    // Property runs execute hundreds of calls per case; metering limits are
    // exercised separately by the journey budget tests.
    env.cost_estimate().budget().reset_unlimited();

    let contract_id = env.register(LendingContract, ());
    let client = LendingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin, &DEBT_CEILING, &MIN_BORROW);
    client.initialize_deposit_settings(&DEPOSIT_CAP, &MIN_DEPOSIT);
    client.initialize_withdraw_settings(&MIN_WITHDRAW);

    let oracle = env.register(FlatOracle, ());
    client.set_oracle(&admin, &oracle);

    Fixture {
        collateral_asset: Address::generate(&env),
        debt_asset: Address::generate(&env),
        env,
        client,
        admin,
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
