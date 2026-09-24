//! Governance voting over parameter proposals (issue #697).
//!
//! Before this module a proposal was accepted by whoever held the governance
//! address. That is fine for a bootstrap deployment and wrong for a live
//! protocol: a parameter that decides who gets liquidated should be decided by
//! the governance body, not by one key.
//!
//! Voting is **opt-in**. Until [`VotingConfig`] is installed, proposals are
//! accepted exactly as before, so existing deployments keep working and can
//! enable voting in a later transaction. Once a config is installed, a proposal
//! must clear both a quorum (enough weight voted) and an approval threshold
//! (enough of that weight in favour) before it can be accepted.

use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::BPS_DIVISOR;

/// Rules a proposal must satisfy before it can be accepted.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct VotingConfig {
    /// Share of total voting power that must participate, in basis points.
    pub quorum_bps: i128,
    /// Share of the votes cast that must be in favour, in basis points.
    pub approval_threshold_bps: i128,
    /// How long after creation a proposal accepts votes, in seconds.
    ///
    /// A proposal's voting window is independent of its timelock: voting closes
    /// first, then the timelock has to elapse before the change takes effect.
    pub voting_period_seconds: u64,
}

impl VotingConfig {
    /// Rejects a configuration that could never be satisfied, or that would let
    /// a proposal pass with no support at all.
    pub fn is_valid(&self) -> bool {
        self.quorum_bps > 0
            && self.quorum_bps <= BPS_DIVISOR
            && self.approval_threshold_bps > 0
            && self.approval_threshold_bps <= BPS_DIVISOR
            && self.voting_period_seconds > 0
    }
}

/// One address's vote on one proposal.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ParameterVote {
    pub proposal_id: u64,
    pub voter: Address,
    /// `true` for, `false` against.
    pub support: bool,
    /// The voter's weight at the time the vote was cast.
    pub weight: i128,
    pub voted_at: u64,
}

/// Running tally for a proposal.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct VoteTally {
    pub proposal_id: u64,
    pub for_weight: i128,
    pub against_weight: i128,
    pub voter_count: u32,
    /// Total voting power registered when the tally was read.
    pub total_voting_power: i128,
}

impl VoteTally {
    /// Total weight cast, for and against.
    pub fn participation(&self) -> i128 {
        self.for_weight.saturating_add(self.against_weight)
    }

    /// Whether enough voting power participated to meet the quorum.
    pub fn meets_quorum(&self, config: &VotingConfig) -> bool {
        if self.total_voting_power <= 0 {
            return false;
        }
        // Cross-multiplied rather than dividing first: truncating the required
        // weight would let a quorum of 20.00% pass on 19.99% of the power.
        self.participation().saturating_mul(BPS_DIVISOR)
            >= self.total_voting_power.saturating_mul(config.quorum_bps)
    }

    /// Whether the votes cast approve the proposal.
    ///
    /// Measured against the weight actually cast, not the total registered
    /// power — abstaining neither approves nor blocks.
    pub fn is_approved(&self, config: &VotingConfig) -> bool {
        let participation = self.participation();
        if participation <= 0 {
            return false;
        }
        // Cross-multiplied for the same reason as the quorum check: with a
        // divide-first comparison, a 50/50 split clears a 50.01% threshold.
        self.for_weight.saturating_mul(BPS_DIVISOR)
            >= participation.saturating_mul(config.approval_threshold_bps)
    }

    /// Whether the proposal has passed: quorum reached and approved.
    pub fn has_passed(&self, config: &VotingConfig) -> bool {
        self.meets_quorum(config) && self.is_approved(config)
    }
}

/// Builds a tally from the recorded votes for a proposal.
pub fn tally_votes(
    proposal_id: u64,
    votes: &Vec<ParameterVote>,
    total_voting_power: i128,
) -> VoteTally {
    let mut for_weight = 0i128;
    let mut against_weight = 0i128;
    for vote in votes.iter() {
        if vote.support {
            for_weight = for_weight.saturating_add(vote.weight);
        } else {
            against_weight = against_weight.saturating_add(vote.weight);
        }
    }
    VoteTally {
        proposal_id,
        for_weight,
        against_weight,
        voter_count: votes.len(),
        total_voting_power,
    }
}

/// Whether `voter` has already voted on this proposal.
pub fn has_voted(votes: &Vec<ParameterVote>, voter: &Address) -> bool {
    votes.iter().any(|vote| &vote.voter == voter)
}

