#![cfg(test)]

use crate::{
    DcaDirection, DcaError, DcaExecution, DcaFrequency, DcaModule, DcaModuleClient, DcaPlanStatus,
};
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

fn setup() -> (Env, DcaModuleClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(1_000);

    let contract_id = env.register(DcaModule, ());
    let client = DcaModuleClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let asset = env.register_stellar_asset_contract_v2(token_admin).address();
    let token = soroban_sdk::token::StellarAssetClient::new(&env, &asset);
    token.mint(&owner, &1_000_000);

    client.initialize(&admin);

    (env, client, admin, owner, asset)
}

#[test]
fn test_initialize_guards() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DcaModule, ());
    let client = DcaModuleClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // First init succeeds
    assert!(client.try_initialize(&admin).is_ok());

    // Second init fails with AlreadyInitialized
    let res = client.try_initialize(&admin);
    assert_eq!(res, Err(Ok(DcaError::AlreadyInitialized)));
}

#[test]
fn test_create_plan_validation() {
    let (_env, client, _admin, owner, asset) = setup();

    // Zero amount per execution
    let res = client.try_create_plan(
        &owner,
        &asset,
        &0,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &10,
        &1_000,
    );
    assert_eq!(res, Err(Ok(DcaError::InvalidAmount)));

    // Negative amount per execution
    let res = client.try_create_plan(
        &owner,
        &asset,
        &-50,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &10,
        &1_000,
    );
    assert_eq!(res, Err(Ok(DcaError::InvalidAmount)));

    // Zero max executions
    let res = client.try_create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &0,
        &1_000,
    );
    assert_eq!(res, Err(Ok(DcaError::InvalidFrequency)));

    // Funded amount <= 0
    let res = client.try_create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &10,
        &0,
    );
    assert_eq!(res, Err(Ok(DcaError::InsufficientFunds)));

    // Funded amount < amount_per_execution
    let res = client.try_create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &10,
        &99,
    );
    assert_eq!(res, Err(Ok(DcaError::InsufficientFunds)));

    // Uninitialized call
    let uninit_env = Env::default();
    uninit_env.mock_all_auths();
    let uninit_id = uninit_env.register(DcaModule, ());
    let uninit_client = DcaModuleClient::new(&uninit_env, &uninit_id);
    let uninit_owner = Address::generate(&uninit_env);
    let uninit_asset = Address::generate(&uninit_env);
    let res = uninit_client.try_create_plan(
        &uninit_owner,
        &uninit_asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &10,
        &1_000,
    );
    assert_eq!(res, Err(Ok(DcaError::NotInitialized)));
}

#[test]
fn test_create_plan_success() {
    let (_env, client, _admin, owner, asset) = setup();

    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &10,
        &1_000,
    );

    assert_eq!(plan_id, 1);

    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.id, 1);
    assert_eq!(plan.owner, owner);
    assert_eq!(plan.asset, asset);
    assert_eq!(plan.amount_per_execution, 100);
    assert_eq!(plan.frequency, DcaFrequency::Daily);
    assert_eq!(plan.direction, DcaDirection::Buy);
    assert_eq!(plan.total_executions, 0);
    assert_eq!(plan.max_executions, 10);
    assert_eq!(plan.funded_amount, 1_000);
    assert_eq!(plan.spent_amount, 0);
    assert_eq!(plan.status, DcaPlanStatus::Active);
    assert_eq!(plan.created_at, 1_000);
    assert_eq!(plan.next_execution_ledger, 1_000 + 17_280);

    let user_plans = client.get_user_plans(&owner);
    assert_eq!(user_plans.len(), 1);
    assert_eq!(user_plans.get(0), Some(1));
}

#[test]
fn test_execute_due_and_not_due() {
    let (env, client, _admin, owner, asset) = setup();
    let keeper = Address::generate(&env);

    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &5,
        &500,
    );

    // Not due yet (current ledger 1000, due at 18280)
    let res = client.try_execute(&keeper, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::ExecutionNotDue)));

    // Fast forward to exactly due ledger
    env.ledger().set_sequence_number(18_280);
    let exec: DcaExecution = client.execute(&keeper, &plan_id);
    assert_eq!(exec.plan_id, plan_id);
    assert_eq!(exec.execution_number, 1);
    assert_eq!(exec.amount, 100);
    assert_eq!(exec.executed_at, 18_280);

    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.total_executions, 1);
    assert_eq!(plan.spent_amount, 100);
    assert_eq!(plan.next_execution_ledger, 18_280 + 17_280);
}

