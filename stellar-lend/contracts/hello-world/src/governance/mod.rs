pub mod analytics;
pub mod execution;
pub mod guardians;
pub mod multisig;
pub mod proposal;
pub mod recovery;
pub mod simulation;
pub mod voting;

use soroban_sdk::{Address, Env, Symbol};

pub use self::execution::execute_proposal_type;
pub use self::proposal::{
    cancel_proposal, create_admin_proposal, create_emergency_proposal, create_proposal,
    execute_proposal, propose_set_min_collateral_ratio, queue_proposal,
};
pub use self::voting::{
    delegate_vote, get_delegation, get_vote_lock, get_vote_power_snapshot, is_vote_locked,
    revoke_delegation, take_vote_power_snapshot, vote,
};
pub use self::simulation::{
    get_dry_run_cache, get_parameter_optimization_recommendation, get_simulation_cache,
    simulate_proposal, simulate_proposal_dry_run,
};
pub use self::multisig::{
    approve_proposal, execute_multisig_proposal, get_multisig_admins, get_multisig_config,
    get_multisig_threshold, get_proposal_approvals, set_multisig_admins, set_multisig_config,
    set_multisig_threshold,
};
pub use self::guardians::{add_guardian, remove_guardian, set_guardian_threshold};
pub use self::recovery::{approve_recovery, execute_recovery, start_recovery};
pub use self::analytics::{
    detect_suspicious_voting, enforce_proposal_rate_limit, get_governance_analytics,
    update_analytics_proposal_created, update_analytics_vote_cast,
};

// Re-export types used by other modules (e.g., top-level recovery.rs)
pub use crate::errors::GovernanceError;
pub use crate::storage::{GovernanceDataKey, GuardianConfig};
pub use crate::types::{
    DelegationRecord, GovernanceAnalytics, GovernanceConfig, MultisigConfig,
    ParameterOptimizationRecommendation, Proposal, ProposalDryRunResult, ProposalOutcome,
    ProposalSimulationResult, StateDiffEntry, ProposalStatus, ProposalType, RecoveryRequest,
    VoteInfo, VoteLock, VotePowerSnapshot, VoteType,
    BASIS_POINTS_SCALE, DEFAULT_EXECUTION_DELAY, DEFAULT_QUORUM_BPS, DEFAULT_RECOVERY_PERIOD,
    DEFAULT_TIMELOCK_DURATION, DEFAULT_VOTING_PERIOD, DEFAULT_VOTING_THRESHOLD,
    DELEGATION_DEADLINE, MAX_DELEGATION_DEPTH, MIN_TIMELOCK_DELAY, PROPOSAL_RATE_LIMIT,
    PROPOSAL_RATE_WINDOW,
};
pub use crate::events::{
    GovernanceInitializedEvent, GuardianAddedEvent, GuardianRemovedEvent, ProposalApprovedEvent,
    ProposalCancelledEvent, ProposalCreatedEvent, ProposalExecutedEvent, ProposalFailedEvent,
    ProposalQueuedEvent, RecoveryApprovedEvent, RecoveryExecutedEvent, RecoveryStartedEvent,
    SuspiciousGovActivityEvent, VoteCastEvent, VoteDelegatedEvent, VoteDelegationRevokedEvent,
    VoteLockedEvent, VotePowerSnapshotTakenEvent,
};

pub const MAX_DESCRIPTION_LEN: u32 = 256;

