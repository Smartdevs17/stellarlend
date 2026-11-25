#![no_std]
use soroban_sdk::{contract, contractimpl, contracterror, Address, Env, Symbol, symbol_short, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PauseError {
    NotAdmin = 1,
    TimelockActive = 2,
    InsufficientSignatures = 3,
    ProposalNotFound = 4,
    AlreadyExecuted = 5,
}

#[contract]
pub struct TimelockedPauseGuard;

#[contractimpl]
impl TimelockedPauseGuard {
    pub const TIMELOCK_DELAY_SECONDS: u64 = 3600; // 1 hour mandatory delay

    pub fn propose_pause(env: Env, proposer: Address, pool_id: Address) -> Result<u64, PauseError> {
        proposer.require_auth();

        let execution_time = env.ledger().timestamp() + Self::TIMELOCK_DELAY_SECONDS;
        let key = (symbol_short!("PAUSE_P"), pool_id.clone());
        env.storage().persistent().set(&key, &(proposer.clone(), execution_time, false));

        env.events().publish((symbol_short!("PROP_PAUS"), pool_id), (proposer, execution_time));
        Ok(execution_time)
    }

    pub fn execute_pause(env: Env, caller: Address, pool_id: Address) -> Result<bool, PauseError> {
        caller.require_auth();

        let key = (symbol_short!("PAUSE_P"), pool_id.clone());
        let (proposer, execution_time, executed): (Address, u64, bool) =
            env.storage().persistent().get(&key).ok_or(PauseError::ProposalNotFound)?;

        if executed {
            return Err(PauseError::AlreadyExecuted);
        }

        let now = env.ledger().timestamp();
        if now < execution_time {
            return Err(PauseError::TimelockActive);
        }

        env.storage().persistent().set(&key, &(proposer, execution_time, true));
        env.events().publish((symbol_short!("EXEC_PAUS"), pool_id), (caller, now));
        Ok(true)
    }
}
