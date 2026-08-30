use soroban_sdk::{contracttype, Address, Env, IntoVal, TryFromVal, Val, Vec};

#[soroban_sdk::contracttype]
pub struct SnapshotValue {
    pub value: Val,
    pub timestamp: u64,
}

/// Get a value from persistent storage, optionally bypassing an in-memory cache layer.
/// Returns `None` when `force_direct` is false (caller is expected to serve from cache).
/// Returns the stored value when `force_direct` is true.
pub fn get_snapshot<K, T>(env: &Env, key: &K, force_direct: bool) -> Option<T>
where
    K: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
{
    if force_direct {
        return env.storage().persistent().get::<K, T>(key);
    }
    None
}

// ─── Packed Pool Config ───────────────────────────────────────────────────────

/// Width of each basis-point field in the rate word.
const BPS_FIELD_BITS: u32 = 16;
/// Mask for a single 16-bit basis-point field.
const BPS_FIELD_MASK: u128 = 0xFFFF;

/// Timestamp occupies the low 40 bits of the status word (~year 36 800).
const TS_BITS: u32 = 40;
const TS_MASK: u64 = (1u64 << TS_BITS) - 1;
/// Status flags occupy 8 bits above the timestamp.
const FLAGS_SHIFT: u32 = TS_BITS;
const FLAGS_MASK: u64 = 0xFF;

// ── Status-flag bit positions ────────────────────────────────────────────

pub const FLAG_PAUSED: u8 = 1 << 0;
pub const FLAG_BORROWING_ENABLED: u8 = 1 << 1;
pub const FLAG_COLLATERAL_ENABLED: u8 = 1 << 2;
pub const FLAG_DEPRECATED: u8 = 1 << 3;

#[soroban_sdk::contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PackError {
    BpsFieldOverflow = 1,
    TimestampOverflow = 2,
}

#[contracttype]
#[derive(Clone)]
pub enum PoolConfigKey {
    /// Both packed words `(rate_word: u128, status_word: u64)` for a pool
    Config(Option<Address>),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolConfig {
    pub min_collateral_ratio_bps: i128,
    pub liquidation_threshold_bps: i128,
    pub reserve_factor_bps: i128,
    pub close_factor_bps: i128,
    pub liquidation_incentive_bps: i128,
    pub last_update: u64,
    pub flags: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PackedConfig {
    pub rate_word: u128,
    pub status_word: u64,
}

fn pack_bps_field(value: i128, slot: u32) -> Result<u128, PackError> {
    if !(0..=BPS_FIELD_MASK as i128).contains(&value) {
        return Err(PackError::BpsFieldOverflow);
    }
    Ok((value as u128) << (slot * BPS_FIELD_BITS))
}

fn unpack_bps_field(word: u128, slot: u32) -> i128 {
    ((word >> (slot * BPS_FIELD_BITS)) & BPS_FIELD_MASK) as i128
}

pub fn pack(config: &PoolConfig) -> Result<PackedConfig, PackError> {
    let rate_word = pack_bps_field(config.min_collateral_ratio_bps, 0)?
        | pack_bps_field(config.liquidation_threshold_bps, 1)?
        | pack_bps_field(config.reserve_factor_bps, 2)?
        | pack_bps_field(config.close_factor_bps, 3)?
        | pack_bps_field(config.liquidation_incentive_bps, 4)?;

    if config.last_update > TS_MASK {
        return Err(PackError::TimestampOverflow);
    }
    let status_word =
        (config.last_update & TS_MASK) | (((config.flags as u64) & FLAGS_MASK) << FLAGS_SHIFT);

    Ok(PackedConfig {
        rate_word,
        status_word,
    })
}

pub fn unpack(packed: &PackedConfig) -> PoolConfig {
    PoolConfig {
        min_collateral_ratio_bps: unpack_bps_field(packed.rate_word, 0),
        liquidation_threshold_bps: unpack_bps_field(packed.rate_word, 1),
        reserve_factor_bps: unpack_bps_field(packed.rate_word, 2),
        close_factor_bps: unpack_bps_field(packed.rate_word, 3),
        liquidation_incentive_bps: unpack_bps_field(packed.rate_word, 4),
        last_update: packed.status_word & TS_MASK,
        flags: ((packed.status_word >> FLAGS_SHIFT) & FLAGS_MASK) as u32,
    }
}

pub fn flag_is_set(flags: u32, flag: u8) -> bool {
    flags & (flag as u32) != 0
}

pub fn flag_with(flags: u32, flag: u8, on: bool) -> u32 {
    if on {
        flags | (flag as u32)
    } else {
        flags & !(flag as u32)
    }
}

// ── Persistence ──────────────────────────────────────────────────────────

pub fn load_pool_config(env: &Env, pool: &Option<Address>) -> Option<PoolConfig> {
    env.storage()
        .persistent()
        .get::<PoolConfigKey, PackedConfig>(&PoolConfigKey::Config(pool.clone()))
        .map(|p| unpack(&p))
}

pub fn store_pool_config(env: &Env, pool: &Option<Address>, config: &PoolConfig) -> Result<(), PackError> {
    let packed = pack(config)?;
    env.storage()
        .persistent()
        .set(&PoolConfigKey::Config(pool.clone()), &packed);
    Ok(())
}

pub fn migrate_from_legacy(env: &Env, pool: &Option<Address>) -> Result<PoolConfig, PackError> {
    if let Some(existing) = load_pool_config(env, pool) {
        return Ok(existing);
    }

    let global_risk = crate::risk_params::get_legacy_risk_params(env).unwrap_or(crate::risk_params::RiskParams {
        min_collateral_ratio: 11_000,
        liquidation_threshold: 10_500,
        close_factor: 5_000,
        liquidation_incentive: 1_000,
        last_update: env.ledger().timestamp(),
    });

    let reserve_factor = crate::reserve::get_legacy_reserve_factor(env, pool.clone());

    let config = PoolConfig {
        min_collateral_ratio_bps: global_risk.min_collateral_ratio,
        liquidation_threshold_bps: global_risk.liquidation_threshold,
        reserve_factor_bps: reserve_factor,
        close_factor_bps: global_risk.close_factor,
        liquidation_incentive_bps: global_risk.liquidation_incentive,
        last_update: env.ledger().timestamp(),
        flags: (FLAG_BORROWING_ENABLED | FLAG_COLLATERAL_ENABLED) as u32,
    };

    store_pool_config(env, pool, &config)?;
    Ok(config)
}

// ─── Guardian config ──────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct GuardianConfig {
    pub guardians: Vec<Address>,
    pub threshold: u32,
}

// ─── Governance storage keys ──────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum GovernanceDataKey {
    // Core governance config
    Admin,
    Config,
    NextProposalId,
    // Multisig
    MultisigConfig,
    MultisigAdmins,
    MultisigThreshold,
    // Guardian / recovery
    GuardianConfig,
    Guardians,
    GuardianThreshold,
    // Proposals
    Proposal(u64),
    UserProposals(Address, u64),
    ProposalApprovals(u64),
    // Votes
    Vote(u64, Address),
    VotePowerSnapshot(u64, Address),
    VoteLock(Address),
    // Delegation
    DelegationRecord(Address),
    // Recovery
    RecoveryRequest,
    RecoveryApprovals,
    // Analytics
    GovernanceAnalytics,
    // Caches
    ProposalSimulationCache(u64),
    ProposalDryRunCache(u64),
    ParameterOptimizationCache,
    // Rate limiting
    ProposalWindowStart(Address),
    ProposalCreationCount(Address),
    // Timelock
    TimelockConfig,
    NextTimelockId,
    TimelockOperation(u64),
    TimelockQueue,
    /// #674 — per-action-type timelock delay override: ActionTypeDelay(action_type_id) -> u64
    ActionTypeDelay(u32),
    /// #674 — guardian approvals collected for an emergency override of a queued
    /// timelock operation: TimelockEmergencyApprovals(operation_id) -> Vec<Address>
    TimelockEmergencyApprovals(u64),
}

