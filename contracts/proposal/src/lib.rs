//! StellarLend Proposal Module
//! Implements proposal creation and voting mechanism

use soroban_sdk::{contract, contractimpl, symbol, vec, Address, Env, Symbol, Vec};

pub struct Proposal {
    pub id: u64,
    pub creator: Address,
    pub title: String,
    pub description: String,
    pub vote_count: u64,
    pub votes: Vec<(Address, bool)> // (voter, supports)
}

pub struct ProposalContract;

#[contract]
pub impl ProposalContract {
    pub fn create_proposal(env: Env, creator: Address, title: String, description: String) -> Proposal {
        let id = env.ledger().sequence() - 1; // Use ledger sequence as unique ID
        Proposal {
            id,
            creator,
            title,
            description,
            vote_count: 0,
            votes: vec![&env]
        }
    }

    pub fn vote(env: Env, proposal: &mut Proposal, voter: Address, supports: bool) {
        // Check if already voted
        for (existing_voter, _) in proposal.votes.iter() {
            if existing_voter == voter {
                panic!("already voted");
            }
        }
        proposal.votes.push_back((voter, supports));
        proposal.vote_count += 1;
    }

    pub fn get_proposal(proposal: &Proposal) -> Proposal {
        proposal.clone()
    }
}
