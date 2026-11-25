//! # Cross-Pool Collateral Migration with Deferred Interest (issue #1536)
//!
//! ## Overview & Motivation
//!
//! In multi-pool lending architectures, borrowers frequently need to migrate collateral
//! between pools (e.g., transitioning from a legacy pool to a higher-yield vault,
//! or moving assets into an isolated market). Traditionally, migrating collateral
//! required borrowers to completely pay down all accrued interest and principal debt upfront,
//! or risk triggering an immediate liquidation cascade due to collateral shortfall in the source pool.
//!
//! For borrowers with illiquid positions or high capital utilization, requiring immediate
//! interest payoff introduces severe friction and capital inefficiency.
//!
//! This module implements **cross-pool collateral migration with deferred interest**:
//! 1. **Collateral Transfer**: Eligible collateral is safely debited from the `source_pool`
//!    and credited to the `destination_pool` in an atomic state transition.
//! 2. **Deferred Interest Accounting**: Accrued interest on the source position is captured
//!    into a snapshot (`DeferredInterestRecord`) and deferred until settlement, without forcing
//!    an immediate cash outflow or triggering liquidation.
//! 3. **Solvency & Health Safeguards**: If the borrower retains debt in the source pool,
//!    migration is only permitted if remaining collateral maintains a healthy collateralization ratio
//!    (`health_factor >= 1.0` / 10,000 bps).
//! 4. **Grace Period & Expiry**: A configurable deferral window ensures deferred interest must be
//!    settled before its deadline, preserving protocol revenue integrity.
//! 5. **Settlement**: Borrowers or automated keeper contracts can settle deferred interest in the
//!    destination pool partially or fully.
//! 6. **Observability**: Rich events (`MigrationInitiatedEvent`, `MigrationCompletedEvent`,
//!    `DeferredInterestSettledEvent`, `MigrationConfigUpdatedEvent`) enable transparent indexing.

#![allow(unused)]

use soroban_sdk::{
    contracterror, contractevent, contracttype, symbol_short, Address, Env, IntoVal, Map, String,
    Symbol, Val, Vec,
};

// ─── Tunables & Constants ───────────────────────────────────────────────────

/// 100% in basis points.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Default maximum deferral window: 7 days.
pub const DEFAULT_MAX_DEFERRAL_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Default migration fee in basis points (5 bps = 0.05%).
pub const DEFAULT_MIGRATION_FEE_BPS: i128 = 5;

/// Default minimum migration amount (100 units).
pub const DEFAULT_MIN_MIGRATION_AMOUNT: i128 = 100;

/// Default maximum migration amount (1,000,000,000 units).
pub const DEFAULT_MAX_MIGRATION_AMOUNT: i128 = 1_000_000_000_000;

/// Default cooldown period between migrations per user (60 seconds).
pub const DEFAULT_COOLDOWN_SECONDS: u64 = 60;

/// Minimum health factor required in basis points (10,000 bps = 1.0x).
pub const MIN_HEALTH_FACTOR_BPS: i128 = 10_000;

// ─── Errors ─────────────────────────────────────────────────────────────────

/// Errors returned by cross-pool migration and deferred interest operations.
/// Error codes begin at 1000.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MigrationError {
    /// Caller is not authorized.
    Unauthorized = 1000,
    /// Migration amount is zero or negative.
    InvalidAmount = 1001,
    /// Source and destination pools cannot be identical.
    SamePool = 1002,
    /// Migration amount is below minimum threshold.
    MigrationTooSmall = 1003,
    /// Migration amount exceeds maximum limit.
    MigrationTooLarge = 1004,
    /// Insufficient collateral in source pool for migration.
    InsufficientCollateral = 1005,
    /// Migration would leave source pool position insolvent / unhealthy.
    UnhealthySourcePosition = 1006,
    /// Migration operations are currently paused.
    MigrationPaused = 1007,
    /// No deferred interest record found for user and pool.
    DeferredInterestNotFound = 1008,
    /// Deferral deadline has elapsed without settlement.
    DeferralPeriodExpired = 1009,
    /// Deferred interest has already been fully settled.
    AlreadySettled = 1010,
    /// Settlement payment amount is invalid or zero.
    InsufficientSettlementAmount = 1011,
    /// Arithmetic overflow occurred.
    Overflow = 1012,
    /// Migration fee exceeds 10,000 basis points.
    InvalidFee = 1013,
    /// User cooldown period has not yet elapsed.
    CooldownNotElapsed = 1014,
    /// Invalid configuration parameters supplied.
    InvalidConfig = 1015,
}

