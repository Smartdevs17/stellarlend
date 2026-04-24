//! # Flash Loan Governance Attack Protection Tests
//!
//! Verifies that the governance system is resistant to flash loan attacks
//! where an attacker borrows tokens within a single transaction to gain
//! temporary voting power.
//!
//! ## Attack Scenarios Covered
//! 1. Attacker acquires tokens after proposal creation → zero voting power
//! 2. Vote locking prevents token return during active vote
//! 3. Delegation established too close to proposal → not counted
//! 4. Delegation depth limit prevents chain amplification
//! 5. Quorum requirements prevent low-participation passage
//! 6. Proposal rate limiting prevents spam attacks
//! 7. Governance analytics track suspicious activity
//! 8. Legitimate large voters are not blocked

#![cfg(test)]

use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env, String};
use soroban_sdk::token::StellarAssetClient;

use crate::{
    types::{ProposalStatus, ProposalType, VoteType},
    HelloContract, HelloContractClient,
};

// ============================================================================
// Helpers
// ============================================================================

fn setup(env: &Env) -> (Address, Address, HelloContractClient) {
    let admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin.clone());

    let contract_id = env.register_contract(None, HelloContract);
    let client = HelloContractClient::new(env, &contract_id);

    env.mock_all_auths();

    client.initialize(&admin);
    client.gov_initialize(
        &admin,
        &token,
        &Some(7 * 24 * 3600_u64), // 7-day voting period
        &Some(2 * 24 * 3600_u64), // 2-day execution delay
        &Some(4000_u32),           // 40% quorum
        &Some(100_i128),           // proposal threshold
        &Some(7 * 24 * 3600_u64), // 7-day timelock
        &Some(5000_i128),          // 50% voting threshold
    );

    (admin, token, client)
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

// ============================================================================
// 1. Snapshot-based voting: tokens acquired AFTER proposal creation carry
//    zero voting power (core flash loan protection)
// ============================================================================

#[test]
fn test_tokens_acquired_after_proposal_have_zero_voting_power() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    // Legitimate proposer holds tokens before proposal
    let proposer = Address::generate(&env);
    mint(&env, &token, &proposer, 1_000);

    // Attacker holds NO tokens at proposal creation time
    let attacker = Address::generate(&env);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Malicious proposal"),
        &None,
    );

    // Snapshot is taken at proposal creation: attacker has 0 balance
    let snap = client.gov_get_vote_power_snapshot(&proposal_id, &attacker);
    assert!(snap.is_none(), "Attacker should have no snapshot");

    // Attacker acquires tokens AFTER proposal creation (simulating flash loan)
    mint(&env, &token, &attacker, 999_000);

    // Advance time so proposal is Active
    env.ledger().with_mut(|l| l.timestamp += 1);

    // Attacker tries to vote — should fail with NoVotingPower
    let result = client.try_gov_vote(&attacker, &proposal_id, &VoteType::For);
    assert!(result.is_err(), "Attacker with post-snapshot tokens must not vote");
}

// ============================================================================
// 2. Vote locking: tokens are locked during the voting period
// ============================================================================

#[test]
fn test_vote_lock_is_set_when_casting_vote() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let voter = Address::generate(&env);
    mint(&env, &token, &voter, 5_000);

    let proposal_id = client.gov_create_proposal(
        &voter,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Test proposal"),
        &None,
    );

    // Advance time so proposal is Active
    env.ledger().with_mut(|l| l.timestamp += 1);

    // Not locked before voting
    assert!(!client.gov_is_vote_locked(&voter));

    client.gov_vote(&voter, &proposal_id, &VoteType::For);

    // Should be locked after voting
    assert!(client.gov_is_vote_locked(&voter), "Voter should be locked after casting vote");

    let lock = client.gov_get_vote_lock(&voter).expect("Lock record must exist");
    assert_eq!(lock.proposal_id, proposal_id);
    assert!(lock.locked_amount > 0);
    assert!(lock.locked_until > env.ledger().timestamp());
}

#[test]
fn test_vote_lock_expires_after_voting_period() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let voter = Address::generate(&env);
    mint(&env, &token, &voter, 5_000);

    let proposal_id = client.gov_create_proposal(
        &voter,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Test proposal"),
        &None,
    );

    env.ledger().with_mut(|l| l.timestamp += 1);
    client.gov_vote(&voter, &proposal_id, &VoteType::For);

    // Advance past the voting period (7 days + 1 second)
    env.ledger().with_mut(|l| l.timestamp += 7 * 24 * 3600 + 1);

    // Lock should have expired
    assert!(!client.gov_is_vote_locked(&voter), "Lock should expire after voting period");
}

// ============================================================================
// 3. Delegation deadline: delegation established too close to proposal
//    creation is NOT counted
// ============================================================================