#[test]
fn test_anti_drift_scheduling() {
    let (env, client, _admin, owner, asset) = setup();
    let keeper = Address::generate(&env);

    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &5,
        &500,
    );

    // Initial due ledger: 1_000 + 17_280 = 18_280
    // Suppose keeper runs 500 ledgers late (at 18_780)
    env.ledger().set_sequence_number(18_780);
    client.execute(&keeper, &plan_id);

    let plan = client.get_plan(&plan_id).unwrap();
    // Anti-drift invariant: next execution ledger MUST anchor to planned interval (18_280 + 17_280 = 35_560),
    // NOT current_ledger + 17_280 (18_780 + 17_280 = 36_060)!
    assert_eq!(plan.next_execution_ledger, 18_280 + 17_280);

    // Next execution: runs on schedule at 35_560
    env.ledger().set_sequence_number(35_560);
    client.execute(&keeper, &plan_id);

    let plan2 = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan2.next_execution_ledger, 35_560 + 17_280);

    // Now test severe delay (keeper delayed by > 1 full interval: 35_560 + 17_280 = 52_840, but executed at 80_000)
    env.ledger().set_sequence_number(80_000);
    client.execute(&keeper, &plan_id);

    let plan3 = client.get_plan(&plan_id).unwrap();
    // In severe delay (> full interval), schedule catches up to current_ledger + interval to prevent past schedules
    assert_eq!(plan3.next_execution_ledger, 80_000 + 17_280);
}

#[test]
fn test_execute_until_completion() {
    let (env, client, _admin, owner, asset) = setup();
    let keeper = Address::generate(&env);

    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Weekly,
        &DcaDirection::Sell,
        &2,
        &200,
    );

    let interval = 17_280 * 7;
    // Execution 1
    env.ledger().set_sequence_number(1_000 + interval);
    client.execute(&keeper, &plan_id);

    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.status, DcaPlanStatus::Active);

    // Execution 2 (final execution)
    env.ledger().set_sequence_number(1_000 + 2 * interval);
    client.execute(&keeper, &plan_id);

    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.status, DcaPlanStatus::Completed);
    assert_eq!(plan.total_executions, 2);
    assert_eq!(plan.spent_amount, 200);

    // Execution 3 fails because Plan is Completed (PlanNotActive)
    env.ledger().set_sequence_number(1_000 + 3 * interval);
    let res = client.try_execute(&keeper, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::PlanNotActive)));
}

#[test]
fn test_execute_insufficient_funds() {
    let (env, client, _admin, owner, asset) = setup();
    let keeper = Address::generate(&env);

    // Funded with 250, amount per execution 100, max executions 5
    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &5,
        &250,
    );

    // Exec 1: spent = 100, remaining = 150
    env.ledger().set_sequence_number(1_000 + 17_280);
    client.execute(&keeper, &plan_id);

    // Exec 2: spent = 200, remaining = 50
    env.ledger().set_sequence_number(1_000 + 2 * 17_280);
    client.execute(&keeper, &plan_id);

    // Exec 3: remaining 50 < 100 -> InsufficientFunds
    env.ledger().set_sequence_number(1_000 + 3 * 17_280);
    let res = client.try_execute(&keeper, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::InsufficientFunds)));
}

