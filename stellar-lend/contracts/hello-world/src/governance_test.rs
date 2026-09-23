#![cfg(test)]

use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String, Vec};

use crate::{
    errors::GovernanceError,
    governance,
    storage::{GovernanceDataKey, GuardianConfig},
    types::{
        DelegationRecord, GovernanceConfig, MultisigConfig, Proposal, ProposalOutcome,
        ProposalStatus, ProposalType, VoteInfo, VoteType,
    },
    HelloContract, HelloContractClient,
};

use soroban_sdk::token::StellarAssetClient;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_test_token(env: &Env, admin: &Address) -> Address {
    let token = env.register_stellar_asset_contract(admin.clone());
    let token_sac = StellarAssetClient::new(env, &token);
    token_sac.mint(admin, &10_000_000_i128);
    token
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let token_sac = StellarAssetClient::new(env, token);
    token_sac.mint(to, &amount);
}

fn setup_governance<'a>(
    env: &'a Env,
    admin: &'a Address,
    vote_token: &'a Address,
) -> HelloContractClient<'a> {
    let contract_id = env.register_contract(None, HelloContract);
    let client = HelloContractClient::new(env, &contract_id);

    env.mock_all_auths();

    client.initialize(admin);

    client.gov_initialize(
        admin,
        vote_token,
        &Some(259200), // 3 days voting period
        &Some(86400),  // 1 day execution delay
        &Some(4000),   // 40% quorum
        &Some(100),    // proposal threshold
        &Some(604800), // 7 days timelock
        &Some(5000),   // 50% voting threshold
    );

    client
}

fn create_test_env() -> (Env, Address, Address, Address, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    (env, admin, proposer, voter1, voter2, voter3)
}

fn create_token_with_balance(env: &Env, admin: &Address, to: &Address, amount: i128) -> Address {
    let token = env.register_stellar_asset_contract(admin.clone());
    StellarAssetClient::new(env, &token).mint(to, &amount);
    token
}

fn init_governance(env: &Env, admin: &Address, vote_token: &Address) {
    governance::initialize(
        env,
        admin.clone(),
        vote_token.clone(),
        Some(300),   // voting_period: 300s
        Some(100),   // execution_delay: 100s
        Some(4000),  // quorum_bps: 40%
        Some(0),     // proposal_threshold: 0 (no token requirement to create)
        Some(1000),  // timelock_duration: 1000s
        Some(5000),  // default_voting_threshold: 50%
    )
    .unwrap();
}

// ============================================================================
// PHASE 1: PROPOSAL LIFECYCLE TESTS
// ============================================================================

#[test]
fn test_phase1_proposal_creation_basic() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Test proposal"),
        &None,
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.id, 0);
    assert_eq!(proposal.proposer, proposer);
    assert!(matches!(proposal.status, ProposalStatus::Pending));
}

#[test]
fn test_phase1_proposal_parameters_validation() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Test"),
        &None,
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    let config = client.gov_get_config().unwrap();

    assert_eq!(proposal.voting_threshold, config.default_voting_threshold);
    assert_eq!(
        proposal.end_time - proposal.start_time,
        config.voting_period
    );
}

#[test]
fn test_phase1_proposal_id_increment() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let id1 = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "First"),
        &None,
    );
    let id2 = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(false),
        &String::from_str(&env, "Second"),
        &None,
    );

    assert_eq!(id1, 0);
    assert_eq!(id2, 1);
}

#[test]
fn test_phase1_proposal_state_transitions() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    mint_tokens(&env, &token, &voter2, 5000);
    mint_tokens(&env, &token, &voter3, 5000);
    let client = setup_governance(&env, &admin, &token);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Lifecycle test"),
        &None,
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert!(matches!(proposal.status, ProposalStatus::Pending));

    env.ledger().set_timestamp(proposal.start_time + 1);
    client.gov_vote(&voter1, &proposal_id, &VoteType::For);

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert!(matches!(proposal.status, ProposalStatus::Active));

    env.ledger().set_timestamp(proposal.end_time + 1);
    let outcome = client.gov_queue_proposal(&admin, &proposal_id).unwrap();
    assert!(outcome.succeeded);

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert!(matches!(proposal.status, ProposalStatus::Queued));
}

