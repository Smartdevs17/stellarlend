// SPDX-License-Identifier: MIT

#![no_std]

use soroban_sdk::{contract, contractimpl, symbol, vec, Address, Env, Symbol, Val, Vec};
use timelock::TimelockContractClient;

#[contract]
pub struct ProposalContract;

#[contractimpl]
impl ProposalContract {
    pub fn create_proposal(
        env: Env,
        creator: Address,
        title: Symbol,
        description: Symbol,
    ) -> u64 {
        creator.require_auth();
        let id = env.ledger().sequence() - 1;
        env.storage().persistent().set(&(id, symbol!("CREATOR")), &creator);
        env.storage().persistent().set(&(id, symbol!("TITLE")), &title);
        env.storage().persistent().set(&(id, symbol!("DESCRIPTION")), &description);
        env.storage().persistent().set(&(id, symbol!("STATUS")), &symbol!("PENDING"));
        id
    }

    pub fn vote(env: Env, voter: Address, proposal_id: u64, supports: bool) {
        voter.require_auth();
        let mut votes: Vec<(Address, bool)> = env
            .storage()
            .persistent()
            .get(&(proposal_id, symbol!("VOTES")))
            .unwrap_or(vec![&env]);
        for (existing_voter, _) in votes.iter() {
            if existing_voter == voter {
                panic!("already voted");
            }
        }
        votes.push_back((voter.clone(), supports));
        env.storage().persistent().set(&(proposal_id, symbol!("VOTES")), &votes);
        env.storage().persistent().set(&(proposal_id, symbol!("STATUS")), &symbol!("VOTED"));
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> (Symbol, Symbol) {
        let title: Symbol = env
            .storage()
            .persistent()
            .get(&(proposal_id, symbol!("TITLE")))
            .unwrap_or(symbol!(""));
        let description: Symbol = env
            .storage()
            .persistent()
            .get(&(proposal_id, symbol!("DESCRIPTION")))
            .unwrap_or(symbol!(""));
        (title, description)
    }

    pub fn approve_and_schedule(
        env: Env,
        caller: Address,
        proposal_id: u64,
        timelock_contract: Address,
        target: Address,
        function: Symbol,
        args: Vec<Val>,
        delay: u64,
    ) {
        caller.require_auth();

        let client = TimelockContractClient::new(&env, &timelock_contract);
        let eta = env.ledger().timestamp() + delay;

        client.schedule(&caller, &target, &function, &args, &eta);

        env.storage().persistent().set(&(proposal_id, symbol!("STATUS")), &symbol!("APPROVED"));
        env.storage().persistent().set(&(proposal_id, symbol!("TIMELOCK_ID")), &eta);
    }
}