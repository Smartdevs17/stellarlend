//! # Governance Lifecycle Tests
//!
//! Covers lock-to-vote voting power, delegation, quorum and voting periods,
//! and timelocked execution:
//!
//! - voting power is checkpointed and only counts if held strictly before a
//!   proposal was created;
//! - delegation moves locked-token power in a single hop;
//! - quorum is a share of the locked supply, fixed when a proposal is created;
//! - approved proposals wait out `execution_delay`, then must execute within
//!   the grace period, and can be vetoed while queued.

#![cfg(test)]

use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String, Vec,
};

use crate::{
    errors::GovernanceError,
    governance,
    types::{GovernanceParams, ProposalStatus, ProposalType, VoteType},
    HelloContract, HelloContractClient,
};

const VOTING_PERIOD: u64 = 3 * 24 * 3600;
const EXECUTION_DELAY: u64 = 24 * 3600;
const QUORUM_BPS: u32 = 4_000;
const PROPOSAL_THRESHOLD: i128 = 100;
const GRACE_PERIOD: u64 = 7 * 24 * 3600;

// ============================================================================
// Helpers
// ============================================================================

fn setup() -> (Env, HelloContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());

    let contract_id = env.register_contract(None, HelloContract);
    let client = HelloContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.gov_initialize(
        &admin,
        &token,
        &Some(VOTING_PERIOD),
        &Some(EXECUTION_DELAY),
        &Some(QUORUM_BPS),
        &Some(PROPOSAL_THRESHOLD),
        &Some(GRACE_PERIOD),
        &Some(5_000),
    );

    (env, client, token, admin)
}

/// Mint `amount` vote tokens to `who` and lock them in governance.
fn fund(env: &Env, client: &HelloContractClient, token: &Address, who: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(who, &amount);
    client.gov_lock_tokens(who, &amount);
}

fn advance(env: &Env, seconds: u64) {
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + seconds);
}

fn propose(env: &Env, client: &HelloContractClient, proposer: &Address, ty: ProposalType) -> u64 {
    client.gov_create_proposal(proposer, &ty, &String::from_str(env, "proposal"), &None)
}

/// Run a governance call inside the contract so its `GovernanceError` is
/// observable (the public entry points map most of them to one code).
fn as_gov<T>(
    client: &HelloContractClient,
    f: impl FnOnce() -> Result<T, GovernanceError>,
) -> Result<T, GovernanceError> {
    client.env.as_contract(&client.address, f)
}

/// Proposer (1,000) and a voter holding the majority (10,000), locked before
/// the proposal. Returns (proposer, voter, proposal_id).
fn passing_proposal(
    env: &Env,
    client: &HelloContractClient,
    token: &Address,
    ty: ProposalType,
) -> (Address, Address, u64) {
    let proposer = Address::generate(env);
    let voter = Address::generate(env);
    fund(env, client, token, &proposer, 1_000);
    fund(env, client, token, &voter, 10_000);
    advance(env, 1);

    let id = propose(env, client, &proposer, ty);
    client.gov_vote(&voter, &id, &VoteType::For);
    (proposer, voter, id)
}

fn end_voting_and_queue(env: &Env, client: &HelloContractClient, id: u64) {
    let proposal = client.gov_get_proposal(&id).unwrap();
    env.ledger().set_timestamp(proposal.end_time);
    client.gov_queue_proposal(&Address::generate(env), &id);
}

// ============================================================================
// Lock-to-vote power (#1083)
// ============================================================================

#[test]
fn test_lock_and_unlock_move_tokens_and_voting_power() {
    let (env, client, token, _) = setup();
    let owner = Address::generate(&env);
    let tokens = TokenClient::new(&env, &token);

    fund(&env, &client, &token, &owner, 1_000);
    assert_eq!(tokens.balance(&owner), 0);
    assert_eq!(tokens.balance(&client.address), 1_000);
    assert_eq!(client.gov_get_locked_balance(&owner), 1_000);
    assert_eq!(client.gov_get_votes(&owner), 1_000);
    assert_eq!(client.gov_get_total_locked(), 1_000);

    assert_eq!(client.gov_unlock_tokens(&owner, &400), 600);
    assert_eq!(tokens.balance(&owner), 400);
    assert_eq!(client.gov_get_locked_balance(&owner), 600);
    assert_eq!(client.gov_get_votes(&owner), 600);
    assert_eq!(client.gov_get_total_locked(), 600);
}

