use soroban_sdk::{token::TokenClient, Address, Env, Vec};

use crate::errors::GovernanceError;
use crate::storage::GovernanceDataKey;
use crate::types::{
    DelegationRecord, GovernanceConfig, Proposal, VoteInfo, VoteLock, VotePowerSnapshot,
    VoteType, BASIS_POINTS_SCALE, DELEGATION_DEADLINE, MAX_DELEGATION_DEPTH,
};
use crate::events::{
    VoteCastEvent, VoteDelegatedEvent, VoteDelegationRevokedEvent, VoteLockedEvent,
    VotePowerSnapshotTakenEvent,
};

/// Cast a vote on a proposal.
pub fn vote(
    env: &Env,
    voter: Address,
    proposal_id: u64,
    vote_type: VoteType,
) -> Result<(), GovernanceError> {
    voter.require_auth();

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

    if proposal.status == ProposalStatus::Pending && now >= proposal.start_time {
        proposal.status = ProposalStatus::Active;
    }

    if proposal.status != ProposalStatus::Active {
        return Err(GovernanceError::ProposalNotActive);
    }

    let vote_key = GovernanceDataKey::Vote(proposal_id, voter.clone());
    if env.storage().persistent().has(&vote_key) {
        return Err(GovernanceError::AlreadyVoted);
    }

    // Flash loan protection: use snapshot-based voting power with delegation.
    let voting_power =
        get_vote_power_with_delegation(env, proposal_id, &voter, &config.vote_token)?;

    if voting_power == 0 {
        return Err(GovernanceError::NoVotingPower);
    }

    match vote_type {
        VoteType::For => proposal.for_votes += voting_power,
        VoteType::Against => proposal.against_votes += voting_power,
        VoteType::Abstain => proposal.abstain_votes += voting_power,
    }
    proposal.total_voting_power += voting_power;

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

    VoteCastEvent {
        proposal_id,
        voter,
        vote_type,
        voting_power,
        timestamp: now,
    }
    .publish(env);

    Ok(())
}

// ========================================================================
// Flash Loan Attack Protection
// ========================================================================

/// Take a vote power snapshot for a voter at proposal creation time.
pub fn take_vote_power_snapshot(
    env: &Env,
    proposal_id: u64,
    voter: &Address,
    vote_token: &Address,
) {
    let token_client = TokenClient::new(env, vote_token);
    let balance = token_client.balance(voter);
    let now = env.ledger().timestamp();

    let snapshot = VotePowerSnapshot {
        proposal_id,
        voter: voter.clone(),
        balance,
        snapshot_time: now,
    };

    env.storage().persistent().set(
        &GovernanceDataKey::VotePowerSnapshot(proposal_id, voter.clone()),
        &snapshot,
    );

    VotePowerSnapshotTakenEvent {
        proposal_id,
        voter: voter.clone(),
        balance,
        snapshot_time: now,
    }
    .publish(env);
}

/// Get the snapshotted vote power for a voter on a proposal.
/// Falls back to the live balance when no snapshot exists.
fn get_snapshotted_vote_power(
    env: &Env,
    proposal_id: u64,
    voter: &Address,
    vote_token: &Address,
) -> i128 {
    let snapshot_key = GovernanceDataKey::VotePowerSnapshot(proposal_id, voter.clone());
    if let Some(snapshot) = env
        .storage()
        .persistent()
        .get::<GovernanceDataKey, VotePowerSnapshot>(&snapshot_key)
    {
        snapshot.balance
    } else {
        TokenClient::new(env, vote_token).balance(voter)
    }
}

/// Resolve effective voting power for a voter, accounting for delegation.
fn get_vote_power_with_delegation(
    env: &Env,
    proposal_id: u64,
    voter: &Address,
    vote_token: &Address,
) -> Result<i128, GovernanceError> {
    let proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    let own_power = get_snapshotted_vote_power(env, proposal_id, voter, vote_token);
    let delegated_extra = get_delegated_power_for_voter(env, proposal_id, voter, &proposal);

    Ok(own_power + delegated_extra)
}

/// Sum up delegated voting power that was validly delegated to `delegatee`.
fn get_delegated_power_for_voter(
    env: &Env,
    proposal_id: u64,
    delegatee: &Address,
    proposal: &Proposal,
) -> i128 {
    let reverse_key = GovernanceDataKey::DelegationRecord(delegatee.clone());
    let delegators: Vec<Address> = env
        .storage()
        .persistent()
        .get(&reverse_key)
        .unwrap_or_else(|| Vec::new(env));

    let deadline = proposal.created_at.saturating_sub(DELEGATION_DEADLINE);
    let mut total: i128 = 0;

    for delegator in delegators.iter() {
        let del_key = GovernanceDataKey::DelegationRecord(delegator.clone());
        if let Some(record) = env
            .storage()
            .persistent()
            .get::<GovernanceDataKey, DelegationRecord>(&del_key)
        {
            if record.delegatee == *delegatee && record.delegated_at <= deadline {
                let snap_key =
                    GovernanceDataKey::VotePowerSnapshot(proposal_id, delegator.clone());
                if let Some(snap) = env
                    .storage()
                    .persistent()
                    .get::<GovernanceDataKey, VotePowerSnapshot>(&snap_key)
                {
                    total += snap.balance;
                }
            }
        }
    }

    total
}

