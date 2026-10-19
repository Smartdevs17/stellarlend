#![cfg(test)]

use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env, Vec};

use crate::{
    EarningsReinvestContract, EarningsReinvestContractClient, ReinvestError, ReinvestSchedule,
    ReinvestStrategy, WeightedTarget,
};

fn setup() -> (
    Env,
    Address,
    Address,
    Address,
    EarningsReinvestContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let contract_id = env.register(EarningsReinvestContract, ());
    let client = EarningsReinvestContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, owner, pool, client)
}

#[test]
fn test_initialize_cannot_double_init() {
    let (_, admin, _, _, client) = setup();
    let result = client.try_initialize(&admin);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ReinvestError::AlreadyInitialized
    );
}

#[test]
fn test_create_plan_same_pool() {
    let (env, _, owner, pool, client) = setup();
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &100,
        &Vec::new(&env),
    );
    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.owner, owner);
    assert_eq!(plan.source_pool, pool);
    assert_eq!(plan.threshold, 100);
    assert!(!plan.paused);
}

#[test]
fn test_create_plan_weighted_requires_full_bps() {
    let (env, _, owner, pool, client) = setup();
    let pool_b = Address::generate(&env);
    let mut targets = Vec::new(&env);
    targets.push_back(WeightedTarget {
        pool: pool.clone(),
        weight_bps: 6_000,
    });
    targets.push_back(WeightedTarget {
        pool: pool_b,
        weight_bps: 3_000,
    });

    let result = client.try_create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::Weighted,
        &ReinvestSchedule::RealTime,
        &0,
        &targets,
    );
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::InvalidWeights);
}

#[test]
fn test_create_plan_rejects_weighted_targets_for_same_pool_strategy() {
    let (env, _, owner, pool, client) = setup();
    let mut targets = Vec::new(&env);
    targets.push_back(WeightedTarget {
        pool: pool.clone(),
        weight_bps: 10_000,
    });

    let result = client.try_create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &0,
        &targets,
    );
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::InvalidWeights);
}

#[test]
fn test_sweep_same_pool_success() {
    let (env, _, owner, pool, client) = setup();
    let keeper = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &100,
        &Vec::new(&env),
    );

    let events = client.sweep(&keeper, &plan_id, &pool, &500, &false, &10);
    assert_eq!(events.len(), 1);
    let event = events.get(0).unwrap();
    assert_eq!(event.pool, pool);
    assert_eq!(event.amount, 500);
    assert_eq!(event.cost_basis, 500);

    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.total_reinvested, 500);
    assert_eq!(plan.total_sweeps, 1);

    let history = client.get_history(&plan_id);
    assert_eq!(history.len(), 1);
}

#[test]
fn test_sweep_weighted_split_covers_full_amount() {
    let (env, _, owner, pool_a, client) = setup();
    let keeper = Address::generate(&env);
    let pool_b = Address::generate(&env);
    let mut targets = Vec::new(&env);
    targets.push_back(WeightedTarget {
        pool: pool_a.clone(),
        weight_bps: 3_333,
    });
    targets.push_back(WeightedTarget {
        pool: pool_b.clone(),
        weight_bps: 6_667,
    });

    let plan_id = client.create_plan(
        &owner,
        &pool_a,
        &ReinvestStrategy::Weighted,
        &ReinvestSchedule::RealTime,
        &0,
        &targets,
    );

    let events = client.sweep(&keeper, &plan_id, &pool_a, &1_000, &false, &1);
    assert_eq!(events.len(), 2);
    let total: i128 = events.iter().map(|e| e.amount).sum();
    assert_eq!(total, 1_000); // no dust lost to rounding
}

#[test]
fn test_sweep_below_threshold_rejected() {
    let (env, _, owner, pool, client) = setup();
    let keeper = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &1_000,
        &Vec::new(&env),
    );

    let result = client.try_sweep(&keeper, &plan_id, &pool, &50, &false, &1);
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::BelowThreshold);
}

#[test]
fn test_sweep_rejected_when_pool_paused() {
    let (env, _, owner, pool, client) = setup();
    let keeper = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &0,
        &Vec::new(&env),
    );

    let result = client.try_sweep(&keeper, &plan_id, &pool, &500, &true, &1);
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::PoolPaused);
}

#[test]
fn test_sweep_rejected_when_gas_exceeds_earnings() {
    let (env, _, owner, pool, client) = setup();
    let keeper = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &0,
        &Vec::new(&env),
    );

    let result = client.try_sweep(&keeper, &plan_id, &pool, &100, &false, &100);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ReinvestError::GasExceedsEarnings
    );
}

#[test]
fn test_sweep_respects_daily_schedule_cadence() {
    let (env, _, owner, pool, client) = setup();
    let keeper = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::Daily,
        &0,
        &Vec::new(&env),
    );

    client.sweep(&keeper, &plan_id, &pool, &500, &false, &1);
    let result = client.try_sweep(&keeper, &plan_id, &pool, &500, &false, &1);
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::ScheduleNotDue);

    env.ledger().with_mut(|l| l.sequence_number += 17_280 + 1);
    let events = client.sweep(&keeper, &plan_id, &pool, &500, &false, &1);
    assert_eq!(events.len(), 1);
}