/// Whether the voting window for a proposal created at `created_at` is open.
pub fn is_voting_open(env: &Env, created_at: u64, config: &VotingConfig) -> bool {
    let now = env.ledger().timestamp();
    now < created_at.saturating_add(config.voting_period_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    fn config() -> VotingConfig {
        VotingConfig {
            quorum_bps: 2_000,             // 20% must participate
            approval_threshold_bps: 5_001, // a simple majority of votes cast
            voting_period_seconds: 3 * 86_400,
        }
    }

    fn vote(env: &Env, proposal_id: u64, support: bool, weight: i128) -> ParameterVote {
        ParameterVote {
            proposal_id,
            voter: Address::generate(env),
            support,
            weight,
            voted_at: 0,
        }
    }

    #[test]
    fn config_validation() {
        assert!(config().is_valid());
        assert!(!VotingConfig {
            quorum_bps: 0,
            ..config()
        }
        .is_valid());
        assert!(!VotingConfig {
            quorum_bps: BPS_DIVISOR + 1,
            ..config()
        }
        .is_valid());
        assert!(!VotingConfig {
            approval_threshold_bps: 0,
            ..config()
        }
        .is_valid());
        assert!(!VotingConfig {
            voting_period_seconds: 0,
            ..config()
        }
        .is_valid());
    }

    #[test]
    fn tally_sums_by_side() {
        let env = Env::default();
        let mut votes = Vec::new(&env);
        votes.push_back(vote(&env, 1, true, 300));
        votes.push_back(vote(&env, 1, true, 200));
        votes.push_back(vote(&env, 1, false, 100));

        let tally = tally_votes(1, &votes, 1_000);
        assert_eq!(tally.for_weight, 500);
        assert_eq!(tally.against_weight, 100);
        assert_eq!(tally.voter_count, 3);
        assert_eq!(tally.participation(), 600);
    }

    #[test]
    fn quorum_needs_enough_participation() {
        let env = Env::default();
        let c = config();

        let mut votes = Vec::new(&env);
        votes.push_back(vote(&env, 1, true, 100)); // 10% of 1,000
        assert!(!tally_votes(1, &votes, 1_000).meets_quorum(&c));

        votes.push_back(vote(&env, 1, false, 100)); // 20% total
        assert!(tally_votes(1, &votes, 1_000).meets_quorum(&c));
    }

    #[test]
    fn quorum_fails_without_registered_power() {
        let env = Env::default();
        let votes = Vec::new(&env);
        assert!(!tally_votes(1, &votes, 0).meets_quorum(&config()));
    }

    #[test]
    fn approval_measured_against_votes_cast() {
        let env = Env::default();
        let c = config();

        let mut votes = Vec::new(&env);
        votes.push_back(vote(&env, 1, true, 501));
        votes.push_back(vote(&env, 1, false, 499));
        assert!(tally_votes(1, &votes, 1_000).is_approved(&c));

        let mut split = Vec::new(&env);
        split.push_back(vote(&env, 1, true, 500));
        split.push_back(vote(&env, 1, false, 500));
        // An exact tie does not clear a >50% threshold.
        assert!(!tally_votes(1, &split, 1_000).is_approved(&c));
    }

    #[test]
    fn no_votes_is_not_approval() {
        let env = Env::default();
        let votes = Vec::new(&env);
        let tally = tally_votes(1, &votes, 1_000);
        assert!(!tally.is_approved(&config()));
        assert!(!tally.has_passed(&config()));
    }

    #[test]
    fn passing_requires_both_quorum_and_approval() {
        let env = Env::default();
        let c = config();

        // Unanimous but below quorum.
        let mut thin = Vec::new(&env);
        thin.push_back(vote(&env, 1, true, 50));
        assert!(!tally_votes(1, &thin, 1_000).has_passed(&c));

        // Quorum reached but voted down.
        let mut rejected = Vec::new(&env);
        rejected.push_back(vote(&env, 1, true, 100));
        rejected.push_back(vote(&env, 1, false, 400));
        assert!(!tally_votes(1, &rejected, 1_000).has_passed(&c));

        // Both satisfied.
        let mut passing = Vec::new(&env);
        passing.push_back(vote(&env, 1, true, 400));
        passing.push_back(vote(&env, 1, false, 100));
        assert!(tally_votes(1, &passing, 1_000).has_passed(&c));
    }

    #[test]
    fn duplicate_voters_are_detectable() {
        let env = Env::default();
        let voter = Address::generate(&env);
        let mut votes = Vec::new(&env);
        votes.push_back(ParameterVote {
            proposal_id: 1,
            voter: voter.clone(),
            support: true,
            weight: 100,
            voted_at: 0,
        });
        assert!(has_voted(&votes, &voter));
        assert!(!has_voted(&votes, &Address::generate(&env)));
    }

    #[test]
    fn voting_window_closes() {
        let env = Env::default();
        let c = config();
        assert!(is_voting_open(&env, 0, &c));

        env.ledger().set_timestamp(c.voting_period_seconds);
        assert!(!is_voting_open(&env, 0, &c));
    }
}