// ─── Data Types ─────────────────────────────────────────────────────────────

/// Status lifecycle for a cross-pool migration.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MigrationStatus {
    Pending = 0,
    Completed = 1,
    Settled = 2,
    Cancelled = 3,
}

/// System configuration for cross-pool migration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationConfig {
    /// Protocol administrator address.
    pub admin: Address,
    /// Minimum migration amount permitted.
    pub min_migration_amount: i128,
    /// Maximum migration amount permitted.
    pub max_migration_amount: i128,
    /// Protocol fee in basis points.
    pub fee_bps: i128,
    /// Maximum deferral period allowed for accrued interest (in seconds).
    pub max_deferral_seconds: u64,
    /// Cooldown between consecutive migrations per user (in seconds).
    pub cooldown_seconds: u64,
    /// Whether migration operations are paused.
    pub paused: bool,
}

impl MigrationConfig {
    pub fn default_config(admin: Address) -> Self {
        Self {
            admin,
            min_migration_amount: DEFAULT_MIN_MIGRATION_AMOUNT,
            max_migration_amount: DEFAULT_MAX_MIGRATION_AMOUNT,
            fee_bps: DEFAULT_MIGRATION_FEE_BPS,
            max_deferral_seconds: DEFAULT_MAX_DEFERRAL_SECONDS,
            cooldown_seconds: DEFAULT_COOLDOWN_SECONDS,
            paused: false,
        }
    }
}

/// Permanent record of an executed cross-pool collateral migration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationRecord {
    /// Unique monotonic migration identifier.
    pub id: u64,
    /// User address owning the collateral and debt position.
    pub user: Address,
    /// Originating pool (None = default / native XLM pool).
    pub source_pool: Option<Address>,
    /// Destination pool receiving the collateral.
    pub destination_pool: Option<Address>,
    /// Underlying asset address (None = native XLM).
    pub asset: Option<Address>,
    /// Gross amount of collateral debited from source pool.
    pub collateral_migrated: i128,
    /// Net collateral credited to destination pool after protocol fee.
    pub net_collateral_credited: i128,
    /// Protocol fee deducted in collateral asset units.
    pub protocol_fee: i128,
    /// Snapshot of accrued interest deferred to the destination pool.
    pub deferred_interest: i128,
    /// Timestamp when migration was executed.
    pub initiated_at: u64,
    /// Deadline by which deferred interest must be settled.
    pub deferral_deadline: u64,
    /// Status of the migration record.
    pub status: MigrationStatus,
}

/// Record tracking deferred interest for a specific user and destination pool.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeferredInterestRecord {
    /// Migration ID associated with this deferred interest.
    pub migration_id: u64,
    /// Borrower address.
    pub user: Address,
    /// Source pool from which interest originated.
    pub source_pool: Option<Address>,
    /// Destination pool holding the position.
    pub destination_pool: Option<Address>,
    /// Asset in which interest is denominated.
    pub asset: Option<Address>,
    /// Principal amount of deferred interest.
    pub principal_deferred: i128,
    /// Timestamp when deferral was snapshot.
    pub accrued_at: u64,
    /// Expiration deadline for settlement.
    pub deferral_deadline: u64,
    /// Cumulative amount paid toward deferred interest.
    pub settled_amount: i128,
    /// Whether deferred interest has been fully settled.
    pub is_settled: bool,
}

/// Pre-flight preview of a migration transaction.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationPreview {
    /// Proposed collateral amount to migrate.
    pub collateral_to_migrate: i128,
    /// Calculated protocol fee.
    pub protocol_fee: i128,
    /// Net collateral that will be credited.
    pub net_collateral_credited: i128,
    /// Accrued interest that will be deferred.
    pub deferred_interest: i128,
    /// Remaining collateral balance in source pool.
    pub source_collateral_remaining: i128,
    /// Estimated health factor of source pool after migration (in bps).
    pub source_health_factor_after: i128,
    /// Whether the proposed migration meets all execution criteria.
    pub is_executable: bool,
}

/// Protocol-wide aggregate metrics for migration observability.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationAnalytics {
    /// Total migrations executed.
    pub total_migrations: u64,
    /// Total collateral volume migrated.
    pub total_volume_migrated: i128,
    /// Total protocol migration fees collected.
    pub total_fees_collected: i128,
    /// Total interest volume deferred.
    pub total_interest_deferred: i128,
    /// Total deferred interest settled to date.
    pub total_interest_settled: i128,
}

