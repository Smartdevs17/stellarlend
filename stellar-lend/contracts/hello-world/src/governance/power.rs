//! Lock-to-vote voting power with timestamped checkpoints.
//!
//! Soroban tokens expose neither historical balances nor a total supply, so
//! governance cannot snapshot holders by reading the vote token. Instead,
//! voters lock vote tokens in this contract, and every change to an account's
//! voting power or to the total locked supply appends a checkpoint.
//!
//! A proposal counts power held *strictly before* the timestamp it was created
//! at. Tokens borrowed, bought, locked or delegated in the proposal's own
//! ledger or later carry no weight on it, which is what makes flash-loan
//! voting and vote-then-transfer double voting impossible.
//!
//! Delegation is single-hop: delegating moves the delegator's locked balance
//! into the delegatee's voting power, and power received by delegation cannot
//! be delegated onward. Chains and cycles cannot form, so no depth limit is
//! needed.

use soroban_sdk::{token::TokenClient, Address, Env};

use crate::errors::GovernanceError;
use crate::events::{
    GovTokensLockedEvent, GovTokensUnlockedEvent, VoteDelegatedEvent, VoteDelegationRevokedEvent,
};
use crate::storage::GovernanceDataKey;
use crate::types::{DelegationRecord, GovernanceConfig, VotingCheckpoint};

use super::voting::is_vote_locked;

/// A checkpointed series: one account's voting power, or the total locked.
enum Series<'a> {
    Account(&'a Address),
    TotalLocked,
}

impl Series<'_> {
    fn count_key(&self) -> GovernanceDataKey {
        match self {
            Series::Account(addr) => GovernanceDataKey::VoteCheckpointCount((*addr).clone()),
            Series::TotalLocked => GovernanceDataKey::TotalLockedCheckpointCount,
        }
    }

    fn checkpoint_key(&self, index: u32) -> GovernanceDataKey {
        match self {
            Series::Account(addr) => GovernanceDataKey::VoteCheckpoint((*addr).clone(), index),
            Series::TotalLocked => GovernanceDataKey::TotalLockedCheckpoint(index),
        }
    }

    fn count(&self, env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&self.count_key())
            .unwrap_or(0)
    }

    fn checkpoint(&self, env: &Env, index: u32) -> VotingCheckpoint {
        env.storage()
            .persistent()
            .get(&self.checkpoint_key(index))
            .expect("checkpoint below count must exist")
    }

    fn latest(&self, env: &Env) -> i128 {
        match self.count(env) {
            0 => 0,
            n => self.checkpoint(env, n - 1).votes,
        }
    }

    /// Value in force strictly before `timestamp`.
    fn value_before(&self, env: &Env, timestamp: u64) -> i128 {
        // Binary search for the first checkpoint at or after `timestamp`; the
        // one before it holds the value we want.
        let (mut lo, mut hi) = (0u32, self.count(env));
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.checkpoint(env, mid).timestamp < timestamp {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        match lo {
            0 => 0,
            n => self.checkpoint(env, n - 1).votes,
        }
    }

    fn add(&self, env: &Env, delta: i128) {
        let now = env.ledger().timestamp();
        let count = self.count(env);
        let votes = self.latest(env) + delta;
        let checkpoint = VotingCheckpoint {
            timestamp: now,
            votes,
        };

        // Several changes in one ledger collapse into a single checkpoint.
        if count > 0 && self.checkpoint(env, count - 1).timestamp == now {
            env.storage()
                .persistent()
                .set(&self.checkpoint_key(count - 1), &checkpoint);
        } else {
            env.storage()
                .persistent()
                .set(&self.checkpoint_key(count), &checkpoint);
            env.storage()
                .persistent()
                .set(&self.count_key(), &(count + 1));
        }
    }
}

fn get_config(env: &Env) -> Result<GovernanceConfig, GovernanceError> {
    env.storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)
}

/// The account whose voting power `owner`'s locked tokens count toward.
fn voting_account(env: &Env, owner: &Address) -> Address {
    get_delegation(env, owner)
        .map(|record| record.delegatee)
        .unwrap_or_else(|| owner.clone())
}

fn set_locked_balance(env: &Env, owner: &Address, amount: i128) {
    let key = GovernanceDataKey::LockedBalance(owner.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
    }
}

