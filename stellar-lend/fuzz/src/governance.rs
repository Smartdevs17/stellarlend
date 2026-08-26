use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    Address, Env, IntoVal, String, Symbol, Vec,
};

use crate::encoding::{parse_actions, ActionBytes};

const MAX_ACTIONS: usize = 32;
const NUM_USERS: usize = 4;

/// Minimal token contract for governance fuzzing.
#[contract]
pub struct FuzzToken;

#[contractimpl]
impl FuzzToken {
    pub fn initialize(env: Env, admin: Address, supply: i128) {
        env.storage().instance().set(&Symbol::new(&env, "admin"), &admin);
        env.storage().instance().set(&Symbol::new(&env, "supply"), &supply);
    }

    pub fn balance(env: Env, user: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&user)
            .unwrap_or(0i128)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> bool {
        let from_bal = Self::balance(env.clone(), from.clone());
        let to_bal = Self::balance(env.clone(), to.clone());

        if from_bal < amount {
            return false;
        }

        env.storage().persistent().set(&from, &(from_bal - amount));
        env.storage().persistent().set(&to, &(to_bal + amount));
        true
    }
}

#[derive(Clone)]
struct GovernanceHarness {
    env: Env,
    admin: Address,
    contract_id: Address,
    users: [Address; NUM_USERS],
    token_id: Address,
}

impl GovernanceHarness {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| li.timestamp = 1);

        let admin = Address::generate(&env);
        let contract_id = env.register(crate::HelloContract, ());
        let client = crate::HelloContractClient::new(&env, &contract_id);

        let token_id = env.register(FuzzToken, ());

        // Initialize lending contract
        let _ = client.initialize(&admin);

        let users = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];

        Self {
            env,
            admin,
            contract_id,
            users,
            token_id,
        }
    }

    fn user(&self, idx: u8) -> Address {
        self.users[(idx as usize) % NUM_USERS].clone()
    }

    fn act(&self, action: ActionBytes) {
        let client = crate::HelloContractClient::new(&self.env, &self.contract_id);

        match action.kind() % 6 {
            // 0: create_proposal
            0 => {
                let user = self.user(action.user());
                let proposal_type = match action.u32_param() % 3 {
                    0 => crate::types::ProposalType::EmergencyPause(true),
                    1 => crate::types::ProposalType::EmergencyPause(false),
                    _ => crate::types::ProposalType::MinCollateralRatio(15000),
                };
                let desc_len = (action.u64_tail() % 200) as usize;
                let desc = format!("fuzz_proposal_{}", desc_len);
                let description = String::from_str(&self.env, &desc);
                let _ = client.try_gov_create_proposal(
                    &user,
                    &proposal_type,
                    &description,
                    &None,
                );
            }
            // 1: vote
            1 => {
                let user = self.user(action.user());
                let proposal_id = action.u64_tail() % 10;
                let vote_type = match action.u32_param() % 3 {
                    0 => crate::types::VoteType::For,
                    1 => crate::types::VoteType::Against,
                    _ => crate::types::VoteType::Abstain,
                };
                let _ = client.try_gov_vote(&user, &proposal_id, &vote_type);
            }
            // 2: queue_proposal
            2 => {
                let proposal_id = action.u64_tail() % 10;
                let _ = client.try_gov_queue_proposal(&self.admin, &proposal_id);
            }
            // 3: execute_proposal
            3 => {
                let proposal_id = action.u64_tail() % 10;
                let _ = client.try_gov_execute_proposal(&self.admin, &proposal_id);
            }
            // 4: cancel_proposal
            4 => {
                let user = self.user(action.user());
                let proposal_id = action.u64_tail() % 10;
                let _ = client.try_gov_cancel_proposal(&user, &proposal_id);
            }
            // 5: approve_proposal (multisig)
            _ => {
                let proposal_id = action.u64_tail() % 10;
                let _ = client.try_gov_approve_proposal(&self.admin, &proposal_id);
            }
        }
    }

    fn assert_invariants(&self) {
        let client = crate::HelloContractClient::new(&self.env, &self.contract_id);

        // Check all proposals are in valid states
        for pid in 0..10u64 {
            if let Some(proposal) = client.gov_get_proposal(&pid) {
                // Proposal ID must match
                assert_eq!(proposal.id, pid);

                // Timestamps must be consistent
                assert!(proposal.end_time >= proposal.start_time);
                assert_eq!(proposal.created_at, proposal.start_time);

                // Vote counts must be non-negative
                assert!(proposal.for_votes >= 0);
                assert!(proposal.against_votes >= 0);
                assert!(proposal.abstain_votes >= 0);
                assert!(proposal.total_voting_power >= 0);

                // Total voting power must equal sum of individual votes
                assert_eq!(
                    proposal.total_voting_power,
                    proposal.for_votes + proposal.against_votes + proposal.abstain_votes
                );
            }
        }

        // Admin must be consistent
        let admin1 = client.gov_get_admin();
        let admin2 = client.gov_get_admin();
        assert_eq!(admin1, admin2);
    }
}

pub fn run(data: &[u8]) {
    let h = GovernanceHarness::new();
    for action in parse_actions(data, MAX_ACTIONS) {
        h.act(action);
    }
    h.assert_invariants();
}