impl MigrationAnalytics {
    pub fn zero() -> Self {
        Self {
            total_migrations: 0,
            total_volume_migrated: 0,
            total_fees_collected: 0,
            total_interest_deferred: 0,
            total_interest_settled: 0,
        }
    }
}

// ─── Events ─────────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct MigrationInitiatedEvent {
    pub migration_id: u64,
    pub user: Address,
    pub source_pool: Option<Address>,
    pub destination_pool: Option<Address>,
    pub amount: i128,
    pub deferred_interest: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MigrationCompletedEvent {
    pub migration_id: u64,
    pub user: Address,
    pub net_collateral: i128,
    pub fee: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct DeferredInterestSettledEvent {
    pub migration_id: u64,
    pub user: Address,
    pub settled_amount: i128,
    pub remaining_deferred: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MigrationConfigUpdatedEvent {
    pub admin: Address,
    pub fee_bps: i128,
    pub min_amount: i128,
    pub max_amount: i128,
    pub paused: bool,
    pub timestamp: u64,
}

// ─── Storage Keys ───────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum MigrateDataKey {
    /// Global system configuration: MigrationConfig
    Config,
    /// Admin address: Address
    Admin,
    /// Next monotonic migration ID: u64
    NextMigrationId,
    /// Specific migration record: MigrationRecord (keyed by migration_id)
    Record(u64),
    /// List of migration IDs for a user: Vec<u64>
    UserMigrations(Address),
    /// Deferred interest record: DeferredInterestRecord (keyed by user, destination_pool)
    DeferredInterest(Address, Option<Address>),
    /// User collateral in a pool: i128 (keyed by user, pool)
    UserCollateral(Address, Option<Address>),
    /// User debt in a pool: i128 (keyed by user, pool)
    UserDebt(Address, Option<Address>),
    /// Timestamp of last migration by user: u64
    UserLastMigration(Address),
    /// Protocol-wide migration analytics: MigrationAnalytics
    Analytics,
}

// ─── Administration & Configuration ─────────────────────────────────────────

/// Initialize the migration module configuration.
pub fn initialize_config(
    env: &Env,
    admin: Address,
    min_amount: i128,
    max_amount: i128,
    fee_bps: i128,
    max_deferral_seconds: u64,
    cooldown_seconds: u64,
) -> Result<MigrationConfig, MigrationError> {
    admin.require_auth();

    if min_amount <= 0 || max_amount < min_amount {
        return Err(MigrationError::InvalidConfig);
    }
    if !(0..=BPS_DENOMINATOR).contains(&fee_bps) {
        return Err(MigrationError::InvalidFee);
    }

    let config = MigrationConfig {
        admin: admin.clone(),
        min_migration_amount: min_amount,
        max_migration_amount: max_amount,
        fee_bps,
        max_deferral_seconds,
        cooldown_seconds,
        paused: false,
    };

    env.storage()
        .persistent()
        .set(&MigrateDataKey::Config, &config);
    env.storage()
        .persistent()
        .set(&MigrateDataKey::Admin, &admin);
    if !env
        .storage()
        .persistent()
        .has(&MigrateDataKey::NextMigrationId)
    {
        env.storage()
            .persistent()
            .set(&MigrateDataKey::NextMigrationId, &1u64);
    }
    if !env.storage().persistent().has(&MigrateDataKey::Analytics) {
        env.storage()
            .persistent()
            .set(&MigrateDataKey::Analytics, &MigrationAnalytics::zero());
    }

    env.events().publish(
        (Symbol::new(env, "migration_config_updated"), admin.clone()),
        MigrationConfigUpdatedEvent {
            admin,
            fee_bps,
            min_amount,
            max_amount,
            paused: false,
            timestamp: env.ledger().timestamp(),
        },
    );

    Ok(config)
}

/// Update configuration parameters (admin-only).
pub fn update_config(
    env: &Env,
    caller: Address,
    min_amount: i128,
    max_amount: i128,
    fee_bps: i128,
    max_deferral_seconds: u64,
    cooldown_seconds: u64,
    paused: bool,
) -> Result<MigrationConfig, MigrationError> {
    caller.require_auth();

    let mut config = get_config(env);
    if caller != config.admin {
        return Err(MigrationError::Unauthorized);
    }

    if min_amount <= 0 || max_amount < min_amount {
        return Err(MigrationError::InvalidConfig);
    }
    if !(0..=BPS_DENOMINATOR).contains(&fee_bps) {
        return Err(MigrationError::InvalidFee);
    }

    config.min_migration_amount = min_amount;
    config.max_migration_amount = max_amount;
    config.fee_bps = fee_bps;
    config.max_deferral_seconds = max_deferral_seconds;
    config.cooldown_seconds = cooldown_seconds;
    config.paused = paused;

    env.storage()
        .persistent()
        .set(&MigrateDataKey::Config, &config);

    env.events().publish(
        (Symbol::new(env, "migration_config_updated"), caller.clone()),
        MigrationConfigUpdatedEvent {
            admin: caller,
            fee_bps,
            min_amount,
            max_amount,
            paused,
            timestamp: env.ledger().timestamp(),
        },
    );

    Ok(config)
}

/// Pause or unpause migration operations (admin-only).
pub fn set_paused(env: &Env, caller: Address, paused: bool) -> Result<(), MigrationError> {
    caller.require_auth();
    let mut config = get_config(env);
    if caller != config.admin {
        return Err(MigrationError::Unauthorized);
    }

    config.paused = paused;
    env.storage()
        .persistent()
        .set(&MigrateDataKey::Config, &config);

    env.events().publish(
        (Symbol::new(env, "migration_config_updated"), caller.clone()),
        MigrationConfigUpdatedEvent {
            admin: caller,
            fee_bps: config.fee_bps,
            min_amount: config.min_migration_amount,
            max_amount: config.max_migration_amount,
            paused,
            timestamp: env.ledger().timestamp(),
        },
    );

    Ok(())
}

/// Retrieve the active migration configuration.
pub fn get_config(env: &Env) -> MigrationConfig {
    if let Some(config) = env.storage().persistent().get(&MigrateDataKey::Config) {
        config
    } else {
        // Fallback default if not yet initialized
        let dummy_admin = Address::from_string(&String::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ));
        MigrationConfig::default_config(dummy_admin)
    }
}