#[test]
fn test_pause_and_resume_lifecycle() {
    let (env, client, _admin, owner, asset) = setup();
    let other_user = Address::generate(&env);

    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &5,
        &500,
    );

    // Unauthorized pause
    let res = client.try_pause(&other_user, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::Unauthorized)));

    // Owner pauses
    assert!(client.try_pause(&owner, &plan_id).is_ok());
    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.status, DcaPlanStatus::Paused);

    // Pause already paused
    let res = client.try_pause(&owner, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::PlanAlreadyPaused)));

    // Execution fails while paused
    env.ledger().set_sequence_number(1_000 + 17_280);
    let res = client.try_execute(&other_user, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::PlanNotActive)));

    // Unauthorized resume
    let res = client.try_resume(&other_user, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::Unauthorized)));

    // Owner resumes when scheduled ledger already arrived
    assert!(client.try_resume(&owner, &plan_id).is_ok());
    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.status, DcaPlanStatus::Active);
    // Since 1_000 + 17_280 has already arrived, next_execution_ledger is current ledger (immediately executable)
    assert_eq!(plan.next_execution_ledger, 1_000 + 17_280);

    // Resume when not paused
    let res = client.try_resume(&owner, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::PlanNotPaused)));

    // Now execute works immediately
    let exec = client.execute(&other_user, &plan_id);
    assert_eq!(exec.execution_number, 1);
}

#[test]
fn test_cancel_and_refund() {
    let (env, client, _admin, owner, asset) = setup();
    let keeper = Address::generate(&env);
    let other = Address::generate(&env);

    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &5,
        &500,
    );

    // Execute once: spent 100, remaining 400
    env.ledger().set_sequence_number(1_000 + 17_280);
    client.execute(&keeper, &plan_id);

    // Unauthorized cancel
    let res = client.try_cancel(&other, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::Unauthorized)));

    // Cancel by owner
    let refund = client.cancel(&owner, &plan_id);
    assert_eq!(refund, 400);

    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.status, DcaPlanStatus::Cancelled);

    // Subsequent cancel fails
    let res = client.try_cancel(&owner, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::PlanNotActive)));

    // Subsequent execute fails
    let res = client.try_execute(&keeper, &plan_id);
    assert_eq!(res, Err(Ok(DcaError::PlanNotActive)));
}

#[test]
fn test_cancel_paused_plan() {
    let (_env, client, _admin, owner, asset) = setup();

    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &5,
        &500,
    );

    client.pause(&owner, &plan_id);
    let refund = client.cancel(&owner, &plan_id);
    assert_eq!(refund, 500);

    let plan = client.get_plan(&plan_id).unwrap();
    assert_eq!(plan.status, DcaPlanStatus::Cancelled);
}

#[test]
fn test_max_user_plans_limit() {
    let (_env, client, _admin, owner, asset) = setup();

    // Create 100 plans
    for i in 1..=100 {
        let pid = client.create_plan(
            &owner,
            &asset,
            &10,
            &DcaFrequency::Monthly,
            &DcaDirection::Buy,
            &1,
            &10,
        );
        assert_eq!(pid, i);
    }

    // 101th plan must fail with MaxPlansReached
    let res = client.try_create_plan(
        &owner,
        &asset,
        &10,
        &DcaFrequency::Monthly,
        &DcaDirection::Buy,
        &1,
        &10,
    );
    assert_eq!(res, Err(Ok(DcaError::MaxPlansReached)));
}

#[test]
fn test_query_nonexistent_plan() {
    let (_env, client, _admin, _owner, _asset) = setup();
    assert_eq!(client.get_plan(&999), None);
}

#[test]
fn test_real_token_custody_and_refund() {
    let (env, client, _admin, owner, asset) = setup();
    let token = soroban_sdk::token::Client::new(&env, &asset);

    let initial_owner_balance = token.balance(&owner);

    let plan_id = client.create_plan(
        &owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &5,
        &500,
    );

    // Verify tokens were transferred from owner to contract custody
    assert_eq!(token.balance(&owner), initial_owner_balance - 500);
    assert_eq!(token.balance(&client.address), 500);

    // Cancelling returns the remaining tokens to owner
    let refund = client.cancel(&owner, &plan_id);
    assert_eq!(refund, 500);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(token.balance(&owner), initial_owner_balance);
}

#[test]
#[should_panic]
fn test_unfunded_owner_cannot_create_plan() {
    let (env, client, _admin, _owner, asset) = setup();
    let poor_owner = Address::generate(&env);

    // Should panic due to insufficient token balance
    client.create_plan(
        &poor_owner,
        &asset,
        &100,
        &DcaFrequency::Daily,
        &DcaDirection::Buy,
        &5,
        &500,
    );
}

