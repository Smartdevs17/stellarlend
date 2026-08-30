use soroban_sdk::{Address, Env, Vec};

use crate::errors::GovernanceError;
use crate::storage::{GovernanceDataKey, GuardianConfig};
use crate::types::{MultisigConfig, RecoveryRequest, DEFAULT_RECOVERY_PERIOD};
use crate::events::{RecoveryApprovedEvent, RecoveryExecutedEvent, RecoveryStartedEvent};

pub fn start_recovery(
    env: &Env,
    initiator: Address,
    old_admin: Address,
    new_admin: Address,
) -> Result<(), GovernanceError> {
    initiator.require_auth();

    let guardian_config: GuardianConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::GuardianConfig)
        .ok_or(GovernanceError::GuardianNotFound)?;

    if !guardian_config.guardians.contains(&initiator) {
        return Err(GovernanceError::Unauthorized);
    }

    let recovery_key = GovernanceDataKey::RecoveryRequest;
    if env.storage().persistent().has(&recovery_key) {
        return Err(GovernanceError::RecoveryInProgress);
    }

    let now = env.ledger().timestamp();
    let request = RecoveryRequest {
        old_admin,
        new_admin: new_admin.clone(),
        initiator: initiator.clone(),
        initiated_at: now,
        expires_at: now + DEFAULT_RECOVERY_PERIOD,
    };

    env.storage().persistent().set(&recovery_key, &request);

    let approvals_key = GovernanceDataKey::RecoveryApprovals;
    let mut approvals = Vec::new(env);
    approvals.push_back(initiator.clone());
    env.storage().persistent().set(&approvals_key, &approvals);

    RecoveryStartedEvent {
        old_admin: request.old_admin,
        new_admin,
        initiator,
        expires_at: request.expires_at,
        timestamp: now,
    }
    .publish(env);

    Ok(())
}

pub fn approve_recovery(env: &Env, approver: Address) -> Result<(), GovernanceError> {
    approver.require_auth();

    let guardian_config: GuardianConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::GuardianConfig)
        .ok_or(GovernanceError::GuardianNotFound)?;

    if !guardian_config.guardians.contains(&approver) {
        return Err(GovernanceError::Unauthorized);
    }

    let recovery_key = GovernanceDataKey::RecoveryRequest;
    if !env.storage().persistent().has(&recovery_key) {
        return Err(GovernanceError::NoRecoveryInProgress);
    }

    let approvals_key = GovernanceDataKey::RecoveryApprovals;
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

    RecoveryApprovedEvent {
        approver,
        current_approvals: approvals.len(),
        threshold: guardian_config.threshold,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

pub fn execute_recovery(env: &Env, executor: Address) -> Result<(), GovernanceError> {
    executor.require_auth();

    let guardian_config: GuardianConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::GuardianConfig)
        .ok_or(GovernanceError::GuardianNotFound)?;

    let recovery_key = GovernanceDataKey::RecoveryRequest;
    let request: RecoveryRequest = env
        .storage()
        .persistent()
        .get(&recovery_key)
        .ok_or(GovernanceError::NoRecoveryInProgress)?;

    let now = env.ledger().timestamp();
    if now > request.expires_at {
        env.storage().persistent().remove(&recovery_key);
        return Err(GovernanceError::ProposalExpired);
    }

    let approvals_key = GovernanceDataKey::RecoveryApprovals;
    let approvals: Vec<Address> = env
        .storage()
        .persistent()
        .get(&approvals_key)
        .unwrap_or_else(|| Vec::new(env));

    if approvals.len() < guardian_config.threshold {
        return Err(GovernanceError::InsufficientApprovals);
    }

    let mut multisig_config: MultisigConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::MultisigConfig)
        .ok_or(GovernanceError::NotInitialized)?;

    let mut new_admins = Vec::new(env);
    for admin in multisig_config.admins.iter() {
        if admin != request.old_admin {
            new_admins.push_back(admin);
        }
    }
    new_admins.push_back(request.new_admin.clone());

    multisig_config.admins = new_admins;
    env.storage()
        .instance()
        .set(&GovernanceDataKey::MultisigConfig, &multisig_config);

    env.storage().persistent().remove(&recovery_key);
    env.storage().persistent().remove(&approvals_key);

    RecoveryExecutedEvent {
        old_admin: request.old_admin,
        new_admin: request.new_admin,
        executor,
        timestamp: now,
    }
    .publish(env);

    Ok(())
}
