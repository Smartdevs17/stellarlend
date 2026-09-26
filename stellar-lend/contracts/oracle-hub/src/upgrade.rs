//! Real upgrade mechanism for the Oracle Hub.
//!
//! Governance proposes a candidate WASM hash with [`stage_upgrade`] and later
//! executes the swap with [`apply_upgrade`]. The actual code swap is performed
//! by Soroban's `env.deployer().update_current_contract_wasm`, which atomically
//! replaces the running contract code while preserving instance storage.
//!
//! Upgrades are versioned: `apply_upgrade` bumps the stored `Version` before
//! swapping code, and staged candidates are cleared after execution. Because
//! instance storage persists across the swap, upgradeable code must keep a
//! storage layout compatible with the previous version.
//!
//! # Multi-signature gate
//!
//! Protocol upgrades never swap code on a single key. By default (no explicit
//! configuration) the approver set is `[governance]` and the threshold is 1,
//! which preserves the legacy behaviour. The admin can tighten this with
//! [`init_upgrade_multisig`]: once a threshold > 1 is installed, an upgrade is
//! executed only after enough distinct approvers have signed **and** the
//! 48-hour upgrade timelock has elapsed, so a single compromised key can never
//! push a malicious build on its own.

use crate::storage::DataKey;
use crate::types::{UpgradeExecutedEvent, UpgradeMultisigConfiguredEvent, UpgradeStagedEvent};
use soroban_sdk::{contracterror, panic_with_error, Address, BytesN, Env, Vec};

/// Standard upgrade timelock: 48 hours in seconds.
pub const UPGRADE_TIMELOCK_SECS: u64 = 172_800;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum UpgradeError {
    NotConfigured = 1,
    InvalidThreshold = 2,
    AlreadyConfigured = 3,
    NotApprover = 4,
    AlreadyApproved = 5,
    NotEnoughApprovals = 6,
    TimelockNotElapsed = 7,
    UpgradeNotStaged = 8,
}

/// Governance-credentialed staging of the next contract code.
///
/// Staging records governance as the first approver; when a multisig threshold
/// is installed and governance is part of the approver set, its signature
/// counts toward the threshold like any other.
pub fn stage_upgrade(env: &Env, new_wasm: BytesN<32>, governance: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::ProposedWasm, &new_wasm);
    env.storage()
        .instance()
        .set(&DataKey::UpgradeStagedAt, &env.ledger().timestamp());

    let mut approvals = pending_approvals(env);
    if !approvals.contains(governance) {
        approvals.push_back(governance.clone());
        env.storage()
            .instance()
            .set(&DataKey::UpgradeApprovals, &approvals);
    }

    // A new proposal restarts the timelock countdown.
    env.storage()
        .instance()
        .set(&DataKey::UpgradeTimelockUntil, &0u64);

    UpgradeStagedEvent {
        wasm_hash: new_wasm.clone(),
        staged_by: governance.clone(),
    }
    .publish(env);
}

/// Install or replace the multi-signature approver set and threshold.
///
/// Admin-only. Threshold 1 with `approvers = [governance]` is the default and
/// matches legacy single-key behaviour. Configuring a threshold of 0 or a
/// threshold greater than the approver count is rejected.
pub fn init_upgrade_multisig(
    env: &Env,
    caller: &Address,
    approvers: Vec<Address>,
    threshold: u32,
) -> Result<(), UpgradeError> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, UpgradeError::NotConfigured));

    if caller != &admin {
        return Err(UpgradeError::NotConfigured);
    }
    if threshold == 0 || threshold > approvers.len() {
        return Err(UpgradeError::InvalidThreshold);
    }

    env.storage()
        .instance()
        .set(&DataKey::UpgradeApprovers, &approvers);
    env.storage()
        .instance()
        .set(&DataKey::UpgradeThreshold, &threshold);
    env.storage()
        .instance()
        .set(&DataKey::UpgradeApprovals, &Vec::<Address>::new(env));

    UpgradeMultisigConfiguredEvent { threshold }.publish(env);

    Ok(())
}