#[test]
fn test_lock_and_unlock_reject_invalid_amounts() {
    let (env, client, token, _) = setup();
    let owner = Address::generate(&env);
    fund(&env, &client, &token, &owner, 500);

    assert_eq!(
        as_gov(&client, || governance::lock_tokens(&env, owner.clone(), 0)),
        Err(GovernanceError::InvalidAmount)
    );
    assert_eq!(
        as_gov(&client, || governance::unlock_tokens(
            &env,
            owner.clone(),
            -1
        )),
        Err(GovernanceError::InvalidAmount)
    );
    assert_eq!(
        as_gov(&client, || governance::unlock_tokens(
            &env,
            owner.clone(),
            501
        )),
        Err(GovernanceError::InsufficientLockedBalance)
    );
}

#[test]
fn test_past_votes_follow_checkpoints() {
    let (env, client, token, _) = setup();
    let owner = Address::generate(&env);

    env.ledger().set_timestamp(10_000);
    fund(&env, &client, &token, &owner, 100);
    env.ledger().set_timestamp(20_000);
    fund(&env, &client, &token, &owner, 50);
    // A second change in the same ledger collapses into one checkpoint.
    fund(&env, &client, &token, &owner, 25);
    env.ledger().set_timestamp(30_000);
    client.gov_unlock_tokens(&owner, &30);

    // Power is measured strictly before the given timestamp.
    assert_eq!(client.gov_get_past_votes(&owner, &10_000), 0);
    assert_eq!(client.gov_get_past_votes(&owner, &10_001), 100);
    assert_eq!(client.gov_get_past_votes(&owner, &20_000), 100);
    assert_eq!(client.gov_get_past_votes(&owner, &20_001), 175);
    assert_eq!(client.gov_get_past_votes(&owner, &30_001), 145);
    assert_eq!(client.gov_get_votes(&owner), 145);
}

#[test]
fn test_power_locked_in_the_proposal_ledger_cannot_vote() {
    let (env, client, token, _) = setup();
    let proposer = Address::generate(&env);
    let attacker = Address::generate(&env);

    fund(&env, &client, &token, &proposer, 1_000);
    advance(&env, 1);

    let id = propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
    // Flash-loan style: acquire and lock in the same ledger as creation.
    fund(&env, &client, &token, &attacker, 1_000_000);
    advance(&env, 1);

    assert_eq!(
        as_gov(&client, || governance::vote(
            &env,
            attacker.clone(),
            id,
            VoteType::For
        )),
        Err(GovernanceError::NoVotingPower)
    );
    // Quorum was fixed before the attacker's lock: 40% of 1,000.
    assert_eq!(client.gov_get_proposal(&id).unwrap().quorum_votes, 400);
}

#[test]
fn test_proposer_threshold_uses_power_held_before_this_ledger() {
    let (env, client, token, _) = setup();
    let proposer = Address::generate(&env);

    fund(&env, &client, &token, &proposer, PROPOSAL_THRESHOLD);
    assert_eq!(
        as_gov(&client, || governance::create_proposal(
            &env,
            proposer.clone(),
            ProposalType::EmergencyPause(true),
            String::from_str(&env, "same ledger"),
            None,
        )),
        Err(GovernanceError::InsufficientProposalPower)
    );

    advance(&env, 1);
    propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
}

#[test]
fn test_voter_cannot_unlock_until_voting_ends() {
    let (env, client, token, _) = setup();
    let (_, voter, id) =
        passing_proposal(&env, &client, &token, ProposalType::EmergencyPause(true));

    assert!(client.gov_is_vote_locked(&voter));
    assert_eq!(
        as_gov(&client, || governance::unlock_tokens(
            &env,
            voter.clone(),
            1
        )),
        Err(GovernanceError::VotesLocked)
    );

    let proposal = client.gov_get_proposal(&id).unwrap();
    env.ledger().set_timestamp(proposal.end_time);
    assert!(!client.gov_is_vote_locked(&voter));
    client.gov_unlock_tokens(&voter, &10_000);
}

// ============================================================================
// Delegation (#1084)
// ============================================================================

