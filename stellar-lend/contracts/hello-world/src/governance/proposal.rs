//! Proposal lifecycle: creation, tallying, queueing into the timelock,
//! execution and cancellation.
//!
//! ```text
//!  create ──► Active ──(end_time)──► Succeeded ──queue──► Queued ──(eta)──► Executed
//!               │                        │                  │
//!               │                        └─(grace)─► Expired└─(eta + grace)─► Expired
//!               └──(end_time, fails)──► Defeated
//!  Pending / Active / Queued ──cancel (proposer, admin, guardian)──► Cancelled
//! ```
//!
//! Only transitions made by a successful call are stored. States that follow
//! from time alone (voting closed, grace period lapsed) are derived by
//! [`get_proposal_state`], because a failing call cannot persist them.

use soroban_sdk::{Address, Env, String, Vec};

use crate::errors::GovernanceError;
use crate::events::{
    ProposalCancelledEvent, ProposalCreatedEvent, ProposalExecutedEvent, ProposalFailedEvent,
    ProposalQueuedEvent,
};
use crate::storage::{GovernanceDataKey, GuardianConfig};
use crate::types::{
    GovernanceConfig, MultisigConfig, Proposal, ProposalOutcome, ProposalStatus, ProposalType,
    BASIS_POINTS_SCALE, MAX_DESCRIPTION_LEN,
};

use super::analytics::{enforce_proposal_rate_limit, update_analytics_proposal_created};
use super::execute_proposal_type;
use super::power::{get_past_total_locked, get_past_votes};

fn get_config(env: &Env) -> Result<GovernanceConfig, GovernanceError> {
    env.storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)
}

fn load_proposal(env: &Env, proposal_id: u64) -> Result<Proposal, GovernanceError> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)
}

fn save_proposal(env: &Env, proposal: &Proposal) {
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal.id), proposal);
}

fn next_proposal_id(env: &Env) -> u64 {
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

/// Result of counting a proposal's votes against its quorum and threshold.
pub(crate) struct Tally {
    pub quorum_reached: bool,
    pub threshold_votes: i128,
    pub threshold_met: bool,
    pub succeeded: bool,
}

/// Count a proposal's votes.
///
/// - Quorum: for + against + abstain must reach `quorum_votes`, which was
///   fixed at creation as a share of the total locked supply.
/// - Threshold: `for` must be at least `voting_threshold` of all votes cast
///   and strictly more than `against`, so a tie never passes.
pub(crate) fn tally(proposal: &Proposal) -> Tally {
    let total_votes = proposal.for_votes + proposal.against_votes + proposal.abstain_votes;
    let quorum_reached = total_votes > 0 && total_votes >= proposal.quorum_votes;

    let threshold_votes = (total_votes * proposal.voting_threshold) / BASIS_POINTS_SCALE;
    let threshold_met = proposal.for_votes > 0
        && proposal.for_votes >= threshold_votes
        && proposal.for_votes > proposal.against_votes;

    Tally {
        quorum_reached,
        threshold_votes,
        threshold_met,
        succeeded: quorum_reached && threshold_met,
    }
}

/// Current lifecycle state of a proposal, including transitions that follow
/// from time alone and have not been written yet.
pub fn get_proposal_state(env: &Env, proposal_id: u64) -> Option<ProposalStatus> {
    let proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))?;
    let grace = get_config(env).ok()?.timelock_duration;
    let now = env.ledger().timestamp();

    Some(match proposal.status {
        ProposalStatus::Pending | ProposalStatus::Active | ProposalStatus::Succeeded => {
            if now < proposal.start_time {
                ProposalStatus::Pending
            } else if now < proposal.end_time {
                ProposalStatus::Active
            } else if !tally(&proposal).succeeded {
                ProposalStatus::Defeated
            } else if now > proposal.end_time.saturating_add(grace) {
                ProposalStatus::Expired
            } else {
                ProposalStatus::Succeeded
            }
        }
        ProposalStatus::Queued => match proposal.execution_time {
            Some(eta) if now > eta.saturating_add(grace) => ProposalStatus::Expired,
            _ => ProposalStatus::Queued,
        },
        status => status,
    })
}

