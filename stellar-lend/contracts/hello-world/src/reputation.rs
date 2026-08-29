use soroban_sdk::{contracterror, contracttype, Address, Env, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ReputationError {
    Unauthorized = 1,
    NotFound = 2,
    AccessDenied = 3,
    InvalidParameter = 4,
    AlreadyExists = 5,
    RateLimitExceeded = 6,
    InsufficientReputation = 7,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeployerPoolRecord {
    pub pool_address: Address,
    pub created_at: u64,
    pub tvl: i128,
    pub active_borrowers: u32,
    pub liquidation_events: u32,
    pub performance_score: u32,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeployerReputation {
    pub address: Address,
    pub score: u32,
    pub tier: ReputationTier,
    pub total_pools_created: u32,
    pub active_pools: u32,
    pub total_tvl: i128,
    pub successful_ops: u32,
    pub defaults: u32,
    pub abandoned_pools: u32,
    pub avg_pool_uptime_bps: u32,
    pub last_activity: u64,
    pub pools: Vec<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserReputation {
    pub address: Address,
    pub score: u32,
    pub tier: ReputationTier,
    pub total_repayments: u32,
    pub on_time_repayments: u32,
    pub defaults: u32,
    pub total_borrowed: i128,
    pub last_activity: u64,
    pub fee_discount_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolDeploymentConfig {
    pub min_deployer_score: u32,
    pub max_pools_per_deployer: u32,
    pub deploy_cooldown_seconds: u64,
    pub min_initial_deposit: i128,
}

#[contracttype]
#[derive(Clone)]
enum ReputationKey {
    Deployer(Address),
    User(Address),
    DecayInterval,
    DeployerLastDeploy(Address),
    PoolRecord(Address),
    PoolToDeployer(Address),
    Config,
}

const DEFAULT_DECAY_INTERVAL: u64 = 2_592_000;
const DEFAULT_MIN_DEPLOYER_SCORE: u32 = 100;
const DEFAULT_MAX_POOLS_PER_DEPLOYER: u32 = 10;
const DEFAULT_DEPLOY_COOLDOWN: u64 = 3600;
const DEFAULT_MIN_INITIAL_DEPOSIT: i128 = 1_000_000;

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

fn borrow_limit_multiplier_bps(tier: &ReputationTier) -> u32 {
    match tier {
        ReputationTier::Bronze => 10_000,
        ReputationTier::Silver => 11_000,
        ReputationTier::Gold => 12_500,
        ReputationTier::Platinum => 15_000,
    }
}

fn compute_user_score(
    total_repayments: u32,
    on_time_repayments: u32,
    defaults: u32,
) -> u32 {
    if total_repayments == 0 {
        return 0;
    }

    let on_time_component: u64 = if total_repayments > 0 {
        (on_time_repayments as u64) * 1000 / (total_repayments as u64)
    } else {
        0
    };

    let capped_total: u64 = if total_repayments > 100 { 100 } else { total_repayments as u64 };
    let count_component: u64 = capped_total * 1000 / 100;

    let default_penalty: u64 = (defaults as u64) * 200;
    let no_default_component: u64 = if default_penalty >= 1000 {
        0
    } else {
        1000 - default_penalty
    };

    let weighted: u64 = on_time_component * 40 + count_component * 30 + no_default_component * 30;
    let score = weighted / 100;

    if score > 1000 { 1000 } else { score as u32 }
}

fn compute_deployer_score(
    total_pools: u32,
    active_pools: u32,
    defaults: u32,
    abandoned: u32,
    avg_uptime_bps: u32,
    total_tvl: i128,
) -> u32 {
    let pool_count_component: u64 = if total_pools >= 10 {
        1000
    } else {
        (total_pools as u64) * 100
    };

    let active_ratio_component: u64 = if total_pools > 0 {
        (active_pools as u64) * 1000 / (total_pools as u64)
    } else {
        0
    };

    let uptime_component: u64 = if avg_uptime_bps > 10_000 {
        1000
    } else {
        (avg_uptime_bps as u64) * 1000 / 10_000
    };

    let default_penalty: u64 = ((defaults + abandoned) as u64) * 150;
    let reliability_component: u64 = if default_penalty >= 1000 {
        0
    } else {
        1000 - default_penalty
    };

    let tvl_component: u64 = if total_tvl >= 1_000_000_000 {
        1000
    } else if total_tvl > 0 {
        ((total_tvl as u64) * 1000) / 1_000_000_000
    } else {
        0
    };

    let weighted: u64 = pool_count_component * 20
        + active_ratio_component * 20
        + uptime_component * 25
        + reliability_component * 25
        + tvl_component * 10;

    let score = weighted / 100;
    if score > 1000 { 1000 } else { score as u32 }
}

fn default_config() -> PoolDeploymentConfig {
    PoolDeploymentConfig {
        min_deployer_score: DEFAULT_MIN_DEPLOYER_SCORE,
        max_pools_per_deployer: DEFAULT_MAX_POOLS_PER_DEPLOYER,
        deploy_cooldown_seconds: DEFAULT_DEPLOY_COOLDOWN,
        min_initial_deposit: DEFAULT_MIN_INITIAL_DEPOSIT,
    }
}

pub fn initialize(env: &Env, admin: &Address) -> Result<(), ReputationError> {
    crate::admin::require_admin(env, admin).map_err(|_| ReputationError::Unauthorized)?;

    if env
        .storage()
        .instance()
        .has(&ReputationKey::Config)
    {
        return Err(ReputationError::AlreadyExists);
    }

    let config = default_config();
    env.storage().instance().set(&ReputationKey::Config, &config);
    Ok(())
}

pub fn set_deployment_config(
    env: &Env,
    admin: &Address,
    config: PoolDeploymentConfig,
) -> Result<(), ReputationError> {
    crate::admin::require_admin(env, admin).map_err(|_| ReputationError::Unauthorized)?;

    if config.max_pools_per_deployer == 0 || config.min_deployer_score > 1000 {
        return Err(ReputationError::InvalidParameter);
    }

    env.storage().instance().set(&ReputationKey::Config, &config);
    Ok(())
}

pub fn get_deployment_config(env: &Env) -> PoolDeploymentConfig {
    env.storage()
        .instance()
        .get(&ReputationKey::Config)
        .unwrap_or_else(default_config)
}

pub fn record_deployer_success(env: &Env, deployer: Address) -> Result<ParticipantReputation, ReputationError> {
    deployer.require_auth();
    let mut rep = get_deployer_reputation_internal(env, &deployer).unwrap_or_else(|_| empty_deployer_participant(&deployer));
    rep.successful_ops = rep.successful_ops.saturating_add(1);
    rep.score = (rep.score.saturating_add(10)).min(1000);
    rep.tier = tier_from_score(rep.score);
    rep.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&ReputationKey::Deployer(deployer.clone()), &rep);
    Ok(rep)
}

pub fn record_user_repayment(env: &Env, user: Address, on_time: bool) -> Result<UserReputation, ReputationError> {
    user.require_auth();
    let mut rep = get_user_reputation_internal(env, &user).unwrap_or_else(|_| empty_user_reputation(&user));
    rep.total_repayments = rep.total_repayments.saturating_add(1);
    if on_time {
        rep.on_time_repayments = rep.on_time_repayments.saturating_add(1);
    }
    rep.score = compute_user_score(rep.total_repayments, rep.on_time_repayments, rep.defaults);
    rep.tier = tier_from_score(rep.score);
    rep.fee_discount_bps = fee_discount_bps(&rep.tier);
    rep.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&ReputationKey::User(user.clone()), &rep);
    Ok(rep)
}

pub fn record_user_borrow(
    env: &Env,
    user: Address,
    amount: i128,
) -> Result<UserReputation, ReputationError> {
    if amount <= 0 {
        return Err(ReputationError::InvalidParameter);
    }
    let mut rep = get_user_reputation_internal(env, &user).unwrap_or_else(|_| empty_user_reputation(&user));
    rep.total_borrowed = rep.total_borrowed.saturating_add(amount);
    rep.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&ReputationKey::User(user.clone()), &rep);
    Ok(rep)
}

pub fn record_user_default(
    env: &Env,
    admin: Address,
    user: Address,
) -> Result<UserReputation, ReputationError> {
    crate::admin::require_admin(env, &admin).map_err(|_| ReputationError::Unauthorized)?;
    let mut rep = get_user_reputation_internal(env, &user).unwrap_or_else(|_| empty_user_reputation(&user));
    rep.defaults = rep.defaults.saturating_add(1);
    rep.score = compute_user_score(rep.total_repayments, rep.on_time_repayments, rep.defaults);
    rep.tier = tier_from_score(rep.score);
    rep.fee_discount_bps = fee_discount_bps(&rep.tier);
    rep.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&ReputationKey::User(user.clone()), &rep);
    Ok(rep)
}

pub fn record_pool_deployment(
    env: &Env,
    deployer: Address,
    pool_address: Address,
    initial_deposit: i128,
) -> Result<DeployerReputation, ReputationError> {
    deployer.require_auth();

    let config = get_deployment_config(env);

    if initial_deposit < config.min_initial_deposit {
        return Err(ReputationError::InsufficientReputation);
    }

    let cooldown_key = ReputationKey::DeployerLastDeploy(deployer.clone());
    if let Some(last_deploy) = env.storage().persistent().get::<_, u64>(&cooldown_key) {
        let now = env.ledger().timestamp();
        if now.saturating_sub(last_deploy) < config.deploy_cooldown_seconds {
            return Err(ReputationError::RateLimitExceeded);
        }
    }

    let mut deployer_rep = get_deployer_reputation_full(env, &deployer)
        .unwrap_or_else(|_| empty_deployer_reputation(env, &deployer));

    if deployer_rep.score < config.min_deployer_score && deployer_rep.total_pools_created > 0 {
        return Err(ReputationError::InsufficientReputation);
    }

    if deployer_rep.total_pools_created >= config.max_pools_per_deployer {
        return Err(ReputationError::AccessDenied);
    }

    deployer_rep.total_pools_created = deployer_rep.total_pools_created.saturating_add(1);
    deployer_rep.active_pools = deployer_rep.active_pools.saturating_add(1);
    deployer_rep.total_tvl = deployer_rep.total_tvl.saturating_add(initial_deposit);
    deployer_rep.successful_ops = deployer_rep.successful_ops.saturating_add(1);
    deployer_rep.last_activity = env.ledger().timestamp();
    deployer_rep.pools.push_back(pool_address.clone());

    deployer_rep.score = compute_deployer_score(
        deployer_rep.total_pools_created,
        deployer_rep.active_pools,
        deployer_rep.defaults,
        deployer_rep.abandoned_pools,
        deployer_rep.avg_pool_uptime_bps,
        deployer_rep.total_tvl,
    );
    deployer_rep.tier = tier_from_score(deployer_rep.score);

    env.storage()
        .persistent()
        .set(&ReputationKey::Deployer(deployer.clone()), &deployer_rep);

    env.storage()
        .persistent()
        .set(&ReputationKey::DeployerLastDeploy(deployer.clone()), &env.ledger().timestamp());

    let pool_record = DeployerPoolRecord {
        pool_address: pool_address.clone(),
        created_at: env.ledger().timestamp(),
        tvl: initial_deposit,
        active_borrowers: 0,
        liquidation_events: 0,
        performance_score: 500,
        is_active: true,
    };
    env.storage()
        .persistent()
        .set(&ReputationKey::PoolRecord(pool_address.clone()), &pool_record);
    env.storage()
        .persistent()
        .set(&ReputationKey::PoolToDeployer(pool_address), &deployer);

    Ok(deployer_rep)
}

pub fn update_pool_metrics(
    env: &Env,
    admin: Address,
    pool_address: Address,
    tvl_delta: i128,
    borrowers_delta: u32,
    liquidation_delta: u32,
    borrowers_add: bool,
) -> Result<(), ReputationError> {
    crate::admin::require_admin(env, &admin).map_err(|_| ReputationError::Unauthorized)?;

    let mut pool_record = env
        .storage()
        .persistent()
        .get::<_, DeployerPoolRecord>(&ReputationKey::PoolRecord(pool_address.clone()))
        .ok_or(ReputationError::NotFound)?;

    pool_record.tvl = pool_record.tvl.saturating_add(tvl_delta);
    if borrowers_add {
        pool_record.active_borrowers = pool_record.active_borrowers.saturating_add(borrowers_delta);
    } else {
        pool_record.active_borrowers = pool_record.active_borrowers.saturating_sub(borrowers_delta);
    }
    pool_record.liquidation_events = pool_record.liquidation_events.saturating_add(liquidation_delta);

    let perf_liquidation_penalty = pool_record.liquidation_events.saturating_mul(20);
    pool_record.performance_score = if perf_liquidation_penalty >= 1000 {
        0
    } else {
        1000 - perf_liquidation_penalty
    };
    if pool_record.active_borrowers > 0 && pool_record.tvl > 0 {
        pool_record.performance_score = pool_record.performance_score.saturating_add(50).min(1000);
    }

    env.storage()
        .persistent()
        .set(&ReputationKey::PoolRecord(pool_address.clone()), &pool_record);

    if let Some(deployer) = env
        .storage()
        .persistent()
        .get::<_, Address>(&ReputationKey::PoolToDeployer(pool_address))
    {
        if let Ok(mut deployer_rep) = get_deployer_reputation_full(env, &deployer) {
            deployer_rep.total_tvl = deployer_rep.total_tvl.saturating_add(tvl_delta);
            deployer_rep.score = compute_deployer_score(
                deployer_rep.total_pools_created,
                deployer_rep.active_pools,
                deployer_rep.defaults,
                deployer_rep.abandoned_pools,
                deployer_rep.avg_pool_uptime_bps,
                deployer_rep.total_tvl,
            );
            deployer_rep.tier = tier_from_score(deployer_rep.score);
            deployer_rep.last_activity = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&ReputationKey::Deployer(deployer), &deployer_rep);
        }
    }

    Ok(())
}