#[test]
fn test_phase1_proposal_retrieval() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Retrieve me"),
        &None,
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.description.to_buffer(), "Retrieve me".as_bytes());

    let missing = client.gov_get_proposal(&999);
    assert!(missing.is_none());
}

#[test]
fn test_phase1_proposal_with_custom_voting_period() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);

    let contract_id = env.register_contract(None, HelloContract);
    let client = HelloContractClient::new(&env, &contract_id);
    env.mock_all_auths();
    client.initialize(&admin);
    client.gov_initialize(
        &admin,
        &token,
        &Some(86400),
        &Some(86400),
        &Some(4000),
        &Some(100),
        &Some(604800),
        &Some(5000),
    );

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Custom period"),
        &None,
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.end_time - proposal.start_time, 86400);
}

#[test]
fn test_phase1_proposal_with_custom_timelock() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);

    let contract_id = env.register_contract(None, HelloContract);
    let client = HelloContractClient::new(&env, &contract_id);
    env.mock_all_auths();
    client.initialize(&admin);
    client.gov_initialize(
        &admin,
        &token,
        &Some(259200),
        &Some(86400),
        &Some(4000),
        &Some(100),
        &Some(1209600), // 14 days timelock
        &Some(5000),
    );

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Custom timelock"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &proposal_id, &VoteType::For);

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    env.ledger().set_timestamp(proposal.end_time + 1);
    client.gov_queue_proposal(&admin, &proposal_id).unwrap();

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    let exec_time = proposal.execution_time.unwrap();

    let result = client.try_gov_execute_proposal(&admin, &proposal_id);
    assert!(result.is_err());

    env.ledger().set_timestamp(exec_time);
    let result = client.try_gov_execute_proposal(&admin, &proposal_id);
    assert!(result.is_ok());
}

#[test]
fn test_phase1_proposal_with_custom_threshold() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Custom threshold"),
        &Some(7000), // 70% threshold
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.voting_threshold, 7000);
}

#[test]
fn test_phase1_proposal_description_storage() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let desc = String::from_str(&env, "Unique description for storage test");
    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &desc,
        &None,
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.description, desc);
}

#[test]
fn test_phase1_proposer_address_tracking() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Track proposer"),
        &None,
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.proposer, proposer);
}

#[test]
fn test_phase1_proposal_timestamp_recording() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    env.ledger().set_timestamp(1000);
    let proposal_id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Timestamp test"),
        &None,
    );

    let proposal = client.gov_get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.created_at, 1000);
    assert_eq!(proposal.start_time, 1000);
}

#[test]
fn test_phase1_proposal_type_handling() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let id = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Emergency pause"),
        &None,
    );
    let p = client.gov_get_proposal(&id).unwrap();
    assert!(matches!(p.proposal_type, ProposalType::EmergencyPause(true)));

    let id2 = client.gov_create_proposal(
        &proposer,
        &ProposalType::MinCollateralRatio(15000),
        &String::from_str(&env, "Min CR"),
        &None,
    );
    let p2 = client.gov_get_proposal(&id2).unwrap();
    assert!(matches!(p2.proposal_type, ProposalType::MinCollateralRatio(15000)));
}

// ============================================================================
// PHASE 2: VOTING MECHANICS TESTS
// ============================================================================

#[test]
fn test_phase2_vote_for_casting() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 500);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Vote For"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.for_votes, 500);
    assert_eq!(p.against_votes, 0);
}

#[test]
fn test_phase2_vote_against_casting() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 300);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Vote Against"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::Against);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.against_votes, 300);
    assert_eq!(p.for_votes, 0);
}

#[test]
fn test_phase2_vote_abstain_casting() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 200);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Abstain"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::Abstain);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.abstain_votes, 200);
}

#[test]
fn test_phase2_vote_threshold_calculation() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 600);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Threshold calc"),
        &Some(5000), // 50% threshold
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.voting_threshold, 5000);
    assert_eq!(p.for_votes, 600);
}

