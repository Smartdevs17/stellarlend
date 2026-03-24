#![cfg(test)]

use crate::types::{ProposalStatus, ProposalType};
use crate::HelloContractClient;
use crate::HelloContract;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Symbol, Vec,
};

fn setup_env() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(HelloContract, ());
    let admin = Address::generate(&env);

    let client = HelloContractClient::new(&env, &contract_id);
    client.gov_initialize(
        &admin,
        &Address::generate(&env), // vote_token
        &None, // voting_period
        &None, // execution_delay
        &None, // quorum_bps
        &None, // proposal_threshold
        &None, // timelock_duration
        &None, // default_voting_threshold
    );

    (env, contract_id, admin)
}

#[test]
fn test_admin_queues_risk_params_with_timelock() {
    let (env, cid, admin) = setup_env();
    let client = HelloContractClient::new(&env, &cid);

    // Admin calls set_risk_params
    let proposal_id = client.set_risk_params(
        &admin,
        &Some(15000), // min_cr
        &None,
        &None,
        &None,
    );

    // Check proposal status is Queued
    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Queued);
    
    // Check execution time is at least 24h in the future
    let now = env.ledger().timestamp();
    assert!(proposal.execution_time.unwrap() >= now + 86400);

    // Try to execute immediately - should fail
    let res = client.try_gov_execute_proposal(&admin, &proposal_id);
    assert!(res.is_err());
}

#[test]
fn test_anyone_can_execute_after_delay() {
    let (env, cid, admin) = setup_env();
    let client = HelloContractClient::new(&env, &cid);
    let executor = Address::generate(&env);

    let proposal_id = client.set_risk_params(&admin, &Some(15000), &None, &None, &None);

    // Advance time by 24 hours + 1 second
    env.ledger().with_mut(|li| {
        li.timestamp += 86401;
    });

    // Anyone executes
    client.gov_execute_proposal(&executor, &proposal_id);

    // Verify it was executed
    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
fn test_multisig_emergency_bypass() {
    let (env, cid, admin) = setup_env();
    let client = HelloContractClient::new(&env, &cid);
    
    // Setup multisig
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    client.ms_set_admins(&admin, &admins, &1);

    // Propose emergency change
    let proposal_id = client.ms_propose_risk_params(
        &admin,
        &Some(16000),
        &None,
        &None,
        &None,
        &true, // emergency
    );

    // Emergency proposal should have execution_time = now
    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.execution_time.unwrap(), env.ledger().timestamp());

    // Execute immediately
    client.ms_execute(&admin, &proposal_id);

    // Verify executed
    let proposal_after = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal_after.status, ProposalStatus::Executed);
}

#[test]
fn test_admin_can_cancel_queued_change() {
    let (env, cid, admin) = setup_env();
    let client = HelloContractClient::new(&env, &cid);

    let proposal_id = client.set_risk_params(&admin, &Some(15000), &None, &None, &None);

    // Admin cancels
    client.gov_cancel_proposal(&admin, &proposal_id);

    // Verify cancelled
    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Cancelled);
}

#[test]
fn test_interest_rate_config_timelock() {
    let (env, cid, admin) = setup_env();
    let client = HelloContractClient::new(&env, &cid);

    // Admin calls update_interest_rate_config
    let proposal_id = client.update_interest_rate_config(
        &admin,
        &Some(200), // base_rate
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
    );

    // Check status
    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Queued);

    // Advance time
    env.ledger().with_mut(|li| {
        li.timestamp += 86401;
    });

    // Execute
    client.gov_execute_proposal(&admin, &proposal_id);

    // Verify executed
    let proposal_after = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal_after.status, ProposalStatus::Executed);
}