#[test]
fn test_delegation_moves_power_and_follows_later_locks() {
    let (env, client, token, _) = setup();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    fund(&env, &client, &token, &a, 1_000);
    client.gov_delegate_vote(&a, &b);
    assert_eq!(client.gov_get_votes(&a), 0);
    assert_eq!(client.gov_get_votes(&b), 1_000);
    assert_eq!(client.gov_get_delegation(&a).unwrap().delegatee, b);

    // Locking more or unlocking adjusts the delegatee's power.
    fund(&env, &client, &token, &a, 500);
    assert_eq!(client.gov_get_votes(&b), 1_500);
    client.gov_unlock_tokens(&a, &200);
    assert_eq!(client.gov_get_votes(&b), 1_300);

    // Re-delegation moves everything to the new delegatee.
    client.gov_delegate_vote(&a, &c);
    assert_eq!(client.gov_get_votes(&b), 0);
    assert_eq!(client.gov_get_votes(&c), 1_300);

    client.gov_revoke_delegation(&a);
    assert_eq!(client.gov_get_votes(&a), 1_300);
    assert_eq!(client.gov_get_votes(&c), 0);
    assert!(client.gov_get_delegation(&a).is_none());
    assert_eq!(client.gov_get_total_locked(), 1_300);
}

#[test]
fn test_delegation_rejects_invalid_requests() {
    let (env, client, token, _) = setup();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    fund(&env, &client, &token, &a, 1_000);

    assert_eq!(
        as_gov(&client, || governance::delegate_vote(
            &env,
            a.clone(),
            a.clone()
        )),
        Err(GovernanceError::SelfDelegation)
    );
    assert_eq!(
        as_gov(&client, || governance::revoke_delegation(&env, a.clone())),
        Err(GovernanceError::NotDelegated)
    );
    client.gov_delegate_vote(&a, &b);
    assert_eq!(
        as_gov(&client, || governance::delegate_vote(
            &env,
            a.clone(),
            b.clone()
        )),
        Err(GovernanceError::AlreadyDelegated)
    );
}

#[test]
fn test_delegatee_can_still_delegate_its_own_tokens() {
    // Regression: the delegatee's list of delegators used to share a storage
    // key with its own delegation record, so receiving a delegation blocked
    // delegating onward.
    let (env, client, token, _) = setup();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    fund(&env, &client, &token, &a, 1_000);
    fund(&env, &client, &token, &b, 300);

    client.gov_delegate_vote(&a, &b);
    client.gov_delegate_vote(&b, &c);

    assert_eq!(client.gov_get_votes(&b), 1_000);
    assert_eq!(client.gov_get_votes(&c), 300);
}

#[test]
fn test_delegatee_votes_with_delegated_power() {
    let (env, client, token, _) = setup();
    let proposer = Address::generate(&env);
    let delegator = Address::generate(&env);
    let delegatee = Address::generate(&env);

    fund(&env, &client, &token, &proposer, 1_000);
    fund(&env, &client, &token, &delegator, 5_000);
    fund(&env, &client, &token, &delegatee, 500);
    client.gov_delegate_vote(&delegator, &delegatee);
    advance(&env, 1);

    let id = propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
    client.gov_vote(&delegatee, &id, &VoteType::For);
    assert_eq!(client.gov_get_proposal(&id).unwrap().for_votes, 5_500);

    // The delegator's power went to the delegatee, so it cannot vote twice.
    assert_eq!(
        as_gov(&client, || governance::vote(
            &env,
            delegator.clone(),
            id,
            VoteType::For
        )),
        Err(GovernanceError::NoVotingPower)
    );
}

// ============================================================================
// Quorum and voting periods (#1086)
// ============================================================================

#[test]
fn test_quorum_is_fixed_when_the_proposal_is_created() {
    let (env, client, token, _) = setup();
    let proposer = Address::generate(&env);
    let late = Address::generate(&env);
    fund(&env, &client, &token, &proposer, 10_000);
    advance(&env, 1);

    let id = propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
    assert_eq!(client.gov_get_proposal(&id).unwrap().quorum_votes, 4_000);

    fund(&env, &client, &token, &late, 90_000);
    assert_eq!(client.gov_get_proposal(&id).unwrap().quorum_votes, 4_000);
}

