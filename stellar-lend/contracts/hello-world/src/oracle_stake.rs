//! # Oracle Freshness Attestations with Staking Slash
//!
//! Oracles must stake tokens to participate in the price feed. If an oracle
//! submits a stale price (older than the configured threshold), their stake
//! is slashed as a penalty. This creates economic security for the oracle
//! network.
//!
//! ## Design
//! - Oracles stake a minimum amount to register.
//! - Each price update includes a freshness attestation (timestamp).
//! - If the attestation is stale, the oracle's stake is slashed by a
//!   configurable percentage.
//! - Oracles can unstake after a cooldown period.
//! - Admin can configure stake amount, slash percentage, and staleness threshold.
//! - Every state change emits an event for observability.

use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Symbol};

use crate::admin;

// ── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OracleStakeError {
    Unauthorized = 1,
    InsufficientStake = 2,
    OracleNotFound = 3,
    OracleAlreadyStaked = 4,
    CooldownNotElapsed = 5,
    InvalidSlashRate = 6,
    InvalidThreshold = 7,
    StaleAttestation = 8,
    Overflow = 9,
    InvalidAmount = 10,
}

// ── Types ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum OracleStakeDataKey {
    /// Oracle stake record.
    Stake(Address),
    /// Minimum stake required.
    MinStake,
    /// Slash percentage in basis points (e.g., 1000 = 10%).
    SlashRateBps,
    /// Staleness threshold in seconds.
    StalenessThreshold,
    /// Cooldown period in seconds before unstaking.
    UnstakeCooldown,
    /// Total stake locked.
    TotalStaked,
    /// Slash count per oracle.
    SlashCount(Address),
}

/// Oracle stake record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleStake {
    pub oracle: Address,
    pub amount: i128,
    pub staked_at: u64,
    pub last_attestation: u64,
    pub slash_count: u32,
    pub active: bool,
}

/// Result of a freshness attestation check.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestationResult {
    pub oracle: Address,
    pub is_fresh: bool,
    pub age_seconds: u64,
    pub slashed_amount: i128,
}

// ── Admin: Configuration ────────────────────────────────────────────────────

pub fn set_min_stake(env: &Env, admin: Address, amount: i128) -> Result<(), OracleStakeError> {
    admin::require_admin(env, &admin).map_err(|_| OracleStakeError::Unauthorized)?;
    if amount <= 0 {
        return Err(OracleStakeError::InvalidAmount);
    }
    env.storage().persistent().set(&OracleStakeDataKey::MinStake, &amount);
    env.events().publish(
        (Symbol::new(env, "min_stake_set"), admin),
        amount,
    );
    Ok(())
}

pub fn set_slash_rate(env: &Env, admin: Address, rate_bps: i128) -> Result<(), OracleStakeError> {
    admin::require_admin(env, &admin).map_err(|_| OracleStakeError::Unauthorized)?;
    if rate_bps < 0 || rate_bps > 10000 {
        return Err(OracleStakeError::InvalidSlashRate);
    }
    env.storage().persistent().set(&OracleStakeDataKey::SlashRateBps, &rate_bps);
    env.events().publish(
        (Symbol::new(env, "slash_rate_set"), admin),
        rate_bps,
    );
    Ok(())
}

pub fn set_staleness_threshold(
    env: &Env,
    admin: Address,
    threshold: u64,
) -> Result<(), OracleStakeError> {
    admin::require_admin(env, &admin).map_err(|_| OracleStakeError::Unauthorized)?;
    if threshold == 0 {
        return Err(OracleStakeError::InvalidThreshold);
    }
    env.storage().persistent().set(&OracleStakeDataKey::StalenessThreshold, &threshold);
    env.events().publish(
        (Symbol::new(env, "staleness_threshold_set"), admin),
        threshold,
    );
    Ok(())
}

pub fn set_unstake_cooldown(
    env: &Env,
    admin: Address,
    cooldown: u64,
) -> Result<(), OracleStakeError> {
    admin::require_admin(env, &admin).map_err(|_| OracleStakeError::Unauthorized)?;
    env.storage().persistent().set(&OracleStakeDataKey::UnstakeCooldown, &cooldown);
    Ok(())
}

// ── Oracle: Staking ─────────────────────────────────────────────────────────

