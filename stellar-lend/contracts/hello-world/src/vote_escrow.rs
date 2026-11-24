//! # Vote-escrow boost for governance tokens
//!
//! Holders lock governance tokens for a chosen duration between one week and
//! four years. While a lock is active, the locked amount carries a voting boost
//! that is largest at the maximum lock and decays linearly to zero as the unlock
//! time approaches. Once the lock expires the holder withdraws the full amount.
//!
//! ## Boost curve
//!
//! ```text
//! remaining    = max(unlock_time - now, 0), capped at MAX_LOCK_SECONDS
//! boost_bps    = MAX_BOOST_BPS * remaining / MAX_LOCK_SECONDS
//! voting_power = amount + amount * boost_bps / 10_000
//! ```
//!
//! A full four-year lock doubles the weight of the locked amount. The weight
//! falls to 1x at expiry, so a stale lock never keeps an inflated boost.
//!
//! ## Scope
//!
//! This module owns the escrow ledger and the boost math. It is separate from
//! the step-function boost on delegated votes in `governance::power`. Wiring this
//! curve into proposal tallies or exposing it through contract entrypoints is a
//! separate decision and is intentionally not done here.
//!
//! ## Security
//!
//! - Every state change requires the owner's authorization.
//! - Amounts and durations are validated, and all arithmetic is checked.
//! - `initialize` is restricted to the current protocol admin and runs once.
//! - Tokens leave the escrow only through `withdraw`, and only after unlock.

#![allow(dead_code)]

use soroban_sdk::{contracterror, contracttype, token::TokenClient, Address, Env};

use crate::admin::get_admin;

/// Shortest lock a holder may create: one week.
pub const MIN_LOCK_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Longest lock a holder may create or extend to: four years.
pub const MAX_LOCK_SECONDS: u64 = 4 * 365 * 24 * 60 * 60;

/// Boost at a full maximum lock, in basis points (10_000 = +100%, i.e. 2x).
pub const MAX_BOOST_BPS: i128 = 10_000;

const BPS_DENOMINATOR: i128 = 10_000;

/// Errors returned by the vote-escrow. Codes start at 300 so they never collide
/// with the oracle or lending error enums.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VoteEscrowError {
    /// Caller is not the protocol admin.
    Unauthorized = 300,
    /// `initialize` has already run.
    AlreadyInitialized = 301,
    /// The escrow token has not been configured yet.
    NotInitialized = 302,
    /// Amount must be strictly positive.
    InvalidAmount = 303,
    /// Lock duration is outside `[MIN_LOCK_SECONDS, MAX_LOCK_SECONDS]`.
    InvalidLockDuration = 304,
    /// The owner already has a lock; increase it instead.
    LockAlreadyExists = 305,
    /// The owner has no lock.
    LockNotFound = 306,
    /// The lock has expired; withdraw it instead of changing it.
    LockExpired = 307,
    /// The lock has not expired yet, so it cannot be withdrawn.
    LockNotExpired = 308,
    /// A new unlock time must be strictly later than the current one.
    InvalidExtension = 309,
    /// An arithmetic operation overflowed.
    Overflow = 310,
}

/// Storage keys for the vote-escrow ledger.
#[contracttype]
#[derive(Clone)]
pub enum VoteEscrowKey {
    /// Governance token held by the escrow. Written once by `initialize`.
    Token,
    /// Active lock for one owner.
    Lock(Address),
}

/// One owner's active lock.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscrowLock {
    /// Tokens held in the escrow for this owner.
    pub amount: i128,
    /// Ledger timestamp at which the tokens can be withdrawn.
    pub unlock_time: u64,
}

/// Configure the governance token the escrow holds. Only the current protocol
/// admin may call this, and only once.
pub fn initialize(env: &Env, admin: &Address, token: Address) -> Result<(), VoteEscrowError> {
    admin.require_auth();
    if get_admin(env) != Some(admin.clone()) {
        return Err(VoteEscrowError::Unauthorized);
    }
    let key = VoteEscrowKey::Token;
    if env.storage().persistent().has(&key) {
        return Err(VoteEscrowError::AlreadyInitialized);
    }
    env.storage().persistent().set(&key, &token);
    Ok(())
}