/// Lock `amount` vote tokens, adding them to the voting power of `owner` (or
/// of their delegatee). Returns the new locked balance.
pub fn lock_tokens(env: &Env, owner: Address, amount: i128) -> Result<i128, GovernanceError> {
    owner.require_auth();
    if amount <= 0 {
        return Err(GovernanceError::InvalidAmount);
    }
    let config = get_config(env)?;

    TokenClient::new(env, &config.vote_token).transfer(
        &owner,
        &env.current_contract_address(),
        &amount,
    );

    let locked = get_locked_balance(env, &owner) + amount;
    set_locked_balance(env, &owner, locked);
    Series::Account(&voting_account(env, &owner)).add(env, amount);
    Series::TotalLocked.add(env, amount);

    GovTokensLockedEvent {
        owner,
        amount,
        locked_balance: locked,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(locked)
}

/// Unlock `amount` vote tokens and return them to `owner`. Fails while the
/// owner has a vote on a proposal whose voting period is still open. Returns
/// the new locked balance.
pub fn unlock_tokens(env: &Env, owner: Address, amount: i128) -> Result<i128, GovernanceError> {
    owner.require_auth();
    if amount <= 0 {
        return Err(GovernanceError::InvalidAmount);
    }
    let config = get_config(env)?;

    if is_vote_locked(env, &owner) {
        return Err(GovernanceError::VotesLocked);
    }

    let current = get_locked_balance(env, &owner);
    if amount > current {
        return Err(GovernanceError::InsufficientLockedBalance);
    }

    let locked = current - amount;
    set_locked_balance(env, &owner, locked);
    Series::Account(&voting_account(env, &owner)).add(env, -amount);
    Series::TotalLocked.add(env, -amount);

    TokenClient::new(env, &config.vote_token).transfer(
        &env.current_contract_address(),
        &owner,
        &amount,
    );

    GovTokensUnlockedEvent {
        owner,
        amount,
        locked_balance: locked,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(locked)
}

/// Delegate the voting power of `delegator`'s locked tokens to `delegatee`,
/// replacing any existing delegation. Tokens locked later follow the
/// delegation automatically.
pub fn delegate_vote(
    env: &Env,
    delegator: Address,
    delegatee: Address,
) -> Result<(), GovernanceError> {
    delegator.require_auth();
    get_config(env)?;

    if delegator == delegatee {
        return Err(GovernanceError::SelfDelegation);
    }
    if is_vote_locked(env, &delegator) {
        return Err(GovernanceError::VotesLocked);
    }

    let previous = voting_account(env, &delegator);
    if previous == delegatee {
        return Err(GovernanceError::AlreadyDelegated);
    }

    let locked = get_locked_balance(env, &delegator);
    if locked > 0 {
        Series::Account(&previous).add(env, -locked);
        Series::Account(&delegatee).add(env, locked);
    }

    let now = env.ledger().timestamp();
    env.storage().persistent().set(
        &GovernanceDataKey::DelegationRecord(delegator.clone()),
        &DelegationRecord {
            delegator: delegator.clone(),
            delegatee: delegatee.clone(),
            delegated_at: now,
            depth: 1,
        },
    );

    VoteDelegatedEvent {
        delegator,
        delegatee,
        delegated_at: now,
    }
    .publish(env);

    Ok(())
}

/// Return `delegator`'s voting power to themselves.
pub fn revoke_delegation(env: &Env, delegator: Address) -> Result<(), GovernanceError> {
    delegator.require_auth();

    if is_vote_locked(env, &delegator) {
        return Err(GovernanceError::VotesLocked);
    }

    let record = get_delegation(env, &delegator).ok_or(GovernanceError::NotDelegated)?;

    let locked = get_locked_balance(env, &delegator);
    if locked > 0 {
        Series::Account(&record.delegatee).add(env, -locked);
        Series::Account(&delegator).add(env, locked);
    }

    env.storage()
        .persistent()
        .remove(&GovernanceDataKey::DelegationRecord(delegator.clone()));

    VoteDelegationRevokedEvent {
        delegator,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

/// Query the active delegation for a delegator.
pub fn get_delegation(env: &Env, delegator: &Address) -> Option<DelegationRecord> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::DelegationRecord(delegator.clone()))
}

/// Vote tokens `owner` has locked in governance.
pub fn get_locked_balance(env: &Env, owner: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::LockedBalance(owner.clone()))
        .unwrap_or(0)
}

/// Current voting power of `account`: its own locked tokens unless delegated
/// away, plus tokens delegated to it.
pub fn get_votes(env: &Env, account: &Address) -> i128 {
    Series::Account(account).latest(env)
}

/// Voting power of `account` strictly before `timestamp`.
pub fn get_past_votes(env: &Env, account: &Address, timestamp: u64) -> i128 {
    Series::Account(account).value_before(env, timestamp)
}

/// Total vote tokens currently locked in governance.
pub fn get_total_locked(env: &Env) -> i128 {
    Series::TotalLocked.latest(env)
}

/// Total vote tokens locked strictly before `timestamp`.
pub fn get_past_total_locked(env: &Env, timestamp: u64) -> i128 {
    Series::TotalLocked.value_before(env, timestamp)
}