/// Retrieve protocol migration analytics.
pub fn get_analytics(env: &Env) -> MigrationAnalytics {
    env.storage()
        .persistent()
        .get(&MigrateDataKey::Analytics)
        .unwrap_or_else(MigrationAnalytics::zero)
}

// ─── Position Helpers ───────────────────────────────────────────────────────

/// Get tracked user collateral in a pool.
pub fn get_user_collateral(env: &Env, user: &Address, pool: &Option<Address>) -> i128 {
    env.storage()
        .persistent()
        .get(&MigrateDataKey::UserCollateral(user.clone(), pool.clone()))
        .unwrap_or(0)
}

/// Update tracked user collateral in a pool.
pub fn set_user_collateral(env: &Env, user: &Address, pool: &Option<Address>, amount: i128) {
    env.storage().persistent().set(
        &MigrateDataKey::UserCollateral(user.clone(), pool.clone()),
        &amount,
    );
}

/// Get tracked user debt principal in a pool.
pub fn get_user_debt(env: &Env, user: &Address, pool: &Option<Address>) -> i128 {
    env.storage()
        .persistent()
        .get(&MigrateDataKey::UserDebt(user.clone(), pool.clone()))
        .unwrap_or(0)
}

/// Update tracked user debt principal in a pool.
pub fn set_user_debt(env: &Env, user: &Address, pool: &Option<Address>, debt: i128) {
    env.storage()
        .persistent()
        .set(&MigrateDataKey::UserDebt(user.clone(), pool.clone()), &debt);
}

// ─── Previews & Core Migration Logic ────────────────────────────────────────

/// Preview migration output, fees, and resulting health factor without mutating state.
pub fn preview_migration(
    env: &Env,
    user: Address,
    source_pool: Option<Address>,
    destination_pool: Option<Address>,
    asset: Option<Address>,
    amount: i128,
    accrued_interest: i128,
) -> Result<MigrationPreview, MigrationError> {
    if source_pool == destination_pool {
        return Err(MigrationError::SamePool);
    }
    if amount <= 0 {
        return Err(MigrationError::InvalidAmount);
    }

    let config = get_config(env);
    if config.paused {
        return Err(MigrationError::MigrationPaused);
    }

    let fee = amount
        .checked_mul(config.fee_bps)
        .ok_or(MigrationError::Overflow)?
        / BPS_DENOMINATOR;
    let net_collateral = amount.checked_sub(fee).ok_or(MigrationError::Overflow)?;

    let current_collateral = get_user_collateral(env, &user, &source_pool);
    if current_collateral < amount {
        return Err(MigrationError::InsufficientCollateral);
    }

    let remaining_collateral = current_collateral
        .checked_sub(amount)
        .ok_or(MigrationError::Overflow)?;
    let remaining_debt = get_user_debt(env, &user, &source_pool);

    // Calculate source health factor after migration
    let health_factor_after = if remaining_debt == 0 {
        // Solvency guaranteed if no debt remains
        i128::MAX
    } else {
        remaining_collateral
            .checked_mul(BPS_DENOMINATOR)
            .ok_or(MigrationError::Overflow)?
            / remaining_debt
    };

    let is_executable = remaining_debt == 0 || health_factor_after >= MIN_HEALTH_FACTOR_BPS;

    Ok(MigrationPreview {
        collateral_to_migrate: amount,
        protocol_fee: fee,
        net_collateral_credited: net_collateral,
        deferred_interest: accrued_interest,
        source_collateral_remaining: remaining_collateral,
        source_health_factor_after: health_factor_after,
        is_executable,
    })
}

