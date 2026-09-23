//! # Governance Contract Gas Benchmarks
//!
//! Measures instruction counts for the governance proposal lifecycle:
//! create → vote → queue → execute/cancel, plus config getters.
//! Registered from `main.rs` as the "Governance Lifecycle" group (Issue #690).

use crate::framework::{
    fresh_env, get_budget, measure_instructions, BenchmarkResult, BenchmarkSuite, RunConfig,
};
use hello_world::{
    types::{ProposalType, VoteType},
    HelloContract, HelloContractClient,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env, String,
};

const CONTRACT: &str = "hello_world";

pub fn register(suite: &mut BenchmarkSuite) {
    suite.register_group("Governance Lifecycle", run_all);
}

fn run_all(config: &RunConfig) -> Vec<BenchmarkResult> {
    vec![
        bench_gov_initialize(config),
        bench_create_proposal(config),
        bench_vote(config),
        bench_queue_proposal(config),
        bench_execute_proposal(config),
        bench_cancel_proposal(config),
        bench_get_proposal(config),
        bench_get_config(config),
    ]
}

// ─── Setup helpers ────────────────────────────────────────────────────────────

fn setup_governance(env: &Env) -> (HelloContractClient<'static>, Address, Address) {
    let contract_id = env.register(HelloContract, ());
    let client = HelloContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let _ = client.try_initialize(&admin);

    let token_id = env.register_stellar_asset_contract(admin.clone());
    let sac = StellarAssetClient::new(env, &token_id);
    let proposer = Address::generate(env);
    let voter = Address::generate(env);
    let _ = sac.mint(&proposer, &1_000_000);
    let _ = sac.mint(&voter, &1_000_000);
    let _ = sac.mint(&admin, &1_000_000);

    let _ = client.try_gov_initialize(
        &admin,
        &token_id,
        &Some(100u64),
        &Some(50u64),
        &Some(4000u32),
        &Some(100i128),
        &Some(200u64),
        &Some(5000i128),
    );

    (client, admin, proposer)
}

fn try_create(client: &HelloContractClient, env: &Env, proposer: &Address) -> u64 {
    let desc = String::from_str(env, "bench proposal");
    client
        .try_gov_create_proposal(
            proposer,
            &ProposalType::MinCollateralRatio(150_000),
            &desc,
            &None,
        )
        .and_then(|r| r)
        .unwrap_or(0)
}

// ─── Benchmarks ───────────────────────────────────────────────────────────────

fn bench_gov_initialize(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::gov_initialize";
    let env = fresh_env();
    let contract_id = env.register(HelloContract, ());
    let client = HelloContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(admin.clone());

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.try_gov_initialize(
            &admin,
            &token_id,
            &Some(100u64),
            &Some(50u64),
            &Some(4000u32),
            &Some(100i128),
            &Some(200u64),
            &Some(5000i128),
        );
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Initialize governance config, vote token, timelock parameters",
        insns,
        mem,
        2,
        3,
        true,
        get_budget(config, op),
        vec!["governance".into(), "init".into()],
    )
}

fn bench_create_proposal(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::gov_create_proposal";
    let env = fresh_env();
    let (client, _admin, proposer) = setup_governance(&env);

    let (insns, mem) = measure_instructions(&env, || {
        let desc = String::from_str(&env, "Raise min collateral ratio");
        let _ = client.try_gov_create_proposal(
            &proposer,
            &ProposalType::MinCollateralRatio(150_000),
            &desc,
            &None,
        );
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Create a governance proposal (threshold check + storage write)",
        insns,
        mem,
        3,
        4,
        true,
        get_budget(config, op),
        vec!["governance".into(), "proposal".into()],
    )
}

fn bench_vote(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::gov_vote";
    let env = fresh_env();
    let (client, _admin, proposer) = setup_governance(&env);
    let proposal_id = try_create(&client, &env, &proposer);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.try_gov_vote(&proposer, &proposal_id, &VoteType::For);
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Cast a vote on a proposal (snapshot power + tally)",
        insns,
        mem,
        4,
        3,
        true,
        get_budget(config, op),
        vec!["governance".into(), "vote".into()],
    )
}

fn bench_queue_proposal(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::gov_queue_proposal";
    let env = fresh_env();
    let (client, admin, proposer) = setup_governance(&env);
    let proposal_id = try_create(&client, &env, &proposer);

    // Advance past voting period so queue is reachable.
    env.ledger().with_mut(|l| l.timestamp += 200);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.try_gov_queue_proposal(&admin, &proposal_id);
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Queue a proposal after voting ends (quorum + threshold tally)",
        insns,
        mem,
        3,
        2,
        true,
        get_budget(config, op),
        vec!["governance".into(), "queue".into()],
    )
}

fn bench_execute_proposal(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::gov_execute_proposal";
    let env = fresh_env();
    let (client, admin, proposer) = setup_governance(&env);
    let proposal_id = try_create(&client, &env, &proposer);

    env.ledger().with_mut(|l| l.timestamp += 200);
    let _ = client.try_gov_queue_proposal(&admin, &proposal_id);
    // Advance past execution delay.
    env.ledger().with_mut(|l| l.timestamp += 100);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.try_gov_execute_proposal(&admin, &proposal_id);
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Execute a queued proposal after execution delay (side-effect apply)",
        insns,
        mem,
        3,
        3,
        true,
        get_budget(config, op),
        vec!["governance".into(), "execute".into()],
    )
}

fn bench_cancel_proposal(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::gov_cancel_proposal";
    let env = fresh_env();
    let (client, _admin, proposer) = setup_governance(&env);
    let proposal_id = try_create(&client, &env, &proposer);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.try_gov_cancel_proposal(&proposer, &proposal_id);
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Cancel a pending proposal by proposer",
        insns,
        mem,
        3,
        1,
        true,
        get_budget(config, op),
        vec!["governance".into(), "cancel".into()],
    )
}

fn bench_get_proposal(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::gov_get_proposal";
    let env = fresh_env();
    let (client, _admin, proposer) = setup_governance(&env);
    let proposal_id = try_create(&client, &env, &proposer);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.gov_get_proposal(&proposal_id);
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Read a proposal by id (view)",
        insns,
        mem,
        1,
        0,
        true,
        get_budget(config, op),
        vec!["governance".into(), "view".into()],
    )
}

fn bench_get_config(config: &RunConfig) -> BenchmarkResult {
    let op = "hello_world::gov_get_governance_config";
    let env = fresh_env();
    let (client, _admin, _proposer) = setup_governance(&env);

    let (insns, mem) = measure_instructions(&env, || {
        let _ = client.gov_get_config();
    });

    BenchmarkResult::new(
        op,
        CONTRACT,
        "Read governance config (view)",
        insns,
        mem,
        1,
        0,
        true,
        get_budget(config, op),
        vec!["governance".into(), "view".into()],
    )
}