/// Lock `amount` tokens for `lock_duration` seconds, starting now.
pub fn create_lock(
    env: &Env,
    owner: &Address,
    amount: i128,
    lock_duration: u64,
) -> Result<EscrowLock, VoteEscrowError> {
    owner.require_auth();
    require_positive(amount)?;
    if !(MIN_LOCK_SECONDS..=MAX_LOCK_SECONDS).contains(&lock_duration) {
        return Err(VoteEscrowError::InvalidLockDuration);
    }
    if get_lock(env, owner).is_some() {
        return Err(VoteEscrowError::LockAlreadyExists);
    }
    let token = escrow_token(env)?;
    let unlock_time = env
        .ledger()
        .timestamp()
        .checked_add(lock_duration)
        .ok_or(VoteEscrowError::Overflow)?;

    let lock = EscrowLock {
        amount,
        unlock_time,
    };
    save_lock(env, owner, &lock);
    pull(env, &token, owner, amount);
    Ok(lock)
}

/// Add `amount` tokens to an active lock. The unlock time does not change.
pub fn increase_amount(
    env: &Env,
    owner: &Address,
    amount: i128,
) -> Result<EscrowLock, VoteEscrowError> {
    owner.require_auth();
    require_positive(amount)?;
    let token = escrow_token(env)?;
    let mut lock = active_lock(env, owner)?;
    lock.amount = lock
        .amount
        .checked_add(amount)
        .ok_or(VoteEscrowError::Overflow)?;

    save_lock(env, owner, &lock);
    pull(env, &token, owner, amount);
    Ok(lock)
}

/// Move an active lock's unlock time later. The new unlock time must be later
/// than the current one and at most `MAX_LOCK_SECONDS` from now.
pub fn extend_lock(
    env: &Env,
    owner: &Address,
    new_unlock_time: u64,
) -> Result<EscrowLock, VoteEscrowError> {
    owner.require_auth();
    let mut lock = active_lock(env, owner)?;
    if new_unlock_time <= lock.unlock_time {
        return Err(VoteEscrowError::InvalidExtension);
    }
    // `new_unlock_time > lock.unlock_time >= now`, so the subtraction cannot wrap.
    let now = env.ledger().timestamp();
    if new_unlock_time.saturating_sub(now) > MAX_LOCK_SECONDS {
        return Err(VoteEscrowError::InvalidLockDuration);
    }

    lock.unlock_time = new_unlock_time;
    save_lock(env, owner, &lock);
    Ok(lock)
}

/// Return the full locked amount to `owner` once the lock has expired.
pub fn withdraw(env: &Env, owner: &Address) -> Result<i128, VoteEscrowError> {
    owner.require_auth();
    let lock = get_lock(env, owner).ok_or(VoteEscrowError::LockNotFound)?;
    if env.ledger().timestamp() < lock.unlock_time {
        return Err(VoteEscrowError::LockNotExpired);
    }
    let token = escrow_token(env)?;

    env.storage()
        .persistent()
        .remove(&VoteEscrowKey::Lock(owner.clone()));
    push(env, &token, owner, lock.amount);
    Ok(lock.amount)
}

/// The owner's active lock, if any. Expired locks remain readable until withdrawn.
pub fn get_lock(env: &Env, owner: &Address) -> Option<EscrowLock> {
    env.storage()
        .persistent()
        .get::<_, EscrowLock>(&VoteEscrowKey::Lock(owner.clone()))
}

/// Boost in basis points for `remaining_seconds` left on a lock. Linear in the
/// remaining time and capped at `MAX_BOOST_BPS`.
pub fn boost_bps(remaining_seconds: u64) -> i128 {
    let capped = remaining_seconds.min(MAX_LOCK_SECONDS) as i128;
    capped * MAX_BOOST_BPS / MAX_LOCK_SECONDS as i128
}

/// Voting weight of `amount` tokens under a boost of `boost_bps`.
pub fn weighted_power(amount: i128, boost_bps: i128) -> Result<i128, VoteEscrowError> {
    let boost = amount
        .checked_mul(boost_bps)
        .ok_or(VoteEscrowError::Overflow)?
        / BPS_DENOMINATOR;
    amount.checked_add(boost).ok_or(VoteEscrowError::Overflow)
}

