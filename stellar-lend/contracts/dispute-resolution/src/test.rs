use super::*;
use soroban_sdk::{
    testutils::{Address as _, BytesN as _},
    vec, Address, BytesN, Env, String,
};

fn setup_test(env: &Env) -> (Address, DisputeResolutionContractClient<'_>) {
    let admin = Address::generate(env);
    let contract_id = env.register(DisputeResolutionContract, ());
    let client = DisputeResolutionContractClient::new(env, &contract_id);
    client.initialize(&admin);
    (admin, client)
}

#[test]
fn test_admin_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_test(&env);
    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_cannot_reinitialize() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_test(&env);
    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "cannot dispute against self")]
fn test_cannot_dispute_against_self() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_test(&env);
    let user = Address::generate(&env);
    let tx = BytesN::random(&env);
    client.file_dispute(&user, &user, &tx, &1000, &tx);
}

#[test]
fn test_dispute_lifecycle_supermajority() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_test(&env);

    let disputer = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let juror1 = Address::generate(&env);
    let juror2 = Address::generate(&env);
    let juror3 = Address::generate(&env);

    client.register_juror(&juror1);
    client.register_juror(&juror2);
    client.register_juror(&juror3);

    let tx = BytesN::random(&env);
    let dispute_id = client.file_dispute(&disputer, &liquidator, &tx, &10_000, &tx);
    assert_eq!(dispute_id, 1);

    // Submit evidence
    client.submit_evidence(
        &dispute_id,
        &disputer,
        &String::from_str(&env, "Unfair liquidation evidence"),
        &tx,
    );

    // Select jurors
    let selected = vec![&env, juror1.clone(), juror2.clone(), juror3.clone()];
    client.select_jurors(&dispute_id, &selected);

    let dispute = client.get_dispute(&dispute_id).unwrap();
    assert_eq!(dispute.status, DisputeStatus::Voting);
    assert_eq!(dispute.jurors.len(), 3);

    // Juror 1 votes Valid
    client.cast_vote(
        &dispute_id,
        &juror1,
        &VoteChoice::Valid,
        &String::from_str(&env, "Price manipulated"),
    );

    // Juror 2 votes Valid
    client.cast_vote(
        &dispute_id,
        &juror2,
        &VoteChoice::Valid,
        &String::from_str(&env, "Agreed"),
    );

    // Juror 3 votes Invalid -> total votes = 3, valid = 2 (66% supermajority achieved)
    client.cast_vote(
        &dispute_id,
        &juror3,
        &VoteChoice::Invalid,
        &String::from_str(&env, "Disagree"),
    );

    let resolved = client.get_dispute(&dispute_id).unwrap();
    assert_eq!(resolved.status, DisputeStatus::Resolved);
    assert_eq!(resolved.resolution, 1); // 1 = Valid
    assert!(resolved.resolved_at.is_some());
}

#[test]
#[should_panic(expected = "insufficient jurors")]
fn test_juror_selection_excludes_liquidator_and_disputer() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_test(&env);

    let disputer = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let juror1 = Address::generate(&env);
    let juror2 = Address::generate(&env);

    // Register disputer and liquidator as jurors in the system
    client.register_juror(&disputer);
    client.register_juror(&liquidator);
    client.register_juror(&juror1);
    client.register_juror(&juror2);

    let tx = BytesN::random(&env);
    let dispute_id = client.file_dispute(&disputer, &liquidator, &tx, &5_000, &tx);

    // Attempt to pass disputer and liquidator in selected list
    let selected = vec![&env, disputer.clone(), liquidator.clone(), juror1.clone(), juror2.clone()];
    
    // Disputer and liquidator are filtered out, leaving only juror1 and juror2 (2 jurors < 3 required)
    // which panics with "insufficient jurors"
    client.select_jurors(&dispute_id, &selected);
}

#[test]
#[should_panic(expected = "insufficient jurors")]
fn test_juror_selection_deduplicates_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_test(&env);

    let disputer = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let juror1 = Address::generate(&env);

    client.register_juror(&juror1);

    let tx = BytesN::random(&env);
    let dispute_id = client.file_dispute(&disputer, &liquidator, &tx, &5_000, &tx);

    // Pass the same juror 3 times
    let selected = vec![&env, juror1.clone(), juror1.clone(), juror1.clone()];
    
    // Deduplication reduces list to 1, failing "insufficient jurors" (< 3)
    client.select_jurors(&dispute_id, &selected);
}

