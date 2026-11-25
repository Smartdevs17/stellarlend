#![no_std]
use soroban_sdk::{contract, contractimpl, contracterror, Address, Env, Symbol, symbol_short};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InterestError {
    InvalidBlockWindow = 1,
    CheckpointAlreadyExists = 2,
    ArithmeticOverflow = 3,
}

#[contract]
pub struct InterestAccrualCheckpoint;

#[contractimpl]
impl InterestAccrualCheckpoint {
    pub fn checkpoint_interest(
        env: Env,
        caller: Address,
        pool_id: Address,
        current_block: u64,
        last_block: u64,
        cumulative_borrow_index: u128,
        borrow_rate_per_block: u128
    ) -> Result<u128, InterestError> {
        caller.require_auth();

        if current_block <= last_block {
            return Err(InterestError::InvalidBlockWindow);
        }

        let delta_blocks = (current_block - last_block) as u128;
        // Simple linear accrual: index * (1 + rate * delta)
        let interest_factor = delta_blocks * borrow_rate_per_block / 1_000_000_000;
        let new_index = cumulative_borrow_index + (cumulative_borrow_index * interest_factor / 1_000_000_000);

        let key = (symbol_short!("CHECKPT"), pool_id.clone(), current_block);
        env.storage().persistent().set(&key, &new_index);

        env.events().publish((symbol_short!("INDEX"), pool_id), (current_block, new_index));
        Ok(new_index)
    }
}
