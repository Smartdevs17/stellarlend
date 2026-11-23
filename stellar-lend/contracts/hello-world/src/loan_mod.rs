//! #1584: On-chain loan modification proposals with lender opt-in.
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanModProposal {
    pub loan_id: u64,
    pub proposer: Address,
    pub new_rate_bps: u32,
    pub new_term_secs: u64,
    pub lender_approved: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum LoanModKey {
    Proposal(u64),
}

/// Propose a loan modification. Only the borrower may propose.
pub fn propose_modification(env: &Env, loan_id: u64, proposer: &Address, new_rate_bps: u32, new_term_secs: u64) {
    proposer.require_auth();
    let proposal = LoanModProposal {
        loan_id,
        proposer: proposer.clone(),
        new_rate_bps,
        new_term_secs,
        lender_approved: false,
    };
    env.storage().persistent().set(&LoanModKey::Proposal(loan_id), &proposal);
}

/// Lender opts in to the modification.
pub fn approve_modification(env: &Env, loan_id: u64, lender: &Address) -> Option<LoanModProposal> {
    lender.require_auth();
    let mut p: LoanModProposal = env.storage().persistent().get(&LoanModKey::Proposal(loan_id))?;
    p.lender_approved = true;
    env.storage().persistent().set(&LoanModKey::Proposal(loan_id), &p);
    Some(p)
}

/// Get the current proposal for a loan.
pub fn get_proposal(env: &Env, loan_id: u64) -> Option<LoanModProposal> {
    env.storage().persistent().get(&LoanModKey::Proposal(loan_id))
}