/// Initialize the governance module.
pub fn initialize(
    env: &Env,
    admin: Address,
    vote_token: Address,
    voting_period: Option<u64>,
    execution_delay: Option<u64>,
    quorum_bps: Option<u32>,
    proposal_threshold: Option<i128>,
    timelock_duration: Option<u64>,
    default_voting_threshold: Option<i128>,
) -> Result<(), GovernanceError> {
    if env.storage().instance().has(&GovernanceDataKey::Admin) {
        return Err(GovernanceError::AlreadyInitialized);
    }

    admin.require_auth();

    let config = GovernanceConfig {
        voting_period: voting_period.unwrap_or(crate::types::DEFAULT_VOTING_PERIOD),
        execution_delay: execution_delay.unwrap_or(crate::types::DEFAULT_EXECUTION_DELAY),
        quorum_bps: quorum_bps.unwrap_or(crate::types::DEFAULT_QUORUM_BPS),
        proposal_threshold: proposal_threshold.unwrap_or(0),
        vote_token,
        timelock_duration: timelock_duration.unwrap_or(crate::types::DEFAULT_TIMELOCK_DURATION),
        default_voting_threshold: default_voting_threshold
            .unwrap_or(crate::types::DEFAULT_VOTING_THRESHOLD),
    };

    if config.quorum_bps > 10000 {
        return Err(GovernanceError::InvalidQuorum);
    }
    if config.voting_period == 0 {
        return Err(GovernanceError::InvalidVotingPeriod);
    }

    env.storage()
        .instance()
        .set(&GovernanceDataKey::Admin, &admin);
    env.storage()
        .instance()
        .set(&GovernanceDataKey::Config, &config);
    env.storage()
        .instance()
        .set(&GovernanceDataKey::NextProposalId, &0u64);

    let mut admins = soroban_sdk::Vec::new(env);
    admins.push_back(admin.clone());
    let multisig_config = MultisigConfig {
        admins,
        threshold: 1,
    };
    env.storage()
        .instance()
        .set(&GovernanceDataKey::MultisigConfig, &multisig_config);

    let guardian_config = GuardianConfig {
        guardians: soroban_sdk::Vec::new(env),
        threshold: 1,
    };
    env.storage()
        .instance()
        .set(&GovernanceDataKey::GuardianConfig, &guardian_config);

    GovernanceInitializedEvent {
        admin,
        vote_token: config.vote_token,
        voting_period: config.voting_period,
        quorum_bps: config.quorum_bps,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

/// Query a proposal by ID.
pub fn get_proposal(env: &Env, proposal_id: u64) -> Option<crate::types::Proposal> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
}

/// Query a vote record.
pub fn get_vote(
    env: &Env,
    proposal_id: u64,
    voter: Address,
) -> Option<crate::types::VoteInfo> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::Vote(proposal_id, voter))
}

/// Query the governance config.
pub fn get_config(env: &Env) -> Option<GovernanceConfig> {
    env.storage().instance().get(&GovernanceDataKey::Config)
}

/// Query the admin address.
pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&GovernanceDataKey::Admin)
}

// ============================================================================
// Event Emitters
// ============================================================================

pub fn emit_proposal_created_event(env: &Env, proposal_id: &u64, proposer: &Address) {
    let topics = (
        Symbol::new(env, "proposal_created"),
        *proposal_id,
        proposer.clone(),
    );
    env.events().publish(topics, ());
}

pub fn emit_proposal_executed_event(env: &Env, proposal_id: &u64, executor: &Address) {
    let topics = (
        Symbol::new(env, "proposal_executed"),
        *proposal_id,
        executor.clone(),
    );
    env.events().publish(topics, ());
}

pub fn emit_guardian_added_event(env: &Env, guardian: &Address) {
    let topics = (Symbol::new(env, "guardian_added"), guardian.clone());
    env.events().publish(topics, ());
}

pub fn emit_guardian_removed_event(env: &Env, guardian: &Address) {
    let topics = (Symbol::new(env, "guardian_removed"), guardian.clone());
    env.events().publish(topics, ());
}

pub fn emit_recovery_started_event(
    env: &Env,
    old_admin: &Address,
    new_admin: &Address,
    initiator: &Address,
) {
    let topics = (
        Symbol::new(env, "recovery_started"),
        old_admin.clone(),
        new_admin.clone(),
    );
    env.events().publish(topics, initiator.clone());
}

pub fn emit_recovery_approved_event(env: &Env, approver: &Address) {
    let topics = (Symbol::new(env, "recovery_approved"), approver.clone());
    env.events().publish(topics, ());
}

pub fn emit_recovery_executed_event(
    env: &Env,
    old_admin: &Address,
    new_admin: &Address,
    executor: &Address,
) {
    let topics = (
        Symbol::new(env, "recovery_executed"),
        old_admin.clone(),
        new_admin.clone(),
    );
    env.events().publish(topics, executor.clone());
}

pub fn emit_recovery_cancelled_event(
    env: &Env,
    old_admin: &Address,
    new_admin: &Address,
    caller: &Address,
) {
    let topics = (
        Symbol::new(env, "recovery_cancelled"),
        old_admin.clone(),
        new_admin.clone(),
    );
    env.events().publish(topics, caller.clone());
}