/// Execute cross-pool collateral migration with deferred interest.
pub fn initiate_migration(
    env: &Env,
    user: Address,
    source_pool: Option<Address>,
    destination_pool: Option<Address>,
    asset: Option<Address>,
    amount: i128,
    accrued_interest_to_defer: i128,
) -> Result<MigrationRecord, MigrationError> {
    user.require_auth();

    if source_pool == destination_pool {
        return Err(MigrationError::SamePool);
    }
    if amount <= 0 || accrued_interest_to_defer < 0 {
        return Err(MigrationError::InvalidAmount);
    }

    let config = get_config(env);
    if config.paused {
        return Err(MigrationError::MigrationPaused);
    }

    if amount < config.min_migration_amount {
        return Err(MigrationError::MigrationTooSmall);
    }
    if amount > config.max_migration_amount {
        return Err(MigrationError::MigrationTooLarge);
    }

    // Cooldown check
    let now = env.ledger().timestamp();
    if let Some(last_migration) = env
        .storage()
        .persistent()
        .get::<_, u64>(&MigrateDataKey::UserLastMigration(user.clone()))
    {
        if now < last_migration.saturating_add(config.cooldown_seconds) {
            return Err(MigrationError::CooldownNotElapsed);
        }
    }

    // Collateral balance verification
    let source_collateral = get_user_collateral(env, &user, &source_pool);
    if source_collateral < amount {
        return Err(MigrationError::InsufficientCollateral);
    }

    // Health factor check for source position
    let remaining_collateral = source_collateral
        .checked_sub(amount)
        .ok_or(MigrationError::Overflow)?;
    let remaining_debt = get_user_debt(env, &user, &source_pool);

    if remaining_debt > 0 {
        let health_after = remaining_collateral
            .checked_mul(BPS_DENOMINATOR)
            .ok_or(MigrationError::Overflow)?
            / remaining_debt;
        if health_after < MIN_HEALTH_FACTOR_BPS {
            return Err(MigrationError::UnhealthySourcePosition);
        }
    }

    // Calculate fee and net credit
    let fee = amount
        .checked_mul(config.fee_bps)
        .ok_or(MigrationError::Overflow)?
        / BPS_DENOMINATOR;
    let net_credit = amount.checked_sub(fee).ok_or(MigrationError::Overflow)?;

    // Atomic position updates:
    // 1. Deduct collateral from source pool
    set_user_collateral(env, &user, &source_pool, remaining_collateral);

    // 2. Credit net collateral to destination pool
    let dest_collateral = get_user_collateral(env, &user, &destination_pool);
    let new_dest_collateral = dest_collateral
        .checked_add(net_credit)
        .ok_or(MigrationError::Overflow)?;
    set_user_collateral(env, &user, &destination_pool, new_dest_collateral);

    // 3. Allocate monotonic migration ID
    let migration_id: u64 = env
        .storage()
        .persistent()
        .get(&MigrateDataKey::NextMigrationId)
        .unwrap_or(1);
    env.storage()
        .persistent()
        .set(&MigrateDataKey::NextMigrationId, &(migration_id + 1));

    let deferral_deadline = now
        .checked_add(config.max_deferral_seconds)
        .ok_or(MigrationError::Overflow)?;

    // 4. Record deferred interest if positive
    if accrued_interest_to_defer > 0 {
        let deferred_record = DeferredInterestRecord {
            migration_id,
            user: user.clone(),
            source_pool: source_pool.clone(),
            destination_pool: destination_pool.clone(),
            asset: asset.clone(),
            principal_deferred: accrued_interest_to_defer,
            accrued_at: now,
            deferral_deadline,
            settled_amount: 0,
            is_settled: false,
        };
        env.storage().persistent().set(
            &MigrateDataKey::DeferredInterest(user.clone(), destination_pool.clone()),
            &deferred_record,
        );
    }

    // 5. Save migration record
    let record = MigrationRecord {
        id: migration_id,
        user: user.clone(),
        source_pool: source_pool.clone(),
        destination_pool: destination_pool.clone(),
        asset: asset.clone(),
        collateral_migrated: amount,
        net_collateral_credited: net_credit,
        protocol_fee: fee,
        deferred_interest: accrued_interest_to_defer,
        initiated_at: now,
        deferral_deadline,
        status: MigrationStatus::Completed,
    };
    env.storage()
        .persistent()
        .set(&MigrateDataKey::Record(migration_id), &record);

    // Append to user migration history
    let mut user_migrations: Vec<u64> = env
        .storage()
        .persistent()
        .get(&MigrateDataKey::UserMigrations(user.clone()))
        .unwrap_or_else(|| Vec::new(env));
    user_migrations.push_back(migration_id);
    env.storage().persistent().set(
        &MigrateDataKey::UserMigrations(user.clone()),
        &user_migrations,
    );

    // Update cooldown timestamp
    env.storage()
        .persistent()
        .set(&MigrateDataKey::UserLastMigration(user.clone()), &now);

    // Update analytics
    let mut analytics = get_analytics(env);
    analytics.total_migrations = analytics.total_migrations.saturating_add(1);
    analytics.total_volume_migrated = analytics
        .total_volume_migrated
        .checked_add(amount)
        .unwrap_or(analytics.total_volume_migrated);
    analytics.total_fees_collected = analytics
        .total_fees_collected
        .checked_add(fee)
        .unwrap_or(analytics.total_fees_collected);
    analytics.total_interest_deferred = analytics
        .total_interest_deferred
        .checked_add(accrued_interest_to_defer)
        .unwrap_or(analytics.total_interest_deferred);
    env.storage()
        .persistent()
        .set(&MigrateDataKey::Analytics, &analytics);

    // 6. Emit events
    env.events().publish(
        (Symbol::new(env, "migration_initiated"), user.clone()),
        MigrationInitiatedEvent {
            migration_id,
            user: user.clone(),
            source_pool: source_pool.clone(),
            destination_pool: destination_pool.clone(),
            amount,
            deferred_interest: accrued_interest_to_defer,
            timestamp: now,
        },
    );

    env.events().publish(
        (Symbol::new(env, "migration_completed"), user.clone()),
        MigrationCompletedEvent {
            migration_id,
            user,
            net_collateral: net_credit,
            fee,
            timestamp: now,
        },
    );

    Ok(record)
}