/// Current voting weight of the owner's lock. Zero when the owner has no lock.
pub fn voting_power(env: &Env, owner: &Address) -> Result<i128, VoteEscrowError> {
    let Some(lock) = get_lock(env, owner) else {
        return Ok(0);
    };
    let remaining = lock.unlock_time.saturating_sub(env.ledger().timestamp());
    weighted_power(lock.amount, boost_bps(remaining))
}

fn escrow_token(env: &Env) -> Result<Address, VoteEscrowError> {
    env.storage()
        .persistent()
        .get::<_, Address>(&VoteEscrowKey::Token)
        .ok_or(VoteEscrowError::NotInitialized)
}

fn active_lock(env: &Env, owner: &Address) -> Result<EscrowLock, VoteEscrowError> {
    let lock = get_lock(env, owner).ok_or(VoteEscrowError::LockNotFound)?;
    if env.ledger().timestamp() >= lock.unlock_time {
        return Err(VoteEscrowError::LockExpired);
    }
    Ok(lock)
}

fn save_lock(env: &Env, owner: &Address, lock: &EscrowLock) {
    env.storage()
        .persistent()
        .set(&VoteEscrowKey::Lock(owner.clone()), lock);
}

fn require_positive(amount: i128) -> Result<(), VoteEscrowError> {
    if amount <= 0 {
        return Err(VoteEscrowError::InvalidAmount);
    }
    Ok(())
}

fn pull(env: &Env, token: &Address, from: &Address, amount: i128) {
    TokenClient::new(env, token).transfer(from, &env.current_contract_address(), &amount);
}

