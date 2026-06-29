#[cfg(test)]
mod tests {
    use soroban_sdk::{Env, String};

    use crate::{SNSIntegration, SNSIntegrationClient};

    #[test]
    fn test_register_and_resolve_name() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SNSIntegration);
        let client = SNSIntegrationClient::new(&env, &contract_id);

        let admin = env.mock_all_auths();
        let user = env.mock_all_auths();
        let name = String::from_slice(&env, "testname");
        let address = user.clone();

        // Initialize
        client.initialize(&admin, &String::from_slice(&env, "3600"), &365);

        // Register name
        let result = client.register_name(&name, &address);
        assert!(result.is_ok());

        // Resolve name
        let resolved = client.resolve_name(&name);
        assert_eq!(resolved.unwrap(), address);
    }

    #[test]
    fn test_name_expiry() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SNSIntegration);
        let client = SNSIntegrationClient::new(&env, &contract_id);

        let admin = env.mock_all_auths();
        let user = env.mock_all_auths();

        client.initialize(&admin, &String::from_slice(&env, "3600"), &365);

        let name = String::from_slice(&env, "expiring");
        client.register_name(&name, &user);

        // Check if expired (should be false immediately after registration)
        let is_expired = client.is_name_expired(&name);
        assert!(!is_expired.unwrap());
    }

    #[test]
    fn test_invalid_name() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SNSIntegration);
        let client = SNSIntegrationClient::new(&env, &contract_id);

        let admin = env.mock_all_auths();
        client.initialize(&admin, &String::from_slice(&env, "3600"), &365);

        let empty_name = String::from_slice(&env, "");
        let user = env.mock_all_auths();

        let result = client.register_name(&empty_name, &user);
        assert!(result.is_err());
    }

    #[test]
    fn test_name_renewal() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SNSIntegration);
        let client = SNSIntegrationClient::new(&env, &contract_id);

        let admin = env.mock_all_auths();
        let user = env.mock_all_auths();

        client.initialize(&admin, &String::from_slice(&env, "3600"), &365);

        let name = String::from_slice(&env, "renew");
        client.register_name(&name, &user);

        // Renew name
        let result = client.renew_name(&name);
        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_resolution() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SNSIntegration);
        let client = SNSIntegrationClient::new(&env, &contract_id);

        let admin = env.mock_all_auths();
        let user1 = env.mock_all_auths();
        let user2 = env.mock_all_auths();

        client.initialize(&admin, &String::from_slice(&env, "3600"), &365);

        let name1 = String::from_slice(&env, "user1");
        let name2 = String::from_slice(&env, "user2");

        client.register_name(&name1, &user1);
        client.register_name(&name2, &user2);

        // Test batch resolution would require Vec implementation
        // Simplified test here
        let result1 = client.resolve_name(&name1);
        let result2 = client.resolve_name(&name2);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[test]
    fn test_analytics() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SNSIntegration);
        let client = SNSIntegrationClient::new(&env, &contract_id);

        let admin = env.mock_all_auths();
        client.initialize(&admin, &String::from_slice(&env, "3600"), &365);

        let analytics = client.get_analytics();
        assert!(analytics.is_ok());
    }
}
