#![no_std]
use soroban_sdk::{contract, contractimpl, contracterror, Address, Env, Symbol, symbol_short};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RedemptionError {
    CooldownNotMet = 1,
    LimitExceeded = 2,
    QueueEmpty = 3,
    InvalidAmount = 4,
}

#[contract]
pub struct RateLimitedRedemptionQueue;

#[contractimpl]
impl RateLimitedRedemptionQueue {
    pub fn enqueue_redemption(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
        epoch_window: u64
    ) -> Result<u64, RedemptionError> {
        user.require_auth();

        if amount <= 0 {
            return Err(RedemptionError::InvalidAmount);
        }

        let unlock_time = env.ledger().timestamp() + epoch_window;
        let key = (symbol_short!("QUEUE"), user.clone(), asset.clone());
        env.storage().persistent().set(&key, &(amount, unlock_time));

        env.events().publish((symbol_short!("ENQUEUE"), user), (asset, amount, unlock_time));
        Ok(unlock_time)
    }

    pub fn execute_redemption(
        env: Env,
        user: Address,
        asset: Address
    ) -> Result<i128, RedemptionError> {
        user.require_auth();

        let key = (symbol_short!("QUEUE"), user.clone(), asset.clone());
        let (amount, unlock_time): (i128, u64) = env.storage().persistent().get(&key).ok_or(RedemptionError::QueueEmpty)?;

        if env.ledger().timestamp() < unlock_time {
            return Err(RedemptionError::CooldownNotMet);
        }

        env.storage().persistent().remove(&key);
        env.events().publish((symbol_short!("EXECUTE"), user), (asset, amount));
        Ok(amount)
    }
}