#[test]
fn test_delegation_too_recent_not_counted() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    let delegator = Address::generate(&env);
    let delegatee = Address::generate(&env);

    mint(&env, &token, &proposer, 1_000);
    mint(&env, &token, &delegator, 5_000);
    mint(&env, &token, &delegatee, 500);

    // Delegator delegates JUST before proposal creation (within deadline window)
    // DELEGATION_DEADLINE = 24 hours; we set delegation only 1 hour before proposal
    env.ledger().with_mut(|l| l.timestamp = 10_000);
    client.gov_delegate_vote(&delegator, &delegatee);

    // Proposal created 1 hour after delegation (< 24h deadline)
    env.ledger().with_mut(|l| l.timestamp = 10_000 + 3600);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Test proposal"),
        &None,
    );

    // Advance time so proposal is Active
    env.ledger().with_mut(|l| l.timestamp += 1);

    // Delegatee votes — should only have their own snapshot power (500),
    // NOT the delegator's 5000 (delegation too recent)
    client.gov_vote(&delegatee, &proposal_id, &VoteType::For);

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    // Only delegatee's own power (500) should count
    assert_eq!(proposal.for_votes, 500,
        "Delegation too recent should not add delegator's power");
}

#[test]
fn test_valid_delegation_before_deadline_is_counted() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    let delegator = Address::generate(&env);
    let delegatee = Address::generate(&env);

    mint(&env, &token, &proposer, 1_000);
    mint(&env, &token, &delegator, 5_000);
    mint(&env, &token, &delegatee, 500);

    // Delegation established 2 days before proposal (> 24h deadline)
    env.ledger().with_mut(|l| l.timestamp = 10_000);
    client.gov_delegate_vote(&delegator, &delegatee);

    // Proposal created 2 days later
    env.ledger().with_mut(|l| l.timestamp = 10_000 + 2 * 24 * 3600);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Test proposal"),
        &None,
    );

    // Snapshot delegator's balance at proposal creation
    client.gov_get_vote_power_snapshot(&proposal_id, &delegator);

    // Advance time so proposal is Active
    env.ledger().with_mut(|l| l.timestamp += 1);

    // Delegatee votes — should have own power (500) + delegator's power (5000)
    client.gov_vote(&delegatee, &proposal_id, &VoteType::For);

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.for_votes, 5500,
        "Valid delegation should add delegator's power to delegatee");
}

// ============================================================================
// 4. Delegation depth limit
// ============================================================================

#[test]
fn test_delegation_depth_limit_enforced() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let d = Address::generate(&env);
    let e = Address::generate(&env); // depth 4 — should fail

    mint(&env, &token, &a, 100);
    mint(&env, &token, &b, 100);
    mint(&env, &token, &c, 100);
    mint(&env, &token, &d, 100);
    mint(&env, &token, &e, 100);

    // Build chain: a → b → c → d (depth 3, allowed)
    client.gov_delegate_vote(&a, &b);
    client.gov_delegate_vote(&b, &c);
    client.gov_delegate_vote(&c, &d);

    // d → e would be depth 4, exceeding MAX_DELEGATION_DEPTH (3)
    let result = client.try_gov_delegate_vote(&d, &e);
    assert!(result.is_err(), "Delegation chain exceeding max depth must be rejected");
}

// ============================================================================
// 5. Self-delegation rejected
// ============================================================================

#[test]
fn test_self_delegation_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);
    let voter = Address::generate(&env);
    mint(&env, &token, &voter, 1_000);

    let result = client.try_gov_delegate_vote(&voter, &voter);
    assert!(result.is_err(), "Self-delegation must be rejected");
}

// ============================================================================
// 6. Proposal rate limiting
// ============================================================================

#[test]
fn test_proposal_rate_limit_enforced() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    mint(&env, &token, &proposer, 100_000);

    // Create up to the rate limit (5 proposals)
    for i in 0..5 {
        client.gov_create_proposal(
            &proposer,
            &ProposalType::EmergencyPause(true),
            &String::from_str(&env, "Proposal"),
            &None,
        );
    }

    // 6th proposal in the same window should fail
    let result = client.try_gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Spam proposal"),
        &None,
    );
    assert!(result.is_err(), "6th proposal in rate window must be rejected");
}

#[test]
fn test_proposal_rate_limit_resets_after_window() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    mint(&env, &token, &proposer, 100_000);

    // Fill the rate limit
    for _ in 0..5 {
        client.gov_create_proposal(
            &proposer,
            &ProposalType::EmergencyPause(true),
            &String::from_str(&env, "Proposal"),
            &None,
        );
    }

    // Advance past the 24-hour window
    env.ledger().with_mut(|l| l.timestamp += 24 * 3600 + 1);

    // Should succeed in the new window
    let result = client.try_gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "New window proposal"),
        &None,
    );
    assert!(result.is_ok(), "Proposal in new rate window must succeed");
}

