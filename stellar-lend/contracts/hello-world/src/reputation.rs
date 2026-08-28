use soroban_sdk::{contracterror, contracttype, Address, Env, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ReputationError {
    Unauthorized = 1,
    NotFound = 2,
    AccessDenied = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReputationTier {
    Bronze = 0,
    Silver = 1,
    Gold = 2,
    Platinum = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParticipantReputation {
    pub address: Address,
    pub score: u32,
    pub tier: ReputationTier,
    pub successful_ops: u32,
    pub defaults: u32,
    pub last_activity: u64,
}

#[contracttype]
#[derive(Clone)]
enum ReputationKey {
    Deployer(Address),
    User(Address),
    DecayInterval,
}

const DEFAULT_DECAY_INTERVAL: u64 = 2_592_000; // 30 days

fn tier_from_score(score: u32) -> ReputationTier {
    if score >= 750 {
        ReputationTier::Platinum
    } else if score >= 500 {
        ReputationTier::Gold
    } else if score >= 250 {
        ReputationTier::Silver
    } else {
        ReputationTier::Bronze
    }
}

fn fee_discount_bps(tier: &ReputationTier) -> u32 {
    match tier {
        ReputationTier::Bronze => 0,
        ReputationTier::Silver => 25,
        ReputationTier::Gold => 50,
        ReputationTier::Platinum => 100,
    }
}

pub fn record_deployer_success(env: &Env, deployer: Address) -> Result<ParticipantReputation, ReputationError> {
    deployer.require_auth();
    let mut rep = get_deployer_reputation(env, &deployer).unwrap_or(empty_reputation(&deployer));
    rep.successful_ops = rep.successful_ops.saturating_add(1);
    rep.score = (rep.score.saturating_add(10)).min(1000);
    rep.tier = tier_from_score(rep.score);
    rep.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&ReputationKey::Deployer(deployer.clone()), &rep);
    Ok(rep)
}

pub fn record_user_repayment(env: &Env, user: Address, on_time: bool) -> Result<ParticipantReputation, ReputationError> {
    user.require_auth();
    let mut rep = get_user_reputation(env, &user).unwrap_or(empty_reputation(&user));
    rep.successful_ops = rep.successful_ops.saturating_add(1);
    rep.score = rep.score.saturating_add(if on_time { 15 } else { 5 }).min(1000);
    rep.tier = tier_from_score(rep.score);
    rep.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&ReputationKey::User(user.clone()), &rep);
    Ok(rep)
}

pub fn record_user_default(env: &Env, admin: Address, user: Address) -> Result<ParticipantReputation, ReputationError> {
    crate::admin::require_admin(env, &admin).map_err(|_| ReputationError::Unauthorized)?;
    let mut rep = get_user_reputation(env, &user).unwrap_or(empty_reputation(&user));
    rep.defaults = rep.defaults.saturating_add(1);
    rep.score = rep.score.saturating_sub(100);
    rep.tier = tier_from_score(rep.score);
    rep.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&ReputationKey::User(user.clone()), &rep);
    Ok(rep)
}

pub fn apply_decay(env: &Env, address: Address, is_deployer: bool) -> Result<ParticipantReputation, ReputationError> {
    let key = if is_deployer {
        ReputationKey::Deployer(address.clone())
    } else {
        ReputationKey::User(address.clone())
    };
    let mut rep = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ReputationError::NotFound)?;
    let interval = env
        .storage()
        .persistent()
        .get(&ReputationKey::DecayInterval)
        .unwrap_or(DEFAULT_DECAY_INTERVAL);
    let now = env.ledger().timestamp();
    if now.saturating_sub(rep.last_activity) >= interval && rep.score > 0 {
        rep.score = rep.score.saturating_sub(10);
        rep.tier = tier_from_score(rep.score);
        rep.last_activity = now;
        env.storage().persistent().set(&key, &rep);
    }
    Ok(rep)
}

pub fn check_access(env: &Env, address: &Address, min_tier: ReputationTier) -> Result<bool, ReputationError> {
    let rep = get_user_reputation(env, address).unwrap_or(empty_reputation(address));
    let allowed = (rep.tier as u32) >= (min_tier as u32);
    if allowed {
        Ok(true)
    } else {
        Err(ReputationError::AccessDenied)
    }
}

pub fn get_deployer_reputation(env: &Env, address: &Address) -> Option<ParticipantReputation> {
    env.storage()
        .persistent()
        .get(&ReputationKey::Deployer(address.clone()))
}

pub fn get_user_reputation(env: &Env, address: &Address) -> Option<ParticipantReputation> {
    env.storage()
        .persistent()
        .get(&ReputationKey::User(address.clone()))
}

pub fn get_fee_discount_bps(env: &Env, address: &Address) -> u32 {
    get_user_reputation(env, address)
        .map(|r| fee_discount_bps(&r.tier))
        .unwrap_or(0)
}

fn empty_reputation(address: &Address) -> ParticipantReputation {
    ParticipantReputation {
        address: address.clone(),
        score: 0,
        tier: ReputationTier::Bronze,
        successful_ops: 0,
        defaults: 0,
        last_activity: 0,
    }
}
