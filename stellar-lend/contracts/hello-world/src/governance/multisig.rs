use soroban_sdk::{Address, Env, Vec};

use crate::errors::GovernanceError;
use crate::events::ProposalApprovedEvent;
use crate::types::ProposalStatus;

use super::execution::execute_proposal_type;
use super::state::{
    get_admin, get_multisig_config, get_proposal_approvals, load_proposal, save_proposal,
    save_proposal_approvals,
};

pub fn approve_proposal(
    env: &Env,
    approver: Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    approver.require_auth();

    let multisig_config = get_multisig_config(env)?;

    if !multisig_config.admins.contains(&approver) {
        return Err(GovernanceError::Unauthorized);
    }

    load_proposal(env, proposal_id)?;

    let mut approvals = get_proposal_approvals(env, proposal_id);

    if approvals.contains(&approver) {
        return Err(GovernanceError::AlreadyVoted);
    }

    approvals.push_back(approver.clone());
    save_proposal_approvals(env, proposal_id, &approvals);

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

    let admin = get_admin(env)?;

    if caller != admin {
        return Err(GovernanceError::Unauthorized);
    }

    if admins.is_empty() || threshold == 0 || threshold > admins.len() {
        return Err(GovernanceError::InvalidMultisigConfig);
    }
    // A duplicated admin inflates `admins.len()` without adding an approver,
    // which can make the threshold unreachable.
    for (i, admin) in admins.iter().enumerate() {
        if admins.iter().skip(i + 1).any(|other| other == admin) {
            return Err(GovernanceError::InvalidMultisigConfig);
        }
    }

    let config = crate::types::MultisigConfig { admins, threshold };
    env.storage()
        .instance()
        .set(&crate::storage::GovernanceDataKey::MultisigConfig, &config);

    Ok(())
}

pub fn get_proposal_approvals(env: &Env, proposal_id: u64) -> Option<Vec<Address>> {
    let approvals = super::state::get_proposal_approvals(env, proposal_id);
    if approvals.is_empty() {
        None
    } else {
        Some(approvals)
    }
}

pub fn get_multisig_config(env: &Env) -> Option<crate::types::MultisigConfig> {
    super::state::get_multisig_config(env).ok()
}

pub fn get_multisig_admins(env: &Env) -> Option<Vec<Address>> {
    super::state::get_multisig_config(env).ok().map(|c| c.admins)
}

pub fn get_multisig_threshold(env: &Env) -> u32 {
    super::state::get_multisig_config(env)
        .ok()
        .map(|c| c.threshold)
        .unwrap_or(1)
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

    let mut proposal = load_proposal(env, proposal_id)?;

    if proposal.status != ProposalStatus::Pending {
        return Err(GovernanceError::InvalidProposalStatus);
    }

    let approvals = get_proposal_approvals(env, proposal_id).unwrap_or_else(|| Vec::new(env));
    if (approvals.len() as u32) < multisig_config.threshold {
        return Err(GovernanceError::InsufficientApprovals);
    }

    execute_proposal_type(env, &proposal.proposal_type)?;

    proposal.status = ProposalStatus::Executed;
    save_proposal(env, &proposal);

    let topics = (soroban_sdk::Symbol::new(env, "proposal_executed"), proposal_id);
    env.events().publish(topics, executor);

    Ok(())
}
