//! #1588: Governance delegate registry with revocable proxy voting.
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DelegateKey {
    Delegate(Address),  // delegator -> delegate
}

/// Set or update a voting delegate for `delegator`.
pub fn set_delegate(env: &Env, delegator: &Address, delegate: &Address) {
    delegator.require_auth();
    env.storage().persistent().set(&DelegateKey::Delegate(delegator.clone()), delegate);
}

/// Revoke an existing delegation.
pub fn revoke_delegate(env: &Env, delegator: &Address) {
    delegator.require_auth();
    env.storage().persistent().remove(&DelegateKey::Delegate(delegator.clone()));
}

/// Resolve effective voter — returns the delegate if one is set, else the caller.
pub fn effective_voter(env: &Env, caller: &Address) -> Address {
    env.storage()
        .persistent()
        .get::<DelegateKey, Address>(&DelegateKey::Delegate(caller.clone()))
        .unwrap_or_else(|| caller.clone())
}
