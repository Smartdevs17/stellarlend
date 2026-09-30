// SPDX-License-Identifier: MIT

#![no_std]

use soroban_sdk::{contract, contractimpl, symbol, Address, Env, Symbol, Vec};
use timelock::TimelockContractClient;

#[contract]
pub struct ProposalContract;

#[contractimpl]
impl ProposalContract {
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
