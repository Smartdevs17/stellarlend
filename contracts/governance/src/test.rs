#[cfg(test)]
use super::quorum::{verify_quorum, verify_voting_period};
use soroban_sdk::Env;

#[test]
test quorum_and_voting_periods() {
    let env = Env::default();
    
    // Quorum tests
    assert!(verify_quorum(&env, 1500, 1000));
    assert!(!verify_quorum(&env, 500, 1000));

    // Voting period tests
    assert!(verify_voting_period(&env, 100, 50, 120));
    assert!(!verify_voting_period(&env, 100, 50, 160));
}