pub fn record_pool_abandonment(
    env: &Env,
    admin: Address,
    pool_address: Address,
) -> Result<DeployerReputation, ReputationError> {
    crate::admin::require_admin(env, &admin).map_err(|_| ReputationError::Unauthorized)?;

    let deployer = env
        .storage()
        .persistent()
        .get::<_, Address>(&ReputationKey::PoolToDeployer(pool_address.clone()))
        .ok_or(ReputationError::NotFound)?;

    let mut deployer_rep = get_deployer_reputation_full(env, &deployer)?;
    deployer_rep.active_pools = deployer_rep.active_pools.saturating_sub(1);
    deployer_rep.abandoned_pools = deployer_rep.abandoned_pools.saturating_add(1);
    deployer_rep.score = compute_deployer_score(
        deployer_rep.total_pools_created,
        deployer_rep.active_pools,
        deployer_rep.defaults,
        deployer_rep.abandoned_pools,
        deployer_rep.avg_pool_uptime_bps,
        deployer_rep.total_tvl,
    );
    deployer_rep.tier = tier_from_score(deployer_rep.score);
    deployer_rep.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&ReputationKey::Deployer(deployer.clone()), &deployer_rep);

    let pool_record_key = ReputationKey::PoolRecord(pool_address.clone());
    if let Some(mut pool_record) = env
        .storage()
        .persistent()
        .get::<_, DeployerPoolRecord>(&pool_record_key)
    {
        pool_record.is_active = false;
        env.storage().persistent().set(&pool_record_key, &pool_record);
    }

    Ok(deployer_rep)
}