#[test]
fn test_proposal_without_quorum_is_defeated() {
    let (env, client, token, _) = setup();
    let proposer = Address::generate(&env);
    let small = Address::generate(&env);
    let whale = Address::generate(&env);
    fund(&env, &client, &token, &proposer, 1_000);
    fund(&env, &client, &token, &small, 1_000);
    fund(&env, &client, &token, &whale, 8_000);
    advance(&env, 1);

    // 1,000 of 10,000 locked votes, short of the 4,000 quorum.
    let id = propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
    client.gov_vote(&small, &id, &VoteType::For);

    let proposal = client.gov_get_proposal(&id).unwrap();
    env.ledger().set_timestamp(proposal.end_time);
    assert_eq!(
        client.gov_get_proposal_state(&id),
        Some(ProposalStatus::Defeated)
    );

    let outcome = client.gov_queue_proposal(&proposer, &id);
    assert!(!outcome.quorum_reached);
    assert!(!outcome.succeeded);
    assert_eq!(outcome.quorum_required, 4_000);
    assert_eq!(
        client.gov_get_proposal(&id).unwrap().status,
        ProposalStatus::Defeated
    );
}

#[test]
fn test_abstain_counts_toward_quorum_but_a_tie_fails() {
    let (env, client, token, _) = setup();
    let proposer = Address::generate(&env);
    let yes = Address::generate(&env);
    let no = Address::generate(&env);
    let abstain = Address::generate(&env);
    fund(&env, &client, &token, &proposer, 1_000);
    fund(&env, &client, &token, &yes, 3_000);
    fund(&env, &client, &token, &no, 3_000);
    fund(&env, &client, &token, &abstain, 3_000);
    advance(&env, 1);

    let id = propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
    client.gov_vote(&yes, &id, &VoteType::For);
    client.gov_vote(&no, &id, &VoteType::Against);
    client.gov_vote(&abstain, &id, &VoteType::Abstain);

    let proposal = client.gov_get_proposal(&id).unwrap();
    env.ledger().set_timestamp(proposal.end_time);
    let outcome = client.gov_queue_proposal(&proposer, &id);
    assert!(outcome.quorum_reached);
    assert!(!outcome.succeeded);
}

#[test]
fn test_voting_threshold_can_only_be_raised() {
    let (env, client, token, _) = setup();
    let proposer = Address::generate(&env);
    fund(&env, &client, &token, &proposer, 1_000);
    advance(&env, 1);

    for threshold in [0, 4_999, 10_001] {
        assert_eq!(
            as_gov(&client, || governance::create_proposal(
                &env,
                proposer.clone(),
                ProposalType::EmergencyPause(true),
                String::from_str(&env, "threshold"),
                Some(threshold),
            )),
            Err(GovernanceError::InvalidVotingThreshold)
        );
    }

    let id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "supermajority"),
        &Some(6_700),
    );
    assert_eq!(
        client.gov_get_proposal(&id).unwrap().voting_threshold,
        6_700
    );
}

#[test]
fn test_initialize_rejects_out_of_bounds_config() {
    let cases: [(u64, u32, u64, i128, GovernanceError); 5] = [
        (
            60,
            4_000,
            GRACE_PERIOD,
            5_000,
            GovernanceError::InvalidVotingPeriod,
        ),
        (
            31 * 24 * 3600,
            4_000,
            GRACE_PERIOD,
            5_000,
            GovernanceError::InvalidVotingPeriod,
        ),
        (
            VOTING_PERIOD,
            0,
            GRACE_PERIOD,
            5_000,
            GovernanceError::InvalidQuorum,
        ),
        (
            VOTING_PERIOD,
            4_000,
            0,
            5_000,
            GovernanceError::InvalidTimelockConfig,
        ),
        (
            VOTING_PERIOD,
            4_000,
            GRACE_PERIOD,
            0,
            GovernanceError::InvalidVotingThreshold,
        ),
    ];

    for (voting_period, quorum, grace, threshold, expected) in cases {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        let contract_id = env.register_contract(None, HelloContract);

        let result = env.as_contract(&contract_id, || {
            governance::initialize(
                &env,
                admin.clone(),
                token.clone(),
                Some(voting_period),
                Some(EXECUTION_DELAY),
                Some(quorum),
                Some(0),
                Some(grace),
                Some(threshold),
            )
        });
        assert_eq!(result, Err(expected));
    }
}

