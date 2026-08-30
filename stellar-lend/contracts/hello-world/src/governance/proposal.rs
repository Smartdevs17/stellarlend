use soroban_sdk::{token::TokenClient, Address, Env, String, Vec};

use crate::errors::GovernanceError;
use crate::storage::GovernanceDataKey;
use crate::types::{
    GovernanceConfig, Proposal, ProposalOutcome, ProposalStatus, ProposalType, VoteType,
    BASIS_POINTS_SCALE, DEFAULT_TIMELOCK_DURATION, MAX_DESCRIPTION_LEN,
};
use crate::events::{
    ProposalCancelledEvent, ProposalCreatedEvent, ProposalExecutedEvent, ProposalFailedEvent,
    ProposalQueuedEvent,
};

use super::{get_admin, execute_proposal_type};

/// Create a new governance proposal.
pub fn create_proposal(
    env: &Env,
    proposer: Address,
    proposal_type: ProposalType,
    description: String,
    voting_threshold: Option<i128>,
) -> Result<u64, GovernanceError> {
    proposer.require_auth();

    if description.len() > MAX_DESCRIPTION_LEN {
        return Err(GovernanceError::InputTooLong);
    }

    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;

    if config.proposal_threshold > 0 {
        let token_client = TokenClient::new(env, &config.vote_token);
        let balance = token_client.balance(&proposer);

        if balance < config.proposal_threshold {
            return Err(GovernanceError::InsufficientProposalPower);
        }
    }

    let next_id: u64 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::NextProposalId)
        .unwrap_or(0);

    let now = env.ledger().timestamp();

    let proposal = Proposal {
        id: next_id,
        proposer: proposer.clone(),
        proposal_type,
        description: description.clone(),
        status: ProposalStatus::Pending,
        start_time: now,
        end_time: now + config.voting_period,
        execution_time: None,
        voting_threshold: voting_threshold.unwrap_or(config.default_voting_threshold),
        for_votes: 0,
        against_votes: 0,
        abstain_votes: 0,
        total_voting_power: 0,
        created_at: now,
    };

    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(next_id), &proposal);

    let user_key = GovernanceDataKey::UserProposals(proposer.clone(), next_id);
    env.storage().persistent().set(&user_key, &true);

    let approvals_key = GovernanceDataKey::ProposalApprovals(next_id);
    let approvals: Vec<Address> = Vec::new(env);
    env.storage().persistent().set(&approvals_key, &approvals);

    env.storage()
        .instance()
        .set(&GovernanceDataKey::NextProposalId, &(next_id + 1));

    ProposalCreatedEvent {
        proposal_id: next_id,
        proposer,
        proposal_type: proposal.proposal_type,
        description,
        start_time: proposal.start_time,
        end_time: proposal.end_time,
        created_at: now,
    }
    .publish(env);

    Ok(next_id)
}

/// Queue a proposal after voting ends, determining its outcome.
pub fn queue_proposal(
    env: &Env,
    caller: Address,
    proposal_id: u64,
) -> Result<ProposalOutcome, GovernanceError> {
    caller.require_auth();

    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;

    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    let now = env.ledger().timestamp();

    if now <= proposal.end_time {
        return Err(GovernanceError::VotingNotEnded);
    }

    match proposal.status {
        ProposalStatus::Executed
        | ProposalStatus::Cancelled
        | ProposalStatus::Expired
        | ProposalStatus::Queued => {
            return Err(GovernanceError::InvalidProposalStatus);
        }
        _ => {}
    }

    if now > proposal.end_time + DEFAULT_TIMELOCK_DURATION {
        proposal.status = ProposalStatus::Expired;
        env.storage()
            .persistent()
            .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
        return Err(GovernanceError::ProposalExpired);
    }

    let total_votes = proposal.for_votes + proposal.against_votes + proposal.abstain_votes;
    let quorum_required = (total_votes * config.quorum_bps as i128) / BASIS_POINTS_SCALE;
    let quorum_reached = total_votes >= quorum_required;

    let threshold_votes =
        (proposal.total_voting_power * proposal.voting_threshold) / BASIS_POINTS_SCALE;
    let threshold_met = proposal.for_votes >= threshold_votes;

    let succeeded = quorum_reached && threshold_met;

    let outcome = ProposalOutcome {
        proposal_id,
        succeeded,
        for_votes: proposal.for_votes,
        against_votes: proposal.against_votes,
        abstain_votes: proposal.abstain_votes,
        quorum_reached,
        quorum_required,
    };

    if succeeded {
        let execution_time = now + config.execution_delay;
        proposal.execution_time = Some(execution_time);
        proposal.status = ProposalStatus::Queued;

        env.storage()
            .persistent()
            .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        ProposalQueuedEvent {
            proposal_id,
            execution_time,
            for_votes: proposal.for_votes,
            against_votes: proposal.against_votes,
            quorum_reached: outcome.quorum_reached,
            threshold_met: outcome.succeeded && outcome.quorum_reached,
        }
        .publish(env);
    } else {
        proposal.status = ProposalStatus::Defeated;
        env.storage()
            .persistent()
            .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        ProposalFailedEvent {
            proposal_id,
            for_votes: proposal.for_votes,
            against_votes: proposal.against_votes,
            quorum_reached,
            threshold_met: !succeeded && quorum_reached,
        }
        .publish(env);
    }

    Ok(outcome)
}