#[test]
fn test_phase2_vote_count_incrementing() {
    let (env, admin, proposer, voter1, voter2, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 200);
    mint_tokens(&env, &token, &voter2, 300);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Count inc"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    client.gov_vote(&voter2, &pid, &VoteType::Against);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.total_voting_power, 500);
}

#[test]
fn test_phase2_vote_duplicate_prevention() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 500);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "No dup"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let result = client.try_gov_vote(&voter1, &pid, &VoteType::Against);
    assert!(result.is_err());
}

#[test]
fn test_phase2_vote_during_voting_window() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 500);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Voting window"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    let result = client.try_gov_vote(&voter1, &pid, &VoteType::For);
    assert!(result.is_ok());
}

#[test]
fn test_phase2_vote_after_voting_window() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 500);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "After window"),
        &None,
    );

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 100);

    let result = client.try_gov_vote(&voter1, &pid, &VoteType::For);
    assert!(result.is_err());
}

#[test]
fn test_phase2_vote_authorization_check() {
    let (env, admin, proposer, voter1, voter2, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 500);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Auth check"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    assert!(client.gov_get_vote(&pid, &voter1).is_some());
    assert!(client.gov_get_vote(&pid, &voter2).is_none());
}

#[test]
fn test_phase2_multi_voter_sequential_voting() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 100);
    mint_tokens(&env, &token, &voter2, 200);
    mint_tokens(&env, &token, &voter3, 300);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Multi voter"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    client.gov_vote(&voter2, &pid, &VoteType::Against);
    client.gov_vote(&voter3, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.for_votes, 400);
    assert_eq!(p.against_votes, 200);
}

#[test]
fn test_phase2_vote_power_tracking() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 750);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Power track"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.total_voting_power, 750);
}

#[test]
fn test_phase2_vote_threshold_met() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 600);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Threshold met"),
        &Some(5000),
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let sim = client.gov_simulate_proposal(&pid).unwrap();
    assert!(sim.threshold_met);
}

#[test]
fn test_phase2_vote_threshold_not_met() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 200);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Threshold not met"),
        &Some(5000),
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let sim = client.gov_simulate_proposal(&pid).unwrap();
    assert!(!sim.threshold_met);
}

#[test]
fn test_phase2_voter_list_tracking() {
    let (env, admin, proposer, voter1, voter2, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 100);
    mint_tokens(&env, &token, &voter2, 200);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Voter list"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    client.gov_vote(&voter2, &pid, &VoteType::Against);

    let v1 = client.gov_get_vote(&pid, &voter1).unwrap();
    let v2 = client.gov_get_vote(&pid, &voter2).unwrap();
    assert_eq!(v1.voter, voter1);
    assert_eq!(v2.voter, voter2);
}

#[test]
fn test_phase2_vote_type_diversity() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 100);
    mint_tokens(&env, &token, &voter2, 200);
    mint_tokens(&env, &token, &voter3, 300);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Diversity"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    client.gov_vote(&voter2, &pid, &VoteType::Against);
    client.gov_vote(&voter3, &pid, &VoteType::Abstain);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.for_votes, 100);
    assert_eq!(p.against_votes, 200);
    assert_eq!(p.abstain_votes, 300);
}

// ============================================================================
// PHASE 3: TIMELOCK & EXECUTION TESTS
// ============================================================================

#[test]
fn test_phase3_voting_period_enforcement() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 500);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Period enforce"),
        &None,
    );

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);

    let result = client.try_gov_queue_proposal(&admin, &pid);
    assert!(result.is_ok());
}

#[test]
fn test_phase3_execution_timelock_enforcement() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Timelock enforce"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    let exec_time = p.execution_time.unwrap();

    let result = client.try_gov_execute_proposal(&admin, &pid);
    assert!(result.is_err());

    env.ledger().set_timestamp(exec_time - 1);
    let result = client.try_gov_execute_proposal(&admin, &pid);
    assert!(result.is_err());
}

#[test]
fn test_phase3_state_transition_active_to_passed() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Active to passed"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Active));

    env.ledger().set_timestamp(p.end_time + 1);
    let outcome = client.gov_queue_proposal(&admin, &pid).unwrap();
    assert!(outcome.succeeded);
}

