#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, Vec};

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum ParameterType {
    LTV,
    LiquidationThreshold,
    InterestRateSlope1,
    InterestRateSlope2,
    BaseInterestRate,
    ReserveFactor,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ParameterChange {
    pub parameter: ParameterType,
    pub old_value: i128,
    pub new_value: i128,
    pub timestamp: u64,
    pub effective_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ParameterProposal {
    pub id: u64,
    pub parameter: ParameterType,
    pub proposed_value: i128,
    pub proposer: Address,
    pub created_at: u64,
    pub effective_at: u64,
    pub accepted: bool,
    pub rejected: bool,
}

#[contract]
pub struct ParameterStoreContract;

#[contractimpl]
impl ParameterStoreContract {
    /// Initialize the parameter store with a governance address.
    pub fn initialize(env: Env, governance: Address, admin: Address) {
        env.storage().instance().set(&DataKey::Governance, &governance);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::ProposalCounter, &0u64);
    }

    /// Propose a parameter change.
    pub fn propose_change(
        env: Env,
        parameter: ParameterType,
        value: i128,
        timelock_seconds: u64,
    ) -> u64 {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let counter: u64 = env.storage().instance().get(&DataKey::ProposalCounter).unwrap_or(0);
        let proposal_id = counter + 1;

        let current_timestamp = env.ledger().timestamp();
        let effective_at = current_timestamp + timelock_seconds;

        let min_timelock = match &parameter {
            ParameterType::LTV | ParameterType::LiquidationThreshold => 48 * 3600,
            _ => 24 * 3600,
        };

        assert!(timelock_seconds >= min_timelock, "Timelock too short");
        assert!(value >= 0, "Invalid parameter value");

        let proposal = ParameterProposal {
            id: proposal_id,
            parameter,
            proposed_value: value,
            proposer: governance.clone(),
            created_at: current_timestamp,
            effective_at,
            accepted: false,
            rejected: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &proposal_id);

        env.events()
            .publish(("propose_change", &parameter), &proposal_id);

        proposal_id
    }

    /// Accept a proposal and execute it.
    pub fn accept_proposal(env: Env, proposal_id: u64) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let mut proposal: ParameterProposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");

        let current_timestamp = env.ledger().timestamp();
        assert!(
            current_timestamp >= proposal.effective_at,
            "Timelock not elapsed"
        );
        assert!(!proposal.accepted && !proposal.rejected, "Proposal already decided");

        let old_value: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Parameter(proposal.parameter.clone()))
            .unwrap_or(0);

        proposal.accepted = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::Parameter(proposal.parameter.clone()), &proposal.proposed_value);

        let change = ParameterChange {
            parameter: proposal.parameter,
            old_value,
            new_value: proposal.proposed_value,
            timestamp: current_timestamp,
            effective_at: proposal.effective_at,
        };

        let history_key = DataKey::ChangeHistory(proposal.parameter);
        let mut history: Vec<ParameterChange> = env
            .storage()
            .instance()
            .get(&history_key)
            .unwrap_or_else(|| Vec::new(&env));
        history.push_back(change);
        env.storage().instance().set(&history_key, &history);

        env.events().publish(("accept_proposal",), &proposal_id);
    }

    /// Reject a proposal.
    pub fn reject_proposal(env: Env, proposal_id: u64) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let mut proposal: ParameterProposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");

        assert!(!proposal.accepted && !proposal.rejected, "Proposal already decided");

        proposal.rejected = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(("reject_proposal",), &proposal_id);
    }

    /// Get a parameter value.
    pub fn get_parameter(env: Env, parameter: ParameterType) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::Parameter(parameter))
            .unwrap_or(0)
    }

    /// Get a proposal.
    pub fn get_proposal(env: Env, proposal_id: u64) -> ParameterProposal {
        env.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found")
    }

    /// Get parameter change history.
    pub fn get_change_history(env: Env, parameter: ParameterType) -> Vec<ParameterChange> {
        env.storage()
            .instance()
            .get(&DataKey::ChangeHistory(parameter))
            .unwrap_or_else(|| Vec::new(&env))
    }
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Governance,
    Admin,
    ProposalCounter,
    Proposal(u64),
    Parameter(ParameterType),
    ChangeHistory(ParameterType),
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[test]
    fn test_initialize() {
        let env = soroban_sdk::Env::default();
        let gov = soroban_sdk::Address::generate(&env);
        let admin = soroban_sdk::Address::generate(&env);

        ParameterStoreContract::initialize(env.clone(), gov.clone(), admin);

        let stored: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        assert_eq!(stored, gov);
    }
}