#[test]
fn test_votes_only_accepted_during_the_voting_window() {
    let (env, client, token, _) = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    fund(&env, &client, &token, &proposer, 1_000);
    fund(&env, &client, &token, &voter, 1_000);
    advance(&env, 1);

    let id = propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
    let proposal = client.gov_get_proposal(&id).unwrap();
    assert_eq!(proposal.end_time - proposal.start_time, VOTING_PERIOD);
    assert_eq!(
        client.gov_get_proposal_state(&id),
        Some(ProposalStatus::Active)
    );

    env.ledger().set_timestamp(proposal.end_time);
    assert_eq!(
        as_gov(&client, || governance::vote(
            &env,
            voter.clone(),
            id,
            VoteType::For
        )),
        Err(GovernanceError::NotInVotingPeriod)
    );
    // Queueing is not possible until the window has closed.
    env.ledger().set_timestamp(proposal.end_time - 1);
    assert_eq!(
        as_gov(&client, || governance::queue_proposal(
            &env,
            voter.clone(),
            id
        )),
        Err(GovernanceError::VotingNotEnded)
    );
}

#[test]
fn test_successful_proposal_expires_if_not_queued_within_grace() {
    let (env, client, token, _) = setup();
    let (proposer, _, id) =
        passing_proposal(&env, &client, &token, ProposalType::EmergencyPause(true));

    let proposal = client.gov_get_proposal(&id).unwrap();
    env.ledger().set_timestamp(proposal.end_time);
    assert_eq!(
        client.gov_get_proposal_state(&id),
        Some(ProposalStatus::Succeeded)
    );

    env.ledger()
        .set_timestamp(proposal.end_time + GRACE_PERIOD + 1);
    assert_eq!(
        client.gov_get_proposal_state(&id),
        Some(ProposalStatus::Expired)
    );
    assert_eq!(
        as_gov(&client, || governance::queue_proposal(
            &env,
            proposer.clone(),
            id
        )),
        Err(GovernanceError::ProposalExpired)
    );
}

// ============================================================================
// Timelock execution (#1085)
// ============================================================================

#[test]
fn test_queued_proposal_executes_only_inside_the_timelock_window() {
    let (env, client, token, _) = setup();
    let (_, voter, id) =
        passing_proposal(&env, &client, &token, ProposalType::EmergencyPause(true));
    end_voting_and_queue(&env, &client, id);

    let proposal = client.gov_get_proposal(&id).unwrap();
    let eta = proposal.execution_time.unwrap();
    assert_eq!(eta, proposal.end_time + EXECUTION_DELAY);
    assert_eq!(
        client.gov_get_proposal_state(&id),
        Some(ProposalStatus::Queued)
    );

    env.ledger().set_timestamp(eta - 1);
    assert_eq!(
        as_gov(&client, || governance::execute_proposal(
            &env,
            voter.clone(),
            id
        )),
        Err(GovernanceError::ExecutionTooEarly)
    );

    env.ledger().set_timestamp(eta + GRACE_PERIOD + 1);
    assert_eq!(
        client.gov_get_proposal_state(&id),
        Some(ProposalStatus::Expired)
    );
    assert!(client.try_gov_execute_proposal(&voter, &id).is_err());

    env.ledger().set_timestamp(eta + GRACE_PERIOD);
    client.gov_execute_proposal(&voter, &id);
    assert_eq!(
        client.gov_get_proposal_state(&id),
        Some(ProposalStatus::Executed)
    );
    assert!(env.as_contract(&client.address, || {
        crate::risk_management::is_emergency_paused(&env)
    }));
}

#[test]
fn test_proposal_cannot_execute_twice() {
    let (env, client, token, _) = setup();
    let (_, voter, id) =
        passing_proposal(&env, &client, &token, ProposalType::EmergencyPause(true));
    end_voting_and_queue(&env, &client, id);
    env.ledger().set_timestamp(
        client
            .gov_get_proposal(&id)
            .unwrap()
            .execution_time
            .unwrap(),
    );

    client.gov_execute_proposal(&voter, &id);
    assert_eq!(
        as_gov(&client, || governance::execute_proposal(
            &env,
            voter.clone(),
            id
        )),
        Err(GovernanceError::NotQueued)
    );
}

