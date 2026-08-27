use soroban_sdk::{Address, Env, Vec};

use crate::errors::GovernanceError;
use crate::storage::GovernanceDataKey;
use crate::types::{MultisigConfig, Proposal, ProposalStatus};
use crate::events::ProposalApprovedEvent;

use super::get_admin;
use super::execution::execute_proposal_type;

pub fn approve_proposal(
    env: &Env,
    approver: Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    approver.require_auth();

    let multisig_config: MultisigConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::MultisigConfig)
        .ok_or(GovernanceError::NotInitialized)?;

    if !multisig_config.admins.contains(&approver) {
        return Err(GovernanceError::Unauthorized);
    }

    let proposal_key = GovernanceDataKey::Proposal(proposal_id);
    if !env.storage().persistent().has(&proposal_key) {
        return Err(GovernanceError::ProposalNotFound);
    }

    let approvals_key = GovernanceDataKey::ProposalApprovals(proposal_id);
    let mut approvals: Vec<Address> = env
        .storage()
        .persistent()
        .get(&approvals_key)
        .unwrap_or_else(|| Vec::new(env));

    if approvals.contains(&approver) {
        return Err(GovernanceError::AlreadyVoted);
    }

    approvals.push_back(approver.clone());
    env.storage().persistent().set(&approvals_key, &approvals);

    ProposalApprovedEvent {
        proposal_id,
        approver,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

pub fn set_multisig_config(
    env: &Env,
    caller: Address,
    admins: Vec<Address>,
    threshold: u32,
) -> Result<(), GovernanceError> {
    caller.require_auth();

    let admin: Address = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;

    if caller != admin {
        return Err(GovernanceError::Unauthorized);
    }

    if admins.is_empty() || threshold == 0 || threshold > admins.len() {
        return Err(GovernanceError::InvalidMultisigConfig);
    }

    let config = MultisigConfig { admins, threshold };
    env.storage()
        .instance()
        .set(&GovernanceDataKey::MultisigConfig, &config);

    Ok(())
}

pub fn get_proposal_approvals(env: &Env, proposal_id: u64) -> Option<Vec<Address>> {
    let approvals_key = GovernanceDataKey::ProposalApprovals(proposal_id);
    env.storage().persistent().get(&approvals_key)
}

pub fn get_multisig_config(env: &Env) -> Option<MultisigConfig> {
    env.storage()
        .instance()
        .get(&GovernanceDataKey::MultisigConfig)
}

pub fn get_multisig_admins(env: &Env) -> Option<Vec<Address>> {
    get_multisig_config(env).map(|c| c.admins)
}

pub fn get_multisig_threshold(env: &Env) -> u32 {
    get_multisig_config(env).map(|c| c.threshold).unwrap_or(1)
}

pub fn set_multisig_admins(
    env: &Env,
    caller: Address,
    admins: Vec<Address>,
) -> Result<(), GovernanceError> {
    let config = get_multisig_config(env).ok_or(GovernanceError::NotInitialized)?;
    set_multisig_config(env, caller, admins, config.threshold)
}

pub fn set_multisig_threshold(
    env: &Env,
    caller: Address,
    threshold: u32,
) -> Result<(), GovernanceError> {
    let config = get_multisig_config(env).ok_or(GovernanceError::NotInitialized)?;
    set_multisig_config(env, caller, config.admins, threshold)
}

pub fn execute_multisig_proposal(
    env: &Env,
    executor: Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    executor.require_auth();

    let multisig_config = get_multisig_config(env).ok_or(GovernanceError::NotInitialized)?;
    if !multisig_config.admins.contains(&executor) {
        return Err(GovernanceError::Unauthorized);
    }

    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    if proposal.status != ProposalStatus::Pending {
        return Err(GovernanceError::InvalidProposalStatus);
    }

    let approvals = get_proposal_approvals(env, proposal_id).unwrap_or_else(|| Vec::new(env));
    if approvals.len() < multisig_config.threshold {
        return Err(GovernanceError::InsufficientApprovals);
    }

    execute_proposal_type(env, &proposal.proposal_type)?;

    proposal.status = ProposalStatus::Executed;
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

    super::emit_proposal_executed_event(env, &proposal_id, &executor);

    Ok(())
}