/// Record one approver's signature on the pending upgrade.
///
/// Returns the running approval count. When the counter reaches the installed
/// threshold the 48-hour upgrade timelock starts; until then the candidate
/// cannot be applied.
pub fn approve_upgrade(env: &Env, approver: &Address) -> Result<u32, UpgradeError> {
    let approvers: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeApprovers)
        .unwrap_or_else(|| {
            let mut v = Vec::new(env);
            v.push_back(
                env.storage()
                    .instance()
                    .get(&DataKey::Governance)
                    .unwrap_or_else(|| panic_with_error!(env, UpgradeError::NotConfigured)),
            );
            v
        });

    if !approvers.contains(approver) {
        return Err(UpgradeError::NotApprover);
    }

    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeThreshold)
        .unwrap_or(1);

    let mut approvals = pending_approvals(env);
    if approvals.contains(approver) {
        return Err(UpgradeError::AlreadyApproved);
    }

    approvals.push_back(approver.clone());
    env.storage()
        .instance()
        .set(&DataKey::UpgradeApprovals, &approvals);

    if approvals.len() >= threshold {
        let timelock_until = env
            .ledger()
            .timestamp()
            .saturating_add(UPGRADE_TIMELOCK_SECS);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeTimelockUntil, &timelock_until);
    }

    Ok(approvals.len())
}

/// Pending (staged, not yet applied) WASM hash, if any.
pub fn pending_wasm(env: &Env) -> Option<BytesN<32>> {
    env.storage().instance().get(&DataKey::ProposedWasm)
}

/// Number of distinct approvals the pending upgrade has collected.
pub fn approval_count(env: &Env) -> u32 {
    pending_approvals(env).len()
}

/// Installed multisig threshold; 1 when not configured.
pub fn upgrade_threshold(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::UpgradeThreshold)
        .unwrap_or(1)
}

/// Whether the pending upgrade has cleared the threshold and the timelock.
pub fn can_execute(env: &Env) -> bool {
    let threshold = upgrade_threshold(env);
    if pending_approvals(env).len() < threshold {
        return false;
    }
    let until: u64 = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeTimelockUntil)
        .unwrap_or(0);
    env.ledger().timestamp() >= until
}

/// Governance-credentialed application of the staged upgrade.
///
/// Raises if `new_wasm` is not yet staged, if the installed multisig threshold
/// has not been met, or if the upgrade timelock has not elapsed.
pub fn apply_upgrade(env: &Env, governance: &Address) -> BytesN<32> {
    if !env.storage().instance().has(&DataKey::ProposedWasm) {
        panic_with_error!(env, UpgradeError::UpgradeNotStaged);
    }

    let threshold = upgrade_threshold(env);
    if pending_approvals(env).len() < threshold {
        panic_with_error!(env, UpgradeError::NotEnoughApprovals);
    }

    let timelock_until: u64 = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeTimelockUntil)
        .unwrap_or(0);
    if threshold > 1 && env.ledger().timestamp() < timelock_until {
        panic_with_error!(env, UpgradeError::TimelockNotElapsed);
    }

    let new_wasm: BytesN<32> = env
        .storage()
        .instance()
        .get(&DataKey::ProposedWasm)
        .unwrap_or_else(|| panic_with_error!(env, UpgradeError::UpgradeNotStaged));

    let old_version: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);
    let new_version = old_version.saturating_add(1);

    // Order matters: clear the candidate and bump the version while still
    // running the old code, then swap. Instance storage survives the swap, so
    // `Version` is observable to the freshly upgraded code.
    env.storage().instance().remove(&DataKey::ProposedWasm);
    env.storage()
        .instance()
        .set(&DataKey::Version, &new_version);
    env.storage()
        .instance()
        .set(&DataKey::UpgradeApprovals, &Vec::<Address>::new(env));
    env.storage()
        .instance()
        .set(&DataKey::UpgradeTimelockUntil, &0u64);

    UpgradeExecutedEvent {
        old_version,
        new_version,
        wasm_hash: new_wasm.clone(),
        executed_by: governance.clone(),
    }
    .publish(env);

    env.deployer()
        .update_current_contract_wasm(new_wasm.clone());

    new_wasm
}

fn pending_approvals(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::UpgradeApprovals)
        .unwrap_or_else(|| Vec::new(env))
}