#[test]
fn test_guardian_can_veto_a_queued_proposal() {
    let (env, client, token, admin) = setup();
    let guardian = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.gov_add_guardian(&admin, &guardian);

    let (_, voter, id) =
        passing_proposal(&env, &client, &token, ProposalType::EmergencyPause(true));
    end_voting_and_queue(&env, &client, id);

    assert_eq!(
        as_gov(&client, || governance::cancel_proposal(
            &env,
            stranger.clone(),
            id
        )),
        Err(GovernanceError::Unauthorized)
    );
    client.gov_cancel_proposal(&guardian, &id);
    assert_eq!(
        client.gov_get_proposal_state(&id),
        Some(ProposalStatus::Cancelled)
    );

    env.ledger().set_timestamp(
        client
            .gov_get_proposal(&id)
            .unwrap()
            .execution_time
            .unwrap(),
    );
    assert_eq!(
        as_gov(&client, || governance::execute_proposal(
            &env,
            voter.clone(),
            id
        )),
        Err(GovernanceError::NotQueued)
    );
}

#[test]
fn test_emergency_proposal_requires_multisig_threshold() {
    let (env, client, _, admin) = setup();
    let admin2 = Address::generate(&env);
    let stranger = Address::generate(&env);

    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.gov_set_multisig_config(&admin, &admins, &2);

    let id = client.gov_create_emergency_proposal(
        &admin,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "emergency"),
    );
    assert!(client.gov_get_proposal(&id).unwrap().emergency);

    // The creator's approval alone is below the threshold of two.
    assert_eq!(
        as_gov(&client, || governance::execute_proposal(
            &env,
            admin.clone(),
            id
        )),
        Err(GovernanceError::InsufficientApprovals)
    );

    client.gov_approve_proposal(&admin2, &id);
    assert_eq!(
        as_gov(&client, || governance::execute_proposal(
            &env,
            stranger.clone(),
            id
        )),
        Err(GovernanceError::Unauthorized)
    );

    client.gov_execute_proposal(&admin, &id);
    assert_eq!(
        client.gov_get_proposal(&id).unwrap().status,
        ProposalStatus::Executed
    );
}

#[test]
fn test_governance_config_changes_through_the_timelock() {
    let (env, client, token, _) = setup();
    let new_params = GovernanceParams {
        voting_period: Some(2 * 24 * 3600),
        execution_delay: None,
        quorum_bps: Some(5_000),
        proposal_threshold: None,
        timelock_duration: None,
        default_voting_threshold: None,
    };
    let (proposer, voter, id) = passing_proposal(
        &env,
        &client,
        &token,
        ProposalType::UpdateGovernanceConfig(new_params),
    );
    // A proposal in flight keeps the rules it was created under.
    let in_flight = propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
    let in_flight_before = client.gov_get_proposal(&in_flight).unwrap();

    end_voting_and_queue(&env, &client, id);
    env.ledger().set_timestamp(
        client
            .gov_get_proposal(&id)
            .unwrap()
            .execution_time
            .unwrap(),
    );
    client.gov_execute_proposal(&voter, &id);

    let config = client.gov_get_config().unwrap();
    assert_eq!(config.voting_period, 2 * 24 * 3600);
    assert_eq!(config.quorum_bps, 5_000);
    assert_eq!(config.execution_delay, EXECUTION_DELAY);
    assert_eq!(
        client.gov_get_proposal(&in_flight).unwrap(),
        in_flight_before
    );

    let next = propose(&env, &client, &proposer, ProposalType::EmergencyPause(true));
    let next = client.gov_get_proposal(&next).unwrap();
    assert_eq!(next.end_time - next.start_time, 2 * 24 * 3600);
    assert_eq!(next.quorum_votes, 5_500); // 50% of 11,000 locked
}

#[test]
fn test_invalid_config_update_fails_at_execution() {
    let (env, client, token, _) = setup();
    let bad_params = GovernanceParams {
        voting_period: None,
        execution_delay: None,
        quorum_bps: Some(0),
        proposal_threshold: None,
        timelock_duration: None,
        default_voting_threshold: None,
    };
    let (_, voter, id) = passing_proposal(
        &env,
        &client,
        &token,
        ProposalType::UpdateGovernanceConfig(bad_params),
    );
    end_voting_and_queue(&env, &client, id);
    env.ledger().set_timestamp(
        client
            .gov_get_proposal(&id)
            .unwrap()
            .execution_time
            .unwrap(),
    );

    assert!(client.try_gov_execute_proposal(&voter, &id).is_err());
    assert_eq!(client.gov_get_config().unwrap().quorum_bps, QUORUM_BPS);
    assert_eq!(
        client.gov_get_proposal(&id).unwrap().status,
        ProposalStatus::Queued
    );
}
