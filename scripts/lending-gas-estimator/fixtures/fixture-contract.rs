/**
 * Contract entry points for the storage indexer tests (#1011).
 *
 * The wiring mirrors the real lending contract: `#[contractimpl]` methods are
 * thin and call into module functions, some of which are re-exported under an
 * alias. Anything the indexer resolves has to survive that indirection.
 */

#![no_std]

extern crate alloc;

use soroban_sdk::{contract, contractimpl, Address, Bytes, Env, Vec};

mod pool;
use pool::{
    batch_apply as batch_apply_logic, flash as flash_logic, is_paused,
    pause_probe as pause_probe_logic, read_limits, PoolError,
};

#[contract]
pub struct FixturePool;

#[contractimpl]
impl FixturePool {
    /// Deposit collateral.
    pub fn deposit(env: Env, user: Address, amount: i128) -> Result<i128, PoolError> {
        if is_paused(&env) {
            return Err(PoolError::Paused);
        }
        pool::apply_deposit(&env, user, amount)
    }

    /// Apply several deposits at once.
    pub fn batch(env: Env, user: Address, assets: Vec<Address>, amount: i128) -> i128 {
        if is_paused(&env) {
            return 0;
        }
        batch_apply_logic(&env, assets, amount)
    }

    /// Read the borrow limits.
    pub fn limits(env: Env) -> pool::Limits {
        read_limits(&env)
    }

    /// Guarded read that probes then reads the same key.
    pub fn probe(env: Env) -> Option<i128> {
        pool::guarded_read(&env)
    }

    /// Flash loan with a token transfer and a callback.
    pub fn flash(env: Env, receiver: Address, amount: i128) -> Result<(), PoolError> {
        flash_logic(&env, receiver, amount)
    }

    /// Echo a note without touching storage.
    pub fn note(env: Env) -> String {
        pause_probe_logic(&env)
    }

    /// Execute a compressed payload.
    pub fn compressed(env: Env, payload: Bytes) -> u32 {
        let _ = payload;
        0
    }
}
