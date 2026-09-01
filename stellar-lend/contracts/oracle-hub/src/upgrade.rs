//! Real upgrade mechanism for the Oracle Hub.
//!
//! Governance stages a candidate WASM hash with [`staged_storage`] and later
//! executes the swap with [`apply_upgrade`]. The actual code swap is performed
//! by Soroban's `env.deployer().update_current_contract_wasm`, which atomically
//! replaces the running contract code while preserving instance storage.
//!
//! Upgrades are versioned: `apply_upgrade` bumps the stored `Version` before
//! swapping code, and staged candidates are cleared after execution. Because
//! instance storage persists across the swap, upgradeable code must keep a
//! storage layout compatible with the previous version.

use crate::storage::DataKey;
use crate::types::{UpgradeExecutedEvent, UpgradeStagedEvent};
use soroban_sdk::{Address, BytesN, Env};

/// Governance-credentialed staging of the next contract code.
pub fn stage_upgrade(env: &Env, new_wasm: BytesN<32>, governance: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::ProposedWasm, &new_wasm);
    UpgradeStagedEvent {
        wasm_hash: new_wasm.clone(),
        staged_by: governance.clone(),
    }
    .publish(env);
}

/// Pending (staged, not yet applied) WASM hash, if any.
pub fn pending_wasm(env: &Env) -> Option<BytesN<32>> {
    env.storage().instance().get(&DataKey::ProposedWasm)
}

/// Governance-credentialed application of the staged upgrade.
///
/// Raises if `new_wasm` is not yet staged.
pub fn apply_upgrade(env: &Env, governance: &Address) -> BytesN<32> {
    let new_wasm: BytesN<32> = env
        .storage()
        .instance()
        .get(&DataKey::ProposedWasm)
        .expect("No upgrade staged: call stage_upgrade first");

    let old_version: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);
    let new_version = old_version.saturating_add(1);

    // Order matters: clear the candidate and bump the version while still
    // running the old code, then swap. Instance storage survives the swap, so
    // `Version` is observable to the freshly upgraded code.
    env.storage().instance().remove(&DataKey::ProposedWasm);
    env.storage()
        .instance()
        .set(&DataKey::Version, &new_version);

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