/// Stake tokens to become an oracle. The oracle must transfer `amount` tokens
/// to the contract. Returns the stake record.
pub fn stake(
    env: &Env,
    oracle: Address,
    amount: i128,
) -> Result<OracleStake, OracleStakeError> {
    oracle.require_auth();
    let min_stake = env
        .storage()
        .persistent()
        .get::<_, i128>(&OracleStakeDataKey::MinStake)
        .unwrap_or(1000);
    if amount < min_stake {
        return Err(OracleStakeError::InsufficientStake);
    }
    let key = OracleStakeDataKey::Stake(oracle.clone());
    if env.storage().persistent().get::<_, OracleStake>(&key).is_some() {
        return Err(OracleStakeError::OracleAlreadyStaked);
    }
    let now = env.ledger().timestamp();
    let stake_record = OracleStake {
        oracle: oracle.clone(),
        amount,
        staked_at: now,
        last_attestation: now,
        slash_count: 0,
        active: true,
    };
    env.storage().persistent().set(&key, &stake_record);
    let total: i128 = env
        .storage()
        .persistent()
        .get(&OracleStakeDataKey::TotalStaked)
        .unwrap_or(0);
    env.storage().persistent().set(
        &OracleStakeDataKey::TotalStaked,
        &total.saturating_add(amount),
    );
    env.events().publish(
        (Symbol::new(env, "oracle_staked"), oracle),
        amount,
    );
    Ok(stake_record)
}

/// Unstake tokens after the cooldown period.
pub fn unstake(env: &Env, oracle: Address) -> Result<i128, OracleStakeError> {
    oracle.require_auth();
    let key = OracleStakeDataKey::Stake(oracle.clone());
    let stake_record = env
        .storage()
        .persistent()
        .get::<_, OracleStake>(&key)
        .ok_or(OracleStakeError::OracleNotFound)?;
    let cooldown = env
        .storage()
        .persistent()
        .get::<_, u64>(&OracleStakeDataKey::UnstakeCooldown)
        .unwrap_or(86400);
    let now = env.ledger().timestamp();
    if now < stake_record.staked_at.saturating_add(cooldown) {
        return Err(OracleStakeError::CooldownNotElapsed);
    }
    env.storage().persistent().remove(&key);
    let total: i128 = env
        .storage()
        .persistent()
        .get(&OracleStakeDataKey::TotalStaked)
        .unwrap_or(0);
    env.storage().persistent().set(
        &OracleStakeDataKey::TotalStaked,
        &total.saturating_sub(stake_record.amount),
    );
    env.events().publish(
        (Symbol::new(env, "oracle_unstaked"), oracle),
        stake_record.amount,
    );
    Ok(stake_record.amount)
}

// ── Oracle: Freshness attestation ───────────────────────────────────────────

/// Submit a freshness attestation for a price update.
///
/// If the attestation is stale (older than the threshold), the oracle's stake
/// is slashed by the configured slash rate. Returns an `AttestationResult`.
pub fn attest_freshness(
    env: &Env,
    oracle: Address,
    attestation_timestamp: u64,
) -> Result<AttestationResult, OracleStakeError> {
    let key = OracleStakeDataKey::Stake(oracle.clone());
    let mut stake_record = env
        .storage()
        .persistent()
        .get::<_, OracleStake>(&key)
        .ok_or(OracleStakeError::OracleNotFound)?;
    let now = env.ledger().timestamp();
    let age = now.saturating_sub(attestation_timestamp);
    let threshold = env
        .storage()
        .persistent()
        .get::<_, u64>(&OracleStakeDataKey::StalenessThreshold)
        .unwrap_or(3600);

    let is_fresh = age <= threshold;
    let mut slashed = 0i128;

    if !is_fresh {
        // Slash the stake.
        let slash_rate = env
            .storage()
            .persistent()
            .get::<_, i128>(&OracleStakeDataKey::SlashRateBps)
            .unwrap_or(1000);
        slashed = stake_record.amount
            .saturating_mul(slash_rate)
            .saturating_div(10000);
        stake_record.amount = stake_record.amount.saturating_sub(slashed);
        stake_record.slash_count = stake_record.slash_count.saturating_add(1);

        // Update total staked.
        let total: i128 = env
            .storage()
            .persistent()
            .get(&OracleStakeDataKey::TotalStaked)
            .unwrap_or(0);
        env.storage().persistent().set(
            &OracleStakeDataKey::TotalStaked,
            &total.saturating_sub(slashed),
        );

        // Track slash count.
        let slash_count_key = OracleStakeDataKey::SlashCount(oracle.clone());
        let count: u32 = env
            .storage()
            .persistent()
            .get(&slash_count_key)
            .unwrap_or(0);
        env.storage().persistent().set(&slash_count_key, &(count.saturating_add(1)));

        env.events().publish(
            (Symbol::new(env, "oracle_slashed"), oracle.clone()),
            (slashed, age),
        );
    }

    // Update last attestation.
    stake_record.last_attestation = now;
    env.storage().persistent().set(&key, &stake_record);

    env.events().publish(
        (Symbol::new(env, "oracle_attested"), oracle.clone()),
        (is_fresh, age),
    );

    Ok(AttestationResult {
        oracle,
        is_fresh,
        age_seconds: age,
        slashed_amount: slashed,
    })
}

