// SPDX-License-Identifier: MIT

#![no_std]

use soroban_sdk::{contract, contractimpl, symbol, vec, Address, Env, Symbol, Vec};

pub const TIMELOCK_ADMIN: Symbol = symbol!("ADMIN");
pub const TIMELOCK_PROPOSER: Symbol = symbol!("PROPOSER");
pub const TIMELOCK_EXECUTOR: Symbol = symbol!("EXECUTOR");

#[contract]
pub struct TimelockContract;

#[contractimpl]
impl TimelockContract {
    pub fn initialize(env: Env, admin: Address, delay: u64) {
        if env.storage().instance().has(&TIMELOCK_ADMIN) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&TIMELOCK_ADMIN, &admin);
        env.storage().instance().set(&symbol!("DELAY"), &delay);
    }

    pub fn schedule(
        env: Env,
        caller: Address,
        target: Address,
        function: Symbol,
        args: Vec<Val>,
        eta: u64,
    ) -> u64 {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&TIMELOCK_ADMIN).unwrap();
        if caller != admin {
            panic!("Not authorized");
        }

        let delay: u64 = env.storage().instance().get(&symbol!("DELAY")).unwrap();
        let block_time = env.ledger().timestamp();
        if eta < block_time + delay {
            panic!("ETA too soon");
        }

        let id = env.storage().instance().get(&symbol!("NEXT_ID")).unwrap_or(0);
        env.storage().instance().set(&symbol!("NEXT_ID"), &(id + 1));

        env.storage().persistent().set(&(id, symbol!("TARGET")), &target);
        env.storage().persistent().set(&(id, symbol!("FUNCTION")), &function);
        env.storage().persistent().set(&(id, symbol!("ARGS")), &args);
        env.storage().persistent().set(&(id, symbol!("ETA")), &eta);

        id
    }

    pub fn execute(env: Env, caller: Address, id: u64) {
        caller.require_auth();
        let executor: Address = env.storage().instance().get(&TIMELOCK_EXECUTOR).unwrap();
        if caller != executor {
            panic!("Not authorized");
        }

        let eta: u64 = env.storage().persistent().get(&(id, symbol!("ETA"))).unwrap();
        let block_time = env.ledger().timestamp();
        if block_time < eta {
            panic!("Timelock not expired");
        }

        let target: Address = env.storage().persistent().get(&(id, symbol!("TARGET"))).unwrap();
        let function: Symbol = env.storage().persistent().get(&(id, symbol!("FUNCTION"))).unwrap();
        let args: Vec<Val> = env.storage().persistent().get(&(id, symbol!("ARGS"))).unwrap();

        env.invoke_contract(&target, &function, args);

        env.storage().persistent().remove(&(id, symbol!("TARGET")));
        env.storage().persistent().remove(&(id, symbol!("FUNCTION")));
        env.storage().persistent().remove(&(id, symbol!("ARGS")));
        env.storage().persistent().remove(&(id, symbol!("ETA")));
    }

    pub fn cancel(env: Env, caller: Address, id: u64) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&TIMELOCK_ADMIN).unwrap();
        if caller != admin {
            panic!("Not authorized");
        }

        env.storage().persistent().remove(&(id, symbol!("TARGET")));
        env.storage().persistent().remove(&(id, symbol!("FUNCTION")));
        env.storage().persistent().remove(&(id, symbol!("ARGS")));
        env.storage().persistent().remove(&(id, symbol!("ETA")));
    }
}
