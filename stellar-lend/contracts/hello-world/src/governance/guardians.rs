use soroban_sdk::{Address, Env, Vec};

use crate::errors::GovernanceError;
use crate::storage::{GovernanceDataKey, GuardianConfig};
use crate::events::{GuardianAddedEvent, GuardianRemovedEvent};

pub fn add_guardian(env: &Env, caller: Address, guardian: Address) -> Result<(), GovernanceError> {
    caller.require_auth();

    let admin: Address = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;

    if caller != admin {
        return Err(GovernanceError::Unauthorized);
    }

    let mut guardian_config: GuardianConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::GuardianConfig)
        .unwrap_or_else(|| GuardianConfig {
            guardians: Vec::new(env),
            threshold: 1,
        });

    if guardian_config.guardians.contains(&guardian) {
        return Err(GovernanceError::GuardianAlreadyExists);
    }

    guardian_config.guardians.push_back(guardian.clone());
    env.storage()
        .instance()
        .set(&GovernanceDataKey::GuardianConfig, &guardian_config);

    GuardianAddedEvent {
        guardian,
        added_by: caller,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

pub fn remove_guardian(
    env: &Env,
    caller: Address,
    guardian: Address,
) -> Result<(), GovernanceError> {
    caller.require_auth();

    let admin: Address = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;

    if caller != admin {
        return Err(GovernanceError::Unauthorized);
    }

    let mut guardian_config: GuardianConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::GuardianConfig)
        .ok_or(GovernanceError::GuardianNotFound)?;

    let mut new_guardians = Vec::new(env);
    let mut found = false;

    for g in guardian_config.guardians.iter() {
        if g != guardian {
            new_guardians.push_back(g);
        } else {
            found = true;
        }
    }

    if !found {
        return Err(GovernanceError::GuardianNotFound);
    }

    guardian_config.guardians = new_guardians;

    if guardian_config.threshold > guardian_config.guardians.len() {
        guardian_config.threshold = guardian_config.guardians.len();
    }

    env.storage()
        .instance()
        .set(&GovernanceDataKey::GuardianConfig, &guardian_config);

    GuardianRemovedEvent {
        guardian,
        removed_by: caller,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

pub fn set_guardian_threshold(
    env: &Env,
    caller: Address,
    threshold: u32,
) -> Result<(), GovernanceError> {
    caller.require_auth();

    let admin: Address = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;

    if caller != admin {
        return Err(GovernanceError::Unauthorized);
    }

    let mut guardian_config: GuardianConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::GuardianConfig)
        .ok_or(GovernanceError::GuardianNotFound)?;

    if threshold == 0 || threshold > guardian_config.guardians.len() {
        return Err(GovernanceError::InvalidGuardianConfig);
    }

    guardian_config.threshold = threshold;
    env.storage()
        .instance()
        .set(&GovernanceDataKey::GuardianConfig, &guardian_config);

    Ok(())
}
