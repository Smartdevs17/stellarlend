use soroban_sdk::{Address, Env};

use crate::errors::GovernanceError;
use crate::events::{to_shared_vote_type, VoteCastEvent, VoteLockedEvent};
use crate::storage::GovernanceDataKey;
use crate::types::{Proposal, ProposalStatus, VoteInfo, VoteLock, VotePowerSnapshot, VoteType};

use super::analytics::{detect_suspicious_voting, update_analytics_vote_cast};
use super::power::{get_locked_balance, get_past_total_locked, get_past_votes};

/// Cast a vote on a proposal.
///
/// Voting is open from `start_time` until (not including) `end_time`. The
/// voter's weight is their voting power strictly before the proposal was
/// created, so power acquired afterwards (including by flash loan or late
/// delegation) cannot be used.
pub fn vote(
    env: &Env,
    voter: Address,
    proposal_id: u64,
    vote_type: VoteType,
) -> Result<(), GovernanceError> {
    voter.require_auth();

    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    match proposal.status {
        ProposalStatus::Pending | ProposalStatus::Active => {}
        _ => return Err(GovernanceError::ProposalNotActive),
    }

    let now = env.ledger().timestamp();
    if now < proposal.start_time || now >= proposal.end_time {
        return Err(GovernanceError::NotInVotingPeriod);
    }

    let vote_key = GovernanceDataKey::Vote(proposal_id, voter.clone());
    if env.storage().persistent().has(&vote_key) {
        return Err(GovernanceError::AlreadyVoted);
    }

    let voting_power = get_past_votes(env, &voter, proposal.created_at);
    if voting_power <= 0 {
        return Err(GovernanceError::NoVotingPower);
    }

    match vote_type {
        VoteType::For => proposal.for_votes += voting_power,
        VoteType::Against => proposal.against_votes += voting_power,
        VoteType::Abstain => proposal.abstain_votes += voting_power,
    }
    proposal.total_voting_power += voting_power;
    proposal.status = ProposalStatus::Active;

    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
    env.storage().persistent().set(
        &vote_key,
        &VoteInfo {
            voter: voter.clone(),
            proposal_id,
            vote_type: vote_type.clone(),
            voting_power,
            timestamp: now,
        },
    );

    lock_votes(env, &voter, &proposal);
    update_analytics_vote_cast(env);
    detect_suspicious_voting(
        env,
        proposal_id,
        &voter,
        voting_power,
        get_past_total_locked(env, proposal.created_at),
    );

    VoteCastEvent {
        proposal_id,
        voter,
        vote_type: to_shared_vote_type(&vote_type),
        voting_power,
        timestamp: now,
    }
    .publish(env);

    Ok(())
}

// ========================================================================
// Vote Lock
// ========================================================================

/// Keep the voter's locked tokens (and delegation) in place until the voting
/// period of every proposal they voted on has ended, so a vote cannot be
/// followed by an immediate exit.
fn lock_votes(env: &Env, voter: &Address, proposal: &Proposal) {
    let lock_key = GovernanceDataKey::VoteLock(voter.clone());
    let existing: Option<VoteLock> = env.storage().persistent().get(&lock_key);

    let (locked_until, proposal_id) = match existing {
        Some(lock) if lock.locked_until > proposal.end_time => {
            (lock.locked_until, lock.proposal_id)
        }
        _ => (proposal.end_time, proposal.id),
    };

    let lock = VoteLock {
        voter: voter.clone(),
        locked_until,
        locked_amount: get_locked_balance(env, voter),
        proposal_id,
    };
    env.storage().persistent().set(&lock_key, &lock);

    VoteLockedEvent {
        voter: voter.clone(),
        proposal_id,
        locked_amount: lock.locked_amount,
        locked_until,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Query whether an address currently has its tokens locked due to an active vote.
pub fn is_vote_locked(env: &Env, voter: &Address) -> bool {
    get_vote_lock(env, voter)
        .map(|lock| env.ledger().timestamp() < lock.locked_until)
        .unwrap_or(false)
}

/// Query the vote lock record for an address.
pub fn get_vote_lock(env: &Env, voter: &Address) -> Option<VoteLock> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::VoteLock(voter.clone()))
}

/// The voting power `voter` can use on a proposal: their power strictly
/// before the proposal's creation time.
pub fn get_vote_power_snapshot(
    env: &Env,
    proposal_id: u64,
    voter: &Address,
) -> Option<VotePowerSnapshot> {
    let proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))?;

    Some(VotePowerSnapshot {
        proposal_id,
        voter: voter.clone(),
        balance: get_past_votes(env, voter, proposal.created_at),
        snapshot_time: proposal.created_at,
    })
}