// ============================================================================
// 7. Governance analytics track suspicious activity
// ============================================================================

#[test]
fn test_governance_analytics_track_suspicious_voting() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    let whale = Address::generate(&env);

    // Whale holds a very large proportion of supply
    mint(&env, &token, &proposer, 100);
    mint(&env, &token, &whale, 999_900); // ~99.99% of supply

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Test"),
        &None,
    );

    env.ledger().with_mut(|l| l.timestamp += 1);
    client.gov_vote(&whale, &proposal_id, &VoteType::For);

    let analytics = client.gov_get_analytics();
    assert!(analytics.suspicious_proposals > 0,
        "Whale vote should trigger suspicious activity flag");
    assert!(analytics.max_single_voter_power > 0);
}

#[test]
fn test_governance_analytics_count_proposals_and_votes() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    mint(&env, &token, &proposer, 1_000);
    mint(&env, &token, &voter, 500);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Test"),
        &None,
    );

    env.ledger().with_mut(|l| l.timestamp += 1);
    client.gov_vote(&voter, &proposal_id, &VoteType::For);

    let analytics = client.gov_get_analytics();
    assert_eq!(analytics.total_proposals, 1);
    assert_eq!(analytics.total_votes, 1);
}

// ============================================================================
// 8. Legitimate large voter is not blocked
// ============================================================================

#[test]
fn test_legitimate_large_voter_can_vote() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    let large_voter = Address::generate(&env);

    // Large voter holds tokens BEFORE proposal creation
    mint(&env, &token, &proposer, 1_000);
    mint(&env, &token, &large_voter, 50_000);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Legitimate proposal"),
        &None,
    );

    env.ledger().with_mut(|l| l.timestamp += 1);

    // Large voter should be able to vote successfully
    client.gov_vote(&large_voter, &proposal_id, &VoteType::For);

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.for_votes, 50_000,
        "Legitimate large voter holding tokens before proposal must be able to vote");
}

// ============================================================================
// 9. Quorum prevents low-participation passage
// ============================================================================

#[test]
fn test_quorum_prevents_low_participation_passage() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    let tiny_voter = Address::generate(&env);

    mint(&env, &token, &proposer, 1_000);
    mint(&env, &token, &tiny_voter, 1); // Tiny balance

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Low participation proposal"),
        &None,
    );

    env.ledger().with_mut(|l| l.timestamp += 1);
    client.gov_vote(&tiny_voter, &proposal_id, &VoteType::For);

    // Advance past voting period
    env.ledger().with_mut(|l| l.timestamp += 7 * 24 * 3600 + 1);

    // Queue should fail or result in Defeated due to quorum not met
    let outcome = client.gov_queue_proposal(&tiny_voter, &proposal_id);
    // Quorum of 40% not met with only 1 token voting out of 1001 total
    assert!(!outcome.quorum_reached,
        "Proposal with insufficient participation must not pass quorum");
}

// ============================================================================
// 10. Delegation revocation works correctly
// ============================================================================

#[test]
fn test_delegation_revocation() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let delegator = Address::generate(&env);
    let delegatee = Address::generate(&env);
    mint(&env, &token, &delegator, 1_000);
    mint(&env, &token, &delegatee, 100);

    client.gov_delegate_vote(&delegator, &delegatee);

    // Verify delegation exists
    let del = client.gov_get_delegation(&delegator);
    assert!(del.is_some(), "Delegation should exist after creation");

    // Revoke
    client.gov_revoke_delegation(&delegator);

    // Delegation should be gone
    let del_after = client.gov_get_delegation(&delegator);
    assert!(del_after.is_none(), "Delegation should be removed after revocation");
}

// ============================================================================
// 11. Proposal execution delay enforced (timelock)
// ============================================================================

#[test]
fn test_proposal_execution_delay_enforced() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, token, client) = setup(&env);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    mint(&env, &token, &proposer, 1_000);
    mint(&env, &token, &voter, 10_000);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Delayed execution"),
        &None,
    );

    env.ledger().with_mut(|l| l.timestamp += 1);
    client.gov_vote(&voter, &proposal_id, &VoteType::For);

    // Advance past voting period
    env.ledger().with_mut(|l| l.timestamp += 7 * 24 * 3600 + 1);
    client.gov_queue_proposal(&voter, &proposal_id);

    // Try to execute immediately after queuing — should fail (2-day delay)
    let result = client.try_gov_execute_proposal(&voter, &proposal_id);
    assert!(result.is_err(), "Execution before delay period must fail");

    // Advance past execution delay (2 days)
    env.ledger().with_mut(|l| l.timestamp += 2 * 24 * 3600 + 1);

    // Now execution should succeed
    client.gov_execute_proposal(&voter, &proposal_id);

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);
}