#[test]
fn test_phase3_state_transition_active_to_failed() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 5000);
    mint_tokens(&env, &token, &voter2, 5000);
    mint_tokens(&env, &token, &voter3, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Active to failed"),
        &Some(5000),
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    client.gov_vote(&voter2, &pid, &VoteType::Against);
    client.gov_vote(&voter3, &pid, &VoteType::Against);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    let outcome = client.gov_queue_proposal(&admin, &pid).unwrap();
    assert!(!outcome.succeeded);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Defeated));
}

#[test]
fn test_phase3_state_transition_passed_to_executed() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Passed to executed"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.execution_time.unwrap());
    client.gov_execute_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Executed));
}

#[test]
fn test_phase3_proposal_expiration() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Expire test"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    let exec_time = p.execution_time.unwrap();
    let config = client.gov_get_config().unwrap();
    env.ledger().set_timestamp(exec_time + config.timelock_duration + 1);

    let result = client.try_gov_execute_proposal(&admin, &pid);
    assert!(result.is_err());
}

#[test]
fn test_phase3_execution_timestamp_boundary() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Boundary"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    let exec_time = p.execution_time.unwrap();

    env.ledger().set_timestamp(exec_time);
    let result = client.try_gov_execute_proposal(&admin, &pid);
    assert!(result.is_ok());
}

#[test]
fn test_phase3_cannot_execute_expired() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Cannot exec expired"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    let exec_time = p.execution_time.unwrap();
    let config = client.gov_get_config().unwrap();
    env.ledger().set_timestamp(exec_time + config.timelock_duration + 1);

    let result = client.try_gov_execute_proposal(&admin, &pid);
    assert!(result.is_err());
}

#[test]
fn test_phase3_multi_timelock_scenarios() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid1 = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "First proposal"),
        &None,
    );
    let pid2 = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(false),
        &String::from_str(&env, "Second proposal"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid1, &VoteType::For);
    client.gov_vote(&voter1, &pid2, &VoteType::For);

    let p1 = client.gov_get_proposal(&pid1).unwrap();
    env.ledger().set_timestamp(p1.end_time + 1);
    client.gov_queue_proposal(&admin, &pid1).unwrap();
    client.gov_queue_proposal(&admin, &pid2).unwrap();

    let p1 = client.gov_get_proposal(&pid1).unwrap();
    let p2 = client.gov_get_proposal(&pid2).unwrap();
    assert!(p1.execution_time.is_some());
    assert!(p2.execution_time.is_some());
}

#[test]
fn test_phase3_ledger_timestamp_consistency() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Timestamp consistency"),
        &None,
    );

    let p = client.gov_get_proposal(&pid).unwrap();
    assert_eq!(p.created_at, p.start_time);
    assert!(p.end_time > p.start_time);
}

// ============================================================================
// PHASE 4: MULTISIG OPERATIONS TESTS
// ============================================================================

#[test]
fn test_phase4_multisig_admin_initialization() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.threshold, 1);
    assert_eq!(config.admins.len(), 1);
    assert_eq!(config.admins.get(0).unwrap(), admin);
}

#[test]
fn test_phase4_multisig_add_admin() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let new_admin = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(new_admin.clone());

    client.gov_set_multisig_config(&admin, &admins, &2);

    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.admins.len(), 2);
    assert_eq!(config.threshold, 2);
}

#[test]
fn test_phase4_multisig_remove_admin() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let admin2 = Address::generate(&env);
    let admins: Vec<Address> = Vec::new(&env);
    // Build a list with both admins, then set config
    let mut admin_list = Vec::new(&env);
    admin_list.push_back(admin.clone());
    admin_list.push_back(admin2.clone());
    client.gov_set_multisig_config(&admin, &admin_list, &1);

    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.admins.len(), 2);

    let mut new_admins = Vec::new(&env);
    new_admins.push_back(admin.clone());
    client.gov_set_multisig_config(&admin, &new_admins, &1);

    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.admins.len(), 1);
}

#[test]
fn test_phase4_multisig_cannot_self_remove() {
    let env = create_test_env();
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    init_governance(&env, &admin, &token);

    // Empty admins list is invalid
    let result = governance::set_multisig_config(&env, admin.clone(), Vec::new(&env), 1);
    assert_eq!(result, Err(GovernanceError::InvalidMultisigConfig));
}