#[test]
fn test_voting_deadlock_resolution_when_all_voted() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_test(&env);

    let disputer = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let j1 = Address::generate(&env);
    let j2 = Address::generate(&env);
    let j3 = Address::generate(&env);
    let j4 = Address::generate(&env);
    let j5 = Address::generate(&env);

    client.register_juror(&j1);
    client.register_juror(&j2);
    client.register_juror(&j3);
    client.register_juror(&j4);
    client.register_juror(&j5);

    let tx = BytesN::random(&env);
    let dispute_id = client.file_dispute(&disputer, &liquidator, &tx, &10_000, &tx);

    let selected = vec![&env, j1.clone(), j2.clone(), j3.clone(), j4.clone(), j5.clone()];
    client.select_jurors(&dispute_id, &selected);

    // Votes: 3 Valid, 2 Invalid (60% valid, 40% invalid; neither reaches >= 66% supermajority)
    client.cast_vote(&dispute_id, &j1, &VoteChoice::Valid, &String::from_str(&env, "ok"));
    client.cast_vote(&dispute_id, &j2, &VoteChoice::Invalid, &String::from_str(&env, "no"));
    client.cast_vote(&dispute_id, &j3, &VoteChoice::Valid, &String::from_str(&env, "ok"));
    client.cast_vote(&dispute_id, &j4, &VoteChoice::Invalid, &String::from_str(&env, "no"));

    // Dispute should still be voting after 4 votes (2 vs 2)
    assert_eq!(client.get_dispute(&dispute_id).unwrap().status, DisputeStatus::Voting);

    // 5th juror votes Valid -> all 5 have voted (3 Valid, 2 Invalid)
    client.cast_vote(&dispute_id, &j5, &VoteChoice::Valid, &String::from_str(&env, "ok"));

    // Deadlock resolved via simple majority since all selected jurors voted
    let resolved = client.get_dispute(&dispute_id).unwrap();
    assert_eq!(resolved.status, DisputeStatus::Resolved);
    assert_eq!(resolved.resolution, 1);
}

#[test]
#[should_panic(expected = "only disputer or liquidator can appeal")]
fn test_unauthorized_third_party_cannot_appeal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_test(&env);

    let disputer = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let random_user = Address::generate(&env);
    let j1 = Address::generate(&env);
    let j2 = Address::generate(&env);
    let j3 = Address::generate(&env);

    client.register_juror(&j1);
    client.register_juror(&j2);
    client.register_juror(&j3);

    let tx = BytesN::random(&env);
    let dispute_id = client.file_dispute(&disputer, &liquidator, &tx, &10_000, &tx);
    let selected = vec![&env, j1.clone(), j2.clone(), j3.clone()];
    client.select_jurors(&dispute_id, &selected);

    // Resolve as Invalid (3 votes)
    client.cast_vote(&dispute_id, &j1, &VoteChoice::Invalid, &String::from_str(&env, "no"));
    client.cast_vote(&dispute_id, &j2, &VoteChoice::Invalid, &String::from_str(&env, "no"));
    client.cast_vote(&dispute_id, &j3, &VoteChoice::Invalid, &String::from_str(&env, "no"));

    let dispute = client.get_dispute(&dispute_id).unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);

    // Random unauthorized third party cannot appeal -> should panic
    client.appeal(&dispute_id, &random_user, &2_000);
}

#[test]
fn test_authorized_appeal_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_test(&env);

    let disputer = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let j1 = Address::generate(&env);
    let j2 = Address::generate(&env);
    let j3 = Address::generate(&env);

    client.register_juror(&j1);
    client.register_juror(&j2);
    client.register_juror(&j3);

    let tx = BytesN::random(&env);
    let dispute_id = client.file_dispute(&disputer, &liquidator, &tx, &10_000, &tx);
    let selected = vec![&env, j1.clone(), j2.clone(), j3.clone()];
    client.select_jurors(&dispute_id, &selected);

    // Resolve as Invalid (3 votes)
    client.cast_vote(&dispute_id, &j1, &VoteChoice::Invalid, &String::from_str(&env, "no"));
    client.cast_vote(&dispute_id, &j2, &VoteChoice::Invalid, &String::from_str(&env, "no"));
    client.cast_vote(&dispute_id, &j3, &VoteChoice::Invalid, &String::from_str(&env, "no"));

    let dispute = client.get_dispute(&dispute_id).unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);

    // Disputer can appeal with required stake (2x fee = 2,000)
    let appeal_id = client.appeal(&dispute_id, &disputer, &2_000);
    assert_eq!(appeal_id, 2);

    let appealed_dispute = client.get_dispute(&appeal_id).unwrap();
    assert_eq!(appealed_dispute.appeal_parent, Some(dispute_id));
    assert_eq!(appealed_dispute.status, DisputeStatus::Filing);
}