// ========================================================================
// Vote Delegation
// ========================================================================

/// Delegate vote power from `delegator` to `delegatee`.
pub fn delegate_vote(
    env: &Env,
    delegator: Address,
    delegatee: Address,
) -> Result<(), GovernanceError> {
    delegator.require_auth();

    if delegator == delegatee {
        return Err(GovernanceError::SelfDelegation);
    }

    if is_vote_locked(env, &delegator) {
        return Err(GovernanceError::VotesLocked);
    }

    let del_key = GovernanceDataKey::DelegationRecord(delegator.clone());
    if env.storage().persistent().has(&del_key) {
        return Err(GovernanceError::AlreadyDelegated);
    }

    let depth = get_delegation_depth(env, &delegatee);
    if depth >= MAX_DELEGATION_DEPTH {
        return Err(GovernanceError::DelegationDepthExceeded);
    }

    let now = env.ledger().timestamp();

    let record = DelegationRecord {
        delegator: delegator.clone(),
        delegatee: delegatee.clone(),
        delegated_at: now,
        depth: depth + 1,
    };

    env.storage().persistent().set(&del_key, &record);

    let reverse_key = GovernanceDataKey::DelegationRecord(delegatee.clone());
    let mut delegators: Vec<Address> = env
        .storage()
        .persistent()
        .get(&reverse_key)
        .unwrap_or_else(|| Vec::new(env));
    delegators.push_back(delegator.clone());
    env.storage().persistent().set(&reverse_key, &delegators);

    VoteDelegatedEvent {
        delegator,
        delegatee,
        delegated_at: now,
    }
    .publish(env);

    Ok(())
}

/// Revoke an existing vote delegation.
pub fn revoke_delegation(env: &Env, delegator: Address) -> Result<(), GovernanceError> {
    delegator.require_auth();

    let del_key = GovernanceDataKey::DelegationRecord(delegator.clone());
    let record: DelegationRecord = env
        .storage()
        .persistent()
        .get(&del_key)
        .ok_or(GovernanceError::NotInitialized)?;

    let reverse_key = GovernanceDataKey::DelegationRecord(record.delegatee.clone());
    let delegators: Vec<Address> = env
        .storage()
        .persistent()
        .get(&reverse_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut new_delegators = Vec::new(env);
    for d in delegators.iter() {
        if d != delegator {
            new_delegators.push_back(d);
        }
    }
    env.storage()
        .persistent()
        .set(&reverse_key, &new_delegators);

    env.storage().persistent().remove(&del_key);

    VoteDelegationRevokedEvent {
        delegator,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

/// Compute the delegation chain depth for an address.
fn get_delegation_depth(env: &Env, addr: &Address) -> u32 {
    let del_key = GovernanceDataKey::DelegationRecord(addr.clone());
    if let Some(record) = env
        .storage()
        .persistent()
        .get::<GovernanceDataKey, DelegationRecord>(&del_key)
    {
        record.depth
    } else {
        0
    }
}

// ========================================================================
// Vote Lock
// ========================================================================

/// Query whether an address currently has its tokens locked due to an active vote.
pub fn is_vote_locked(env: &Env, voter: &Address) -> bool {
    let lock_key = GovernanceDataKey::VoteLock(voter.clone());
    if let Some(lock) = env
        .storage()
        .persistent()
        .get::<GovernanceDataKey, VoteLock>(&lock_key)
    {
        env.ledger().timestamp() < lock.locked_until
    } else {
        false
    }
}

/// Query the vote lock record for an address.
pub fn get_vote_lock(env: &Env, voter: &Address) -> Option<VoteLock> {
    let lock_key = GovernanceDataKey::VoteLock(voter.clone());
    env.storage().persistent().get(&lock_key)
}

/// Query the vote power snapshot for a voter on a specific proposal.
pub fn get_vote_power_snapshot(
    env: &Env,
    proposal_id: u64,
    voter: &Address,
) -> Option<VotePowerSnapshot> {
    let snap_key = GovernanceDataKey::VotePowerSnapshot(proposal_id, voter.clone());
    env.storage().persistent().get(&snap_key)
}

/// Query the delegation record for a delegator.
pub fn get_delegation(env: &Env, delegator: &Address) -> Option<DelegationRecord> {
    let del_key = GovernanceDataKey::DelegationRecord(delegator.clone());
    env.storage().persistent().get(&del_key)
}

use crate::types::ProposalStatus;