pub fn apply_decay(env: &Env, address: Address, is_deployer: bool) -> Result<(), ReputationError> {
    let key = if is_deployer {
        ReputationKey::Deployer(address.clone())
    } else {
        ReputationKey::User(address.clone())
    };

    let interval = env
        .storage()
        .persistent()
        .get::<_, u64>(&ReputationKey::DecayInterval)
        .unwrap_or(DEFAULT_DECAY_INTERVAL);

    let now = env.ledger().timestamp();

    if is_deployer {
        let mut rep = env
            .storage()
            .persistent()
            .get::<_, DeployerReputation>(&key)
            .ok_or(ReputationError::NotFound)?;

        if now.saturating_sub(rep.last_activity) >= interval && rep.score > 0 {
            rep.score = rep.score.saturating_sub(5);
            rep.tier = tier_from_score(rep.score);
            rep.last_activity = now;
            env.storage().persistent().set(&key, &rep);
        }
    } else {
        let mut rep = env
            .storage()
            .persistent()
            .get::<_, UserReputation>(&key)
            .ok_or(ReputationError::NotFound)?;

        if now.saturating_sub(rep.last_activity) >= interval && rep.score > 0 {
            rep.score = rep.score.saturating_sub(10);
            rep.tier = tier_from_score(rep.score);
            rep.fee_discount_bps = fee_discount_bps(&rep.tier);
            rep.last_activity = now;
            env.storage().persistent().set(&key, &rep);
        }
    }
    Ok(())
}