/// Create a new governance proposal.
///
/// The proposer needs `proposal_threshold` voting power held before this
/// ledger. `voting_threshold` may raise the approval bar above the configured
/// default but never lower it.
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

    let config = get_config(env)?;
    let now = env.ledger().timestamp();

    if get_past_votes(env, &proposer, now) < config.proposal_threshold {
        return Err(GovernanceError::InsufficientProposalPower);
    }

    let voting_threshold = voting_threshold.unwrap_or(config.default_voting_threshold);
    if voting_threshold < config.default_voting_threshold || voting_threshold > BASIS_POINTS_SCALE {
        return Err(GovernanceError::InvalidVotingThreshold);
    }

    enforce_proposal_rate_limit(env, &proposer)?;

    // Round up so a non-zero quorum never truncates to zero.
    let total_locked = get_past_total_locked(env, now);
    let quorum_votes =
        (total_locked * config.quorum_bps as i128 + BASIS_POINTS_SCALE - 1) / BASIS_POINTS_SCALE;

    let proposal_id = next_proposal_id(env);
    let proposal = Proposal {
        id: proposal_id,
        proposer: proposer.clone(),
        proposal_type,
        description: description.clone(),
        status: ProposalStatus::Pending,
        start_time: now,
        end_time: now + config.voting_period,
        execution_time: None,
        voting_threshold,
        for_votes: 0,
        against_votes: 0,
        abstain_votes: 0,
        total_voting_power: 0,
        created_at: now,
        quorum_votes,
        emergency: false,
    };
    save_proposal(env, &proposal);

    env.storage().persistent().set(
        &GovernanceDataKey::UserProposals(proposer.clone(), proposal_id),
        &true,
    );
    env.storage().persistent().set(
        &GovernanceDataKey::ProposalApprovals(proposal_id),
        &Vec::<Address>::new(env),
    );

    update_analytics_proposal_created(env);

    ProposalCreatedEvent {
        proposal_id,
        proposer,
        proposal_type: crate::events::to_shared_proposal_type(&proposal.proposal_type),
        description,
        start_time: proposal.start_time,
        end_time: proposal.end_time,
        created_at: now,
    }
    .publish(env);

    Ok(proposal_id)
}

/// Close voting on a proposal. A proposal that passed is queued in the
/// timelock with `eta = now + execution_delay`; one that failed is marked
/// defeated. Callable by anyone once voting has ended, until the grace period
/// after `end_time` lapses.
pub fn queue_proposal(
    env: &Env,
    caller: Address,
    proposal_id: u64,
) -> Result<ProposalOutcome, GovernanceError> {
    caller.require_auth();

    let config = get_config(env)?;
    let mut proposal = load_proposal(env, proposal_id)?;
    let now = env.ledger().timestamp();

    match proposal.status {
        ProposalStatus::Pending | ProposalStatus::Active => {}
        _ => return Err(GovernanceError::InvalidProposalStatus),
    }
    if now < proposal.end_time {
        return Err(GovernanceError::VotingNotEnded);
    }
    if now > proposal.end_time.saturating_add(config.timelock_duration) {
        return Err(GovernanceError::ProposalExpired);
    }

    let tally = tally(&proposal);
    let outcome = ProposalOutcome {
        proposal_id,
        succeeded: tally.succeeded,
        for_votes: proposal.for_votes,
        against_votes: proposal.against_votes,
        abstain_votes: proposal.abstain_votes,
        quorum_reached: tally.quorum_reached,
        quorum_required: proposal.quorum_votes,
    };

    if tally.succeeded {
        let execution_time = now + config.execution_delay;
        proposal.execution_time = Some(execution_time);
        proposal.status = ProposalStatus::Queued;
        save_proposal(env, &proposal);

        ProposalQueuedEvent {
            proposal_id,
            execution_time,
            for_votes: proposal.for_votes,
            against_votes: proposal.against_votes,
            quorum_reached: tally.quorum_reached,
            threshold_met: tally.threshold_met,
        }
        .publish(env);
    } else {
        proposal.status = ProposalStatus::Defeated;
        save_proposal(env, &proposal);

        ProposalFailedEvent {
            proposal_id,
            for_votes: proposal.for_votes,
            against_votes: proposal.against_votes,
            quorum_reached: tally.quorum_reached,
            threshold_met: tally.threshold_met,
        }
        .publish(env);
    }

    Ok(outcome)
}