fn push(env: &Env, token: &Address, to: &Address, amount: i128) {
    TokenClient::new(env, token).transfer(&env.current_contract_address(), to, &amount);
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger as _},
        token::StellarAssetClient,
    };

    #[contract]
    struct VoteEscrowTestContract;

    #[contractimpl]
    impl VoteEscrowTestContract {
        pub fn marker() {}
    }

    const START: u64 = 1_000;

    struct Fixture {
        env: Env,
        contract: Address,
        token: Address,
        owner: Address,
    }

    fn fixture() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|ledger| ledger.timestamp = START);
        let contract = env.register(VoteEscrowTestContract, ());
        let token_admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let owner = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&owner, &1_000_000);
        env.as_contract(&contract, || {
            env.storage()
                .persistent()
                .set(&VoteEscrowKey::Token, &token);
        });
        Fixture {
            env,
            contract,
            token,
            owner,
        }
    }

    #[test]
    fn boost_curve_is_linear_capped_and_zero_at_expiry() {
        assert_eq!(boost_bps(0), 0);
        assert_eq!(boost_bps(MAX_LOCK_SECONDS / 2), MAX_BOOST_BPS / 2);
        assert_eq!(boost_bps(MAX_LOCK_SECONDS), MAX_BOOST_BPS);
        assert_eq!(boost_bps(MAX_LOCK_SECONDS * 3), MAX_BOOST_BPS);
    }

    #[test]
    fn full_lock_doubles_weight_and_decays_to_base_at_expiry() {
        let f = fixture();
        f.env.as_contract(&f.contract, || {
            create_lock(&f.env, &f.owner, 1_000, MAX_LOCK_SECONDS).unwrap();
            assert_eq!(voting_power(&f.env, &f.owner), Ok(2_000));

            f.env
                .ledger()
                .with_mut(|ledger| ledger.timestamp = START + MAX_LOCK_SECONDS / 2);
            assert_eq!(voting_power(&f.env, &f.owner), Ok(1_500));

            f.env
                .ledger()
                .with_mut(|ledger| ledger.timestamp = START + MAX_LOCK_SECONDS);
            assert_eq!(voting_power(&f.env, &f.owner), Ok(1_000));
        });
    }

    #[test]
    fn create_lock_moves_tokens_and_rejects_invalid_input() {
        let f = fixture();
        f.env.as_contract(&f.contract, || {
            assert_eq!(
                create_lock(&f.env, &f.owner, 0, MAX_LOCK_SECONDS),
                Err(VoteEscrowError::InvalidAmount)
            );
            assert_eq!(
                create_lock(&f.env, &f.owner, 10, MIN_LOCK_SECONDS - 1),
                Err(VoteEscrowError::InvalidLockDuration)
            );
            assert_eq!(
                create_lock(&f.env, &f.owner, 10, MAX_LOCK_SECONDS + 1),
                Err(VoteEscrowError::InvalidLockDuration)
            );

            create_lock(&f.env, &f.owner, 400, MIN_LOCK_SECONDS).unwrap();
            assert_eq!(
                create_lock(&f.env, &f.owner, 10, MIN_LOCK_SECONDS),
                Err(VoteEscrowError::LockAlreadyExists)
            );
        });
        let balances = TokenClient::new(&f.env, &f.token);
        assert_eq!(balances.balance(&f.owner), 1_000_000 - 400);
        assert_eq!(balances.balance(&f.contract), 400);
    }

    #[test]
    fn withdraw_waits_for_unlock_then_returns_tokens() {
        let f = fixture();
        f.env.as_contract(&f.contract, || {
            create_lock(&f.env, &f.owner, 250, MIN_LOCK_SECONDS).unwrap();
            assert_eq!(
                withdraw(&f.env, &f.owner),
                Err(VoteEscrowError::LockNotExpired)
            );

            f.env
                .ledger()
                .with_mut(|ledger| ledger.timestamp = START + MIN_LOCK_SECONDS);
            assert_eq!(withdraw(&f.env, &f.owner), Ok(250));
            assert!(get_lock(&f.env, &f.owner).is_none());
            assert_eq!(
                withdraw(&f.env, &f.owner),
                Err(VoteEscrowError::LockNotFound)
            );
        });
        assert_eq!(
            TokenClient::new(&f.env, &f.token).balance(&f.owner),
            1_000_000
        );
    }

    #[test]
    fn extend_and_increase_stay_within_bounds() {
        let f = fixture();
        f.env.as_contract(&f.contract, || {
            create_lock(&f.env, &f.owner, 100, MIN_LOCK_SECONDS).unwrap();

            assert_eq!(
                extend_lock(&f.env, &f.owner, START + MIN_LOCK_SECONDS / 2),
                Err(VoteEscrowError::InvalidExtension)
            );
            assert_eq!(
                extend_lock(&f.env, &f.owner, START + MAX_LOCK_SECONDS + 1),
                Err(VoteEscrowError::InvalidLockDuration)
            );

            let extended = extend_lock(&f.env, &f.owner, START + MAX_LOCK_SECONDS).unwrap();
            assert_eq!(extended.unlock_time, START + MAX_LOCK_SECONDS);

            let increased = increase_amount(&f.env, &f.owner, 50).unwrap();
            assert_eq!(increased.amount, 150);
            assert_eq!(
                increase_amount(&f.env, &f.owner, 0),
                Err(VoteEscrowError::InvalidAmount)
            );
        });
    }

    #[test]
    fn expired_lock_cannot_be_extended_or_increased() {
        let f = fixture();
        f.env.as_contract(&f.contract, || {
            create_lock(&f.env, &f.owner, 100, MIN_LOCK_SECONDS).unwrap();
            f.env
                .ledger()
                .with_mut(|ledger| ledger.timestamp = START + MIN_LOCK_SECONDS);
            assert_eq!(
                increase_amount(&f.env, &f.owner, 10),
                Err(VoteEscrowError::LockExpired)
            );
            assert_eq!(
                extend_lock(&f.env, &f.owner, START + MAX_LOCK_SECONDS),
                Err(VoteEscrowError::LockExpired)
            );
        });
    }

    #[test]
    fn initialize_is_admin_only_and_runs_once() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register(VoteEscrowTestContract, ());
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        env.as_contract(&contract, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
            let outsider = Address::generate(&env);
            assert_eq!(
                initialize(&env, &outsider, token.clone()),
                Err(VoteEscrowError::Unauthorized)
            );
            initialize(&env, &admin, token.clone()).unwrap();
            assert_eq!(
                initialize(&env, &admin, token.clone()),
                Err(VoteEscrowError::AlreadyInitialized)
            );
        });
    }
}
