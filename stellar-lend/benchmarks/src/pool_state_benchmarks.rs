//! # Lazy Pool-State Gas Benchmarks (#721)
//!
//! Measures the cost of the on-demand pool-state snapshot path:
//!
//! * **cold** — first `get_pool_state` call for a pool: lazy initialization
//!   marker write + full on-demand resolve + cache write.
//! * **warm** — subsequent `get_pool_state` call served from the epoch-keyed
//!   snapshot cache.
//! * **invalidation** — `invalidate_pool_state` epoch bump.
//! * **metrics** — `get_pool_state_metrics` read.
//!
//! The cold/warm delta quantifies the caching win; the warm path is the
//! number that must stay well inside the <50ms (instruction-proxied) target.

use crate::framework::{
    fresh_env, get_budget, measure_instructions, BenchmarkResult, BenchmarkSuite, RunConfig,
};
use hello_world::{HelloContract, HelloContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

const CONTRACT: &str = "hello_world";

pub fn register(suite: &mut BenchmarkSuite) {
    suite.register_group("Lazy Pool State (#721)", run_all);
}

fn run_all(config: &RunConfig) -> Vec<BenchmarkResult> {
    vec![
        bench_get_pool_state_cold(config),
        bench_get_pool_state_warm(config),
        bench_invalidate_pool_state(config),
        bench_get_pool_state_metrics(config),
    ]
}

fn setup(env: &Env) -> (HelloContractClient<'static>, Address) {
    let contract_id = env.register(HelloContract, ());
    let client = HelloContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let _ = client.try_initialize(&admin);
    (client, admin)
}

fn bench_get_pool_state_cold(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::get_pool_state";
    let env = fresh_env();
    let (client, _) = setup(&env);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.get_pool_state(&None);
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Lazy pool-state — first load (lazy init marker + on-demand resolve + cache write)",
        insns,
        mem,
        6,
        3,
        true,
        get_budget(config, op),
        vec!["pool-state".into(), "lazy".into(), "cold".into()],
    )
}

fn bench_get_pool_state_warm(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::get_pool_state_warm";
    let env = fresh_env();
    let (client, _) = setup(&env);
    let _ = client.get_pool_state(&None); // materialize + cache

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.get_pool_state(&None); // served from epoch-keyed cache
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Lazy pool-state — cached load (epoch-keyed snapshot hit)",
        insns,
        mem,
        2,
        1,
        false,
        get_budget(config, "hello_world::get_pool_state"),
        vec!["pool-state".into(), "lazy".into(), "warm".into()],
    )
}

fn bench_invalidate_pool_state(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::invalidate_pool_state";
    let env = fresh_env();
    let (client, admin) = setup(&env);
    let _ = client.get_pool_state(&None);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.try_invalidate_pool_state(&admin, &None);
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Lazy pool-state — admin invalidation (global epoch bump)",
        insns,
        mem,
        2,
        2,
        false,
        get_budget(config, op),
        vec!["pool-state".into(), "invalidation".into()],
    )
}

fn bench_get_pool_state_metrics(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::get_pool_state_metrics";
    let env = fresh_env();
    let (client, _) = setup(&env);
    let _ = client.get_pool_state(&None);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.get_pool_state_metrics();
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Lazy pool-state — monitoring counters read",
        insns,
        mem,
        1,
        0,
        false,
        get_budget(config, op),
        vec!["pool-state".into(), "monitoring".into()],
    )
}