pub fn check_user_access(env: &Env, address: &Address, min_tier: ReputationTier) -> Result<bool, ReputationError> {
    let rep = get_user_reputation_internal(env, address).unwrap_or_else(|_| empty_user_reputation(address));
    let allowed = (rep.tier as u32) >= (min_tier as u32);
    if allowed {
        Ok(true)
    } else {
        Err(ReputationError::AccessDenied)
    }
}

pub fn check_deployer_eligibility(env: &Env, deployer: &Address) -> Result<bool, ReputationError> {
    let config = get_deployment_config(env);
    let rep = get_deployer_reputation_full(env, deployer).unwrap_or_else(|_| empty_deployer_reputation(env, deployer));

    if rep.total_pools_created == 0 {
        return Ok(true);
    }

    if rep.score < config.min_deployer_score {
        return Err(ReputationError::InsufficientReputation);
    }

    if rep.total_pools_created >= config.max_pools_per_deployer {
        return Err(ReputationError::AccessDenied);
    }

    Ok(true)
}

pub fn get_deployer_reputation(env: &Env, address: &Address) -> Option<ParticipantReputation> {
    env.storage()
        .persistent()
        .get(&ReputationKey::Deployer(address.clone()))
}

fn get_deployer_reputation_internal(env: &Env, address: &Address) -> Result<ParticipantReputation, ReputationError> {
    env.storage()
        .persistent()
        .get(&ReputationKey::Deployer(address.clone()))
        .ok_or(ReputationError::NotFound)
}

