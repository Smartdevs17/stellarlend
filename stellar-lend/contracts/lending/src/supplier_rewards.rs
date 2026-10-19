//! Funded, admin-allocated supplier incentives. Deposit accounting alone never
//! creates an entitlement: the admin must verify eligibility before allocating.

use crate::{
    borrow,
    deposit::{DepositCollateral, DepositDataKey},
    reentrancy::ReentrancyGuard,
};
use soroban_sdk::{contracterror, contractevent, contracttype, token, Address, Env};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SupplierRewardsError {
    NotConfigured = 1,
    Unauthorized = 2,
    InvalidAmount = 3,
    TokenImmutable = 4,
    Disabled = 5,
    NotSupplier = 6,
    InsufficientBudget = 7,
    Overflow = 8,
    Reentrancy = 9,
    InvalidAddress = 10,
    UnsupportedTransfer = 11,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierRewardsConfig {
    pub token: Address,
    /// Disables new allocations only; vested rewards remain claimable.
    pub enabled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SupplierReward {
    pub allocated: i128,
    pub claimed: i128,
}

#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SupplierRewardPool {
    pub available: i128,
    pub reserved: i128,
    pub paid: i128,
}

#[contracttype]
#[derive(Clone)]
enum RewardKey {
    SupplierRewardsConfig,
    SupplierRewardsPool,
    SupplierReward(Address),
}

#[contractevent]
#[derive(Clone, Debug)]
struct SupplierRewardsConfigured {
    #[topic]
    admin: Address,
    token: Address,
    enabled: bool,
}

#[contractevent]
#[derive(Clone, Debug)]
struct SupplierRewardsFunded {
    #[topic]
    funder: Address,
    amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
struct SupplierRewardAllocated {
    #[topic]
    user: Address,
    total: i128,
    added: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
struct SupplierRewardsClaimed {
    #[topic]
    user: Address,
    amount: i128,
}

fn require_admin(env: &Env, admin: &Address) -> Result<(), SupplierRewardsError> {
    if borrow::get_admin(env).as_ref() != Some(admin) {
        return Err(SupplierRewardsError::Unauthorized);
    }
    admin.require_auth();
    Ok(())
}

pub fn config(env: &Env) -> Option<SupplierRewardsConfig> {
    env.storage()
        .instance()
        .get(&RewardKey::SupplierRewardsConfig)
}

pub fn pool(env: &Env) -> SupplierRewardPool {
    env.storage()
        .instance()
        .get(&RewardKey::SupplierRewardsPool)
        .unwrap_or_default()
}

pub fn reward(env: &Env, user: &Address) -> SupplierReward {
    env.storage()
        .persistent()
        .get(&RewardKey::SupplierReward(user.clone()))
        .unwrap_or_default()
}

pub fn configure(
    env: &Env,
    admin: Address,
    token: Address,
    enabled: bool,
) -> Result<(), SupplierRewardsError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| SupplierRewardsError::Reentrancy)?;
    require_admin(env, &admin)?;
    if token == env.current_contract_address() {
        return Err(SupplierRewardsError::InvalidAddress);
    }
    if let Some(current) = config(env) {
        if current.token != token {
            return Err(SupplierRewardsError::TokenImmutable);
        }
    }
    env.storage().instance().set(
        &RewardKey::SupplierRewardsConfig,
        &SupplierRewardsConfig {
            token: token.clone(),
            enabled,
        },
    );
    SupplierRewardsConfigured {
        admin,
        token,
        enabled,
    }
    .publish(env);
    Ok(())
}

/// Only exact-transfer SEP-41 tokens are supported. Check both sides so reward
/// funding cannot silently accept fees and claims cannot silently underpay.
fn transfer_exact(
    env: &Env,
    asset: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), SupplierRewardsError> {
    let token = token::Client::new(env, asset);
    let from_before = token.balance(from);
    let to_before = token.balance(to);
    token.transfer(from, to, &amount);
    if from_before.checked_sub(token.balance(from)) != Some(amount)
        || token.balance(to).checked_sub(to_before) != Some(amount)
    {
        return Err(SupplierRewardsError::UnsupportedTransfer);
    }
    Ok(())
}

pub fn fund(env: &Env, funder: Address, amount: i128) -> Result<(), SupplierRewardsError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| SupplierRewardsError::Reentrancy)?;
    funder.require_auth();
    let config = config(env).ok_or(SupplierRewardsError::NotConfigured)?;
    if amount <= 0 {
        return Err(SupplierRewardsError::InvalidAmount);
    }
    let contract = env.current_contract_address();
    if funder == contract {
        return Err(SupplierRewardsError::InvalidAddress);
    }
    let mut pool = pool(env);
    pool.available = pool
        .available
        .checked_add(amount)
        .ok_or(SupplierRewardsError::Overflow)?;
    env.storage()
        .instance()
        .set(&RewardKey::SupplierRewardsPool, &pool);
    transfer_exact(env, &config.token, &funder, &contract, amount)?;
    SupplierRewardsFunded { funder, amount }.publish(env);
    Ok(())
}

