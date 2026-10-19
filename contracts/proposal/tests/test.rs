#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, vec, Address, Env, Symbol, Val};

    #[test]
    fn test_approve_and_schedule() {
        let env = Env::default();

        let timelock_contract_id = env.register_contract(None, timelock::TimelockContract);
        let proposal_contract_id = env.register_contract(None, ProposalContract);

        let timelock_client = timelock::TimelockContractClient::new(&env, &timelock_contract_id);
        let proposal_client = ProposalContractClient::new(&env, &proposal_contract_id);

        let admin = Address::generate(&env);
        let target = Address::generate(&env);

        timelock_client.initialize(&admin, &admin, &100);

        let proposal_id = 1;
        let delay = 100;
        let eta = env.ledger().timestamp() + delay;

        proposal_client.approve_and_schedule(
            &admin,
            &proposal_id,
            &timelock_contract_id,
            &target,
            &symbol!("test_func"),
            &vec![&env, Val::U64(123)],
            &delay,
        );

        let status: Symbol = env.storage().persistent().get(&(proposal_id, symbol!("STATUS"))).unwrap();
        assert_eq!(status, symbol!("APPROVED"));

        let timelock_id: u64 = env.storage().persistent().get(&(proposal_id, symbol!("TIMELOCK_ID"))).unwrap();
        assert_eq!(timelock_id, eta);
    }
}
