#[cfg(test)]
mod tests {
    use super::super::quorum::{verify_quorum, verify_voting_period, verify_quorum_with_threshold, set_quorum_config, get_quorum_config};
    use soroban_sdk::Env;

    #[test]
    fn test_quorum_and_voting_periods() {
        let env = Env::default();

        // Quorum tests with threshold
        assert!(verify_quorum_with_threshold(&env, 1500, 1000));
        assert!(!verify_quorum_with_threshold(&env, 500, 1000));

        // Voting period tests
        assert!(verify_voting_period(&env, 100, 50, 120));
        assert!(!verify_voting_period(&env, 100, 50, 160));
    }

    #[test]
    fn test_configurable_quorum() {
        let env = Env::default();

        // Test default quorum
        assert_eq!(get_quorum_config(&env), 1000);

        // Set custom quorum
        set_quorum_config(&env, 2000);
        assert_eq!(get_quorum_config(&env), 2000);

        // Verify quorum with configured threshold
        assert!(verify_quorum(&env, 2500));
        assert!(!verify_quorum(&env, 1500));
    }
}
