use soroban_sdk::{Address, Env, Symbol};

use crate::errors::GovernanceError;
use crate::events::SuspiciousGovActivityEvent;
use crate::storage::GovernanceDataKey;
use crate::types::{
    GovernanceAnalytics, BASIS_POINTS_SCALE, PROPOSAL_RATE_LIMIT, PROPOSAL_RATE_WINDOW,
};

/// Query governance analytics.
pub fn get_governance_analytics(env: &Env) -> GovernanceAnalytics {
    let analytics_key = GovernanceDataKey::GovernanceAnalytics;
    env.storage()
        .persistent()
        .get(&analytics_key)
        .unwrap_or(GovernanceAnalytics {
            total_proposals: 0,
            total_votes: 0,
            suspicious_proposals: 0,
            last_suspicious_at: 0,
            max_single_voter_power: 0,
        })
}

/// Enforce proposal rate limiting.
pub fn enforce_proposal_rate_limit(env: &Env, proposer: &Address) -> Result<(), GovernanceError> {
    let now = env.ledger().timestamp();

    let window_key = GovernanceDataKey::ProposalWindowStart(proposer.clone());
    let count_key = GovernanceDataKey::ProposalCreationCount(proposer.clone());

    let window_start: Option<u64> = env.storage().persistent().get(&window_key);
    let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

    let window_elapsed = window_start
        .map(|start| now.saturating_sub(start) > PROPOSAL_RATE_WINDOW)
        .unwrap_or(true);
    if window_elapsed {
        env.storage().persistent().set(&window_key, &now);
        env.storage().persistent().set(&count_key, &1u32);
    } else {
        if count >= PROPOSAL_RATE_LIMIT {
            return Err(GovernanceError::ProposalRateLimitExceeded);
        }
        env.storage().persistent().set(&count_key, &(count + 1));
    }

    Ok(())
}

/// Flag a single voter holding more than a third of the voting power that
/// counts on a proposal. This is informational: snapshots already stop
/// flash-loaned power from voting, so a large voter is legitimate but worth
/// surfacing to monitoring.
pub fn detect_suspicious_voting(
    env: &Env,
    proposal_id: u64,
    voter: &Address,
    voter_power: i128,
    total_supply_estimate: i128,
) {
    let threshold_bps: i128 = 3333;
    if total_supply_estimate > 0
        && (voter_power * BASIS_POINTS_SCALE) / total_supply_estimate > threshold_bps
    {
        let reason = Symbol::new(env, "large_single_voter");

        SuspiciousGovActivityEvent {
            proposal_id,
            voter: voter.clone(),
            voter_power,
            total_supply_estimate,
            reason,
            timestamp: env.ledger().timestamp(),
        }
        .publish(env);

        let analytics_key = GovernanceDataKey::GovernanceAnalytics;
        let mut analytics: GovernanceAnalytics = env
            .storage()
            .persistent()
            .get(&analytics_key)
            .unwrap_or(GovernanceAnalytics {
                total_proposals: 0,
                total_votes: 0,
                suspicious_proposals: 0,
                last_suspicious_at: 0,
                max_single_voter_power: 0,
            });

        analytics.suspicious_proposals += 1;
        analytics.last_suspicious_at = env.ledger().timestamp();
        if voter_power > analytics.max_single_voter_power {
            analytics.max_single_voter_power = voter_power;
        }

        env.storage().persistent().set(&analytics_key, &analytics);
    }
}

/// Update analytics when a proposal is created.
pub fn update_analytics_proposal_created(env: &Env) {
    let analytics_key = GovernanceDataKey::GovernanceAnalytics;
    let mut analytics: GovernanceAnalytics = env
        .storage()
        .persistent()
        .get(&analytics_key)
        .unwrap_or(GovernanceAnalytics {
            total_proposals: 0,
            total_votes: 0,
            suspicious_proposals: 0,
            last_suspicious_at: 0,
            max_single_voter_power: 0,
        });
    analytics.total_proposals += 1;
    env.storage().persistent().set(&analytics_key, &analytics);
}

/// Update analytics when a vote is cast.
pub fn update_analytics_vote_cast(env: &Env) {
    let analytics_key = GovernanceDataKey::GovernanceAnalytics;
    let mut analytics: GovernanceAnalytics = env
        .storage()
        .persistent()
        .get(&analytics_key)
        .unwrap_or(GovernanceAnalytics {
            total_proposals: 0,
            total_votes: 0,
            suspicious_proposals: 0,
            last_suspicious_at: 0,
            max_single_voter_power: 0,
        });
    analytics.total_votes += 1;
    env.storage().persistent().set(&analytics_key, &analytics);
}