/// Set a cumulative lifetime entitlement. Repeating the same total is a no-op,
/// even after claiming or withdrawing. Previously vested amounts cannot be cut.
pub fn allocate(
    env: &Env,
    admin: Address,
    user: Address,
    total: i128,
) -> Result<(), SupplierRewardsError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| SupplierRewardsError::Reentrancy)?;
    require_admin(env, &admin)?;
    let config = config(env).ok_or(SupplierRewardsError::NotConfigured)?;
    let mut reward = reward(env, &user);
    if total < 0 || total < reward.allocated {
        return Err(SupplierRewardsError::InvalidAmount);
    }
    let added = total
        .checked_sub(reward.allocated)
        .ok_or(SupplierRewardsError::Overflow)?;
    if added == 0 {
        return Ok(());
    }
    if !config.enabled {
        return Err(SupplierRewardsError::Disabled);
    }
    if user == env.current_contract_address() {
        return Err(SupplierRewardsError::InvalidAddress);
    }
    let position: Option<DepositCollateral> = env
        .storage()
        .persistent()
        .get(&DepositDataKey::UserCollateral(user.clone()));
    if !position.is_some_and(|p| p.amount > 0) {
        return Err(SupplierRewardsError::NotSupplier);
    }
    let mut pool = pool(env);
    if added > pool.available {
        return Err(SupplierRewardsError::InsufficientBudget);
    }
    pool.available -= added;
    pool.reserved = pool
        .reserved
        .checked_add(added)
        .ok_or(SupplierRewardsError::Overflow)?;
    reward.allocated = total;
    env.storage()
        .instance()
        .set(&RewardKey::SupplierRewardsPool, &pool);
    env.storage()
        .persistent()
        .set(&RewardKey::SupplierReward(user.clone()), &reward);
    SupplierRewardAllocated { user, total, added }.publish(env);
    Ok(())
}

pub fn claim(env: &Env, user: Address) -> Result<i128, SupplierRewardsError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| SupplierRewardsError::Reentrancy)?;
    user.require_auth();
    let config = config(env).ok_or(SupplierRewardsError::NotConfigured)?;
    let mut reward = reward(env, &user);
    let amount = reward
        .allocated
        .checked_sub(reward.claimed)
        .ok_or(SupplierRewardsError::Overflow)?;
    if amount == 0 {
        return Ok(0);
    }
    let mut pool = pool(env);
    pool.reserved = pool
        .reserved
        .checked_sub(amount)
        .ok_or(SupplierRewardsError::Overflow)?;
    pool.paid = pool
        .paid
        .checked_add(amount)
        .ok_or(SupplierRewardsError::Overflow)?;
    reward.claimed = reward.allocated;
    env.storage()
        .instance()
        .set(&RewardKey::SupplierRewardsPool, &pool);
    env.storage()
        .persistent()
        .set(&RewardKey::SupplierReward(user.clone()), &reward);
    transfer_exact(
        env,
        &config.token,
        &env.current_contract_address(),
        &user,
        amount,
    )?;
    SupplierRewardsClaimed { user, amount }.publish(env);
    Ok(amount)
}

stellarlend_errors::impl_contract_error! {
    SupplierRewardsError => stellarlend_errors::domains::LENDING_SUPPLIER_REWARDS;
    NotConfigured => InvalidInput, "Supplier rewards are not configured";
    Unauthorized => Unauthorized, "Stored admin authorization required";
    InvalidAmount => InvalidInput, "Amount must be positive and entitlements cannot decrease";
    TokenImmutable => InvalidAsset, "The reward token cannot be replaced";
    Disabled => Paused, "New supplier reward allocations are disabled";
    NotSupplier => InvalidInput, "Supplier must have a positive deposit balance";
    InsufficientBudget => LimitExceeded, "Allocation exceeds the funded reward budget";
    Overflow => Overflow, "Reward arithmetic overflow";
    Reentrancy => Reentrancy, "Reentrant reward operation blocked";
    InvalidAddress => InvalidInput, "The lending contract cannot be the token or recipient";
    UnsupportedTransfer => InvalidAsset, "Reward token must transfer the exact amount";
}