pub fn get_deployer_reputation_full(env: &Env, address: &Address) -> Result<DeployerReputation, ReputationError> {
    env.storage()
        .persistent()
        .get(&ReputationKey::Deployer(address.clone()))
        .ok_or(ReputationError::NotFound)
}

pub fn get_user_reputation(env: &Env, address: &Address) -> Option<UserReputation> {
    env.storage()
        .persistent()
        .get(&ReputationKey::User(address.clone()))
}

fn get_user_reputation_internal(env: &Env, address: &Address) -> Result<UserReputation, ReputationError> {
    env.storage()
        .persistent()
        .get(&ReputationKey::User(address.clone()))
        .ok_or(ReputationError::NotFound)
}

pub fn get_pool_record(env: &Env, pool_address: &Address) -> Result<DeployerPoolRecord, ReputationError> {
    env.storage()
        .persistent()
        .get(&ReputationKey::PoolRecord(pool_address.clone()))
        .ok_or(ReputationError::NotFound)
}

pub fn get_fee_discount_bps(env: &Env, address: &Address) -> u32 {
    get_user_reputation(env, address)
        .map(|r| r.fee_discount_bps)
        .unwrap_or(0)
}

pub fn get_borrow_limit_multiplier_bps(env: &Env, address: &Address) -> u32 {
    get_user_reputation(env, address)
        .map(|r| borrow_limit_multiplier_bps(&r.tier))
        .unwrap_or(10_000)
}