// ─── General data keys (used by credit score and other modules) ───────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    // Credit scoring keys
    CreditScore(Address),

    // Circuit breaker keys
    CircuitBreakerConfig,
    CircuitBreakerState,
    CircuitBreakerWhitelist,

    // Liquidation queue keys
    LiquidationQueueConfig,
    NextLiquidationQueueId,
    LiquidationQueueEntry(u64),
    LiquidatorRegistration(Address),
}

// ─── Lazy pool-state keys (#721) ────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum PoolStateKey {
    /// Global monotonic epoch. Bumped by every mutation that can change a
    /// resolved pool-state snapshot; cached snapshots keyed by an older epoch
    /// are transparently ignored and rebuilt on next access.
    Epoch,
    /// Persistent marker recording that a pool's aggregate state has been
    /// lazily materialized at least once: `Initialized(pool) -> bool`.
    Initialized(Option<Address>),
    /// Cache hit / miss / rebuild / invalidation counters (`PoolStateMetrics`).
    Metrics,
}

#[contracttype]
#[derive(Clone)]
pub enum PoolStateTempKey {
    /// Short-lived resolved snapshot cache: `Snapshot(pool, epoch) -> PoolStateSnapshot`.
    Snapshot(Option<Address>, u64),
}

// ─── Temporary transaction-local cache keys ─────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum TempDataKey {
    TokenBalanceCache(Address, Address),
    LendingIndexCache,
}

pub fn get_temp_token_balance(env: &Env, token: &Address, owner: &Address) -> Option<i128> {
    env.storage()
        .temporary()
        .get::<TempDataKey, i128>(&TempDataKey::TokenBalanceCache(
            token.clone(),
            owner.clone(),
        ))
}

pub fn set_temp_token_balance(env: &Env, token: &Address, owner: &Address, balance: i128) {
    env.storage()
        .temporary()
        .set(&TempDataKey::TokenBalanceCache(token.clone(), owner.clone()), &balance);
}

pub fn get_temp_lending_index(env: &Env) -> Option<crate::interest_rate::LendingIndex> {
    env.storage()
        .temporary()
        .get::<TempDataKey, crate::interest_rate::LendingIndex>(&TempDataKey::LendingIndexCache)
}

pub fn set_temp_lending_index(env: &Env, index: crate::interest_rate::LendingIndex) {
    env.storage()
        .temporary()
        .set(&TempDataKey::LendingIndexCache, &index);
}