#[test]
fn test_phase4_multisig_duplicate_prevention() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin.clone());

    let result = client.try_gov_set_multisig_config(&admin, &admins, &1);
    assert!(result.is_err());
}

#[test]
fn test_phase4_multisig_threshold_validation() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let admin2 = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    let result = client.try_gov_set_multisig_config(&admin, &admins, &0);
    assert!(result.is_err());

    let result = client.try_gov_set_multisig_config(&admin, &admins, &3);
    assert!(result.is_err());
}

#[test]
fn test_phase4_multisig_threshold_increase() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    client.gov_set_multisig_config(&admin, &admins, &3);

    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.threshold, 3);
}

#[test]
fn test_phase4_multisig_threshold_decrease() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let admin2 = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.gov_set_multisig_config(&admin, &admins, &2);

    client.gov_set_multisig_threshold(&admin, &1);
    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.threshold, 1);
}

#[test]
fn test_phase4_multisig_approval_required() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Approval required"),
        &None,
    );

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Pending));
}

#[test]
fn test_phase4_multisig_approval_threshold_met() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let admin2 = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.gov_set_multisig_config(&admin, &admins, &2);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Approval met"),
        &None,
    );

    client.gov_approve_proposal(&admin, &pid);
    client.gov_approve_proposal(&admin2, &pid);

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    let result = client.try_gov_queue_proposal(&admin, &pid);
    assert!(result.is_ok());
}

#[test]
fn test_phase4_multisig_approval_threshold_not_met() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let admin2 = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.gov_set_multisig_config(&admin, &admins, &2);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Approval not met"),
        &None,
    );

    client.gov_approve_proposal(&admin, &pid);

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);

    let approvals = client.gov_get_proposal_approvals(&pid).unwrap();
    assert_eq!(approvals.len(), 1);
}

#[test]
fn test_phase4_multisig_duplicate_approval_prevention() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "No dup approval"),
        &None,
    );

    client.gov_approve_proposal(&admin, &pid);
    let result = client.try_gov_approve_proposal(&admin, &pid);
    assert!(result.is_err());
}

#[test]
fn test_phase4_multisig_transfer_admin() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&admin, &new_admin);

    let stored_admin = client.gov_get_admin().unwrap();
    assert_eq!(stored_admin, new_admin);
}

#[test]
fn test_phase4_multisig_admin_list_tracking() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    client.gov_set_multisig_config(&admin, &admins, &2);

    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.admins.len(), 3);
}

#[test]
fn test_phase4_multisig_authorization_enforcement() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let non_admin = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    client.gov_set_multisig_config(&admin, &admins, &1);

    let pid = Address::generate(&env);
    let result = client.try_gov_approve_proposal(&non_admin, &Address::generate(&env));
    assert!(result.is_err());
}

// ============================================================================
// PHASE 5: ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_phase5_error_unauthorized() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Auth error"),
        &None,
    );

    let attacker = Address::generate(&env);
    let result = client.try_gov_cancel_proposal(&attacker, &pid);
    assert!(result.is_err());
}

#[test]
fn test_phase5_error_proposal_not_found() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let result = client.try_gov_get_proposal(&999);
    assert!(result.is_none());
}

#[test]
fn test_phase5_error_invalid_proposal() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Invalid"),
        &None,
    );

    let result = client.try_gov_execute_proposal(&admin, &pid);
    assert!(result.is_err());
}

#[test]
fn test_phase5_error_invalid_arguments() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let long_desc = String::from_str(&env, &"x".repeat(300));
    let result = client.try_gov_create_proposal(
        &admin,
        &ProposalType::EmergencyPause(true),
        &long_desc,
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_phase5_error_vote_already_cast() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 500);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Already voted"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    let result = client.try_gov_vote(&voter1, &pid, &VoteType::Against);
    assert!(result.is_err());
}