#[test]
fn test_pause_blocks_sweep_and_resume_restores_it() {
    let (env, _, owner, pool, client) = setup();
    let keeper = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &0,
        &Vec::new(&env),
    );

    client.pause(&owner, &plan_id);
    let result = client.try_sweep(&keeper, &plan_id, &pool, &500, &false, &1);
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::PlanPaused);

    // Strategy is untouched by pause/resume.
    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.strategy, ReinvestStrategy::SamePool);

    client.resume(&owner, &plan_id);
    let events = client.sweep(&keeper, &plan_id, &pool, &500, &false, &1);
    assert_eq!(events.len(), 1);
}

#[test]
fn test_pause_by_non_owner_rejected() {
    let (env, _, owner, pool, client) = setup();
    let intruder = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &0,
        &Vec::new(&env),
    );

    let result = client.try_pause(&intruder, &plan_id);
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::Unauthorized);
}

#[test]
fn test_get_user_plans_lists_created_plans() {
    let (env, _, owner, pool, client) = setup();
    let plan_a = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &0,
        &Vec::new(&env),
    );
    let plan_b = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::Weekly,
        &0,
        &Vec::new(&env),
    );

    let plans = client.get_user_plans(&owner);
    assert_eq!(plans.len(), 2);
    assert_eq!(plans.get(0).unwrap(), plan_a);
    assert_eq!(plans.get(1).unwrap(), plan_b);
}

#[test]
fn test_sweep_rejects_zero_or_negative_earned_even_with_zero_threshold() {
    let (env, _, owner, pool, client) = setup();
    let keeper = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::Daily,
        &0,
        &Vec::new(&env),
    );

    // Attempt zero earnings sweep
    let result_zero = client.try_sweep(&keeper, &plan_id, &pool, &0, &false, &0);
    assert_eq!(result_zero.unwrap_err().unwrap(), ReinvestError::BelowThreshold);

    // Attempt negative earnings sweep
    let result_neg = client.try_sweep(&keeper, &plan_id, &pool, &-100, &false, &0);
    assert_eq!(result_neg.unwrap_err().unwrap(), ReinvestError::BelowThreshold);

    // Verify schedule was not locked by the rejected zero/negative sweep
    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.total_sweeps, 0);
    assert_eq!(plan.next_eligible_ledger, 0);

    // Valid positive sweep succeeds immediately
    let events = client.sweep(&keeper, &plan_id, &pool, &100, &false, &10);
    assert_eq!(events.len(), 1);
}

#[test]
fn test_sweep_rejects_negative_gas_cost() {
    let (env, _, owner, pool, client) = setup();
    let keeper = Address::generate(&env);
    let plan_id = client.create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::SamePool,
        &ReinvestSchedule::RealTime,
        &0,
        &Vec::new(&env),
    );

    let result = client.try_sweep(&keeper, &plan_id, &pool, &100, &false, &-1);
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::GasExceedsEarnings);
}

#[test]
fn test_create_plan_rejects_zero_weight_target() {
    let (env, _, owner, pool, client) = setup();
    let pool_b = Address::generate(&env);
    let mut targets = Vec::new(&env);
    targets.push_back(WeightedTarget {
        pool: pool.clone(),
        weight_bps: 10_000,
    });
    targets.push_back(WeightedTarget {
        pool: pool_b,
        weight_bps: 0,
    });

    let result = client.try_create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::Weighted,
        &ReinvestSchedule::RealTime,
        &0,
        &targets,
    );
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::InvalidWeights);
}

#[test]
fn test_create_plan_rejects_duplicate_target_pools() {
    let (env, _, owner, pool, client) = setup();
    let mut targets = Vec::new(&env);
    targets.push_back(WeightedTarget {
        pool: pool.clone(),
        weight_bps: 5_000,
    });
    targets.push_back(WeightedTarget {
        pool: pool.clone(),
        weight_bps: 5_000,
    });

    let result = client.try_create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::Weighted,
        &ReinvestSchedule::RealTime,
        &0,
        &targets,
    );
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::InvalidWeights);
}

#[test]
fn test_create_plan_rejects_excessive_targets() {
    let (env, _, owner, pool, client) = setup();
    let mut targets = Vec::new(&env);
    for _ in 0..21 {
        targets.push_back(WeightedTarget {
            pool: Address::generate(&env),
            weight_bps: 400,
        });
    }

    let result = client.try_create_plan(
        &owner,
        &pool,
        &ReinvestStrategy::Weighted,
        &ReinvestSchedule::RealTime,
        &0,
        &targets,
    );
    assert_eq!(result.unwrap_err().unwrap(), ReinvestError::InvalidWeights);
}

#[test]
fn test_sweep_best_apy_strategy_success() {
    let (env, _, owner, source_pool, client) = setup();
    let keeper = Address::generate(&env);
    let target_pool = Address::generate(&env);

    let plan_id = client.create_plan(
        &owner,
        &source_pool,
        &ReinvestStrategy::BestApy,
        &ReinvestSchedule::RealTime,
        &50,
        &Vec::new(&env),
    );

    let events = client.sweep(&keeper, &plan_id, &target_pool, &250, &false, &5);
    assert_eq!(events.len(), 1);
    let event = events.get(0).unwrap();
    assert_eq!(event.pool, target_pool);
    assert_eq!(event.amount, 250);
    assert_eq!(event.cost_basis, 250);

    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.total_reinvested, 250);
    assert_eq!(plan.total_sweeps, 1);
}
