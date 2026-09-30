use soroban_sdk::{Env, Address, symbol_short};

pub fn verify_quorum(env: &Env, total_votes: i128, quorum_threshold: i128) -> bool {
    total_votes >= quorum_threshold
}

pub fn verify_voting_period(env: &Env, start_ledger: u32, duration: u32, current_ledger: u32) -> bool {
    current_ledger >= start_ledger && current_ledger <= start_ledger.saturating_add(duration)
}