#[test]
fn test_phase5_error_proposal_expired() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Expired"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    let exec_time = p.end_time + 1 + 604800 + 1;
    env.ledger().set_timestamp(exec_time);

    let result = client.try_gov_queue_proposal(&admin, &pid);
    if result.is_ok() {
        let p = client.gov_get_proposal(&pid).unwrap();
        if matches!(p.status, ProposalStatus::Queued) {
            let config = client.gov_get_config().unwrap();
            env.ledger().set_timestamp(exec_time + config.timelock_duration + 1);
            let result = client.try_gov_execute_proposal(&admin, &pid);
            assert!(result.is_err());
        }
    }
}

#[test]
fn test_phase5_error_insufficient_votes() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 100);
    mint_tokens(&env, &token, &voter2, 100);
    mint_tokens(&env, &token, &voter3, 100);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Insufficient votes"),
        &Some(5000),
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);

    let sim = client.gov_simulate_proposal(&pid).unwrap();
    assert!(!sim.would_succeed);
}

#[test]
fn test_phase5_error_state_consistency() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "State consistency"),
        &None,
    );

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Pending));
    assert_eq!(p.id, pid);
}

#[test]
fn test_phase5_error_double_initialization() {
    let env = create_test_env();
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    init_governance(&env, &admin, &token);
    // Double initialization must be rejected
    let result = governance::initialize(&env, admin.clone(), token.clone(), Some(100), Some(10), Some(4000), Some(0), Some(500), Some(5000));
    assert_eq!(result, Err(GovernanceError::AlreadyInitialized));
}

// ============================================================================
// PHASE 6: EVENT VALIDATION TESTS
// ============================================================================

#[test]
fn test_phase6_event_proposal_created() {
    let (env, admin, proposer, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Event test"),
        &None,
    );

    let events = env.events().all();
    let found = events.iter().any(|e| {
        e.topics
            .get(0)
            .map(|t| t.to_buffer() == "proposal_created".as_bytes())
            .unwrap_or(false)
    });
    assert!(found);
}

#[test]
fn test_phase6_event_vote_cast() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 500);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Vote event"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let events = env.events().all();
    let found = events.iter().any(|e| {
        e.topics
            .get(0)
            .map(|t| t.to_buffer() == "vote_cast".as_bytes())
            .unwrap_or(false)
    });
    assert!(found);
}

#[test]
fn test_phase6_event_proposal_executed() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Exec event"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.execution_time.unwrap());
    client.gov_execute_proposal(&admin, &pid).unwrap();

    let events = env.events().all();
    let found = events.iter().any(|e| {
        e.topics
            .get(0)
            .map(|t| t.to_buffer() == "proposal_executed".as_bytes())
            .unwrap_or(false)
    });
    assert!(found);
}

#[test]
fn test_phase6_event_proposal_failed() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 1000);
    mint_tokens(&env, &token, &voter1, 100);
    mint_tokens(&env, &token, &voter2, 5000);
    mint_tokens(&env, &token, &voter3, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Failed event"),
        &Some(5000),
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    client.gov_vote(&voter2, &pid, &VoteType::Against);
    client.gov_vote(&voter3, &pid, &VoteType::Against);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Defeated));
}

// ============================================================================
// PHASE 7: INTEGRATION SCENARIOS TESTS
// ============================================================================

#[test]
fn test_phase7_full_proposal_lifecycle() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    mint_tokens(&env, &token, &voter2, 3000);
    mint_tokens(&env, &token, &voter3, 2000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Full lifecycle"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);
    client.gov_vote(&voter2, &pid, &VoteType::For);
    client.gov_vote(&voter3, &pid, &VoteType::Against);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    let outcome = client.gov_queue_proposal(&admin, &pid).unwrap();
    assert!(outcome.succeeded);

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Queued));

    env.ledger().set_timestamp(p.execution_time.unwrap());
    client.gov_execute_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Executed));
}