fn empty_deployer_participant(address: &Address) -> ParticipantReputation {
    ParticipantReputation {
        address: address.clone(),
        score: 0,
        tier: ReputationTier::Bronze,
        successful_ops: 0,
        defaults: 0,
        last_activity: 0,
    }
}

fn empty_deployer_reputation(env: &Env, address: &Address) -> DeployerReputation {
    DeployerReputation {
        address: address.clone(),
        score: 0,
        tier: ReputationTier::Bronze,
        total_pools_created: 0,
        active_pools: 0,
        total_tvl: 0,
        successful_ops: 0,
        defaults: 0,
        abandoned_pools: 0,
        avg_pool_uptime_bps: 10_000,
        last_activity: 0,
        pools: Vec::new(env),
    }
}

fn empty_user_reputation(address: &Address) -> UserReputation {
    UserReputation {
        address: address.clone(),
        score: 0,
        tier: ReputationTier::Bronze,
        total_repayments: 0,
        on_time_repayments: 0,
        defaults: 0,
        total_borrowed: 0,
        last_activity: 0,
        fee_discount_bps: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Env,
    };

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        (env, admin)
    }

    #[test]
    fn test_tier_from_score() {
        assert_eq!(tier_from_score(0), ReputationTier::Bronze);
        assert_eq!(tier_from_score(249), ReputationTier::Bronze);
        assert_eq!(tier_from_score(250), ReputationTier::Silver);
        assert_eq!(tier_from_score(499), ReputationTier::Silver);
        assert_eq!(tier_from_score(500), ReputationTier::Gold);
        assert_eq!(tier_from_score(749), ReputationTier::Gold);
        assert_eq!(tier_from_score(750), ReputationTier::Platinum);
        assert_eq!(tier_from_score(1000), ReputationTier::Platinum);
    }

    #[test]
    fn test_compute_user_score_perfect() {
        let score = compute_user_score(100, 100, 0);
        assert_eq!(score, 1000);
    }

    #[test]
    fn test_compute_user_score_no_history() {
        let score = compute_user_score(0, 0, 0);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_compute_user_score_with_defaults() {
        let score = compute_user_score(10, 10, 3);
        assert!(score < 700);
    }

    #[test]
    fn test_compute_deployer_score_good() {
        let score = compute_deployer_score(5, 5, 0, 0, 10_000, 500_000_000);
        assert!(score >= 500);
    }

    #[test]
    fn test_compute_deployer_score_abandoned() {
        let good = compute_deployer_score(5, 5, 0, 0, 10_000, 500_000_000);
        let bad = compute_deployer_score(5, 3, 0, 2, 10_000, 500_000_000);
        assert!(bad < good);
    }

    #[test]
    fn test_record_user_repayment_on_time() {
        let (env, _admin) = setup();
        let user = Address::generate(&env);
        let rep = record_user_repayment(&env, user.clone(), true).unwrap();
        assert_eq!(rep.total_repayments, 1);
        assert_eq!(rep.on_time_repayments, 1);
        assert_eq!(rep.defaults, 0);
        assert!(rep.score > 0);
    }

    #[test]
    fn test_record_user_repayment_late() {
        let (env, _admin) = setup();
        let user = Address::generate(&env);
        let rep = record_user_repayment(&env, user.clone(), false).unwrap();
        assert_eq!(rep.total_repayments, 1);
        assert_eq!(rep.on_time_repayments, 0);
    }

    #[test]
    fn test_user_score_increases_with_on_time() {
        let (env, _admin) = setup();
        let user = Address::generate(&env);
        let mut prev_score = 0;
        for _ in 0..10 {
            let rep = record_user_repayment(&env, user.clone(), true).unwrap();
            assert!(rep.score >= prev_score);
            prev_score = rep.score;
        }
    }

    #[test]
    fn test_record_user_default_penalizes() {
        let (env, admin) = setup();
        let user = Address::generate(&env);
        crate::admin::set_admin(&env, admin.clone(), None).unwrap();

        for _ in 0..5 {
            record_user_repayment(&env, user.clone(), true).unwrap();
        }
        let before = get_user_reputation(&env, &user).unwrap();
        let after = record_user_default(&env, admin, user.clone()).unwrap();
        assert_eq!(after.defaults, 1);
        assert!(after.score < before.score);
    }

    #[test]
    fn test_record_user_borrow_increases_total() {
        let (env, _admin) = setup();
        let user = Address::generate(&env);
        record_user_borrow(&env, user.clone(), 1000).unwrap();
        let rep = get_user_reputation(&env, &user).unwrap();
        assert_eq!(rep.total_borrowed, 1000);
    }

    #[test]
    fn test_record_user_borrow_zero_fails() {
        let (env, _admin) = setup();
        let user = Address::generate(&env);
        let result = record_user_borrow(&env, user.clone(), 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_fee_discount_by_tier() {
        assert_eq!(fee_discount_bps(&ReputationTier::Bronze), 0);
        assert_eq!(fee_discount_bps(&ReputationTier::Silver), 25);
        assert_eq!(fee_discount_bps(&ReputationTier::Gold), 50);
        assert_eq!(fee_discount_bps(&ReputationTier::Platinum), 100);
    }

    #[test]
    fn test_borrow_limit_multiplier_by_tier() {
        assert_eq!(borrow_limit_multiplier_bps(&ReputationTier::Bronze), 10_000);
        assert_eq!(borrow_limit_multiplier_bps(&ReputationTier::Silver), 11_000);
        assert_eq!(borrow_limit_multiplier_bps(&ReputationTier::Gold), 12_500);
        assert_eq!(borrow_limit_multiplier_bps(&ReputationTier::Platinum), 15_000);
    }

    #[test]
    fn test_default_config() {
        let config = default_config();
        assert_eq!(config.min_deployer_score, 100);
        assert_eq!(config.max_pools_per_deployer, 10);
        assert_eq!(config.deploy_cooldown_seconds, 3600);
        assert_eq!(config.min_initial_deposit, 1_000_000);
    }

    #[test]
    fn test_record_pool_deployment_first_pool() {
        let (env, _admin) = setup();
        let deployer = Address::generate(&env);
        let pool = Address::generate(&env);

        let result = record_pool_deployment(&env, deployer.clone(), pool.clone(), 5_000_000);
        assert!(result.is_ok());
        let rep = result.unwrap();
        assert_eq!(rep.total_pools_created, 1);
        assert_eq!(rep.active_pools, 1);
        assert_eq!(rep.total_tvl, 5_000_000);
    }

    #[test]
    fn test_record_pool_deployment_insufficient_deposit() {
        let (env, _admin) = setup();
        let deployer = Address::generate(&env);
        let pool = Address::generate(&env);

        let result = record_pool_deployment(&env, deployer, pool, 100);
        assert!(matches!(result, Err(ReputationError::InsufficientReputation)));
    }

    #[test]
    fn test_record_pool_deployment_cooldown() {
        let (env, _admin) = setup();
        let deployer = Address::generate(&env);
        let pool1 = Address::generate(&env);
        let pool2 = Address::generate(&env);

        record_pool_deployment(&env, deployer.clone(), pool1, 5_000_000).unwrap();

        let result = record_pool_deployment(&env, deployer.clone(), pool2, 5_000_000);
        assert!(matches!(result, Err(ReputationError::RateLimitExceeded)));
    }

    #[test]
    fn test_record_pool_deployment_after_cooldown() {
        let (env, _admin) = setup();
        let deployer = Address::generate(&env);
        let pool1 = Address::generate(&env);
        let pool2 = Address::generate(&env);

        record_pool_deployment(&env, deployer.clone(), pool1, 5_000_000).unwrap();

        let now = env.ledger().timestamp();
        env.ledger().with_mut(|li| li.timestamp = now + 4000);

        let result = record_pool_deployment(&env, deployer, pool2, 5_000_000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_pool_record_after_deployment() {
        let (env, _admin) = setup();
        let deployer = Address::generate(&env);
        let pool = Address::generate(&env);

        record_pool_deployment(&env, deployer, pool.clone(), 5_000_000).unwrap();

        let record = get_pool_record(&env, &pool).unwrap();
        assert_eq!(record.tvl, 5_000_000);
        assert_eq!(record.active_borrowers, 0);
        assert!(record.is_active);
    }

    #[test]
    fn test_check_deployer_eligibility_first_pool() {
        let (env, _admin) = setup();
        let deployer = Address::generate(&env);
        let result = check_deployer_eligibility(&env, &deployer);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_update_pool_metrics() {
        let (env, admin) = setup();
        crate::admin::set_admin(&env, admin.clone(), None).unwrap();
        let deployer = Address::generate(&env);
        let pool = Address::generate(&env);

        record_pool_deployment(&env, deployer.clone(), pool.clone(), 5_000_000).unwrap();

        update_pool_metrics(
            &env,
            admin,
            pool.clone(),
            2_000_000,
            5,
            0,
            true,
        )
        .unwrap();

        let record = get_pool_record(&env, &pool).unwrap();
        assert_eq!(record.tvl, 7_000_000);
        assert_eq!(record.active_borrowers, 5);
    }

    #[test]
    fn test_record_pool_abandonment() {
        let (env, admin) = setup();
        crate::admin::set_admin(&env, admin.clone(), None).unwrap();
        let deployer = Address::generate(&env);
        let pool = Address::generate(&env);

        record_pool_deployment(&env, deployer.clone(), pool.clone(), 5_000_000).unwrap();
        let before = get_deployer_reputation_full(&env, &deployer).unwrap();

        record_pool_abandonment(&env, admin, pool).unwrap();
        let after = get_deployer_reputation_full(&env, &deployer).unwrap();

        assert_eq!(after.active_pools, before.active_pools - 1);
        assert_eq!(after.abandoned_pools, 1);
        assert!(after.score <= before.score);
    }

    #[test]
    fn test_check_user_access_allowed() {
        let (env, _admin) = setup();
        let user = Address::generate(&env);

        for _ in 0..50 {
            record_user_repayment(&env, user.clone(), true).unwrap();
        }

        let result = check_user_access(&env, &user, ReputationTier::Silver);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_user_access_denied() {
        let (env, _admin) = setup();
        let user = Address::generate(&env);

        let result = check_user_access(&env, &user, ReputationTier::Gold);
        assert!(matches!(result, Err(ReputationError::AccessDenied)));
    }

    #[test]
    fn test_apply_decay_user() {
        let (env, _admin) = setup();
        let user = Address::generate(&env);

        for _ in 0..20 {
            record_user_repayment(&env, user.clone(), true).unwrap();
        }
        let before = get_user_reputation(&env, &user).unwrap();

        let later = env.ledger().timestamp() + DEFAULT_DECAY_INTERVAL + 100;
        env.ledger().with_mut(|li| li.timestamp = later);

        apply_decay(&env, user.clone(), false).unwrap();
        let after = get_user_reputation(&env, &user).unwrap();
        assert!(after.score < before.score || after.score == 0);
    }

    #[test]
    fn test_set_deployment_config_requires_admin() {
        let (env, admin) = setup();
        let not_admin = Address::generate(&env);
        crate::admin::set_admin(&env, admin.clone(), None).unwrap();

        let mut config = default_config();
        config.min_deployer_score = 200;

        let result = set_deployment_config(&env, &not_admin, config.clone());
        assert!(matches!(result, Err(ReputationError::Unauthorized)));

        let result = set_deployment_config(&env, &admin, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_deployment_config_invalid() {
        let (env, admin) = setup();
        crate::admin::set_admin(&env, admin.clone(), None).unwrap();

        let mut config = default_config();
        config.max_pools_per_deployer = 0;

        let result = set_deployment_config(&env, &admin, config);
        assert!(matches!(result, Err(ReputationError::InvalidParameter)));
    }
}