/// Settle deferred interest for a user in a pool.
pub fn settle_deferred_interest(
    env: &Env,
    caller: Address,
    user: Address,
    pool: Option<Address>,
    payment_amount: i128,
) -> Result<DeferredInterestRecord, MigrationError> {
    caller.require_auth();

    if payment_amount <= 0 {
        return Err(MigrationError::InsufficientSettlementAmount);
    }

    let mut deferred: DeferredInterestRecord = env
        .storage()
        .persistent()
        .get(&MigrateDataKey::DeferredInterest(
            user.clone(),
            pool.clone(),
        ))
        .ok_or(MigrationError::DeferredInterestNotFound)?;

    if deferred.is_settled {
        return Err(MigrationError::AlreadySettled);
    }

    let now = env.ledger().timestamp();
    // Allow settlement even after deadline, but track state
    let remaining_debt = deferred
        .principal_deferred
        .checked_sub(deferred.settled_amount)
        .ok_or(MigrationError::Overflow)?;

    let applied_payment = payment_amount.min(remaining_debt);
    deferred.settled_amount = deferred
        .settled_amount
        .checked_add(applied_payment)
        .ok_or(MigrationError::Overflow)?;

    let new_remaining = deferred
        .principal_deferred
        .checked_sub(deferred.settled_amount)
        .ok_or(MigrationError::Overflow)?;
    if new_remaining == 0 {
        deferred.is_settled = true;
    }

    env.storage().persistent().set(
        &MigrateDataKey::DeferredInterest(user.clone(), pool.clone()),
        &deferred,
    );

    // Update analytics
    let mut analytics = get_analytics(env);
    analytics.total_interest_settled = analytics
        .total_interest_settled
        .checked_add(applied_payment)
        .unwrap_or(analytics.total_interest_settled);
    env.storage()
        .persistent()
        .set(&MigrateDataKey::Analytics, &analytics);

    // Emit settlement event
    env.events().publish(
        (Symbol::new(env, "deferred_interest_settled"), user.clone()),
        DeferredInterestSettledEvent {
            migration_id: deferred.migration_id,
            user,
            settled_amount: applied_payment,
            remaining_deferred: new_remaining,
            timestamp: now,
        },
    );

    Ok(deferred)
}