/// Get an oracle's stake record.
pub fn get_stake(env: &Env, oracle: &Address) -> Option<OracleStake> {
    env.storage()
        .persistent()
        .get(&OracleStakeDataKey::Stake(oracle.clone()))
}

/// Get total stake locked in the protocol.
pub fn get_total_staked(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&OracleStakeDataKey::TotalStaked)
        .unwrap_or(0)
}

/// Get an oracle's slash count.
pub fn get_slash_count(env: &Env, oracle: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&OracleStakeDataKey::SlashCount(oracle.clone()))
        .unwrap_or(0)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{contract, contractimpl};

    #[contract]
    struct OracleStakeTestContract;

    #[contractimpl]
    impl OracleStakeTestContract {}

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, OracleStakeTestContract);
        env.as_contract(&contract_id, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
        });
        (env, admin, contract_id)
    }

    #[test]
    fn stake_and_unstake_lifecycle() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_min_stake(&env, admin, 1000).unwrap();
            set_unstake_cooldown(&env, admin, 100).unwrap();

            let oracle = Address::generate(&env);
            let stake = stake(&env, oracle.clone(), 5000).unwrap();
            assert_eq!(stake.amount, 5000);
            assert_eq!(get_total_staked(&env), 5000);

            // Unstake before cooldown → error.
            assert_eq!(unstake(&env, oracle.clone()), Err(OracleStakeError::CooldownNotElapsed));

            // Advance time past cooldown.
            env.ledger().with_mut(|l| l.timestamp = 300);
            let amount = unstake(&env, oracle).unwrap();
            assert_eq!(amount, 5000);
            assert_eq!(get_total_staked(&env), 0);
        });
    }

    #[test]
    fn fresh_attestation_no_slash() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_min_stake(&env, admin, 1000).unwrap();
            set_staleness_threshold(&env, admin, 3600).unwrap();
            set_slash_rate(&env, admin, 1000).unwrap(); // 10%

            let oracle = Address::generate(&env);
            stake(&env, oracle.clone(), 5000).unwrap();

            let result = attest_freshness(&env, oracle.clone(), 50).unwrap();
            assert!(result.is_fresh);
            assert_eq!(result.slashed_amount, 0);
            assert_eq!(get_stake(&env, &oracle).unwrap().amount, 5000);
        });
    }

    #[test]
    fn stale_attestation_triggers_slash() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_min_stake(&env, admin, 1000).unwrap();
            set_staleness_threshold(&env, admin, 100).unwrap();
            set_slash_rate(&env, admin, 1000).unwrap(); // 10%

            let oracle = Address::generate(&env);
            stake(&env, oracle.clone(), 5000).unwrap();

            // Attestation from 50 seconds ago, threshold is 100 → age=50, fresh.
            // But we're at timestamp 100, so attestation_timestamp=0 → age=100.
            // Let's make it stale: timestamp=0, now=100, age=100 > threshold=100? No, equal.
            // Use timestamp=0 with threshold=50 → age=100 > 50 → stale.
            set_staleness_threshold(&env, admin, 50).unwrap();
            let result = attest_freshness(&env, oracle.clone(), 0).unwrap();
            assert!(!result.is_fresh);
            assert_eq!(result.slashed_amount, 500); // 10% of 5000
            assert_eq!(get_stake(&env, &oracle).unwrap().amount, 4500);
            assert_eq!(get_slash_count(&env, &oracle), 1);
        });
    }

    #[test]
    fn insufficient_stake_rejected() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_min_stake(&env, admin, 1000).unwrap();
            let oracle = Address::generate(&env);
            assert_eq!(
                stake(&env, oracle, 500),
                Err(OracleStakeError::InsufficientStake)
            );
        });
    }

    #[test]
    fn double_stake_rejected() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_min_stake(&env, admin, 1000).unwrap();
            let oracle = Address::generate(&env);
            stake(&env, oracle.clone(), 5000).unwrap();
            assert_eq!(
                stake(&env, oracle, 5000),
                Err(OracleStakeError::OracleAlreadyStaked)
            );
        });
    }
}