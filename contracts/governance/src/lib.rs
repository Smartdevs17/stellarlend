// SPDX-License-Identifier: MIT

#![no_std]

pub mod quorum;
pub mod delegation;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, symbol, Address, Env, Symbol};

pub const GOVERNANCE_ADMIN: Symbol = symbol!("ADMIN");
pub const TIMELOCK_DELAY_CONFIG: Symbol = symbol!("TIMELOCK_DELAY");
pub const EMERGENCY_ENABLED: Symbol = symbol!("EMERGENCY_ENABLED");

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    /// Initialize governance with admin and timelock delay configuration
    pub fn initialize(env: Env, admin: Address, timelock_delay: u64) {
        if env.storage().instance().has(&GOVERNANCE_ADMIN) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&GOVERNANCE_ADMIN, &admin);
        env.storage().instance().set(&TIMELOCK_DELAY_CONFIG, &timelock_delay);
        env.storage().instance().set(&EMERGENCY_ENABLED, &false);
    }

    /// Set the timelock delay for proposal execution (#1453)
    pub fn set_timelock_delay(env: Env, caller: Address, delay: u64) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&GOVERNANCE_ADMIN).unwrap();
        if caller != admin {
            panic!("Not authorized");
        }
        env.storage().instance().set(&TIMELOCK_DELAY_CONFIG, &delay);
    }

    /// Get the current timelock delay
    pub fn get_timelock_delay(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&TIMELOCK_DELAY_CONFIG)
            .unwrap_or(86400) // Default 1 day in seconds
    }

    /// Enable emergency governance fast-path for critical situations (#1455)
    /// This allows bypassing the normal voting and timelock procedures
    pub fn enable_emergency_mode(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&GOVERNANCE_ADMIN).unwrap();
        if caller != admin {
            panic!("Not authorized");
        }
        env.storage().instance().set(&EMERGENCY_ENABLED, &true);
    }

    /// Disable emergency governance fast-path
    pub fn disable_emergency_mode(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&GOVERNANCE_ADMIN).unwrap();
        if caller != admin {
            panic!("Not authorized");
        }
        env.storage().instance().set(&EMERGENCY_ENABLED, &false);
    }

    /// Check if emergency mode is active
    pub fn is_emergency_mode(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&EMERGENCY_ENABLED)
            .unwrap_or(false)
    }

    /// Set quorum configuration (#1454)
    pub fn set_quorum(env: Env, caller: Address, threshold: i128) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&GOVERNANCE_ADMIN).unwrap();
        if caller != admin {
            panic!("Not authorized");
        }
        if threshold <= 0 {
            panic!("Invalid quorum threshold");
        }
        quorum::set_quorum_config(&env, threshold);
    }

    /// Get current quorum configuration
    pub fn get_quorum(env: Env) -> i128 {
        quorum::get_quorum_config(&env)
    }

    /// Delegate voting power (#1452)
    pub fn delegate_votes(env: Env, delegator: Address, delegate: Address) {
        delegation::delegate_voting_power(&env, delegator, delegate);
    }

    /// Revoke voting power delegation
    pub fn revoke_delegation(env: Env, delegator: Address) {
        delegation::revoke_delegation(&env, delegator);
    }

    /// Get the delegate of an address
    pub fn get_delegate(env: Env, delegator: Address) -> Option<Address> {
        delegation::get_delegate(&env, delegator)
    }
}