fn require_multisig_approvals(
    env: &Env,
    executor: &Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    let multisig: MultisigConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::MultisigConfig)
        .ok_or(GovernanceError::NotInitialized)?;
    if !multisig.admins.contains(executor) {
        return Err(GovernanceError::Unauthorized);
    }

    let approvals: Vec<Address> = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::ProposalApprovals(proposal_id))
        .unwrap_or_else(|| Vec::new(env));
    // Count only approvals from current admins, so removing an admin also
    // withdraws their approval.
    let valid = approvals
        .iter()
        .filter(|approver| multisig.admins.contains(approver))
        .count() as u32;
    if valid < multisig.threshold {
        return Err(GovernanceError::InsufficientApprovals);
    }
    Ok(())
}

/// Execute a queued proposal once its timelock has elapsed and before its
/// grace period (`timelock_duration`) runs out. Anyone may execute a
/// voted proposal; an emergency proposal skips the delay but needs the
/// multisig approval threshold and a multisig admin as executor.
pub fn execute_proposal(
    env: &Env,
    executor: Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    executor.require_auth();

    let config = get_config(env)?;
    let mut proposal = load_proposal(env, proposal_id)?;
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
    if now > execution_time.saturating_add(config.timelock_duration) {
        return Err(GovernanceError::ProposalExpired);
    }
    if proposal.emergency {
        require_multisig_approvals(env, &executor, proposal_id)?;
    }

    // Mark executed before running the action so the proposal cannot be
    // re-entered and executed twice.
    proposal.status = ProposalStatus::Executed;
    save_proposal(env, &proposal);

    execute_proposal_type(env, &proposal.proposal_type)?;

    ProposalExecutedEvent {
        proposal_id,
        executor,
        timestamp: now,
    }
    .publish(env);

    Ok(())
}

/// Create an admin proposal (skips voting, goes straight to queued). The
/// timelock still applies, at least `MIN_TIMELOCK_DELAY`, so guardians can
/// cancel it before it executes.
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

    let config = get_config(env)?;
    let now = env.ledger().timestamp();
    let proposal_id = next_proposal_id(env);
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
        quorum_votes: 0,
        emergency: false,
    };
    save_proposal(env, &proposal);

    super::emit_proposal_created_event(env, &proposal_id, &admin);

    let topics = (
        soroban_sdk::Symbol::new(env, "proposal_queued"),
        proposal_id,
    );
    env.events().publish(topics, execution_time);

    Ok(proposal_id)
}

/// Create an emergency proposal. It skips voting and the timelock, so it can
/// only be executed once the multisig approval threshold is reached; the
/// creator's approval is recorded automatically.
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

    let multisig_config: MultisigConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::MultisigConfig)
        .ok_or(GovernanceError::NotInitialized)?;

    if !multisig_config.admins.contains(&caller) {
        return Err(GovernanceError::Unauthorized);
    }

    let now = env.ledger().timestamp();
    let proposal_id = next_proposal_id(env);

    let proposal = Proposal {
        id: proposal_id,
        proposer: caller.clone(),
        proposal_type,
        description,
        status: ProposalStatus::Queued,
        start_time: now,
        end_time: now,
        execution_time: Some(now),
        voting_threshold: 0,
        for_votes: 0,
        against_votes: 0,
        abstain_votes: 0,
        total_voting_power: 0,
        created_at: now,
        quorum_votes: 0,
        emergency: true,
    };
    save_proposal(env, &proposal);

    let mut approvals = Vec::new(env);
    approvals.push_back(caller.clone());
    env.storage().persistent().set(
        &GovernanceDataKey::ProposalApprovals(proposal_id),
        &approvals,
    );

    super::emit_proposal_created_event(env, &proposal_id, &caller);

    Ok(proposal_id)
}

/// Cancel a proposal that has not executed. The proposer and admin can cancel
/// at any point before execution; guardians can veto, which is what makes the
/// timelock delay a real review window.
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

    let mut proposal = load_proposal(env, proposal_id)?;

    let is_guardian = env
        .storage()
        .instance()
        .get::<_, GuardianConfig>(&GovernanceDataKey::GuardianConfig)
        .map(|config| config.guardians.contains(&caller))
        .unwrap_or(false);

    if caller != proposal.proposer && caller != admin && !is_guardian {
        return Err(GovernanceError::Unauthorized);
    }

    match proposal.status {
        ProposalStatus::Pending | ProposalStatus::Active | ProposalStatus::Queued => {}
        _ => return Err(GovernanceError::InvalidProposalStatus),
    }

    proposal.status = ProposalStatus::Cancelled;
    save_proposal(env, &proposal);

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
