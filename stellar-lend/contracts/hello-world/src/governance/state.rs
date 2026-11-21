//! Shared state access for governance module.
//!
//! Centralizes all storage key access patterns to avoid duplication across
//! proposal, multisig, and power modules.

use soroban_sdk::{Address, Env, Vec};

use crate::errors::GovernanceError;
use crate::storage::GovernanceDataKey;
use crate::types::{GovernanceConfig, MultisigConfig, Proposal, ProposalStatus};

/// Load governance config from storage.
pub fn get_config(env: &Env) -> Result<GovernanceConfig, GovernanceError> {
    env.storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)
}

/// Load a proposal by id.
pub fn load_proposal(env: &Env, proposal_id: u64) -> Result<Proposal, GovernanceError> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)
}

/// Save a proposal to storage.
pub fn save_proposal(env: &Env, proposal: &Proposal) {
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal.id), proposal);
}

/// Get the next proposal id and increment the counter.
pub fn next_proposal_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::NextProposalId)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&GovernanceDataKey::NextProposalId, &(id + 1));
    id
}

/// Load the admin address.
pub fn get_admin(env: &Env) -> Result<Address, GovernanceError> {
    env.storage()
        .instance()
        .get(&GovernanceDataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)
}

/// Load multisig config from storage.
pub fn get_multisig_config(env: &Env) -> Result<MultisigConfig, GovernanceError> {
    env.storage()
        .instance()
        .get(&GovernanceDataKey::MultisigConfig)
        .ok_or(GovernanceError::NotInitialized)
}

/// Load proposal approvals list.
pub fn get_proposal_approvals(env: &Env, proposal_id: u64) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::ProposalApprovals(proposal_id))
        .unwrap_or_else(|| Vec::new(env))
}

/// Save proposal approvals list.
pub fn save_proposal_approvals(env: &Env, proposal_id: u64, approvals: &Vec<Address>) {
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::ProposalApprovals(proposal_id), approvals);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GovernanceConfig, MultisigConfig, Proposal, ProposalStatus, ProposalType};

    fn setup_config(env: &Env) -> (Address, GovernanceConfig) {
        let admin = Address::generate(env);
        let vote_token = Address::generate(env);

        let config = GovernanceConfig {
            voting_period: 300,
            execution_delay: 100,
            quorum_bps: 4000,
            proposal_threshold: 0,
            vote_token,
            timelock_duration: 1000,
            default_voting_threshold: 5000,
        };

        env.storage()
            .instance()
            .set(&GovernanceDataKey::Config, &config);
        env.storage()
            .instance()
            .set(&GovernanceDataKey::Admin, &admin);

        (admin, config)
    }

    #[test]
    fn test_get_config_success() {
        let env = Env::default();
        let (_admin, expected_config) = setup_config(&env);

        let result = get_config(&env);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.voting_period, expected_config.voting_period);
        assert_eq!(config.quorum_bps, expected_config.quorum_bps);
    }

    #[test]
    fn test_get_config_not_initialized() {
        let env = Env::default();
        let result = get_config(&env);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GovernanceError::NotInitialized));
    }

    #[test]
    fn test_get_admin_success() {
        let env = Env::default();
        let (admin, _config) = setup_config(&env);

        let result = get_admin(&env);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), admin);
    }

    #[test]
    fn test_get_admin_not_initialized() {
        let env = Env::default();
        let result = get_admin(&env);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GovernanceError::NotInitialized));
    }

    #[test]
    fn test_next_proposal_id_increments() {
        let env = Env::default();

        let id1 = next_proposal_id(&env);
        assert_eq!(id1, 0);

        let id2 = next_proposal_id(&env);
        assert_eq!(id2, 1);

        let id3 = next_proposal_id(&env);
        assert_eq!(id3, 2);
    }

    #[test]
    fn test_save_and_load_proposal() {
        let env = Env::default();

        let proposal = Proposal {
            id: 0,
            proposer: Address::generate(&env),
            proposal_type: ProposalType::EmergencyPause(true),
            description: soroban_sdk::String::from_str(&env, "Test proposal"),
            status: ProposalStatus::Pending,
            start_time: 1000,
            end_time: 2000,
            execution_time: None,
            voting_threshold: 5000,
            for_votes: 0,
            against_votes: 0,
            abstain_votes: 0,
            total_voting_power: 0,
            created_at: 1000,
            quorum_votes: 100,
            emergency: false,
        };

        save_proposal(&env, &proposal);

        let loaded = load_proposal(&env, 0);
        assert!(loaded.is_ok());
        let loaded_proposal = loaded.unwrap();
        assert_eq!(loaded_proposal.id, proposal.id);
        assert_eq!(loaded_proposal.status, proposal.status);
        assert_eq!(loaded_proposal.voting_threshold, proposal.voting_threshold);
    }

    #[test]
    fn test_load_proposal_not_found() {
        let env = Env::default();

        let result = load_proposal(&env, 999);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GovernanceError::ProposalNotFound));
    }

    #[test]
    fn test_multisig_config() {
        let env = Env::default();
        let admin = Address::generate(&env);

        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());

        let config = MultisigConfig {
            admins,
            threshold: 1,
        };

        env.storage()
            .instance()
            .set(&GovernanceDataKey::MultisigConfig, &config);

        let result = get_multisig_config(&env);
        assert!(result.is_ok());
        let loaded = result.unwrap();
        assert_eq!(loaded.threshold, 1);
        assert_eq!(loaded.admins.len(), 1);
    }

    #[test]
    fn test_multisig_config_not_initialized() {
        let env = Env::default();
        let result = get_multisig_config(&env);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GovernanceError::NotInitialized));
    }

    #[test]
    fn test_proposal_approvals() {
        let env = Env::default();
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let mut approvals = Vec::new(&env);
        approvals.push_back(addr1.clone());
        approvals.push_back(addr2.clone());

        save_proposal_approvals(&env, 0, &approvals);

        let loaded = get_proposal_approvals(&env, 0);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get(0).unwrap(), addr1);
        assert_eq!(loaded.get(1).unwrap(), addr2);
    }

    #[test]
    fn test_proposal_approvals_empty() {
        let env = Env::default();

        let loaded = get_proposal_approvals(&env, 999);
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_proposal_approvals_overwrite() {
        let env = Env::default();
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);
        let addr3 = Address::generate(&env);

        let mut approvals1 = Vec::new(&env);
        approvals1.push_back(addr1.clone());
        save_proposal_approvals(&env, 0, &approvals1);

        let loaded = get_proposal_approvals(&env, 0);
        assert_eq!(loaded.len(), 1);

        let mut approvals2 = Vec::new(&env);
        approvals2.push_back(addr2.clone());
        approvals2.push_back(addr3.clone());
        save_proposal_approvals(&env, 0, &approvals2);

        let reloaded = get_proposal_approvals(&env, 0);
        assert_eq!(reloaded.len(), 2);
        assert_eq!(reloaded.get(0).unwrap(), addr2);
    }
}