#[test]
fn test_phase7_multiple_proposals_concurrent() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid1 = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Concurrent 1"),
        &None,
    );
    let pid2 = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(false),
        &String::from_str(&env, "Concurrent 2"),
        &None,
    );
    let pid3 = client.gov_create_proposal(
        &proposer,
        &ProposalType::MinCollateralRatio(15000),
        &String::from_str(&env, "Concurrent 3"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid1, &VoteType::For);
    client.gov_vote(&voter1, &pid2, &VoteType::Against);
    client.gov_vote(&voter1, &pid3, &VoteType::For);

    let p1 = client.gov_get_proposal(&pid1).unwrap();
    env.ledger().set_timestamp(p1.end_time + 1);

    client.gov_queue_proposal(&admin, &pid1).unwrap();
    client.gov_queue_proposal(&admin, &pid2).unwrap();
    client.gov_queue_proposal(&admin, &pid3).unwrap();

    let p1 = client.gov_get_proposal(&pid1).unwrap();
    assert!(matches!(p1.status, ProposalStatus::Queued));
    let p2 = client.gov_get_proposal(&pid2).unwrap();
    assert!(matches!(p2.status, ProposalStatus::Queued));
    let p3 = client.gov_get_proposal(&pid3).unwrap();
    assert!(matches!(p3.status, ProposalStatus::Queued));
}

#[test]
fn test_phase7_governance_parameter_updates() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::MinCollateralRatio(20000),
        &String::from_str(&env, "Param update"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.execution_time.unwrap());
    client.gov_execute_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Executed));
    assert!(matches!(p.proposal_type, ProposalType::MinCollateralRatio(20000)));
}

#[test]
fn test_phase7_emergency_pause_execution() {
    let (env, admin, proposer, voter1, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "Emergency"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid, &VoteType::For);

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.end_time + 1);
    client.gov_queue_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    env.ledger().set_timestamp(p.execution_time.unwrap());
    client.gov_execute_proposal(&admin, &pid).unwrap();

    let p = client.gov_get_proposal(&pid).unwrap();
    assert!(matches!(p.status, ProposalStatus::Executed));
    assert!(matches!(p.proposal_type, ProposalType::EmergencyPause(true)));
}

#[test]
fn test_phase7_admin_management_workflow() {
    let (env, admin, _, _, _, _) = create_test_env();
    let token = create_test_token(&env, &admin);
    let client = setup_governance(&env, &admin, &token);

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.gov_set_multisig_config(&admin, &admins, &1);

    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.admins.len(), 2);

    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    client.gov_set_multisig_config(&admin, &admins, &2);

    let config = client.gov_get_multisig_config().unwrap();
    assert_eq!(config.admins.len(), 3);
    assert_eq!(config.threshold, 2);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&admin, &new_admin);

    let stored_admin = client.gov_get_admin().unwrap();
    assert_eq!(stored_admin, new_admin);
}

#[test]
fn test_phase7_vote_reversal_scenario() {
    let (env, admin, proposer, voter1, voter2, voter3) = create_test_env();
    let token = create_test_token(&env, &admin);
    mint_tokens(&env, &token, &proposer, 5000);
    mint_tokens(&env, &token, &voter1, 5000);
    mint_tokens(&env, &token, &voter2, 5000);
    mint_tokens(&env, &token, &voter3, 5000);
    let client = setup_governance(&env, &admin, &token);

    let pid1 = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(true),
        &String::from_str(&env, "First proposal"),
        &None,
    );
    let pid2 = client.gov_create_proposal(
        &proposer,
        &ProposalType::EmergencyPause(false),
        &String::from_str(&env, "Second proposal"),
        &None,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.gov_vote(&voter1, &pid1, &VoteType::For);
    client.gov_vote(&voter1, &pid2, &VoteType::For);
    client.gov_vote(&voter2, &pid1, &VoteType::For);
    client.gov_vote(&voter2, &pid2, &VoteType::Against);
    client.gov_vote(&voter3, &pid1, &VoteType::Against);
    client.gov_vote(&voter3, &pid2, &VoteType::For);

    let p1 = client.gov_get_proposal(&pid1).unwrap();
    env.ledger().set_timestamp(p1.end_time + 1);

    let out1 = client.gov_queue_proposal(&admin, &pid1).unwrap();
    let out2 = client.gov_queue_proposal(&admin, &pid2).unwrap();

    assert!(out1.succeeded);
    assert!(out2.succeeded);

    let p1 = client.gov_get_proposal(&pid1).unwrap();
    assert!(matches!(p1.status, ProposalStatus::Queued));
    let p2 = client.gov_get_proposal(&pid2).unwrap();
    assert!(matches!(p2.status, ProposalStatus::Queued));
}
