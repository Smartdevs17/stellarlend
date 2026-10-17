use soroban_sdk::{Env, Address, Map, symbol};

pub const DELEGATION_KEY: &str = "DELEGATION";

/// Delegate voting power from delegator to delegate.
/// delegator: The address delegating their voting power
/// delegate: The address receiving the delegated voting power
pub fn delegate_voting_power(env: &Env, delegator: Address, delegate: Address) {
    delegator.require_auth();

    if delegator == delegate {
        panic!("Cannot delegate to self");
    }

    // Store delegation: delegator -> delegate
    let mut delegations: Map<Address, Address> = env
        .storage()
        .persistent()
        .get(&symbol!("DELEGATIONS"))
        .unwrap_or_else(|| Map::new(env));

    delegations.set(delegator.clone(), delegate.clone());
    env.storage().persistent().set(&symbol!("DELEGATIONS"), &delegations);
}

/// Revoke delegation for a delegator, restoring their voting power
pub fn revoke_delegation(env: &Env, delegator: Address) {
    delegator.require_auth();

    let mut delegations: Map<Address, Address> = env
        .storage()
        .persistent()
        .get(&symbol!("DELEGATIONS"))
        .unwrap_or_else(|| Map::new(env));

    delegations.remove(delegator);
    env.storage().persistent().set(&symbol!("DELEGATIONS"), &delegations);
}

/// Get the delegate of a delegator, if any
pub fn get_delegate(env: &Env, delegator: Address) -> Option<Address> {
    let delegations: Map<Address, Address> = env
        .storage()
        .persistent()
        .get(&symbol!("DELEGATIONS"))
        .unwrap_or_else(|| Map::new(env));

    delegations.get(delegator).ok()
}

/// Check if an address has delegated their voting power
pub fn has_delegated(env: &Env, delegator: Address) -> bool {
    get_delegate(env, delegator).is_some()
}
