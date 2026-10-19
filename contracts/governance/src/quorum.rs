use soroban_sdk::{Env, Address, symbol};

pub const QUORUM_CONFIG_KEY: &str = "QUORUM_CONFIG";
pub const VOTING_PERIOD_CONFIG_KEY: &str = "VOTING_PERIOD_CONFIG";

#[derive(Clone)]
pub struct QuorumConfig {
    pub threshold: i128,
}

pub fn set_quorum_config(env: &Env, quorum_threshold: i128) {
    env.storage().instance().set(&symbol!("QUORUM_CONFIG"), &quorum_threshold);
}

pub fn get_quorum_config(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&symbol!("QUORUM_CONFIG"))
        .unwrap_or(1000) // Default 1000 votes
}

pub fn verify_quorum(env: &Env, total_votes: i128) -> bool {
    let threshold = get_quorum_config(env);
    total_votes >= threshold
}

pub fn verify_quorum_with_threshold(env: &Env, total_votes: i128, quorum_threshold: i128) -> bool {
    total_votes >= quorum_threshold
}

pub fn verify_voting_period(env: &Env, start_ledger: u32, duration: u32, current_ledger: u32) -> bool {
    current_ledger >= start_ledger && current_ledger <= start_ledger.saturating_add(duration)
}