/// Execute a queued proposal.
pub fn execute_proposal(
    env: &Env,
    executor: Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    executor.require_auth();

    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;

    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    let now = env.ledger().timestamp();

    if proposal.status != ProposalStatus::Queued {
        return Err(GovernanceError::NotQueued);
    }

    let execution_time = proposal
        .execution_time
        .ok_or(GovernanceError::InvalidExecutionTime)?;

    if now < execution_time {
        return Err(GovernanceError::ExecutionTooEarly);
    }

    if now > execution_time + config.timelock_duration {
        proposal.status = ProposalStatus::Expired;
        env.storage()
            .persistent()
            .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
        return Err(GovernanceError::ProposalExpired);
    }

    execute_proposal_type(env, &proposal.proposal_type)?;

    proposal.status = ProposalStatus::Executed;
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

    ProposalExecutedEvent {
        proposal_id,
        executor,
        timestamp: now,
    }
    .publish(env);

    Ok(())
}

/// Create an admin proposal (skips voting, goes straight to queued).
pub fn create_admin_proposal(
    env: &Env,
    admin: Address,
    proposal_type: ProposalType,
    description: String,
) -> Result<u64, GovernanceError> {
    admin.require_auth();

    if description.len() > MAX_DESCRIPTION_LEN {
        return Err(GovernanceError::InputTooLong);
    }

    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;

    if admin != stored_admin {
        return Err(GovernanceError::Unauthorized);
    }

    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;

    let now = env.ledger().timestamp();
    let proposal_id: u64 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::NextProposalId)
        .unwrap_or(0);

    let execution_time = now + config.execution_delay.max(crate::types::MIN_TIMELOCK_DELAY);

    let proposal = Proposal {
        id: proposal_id,
        proposer: admin.clone(),
        proposal_type,
        description,
        status: ProposalStatus::Queued,
        start_time: now,
        end_time: now,
        execution_time: Some(execution_time),
        voting_threshold: 0,
        for_votes: 0,
        against_votes: 0,
        abstain_votes: 0,
        total_voting_power: 0,
        created_at: now,
    };

    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

    env.storage()
        .instance()
        .set(&GovernanceDataKey::NextProposalId, &(proposal_id + 1));

    super::emit_proposal_created_event(env, &proposal_id, &admin);

    let topics = (
        soroban_sdk::Symbol::new(env, "proposal_queued"),
        proposal_id,
    );
    env.events().publish(topics, execution_time);

    Ok(proposal_id)
}

/// Create an emergency proposal (requires multisig admin, no delay).
pub fn create_emergency_proposal(
    env: &Env,
    caller: Address,
    proposal_type: ProposalType,
    description: String,
) -> Result<u64, GovernanceError> {
    caller.require_auth();

    if description.len() > MAX_DESCRIPTION_LEN {
        return Err(GovernanceError::InputTooLong);
    }

    let multisig_config: crate::types::MultisigConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::MultisigConfig)
        .ok_or(GovernanceError::NotInitialized)?;

    if !multisig_config.admins.contains(&caller) {
        return Err(GovernanceError::Unauthorized);
    }

    let now = env.ledger().timestamp();
    let proposal_id: u64 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::NextProposalId)
        .unwrap_or(0);

    let proposal = Proposal {
        id: proposal_id,
        proposer: caller.clone(),
        proposal_type,
        description,
        status: ProposalStatus::Queued,
        start_time: now,
        end_time: now,
        execution_time: Some(now), // No delay for emergency
        voting_threshold: 0,
        for_votes: 0,
        against_votes: 0,
        abstain_votes: 0,
        total_voting_power: 0,
        created_at: now,
    };

    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

    env.storage()
        .instance()
        .set(&GovernanceDataKey::NextProposalId, &(proposal_id + 1));

    super::emit_proposal_created_event(env, &proposal_id, &caller);

    Ok(proposal_id)
}

/// Cancel a proposal (proposer or admin only).
pub fn cancel_proposal(
    env: &Env,
    caller: Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    caller.require_auth();

    let admin: Address = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;

    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    if caller != proposal.proposer && caller != admin {
        return Err(GovernanceError::Unauthorized);
    }

    match proposal.status {
        ProposalStatus::Executed | ProposalStatus::Queued => {
            return Err(GovernanceError::InvalidProposalStatus);
        }
        _ => {}
    }

    proposal.status = ProposalStatus::Cancelled;
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

    ProposalCancelledEvent {
        proposal_id,
        caller,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

/// Propose setting minimum collateral ratio (convenience helper).
pub fn propose_set_min_collateral_ratio(
    env: &Env,
    proposer: Address,
    new_ratio: i128,
) -> Result<u64, GovernanceError> {
    create_proposal(
        env,
        proposer,
        ProposalType::MinCollateralRatio(new_ratio),
        String::from_str(env, "Update min collateral ratio"),
        None,
    )
}
