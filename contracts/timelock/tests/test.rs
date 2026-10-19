#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, vec, Address, Env, Symbol, Val};

    #[test]
    fn test_timelock_schedule_and_execute() {
        let env = Env::default();
        let contract_id = env.register_contract(None, TimelockContract);
        let client = TimelockContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);
        let target = Address::generate(&env);

        client.initialize(&admin, &executor, &100);

        let eta = env.ledger().timestamp() + 100;
        let id = client.schedule(&admin, &target, &symbol!("test_func"), &vec![&env, Val::U64(123)], &eta);

        env.ledger().set_timestamp(eta);
        client.execute(&executor, &id);

        assert!(true);
    }

    #[test]
    #[should_panic(expected = "Not authorized")]
    fn test_timelock_unauthorized_schedule() {
        let env = Env::default();
        let contract_id = env.register_contract(None, TimelockContract);
        let client = TimelockContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let unauthorized = Address::generate(&env);
        let target = Address::generate(&env);

        client.initialize(&admin, &unauthorized, &100);
        let eta = env.ledger().timestamp() + 100;
        client.schedule(&unauthorized, &target, &symbol!("test_func"), &vec![&env, Val::U64(123)], &eta);
    }

    #[test]
    #[should_panic(expected = "Timelock not expired")]
    fn test_timelock_early_execution() {
        let env = Env::default();
        let contract_id = env.register_contract(None, TimelockContract);
        let client = TimelockContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let executor = Address::generate(&env);
        let target = Address::generate(&env);

        client.initialize(&admin, &executor, &100);

        let eta = env.ledger().timestamp() + 100;
        let id = client.schedule(&admin, &target, &symbol!("test_func"), &vec![&env, Val::U64(123)], &eta);

        client.execute(&executor, &id);
    }
}