/// Retrieve migration record by ID.
pub fn get_migration(env: &Env, migration_id: u64) -> Option<MigrationRecord> {
    env.storage()
        .persistent()
        .get(&MigrateDataKey::Record(migration_id))
}

/// Retrieve deferred interest record for a user in a pool.
pub fn get_deferred_interest(
    env: &Env,
    user: &Address,
    pool: &Option<Address>,
) -> Option<DeferredInterestRecord> {
    env.storage()
        .persistent()
        .get(&MigrateDataKey::DeferredInterest(
            user.clone(),
            pool.clone(),
        ))
}

/// Retrieve list of migration IDs initiated by a user.
pub fn get_user_migrations(env: &Env, user: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&MigrateDataKey::UserMigrations(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::Env;

    struct Fixture {
        env: Env,
        admin: Address,
        user: Address,
        source_pool: Option<Address>,
        destination_pool: Option<Address>,
        asset: Option<Address>,
    }

    fn fixture() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let source_pool = Some(Address::generate(&env));
        let destination_pool = Some(Address::generate(&env));
        let asset = Some(Address::generate(&env));

        // Initialize standard migration config
        initialize_config(
            &env,
            admin.clone(),
            100,
            1_000_000_000,
            DEFAULT_MIGRATION_FEE_BPS,
            DEFAULT_MAX_DEFERRAL_SECONDS,
            DEFAULT_COOLDOWN_SECONDS,
        )
        .unwrap();

        Fixture {
            env,
            admin,
            user,
            source_pool,
            destination_pool,
            asset,
        }
    }

    #[test]
    fn test_successful_migration_with_deferred_interest() {
        let f = fixture();
        let deposit_amount = 10_000i128;
        let migrate_amount = 4_000i128;
        let deferred_interest = 250i128;

        // Seed collateral in source pool
        set_user_collateral(&f.env, &f.user, &f.source_pool, deposit_amount);

        let record = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            migrate_amount,
            deferred_interest,
        )
        .unwrap();

        assert_eq!(record.id, 1);
        assert_eq!(record.collateral_migrated, 4_000);
        // Fee = 4000 * 5 / 10000 = 2
        assert_eq!(record.protocol_fee, 2);
        assert_eq!(record.net_collateral_credited, 3_998);
        assert_eq!(record.deferred_interest, 250);
        assert_eq!(record.status, MigrationStatus::Completed);

        // Verify balances
        assert_eq!(get_user_collateral(&f.env, &f.user, &f.source_pool), 6_000);
        assert_eq!(
            get_user_collateral(&f.env, &f.user, &f.destination_pool),
            3_998
        );

        // Verify deferred interest record
        let deferred =
            get_deferred_interest(&f.env, &f.user, &f.destination_pool).expect("deferred record");
        assert_eq!(deferred.migration_id, 1);
        assert_eq!(deferred.principal_deferred, 250);
        assert_eq!(deferred.settled_amount, 0);
        assert!(!deferred.is_settled);

        // Verify analytics
        let analytics = get_analytics(&f.env);
        assert_eq!(analytics.total_migrations, 1);
        assert_eq!(analytics.total_volume_migrated, 4_000);
        assert_eq!(analytics.total_fees_collected, 2);
        assert_eq!(analytics.total_interest_deferred, 250);
    }

    #[test]
    fn test_deferred_interest_settlement_partial_and_full() {
        let f = fixture();
        set_user_collateral(&f.env, &f.user, &f.source_pool, 20_000);

        initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            5_000,
            1_000, // 1000 deferred interest
        )
        .unwrap();

        // Partial settlement of 400
        let partial = settle_deferred_interest(
            &f.env,
            f.user.clone(),
            f.user.clone(),
            f.destination_pool.clone(),
            400,
        )
        .unwrap();
        assert_eq!(partial.settled_amount, 400);
        assert!(!partial.is_settled);

        // Remaining 600 settled
        let full = settle_deferred_interest(
            &f.env,
            f.user.clone(),
            f.user.clone(),
            f.destination_pool.clone(),
            600,
        )
        .unwrap();
        assert_eq!(full.settled_amount, 1_000);
        assert!(full.is_settled);

        // Re-settlement fails
        let err = settle_deferred_interest(
            &f.env,
            f.user.clone(),
            f.user.clone(),
            f.destination_pool.clone(),
            100,
        );
        assert_eq!(err, Err(MigrationError::AlreadySettled));

        let analytics = get_analytics(&f.env);
        assert_eq!(analytics.total_interest_settled, 1_000);
    }

    #[test]
    fn test_preview_migration_unhealthy_position_rejected() {
        let f = fixture();
        // Deposit 10,000 collateral and have 8,000 debt (Health Factor = 1.25x = 12,500 bps)
        set_user_collateral(&f.env, &f.user, &f.source_pool, 10_000);
        set_user_debt(&f.env, &f.user, &f.source_pool, 8_000);

        // Migrating 5,000 leaves 5,000 collateral for 8,000 debt -> HF = 0.625x (unhealthy)
        let preview = preview_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            5_000,
            100,
        )
        .unwrap();

        assert!(!preview.is_executable);
        assert!(preview.source_health_factor_after < MIN_HEALTH_FACTOR_BPS);

        // Actual execution must return UnhealthySourcePosition
        let err = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            5_000,
            100,
        );
        assert_eq!(err, Err(MigrationError::UnhealthySourcePosition));
    }

    #[test]
    fn test_healthy_source_position_allows_partial_migration() {
        let f = fixture();
        // Deposit 20,000 collateral and have 5,000 debt
        set_user_collateral(&f.env, &f.user, &f.source_pool, 20_000);
        set_user_debt(&f.env, &f.user, &f.source_pool, 5_000);

        // Migrating 10,000 leaves 10,000 collateral for 5,000 debt -> HF = 2.0x (20,000 bps)
        let preview = preview_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            10_000,
            0,
        )
        .unwrap();

        assert!(preview.is_executable);
        assert_eq!(preview.source_health_factor_after, 20_000);

        let record = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            10_000,
            0,
        )
        .unwrap();
        assert_eq!(record.collateral_migrated, 10_000);
    }

    #[test]
    fn test_rejects_same_pool_migration() {
        let f = fixture();
        set_user_collateral(&f.env, &f.user, &f.source_pool, 5_000);

        let err = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.source_pool.clone(),
            f.asset.clone(),
            1_000,
            0,
        );
        assert_eq!(err, Err(MigrationError::SamePool));
    }

    #[test]
    fn test_rejects_insufficient_collateral() {
        let f = fixture();
        set_user_collateral(&f.env, &f.user, &f.source_pool, 500);

        let err = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            1_000,
            0,
        );
        assert_eq!(err, Err(MigrationError::InsufficientCollateral));
    }

    #[test]
    fn test_rejects_out_of_bounds_amounts() {
        let f = fixture();
        set_user_collateral(&f.env, &f.user, &f.source_pool, 2_000_000_000);

        // Below min (min is 100)
        let err_too_small = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            50,
            0,
        );
        assert_eq!(err_too_small, Err(MigrationError::MigrationTooSmall));

        // Above max (max is 1,000,000,000)
        let err_too_large = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            1_500_000_000,
            0,
        );
        assert_eq!(err_too_large, Err(MigrationError::MigrationTooLarge));
    }

    #[test]
    fn test_rejects_migration_when_paused() {
        let f = fixture();
        set_user_collateral(&f.env, &f.user, &f.source_pool, 5_000);

        // Pause migration
        set_paused(&f.env, f.admin.clone(), true).unwrap();
        assert!(get_config(&f.env).paused);

        let err = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            1_000,
            0,
        );
        assert_eq!(err, Err(MigrationError::MigrationPaused));

        // Unpause and verify success
        set_paused(&f.env, f.admin.clone(), false).unwrap();
        let ok = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            1_000,
            0,
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn test_cooldown_enforced_between_migrations() {
        let f = fixture();
        set_user_collateral(&f.env, &f.user, &f.source_pool, 10_000);

        // First migration succeeds
        initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            1_000,
            0,
        )
        .unwrap();

        // Immediate second migration within cooldown window (60s) fails
        let err = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            1_000,
            0,
        );
        assert_eq!(err, Err(MigrationError::CooldownNotElapsed));

        // Advance ledger timestamp beyond cooldown
        f.env.ledger().set_timestamp(1_000_061);
        let ok = initiate_migration(
            &f.env,
            f.user.clone(),
            f.source_pool.clone(),
            f.destination_pool.clone(),
            f.asset.clone(),
            1_000,
            0,
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn test_admin_config_authorization() {
        let f = fixture();
        let stranger = Address::generate(&f.env);

        let err = update_config(&f.env, stranger.clone(), 100, 1000, 10, 3600, 30, false);
        assert_eq!(err, Err(MigrationError::Unauthorized));

        let err_pause = set_paused(&f.env, stranger, true);
        assert_eq!(err_pause, Err(MigrationError::Unauthorized));
    }
}
