#![allow(unused)]
use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Symbol, Val, Vec};

/// Errors that can occur during risk parameter management
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RiskParamsError {
    /// Unauthorized access - caller is not admin
    Unauthorized = 1,
    /// Invalid parameter value
    InvalidParameter = 2,
    /// Parameter change exceeds maximum allowed change
    ParameterChangeTooLarge = 3,
    /// Invalid collateral ratio (must be >= liquidation threshold)
    InvalidCollateralRatio = 4,
    /// Invalid liquidation threshold (must be <= collateral ratio)
    InvalidLiquidationThreshold = 5,
    /// Close factor out of valid range (0-100%)
    InvalidCloseFactor = 6,
    /// Liquidation incentive out of valid range (0-50%)
    InvalidLiquidationIncentive = 7,
}

/// Storage keys for risk params data
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum RiskParamsDataKey {
    /// Legacy spread risk configuration (pre #722 packing)
    RiskParamsConfig,
    /// Packed pool configuration — all bps fields + last_update in one u128 slot.
    /// Bit layout (issue #722):
    ///   bits  0..16  min_collateral_ratio (u16 bps)
    ///   bits 16..32  liquidation_threshold (u16 bps)
    ///   bits 32..48  close_factor (u16 bps)
    ///   bits 48..64  liquidation_incentive (u16 bps)
    ///   bits 64..128 last_update (u64 timestamp)
    PackedRiskParamsConfig,
}

/// Risk parameters
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RiskParams {
    /// Minimum collateral ratio (in basis points, e.g., 11000 = 110%)
    /// Users must maintain this ratio or face liquidation
    pub min_collateral_ratio: i128,
    /// Liquidation threshold (in basis points, e.g., 10500 = 105%)
    /// When collateral ratio falls below this, liquidation is allowed
    pub liquidation_threshold: i128,
    /// Close factor (in basis points, e.g., 5000 = 50%)
    /// Maximum percentage of debt that can be liquidated in a single transaction
    pub close_factor: i128,
    /// Liquidation incentive (in basis points, e.g., 1000 = 10%)
    /// Bonus given to liquidators
    pub liquidation_incentive: i128,
    /// Last update timestamp
    pub last_update: u64,
}

/// Constants for parameter validation
const BASIS_POINTS_SCALE: i128 = 10_000; // 100% = 10,000 basis points
const MIN_COLLATERAL_RATIO_MIN: i128 = 10_000; // 100% minimum
const MIN_COLLATERAL_RATIO_MAX: i128 = 50_000; // 500% maximum
const LIQUIDATION_THRESHOLD_MIN: i128 = 10_000; // 100% minimum
const LIQUIDATION_THRESHOLD_MAX: i128 = 50_000; // 500% maximum
const CLOSE_FACTOR_MIN: i128 = 0; // 0% minimum
const CLOSE_FACTOR_MAX: i128 = BASIS_POINTS_SCALE; // 100% maximum
const LIQUIDATION_INCENTIVE_MIN: i128 = 0; // 0% minimum
const LIQUIDATION_INCENTIVE_MAX: i128 = 5_000; // 50% maximum (safety limit)
const MAX_PARAMETER_CHANGE_BPS: i128 = 1_000; // 10% maximum change per update

// ---------------------------------------------------------------------------
// Packed pool configuration (issue #722)
// ---------------------------------------------------------------------------

const BPS_FIELD_MASK: u128 = 0xFFFF; // 16 bits enough for any validated bps value (<= 50_000)

/// Collapse a `RiskParams` into a single `u128` storage slot.
/// All four bps fields are validated to fit 16 bits before packing.
pub fn pack_risk_params(config: &RiskParams) -> u128 {
    let mcr = (config.min_collateral_ratio as u128) & BPS_FIELD_MASK;
    let lt = (config.liquidation_threshold as u128) & BPS_FIELD_MASK;
    let cf = (config.close_factor as u128) & BPS_FIELD_MASK;
    let li = (config.liquidation_incentive as u128) & BPS_FIELD_MASK;
    let ts = config.last_update as u128;
    mcr | (lt << 16) | (cf << 32) | (li << 48) | (ts << 64)
}

/// Expand a packed `u128` slot back into the public `RiskParams` shape.
pub fn unpack_risk_params(packed: u128) -> RiskParams {
    RiskParams {
        min_collateral_ratio: (packed & BPS_FIELD_MASK) as i128,
        liquidation_threshold: ((packed >> 16) & BPS_FIELD_MASK) as i128,
        close_factor: ((packed >> 32) & BPS_FIELD_MASK) as i128,
        liquidation_incentive: ((packed >> 48) & BPS_FIELD_MASK) as i128,
        last_update: (packed >> 64) as u64,
    }
}

/// Initialize risk parameters
///
/// Sets up default risk parameters.
/// Should be called during contract initialization.
///
/// # Arguments
/// * `env` - The Soroban environment
///
/// # Returns
/// Returns Ok(()) on success
///
/// # Errors
/// * `RiskParamsError::InvalidParameter` - If default parameters are invalid
pub fn initialize_risk_params(env: &Env) -> Result<(), RiskParamsError> {
    let default_config = RiskParams {
        min_collateral_ratio: 11_000,  // 110% default
        liquidation_threshold: 10_500, // 105% default
        close_factor: 5_000,           // 50% default
        liquidation_incentive: 1_000,  // 10% default
        last_update: env.ledger().timestamp(),
    };

    validate_risk_params(&default_config)?;

    env.storage()
        .persistent()
        .set(&RiskParamsDataKey::PackedRiskParamsConfig, &pack_risk_params(&default_config));

    Ok(())
}

/// Get current risk parameters (legacy storage)
pub fn get_legacy_risk_params(env: &Env) -> Option<RiskParams> {
    let config_key = RiskParamsDataKey::RiskParamsConfig;
    env.storage()
/// Get current risk parameters.
///
/// Reads the packed pool-config slot (issue #722). If only the legacy spread
/// layout exists, it is migrated lazily on first read so deployed contracts
/// upgrade without a separate migration step.
pub fn get_risk_params(env: &Env) -> Option<RiskParams> {
    if let Some(packed) = env.storage().persistent().get::<RiskParamsDataKey, u128>(
        &RiskParamsDataKey::PackedRiskParamsConfig,
    ) {
        return Some(unpack_risk_params(packed));
    }
    match env
        .storage()
        .persistent()
        .get::<RiskParamsDataKey, RiskParams>(&RiskParamsDataKey::RiskParamsConfig)
    {
        Some(legacy) => {
            migrate_from_legacy(env);
            Some(legacy)
        }
        None => None,
    }
}

/// One-time migration of a legacy (unpacked) pool config into the packed slot.
///
/// Idempotent: returns `false` when migration is unnecessary (packed slot
/// already present or no legacy data). Returns `true` when a migration ran.
pub fn migrate_from_legacy(env: &Env) -> bool {
    if env
        .storage()
        .persistent()
        .has(&RiskParamsDataKey::PackedRiskParamsConfig)
    {
        return false;
    }
    match env
        .storage()
        .persistent()
        .get::<RiskParamsDataKey, RiskParams>(&RiskParamsDataKey::RiskParamsConfig)
    {
        Some(legacy) => {
            env.storage().persistent().set(
                &RiskParamsDataKey::PackedRiskParamsConfig,
                &pack_risk_params(&legacy),
            );
            true
        }
        None => false,
    }
}

/// Get current risk parameters from packed config (#713)
pub fn get_risk_params(env: &Env) -> Option<RiskParams> {
    let packed = crate::storage::migrate_from_legacy(env, &None).ok()?;
    Some(RiskParams {
        min_collateral_ratio: packed.min_collateral_ratio_bps,
        liquidation_threshold: packed.liquidation_threshold_bps,
        close_factor: packed.close_factor_bps,
        liquidation_incentive: packed.liquidation_incentive_bps,
        last_update: packed.last_update,
    })
}

/// Validate risk configuration
fn validate_risk_params(config: &RiskParams) -> Result<(), RiskParamsError> {
    // Validate min collateral ratio
    if config.min_collateral_ratio < MIN_COLLATERAL_RATIO_MIN
        || config.min_collateral_ratio > MIN_COLLATERAL_RATIO_MAX
    {
        return Err(RiskParamsError::InvalidParameter);
    }

    // Validate liquidation threshold
    if config.liquidation_threshold < LIQUIDATION_THRESHOLD_MIN
        || config.liquidation_threshold > LIQUIDATION_THRESHOLD_MAX
    {
        return Err(RiskParamsError::InvalidLiquidationThreshold);
    }

    // Validate that min collateral ratio >= liquidation threshold
    if config.min_collateral_ratio < config.liquidation_threshold {
        return Err(RiskParamsError::InvalidCollateralRatio);
    }

    // Validate close factor
    if config.close_factor < CLOSE_FACTOR_MIN || config.close_factor > CLOSE_FACTOR_MAX {
        return Err(RiskParamsError::InvalidCloseFactor);
    }

    // Validate liquidation incentive
    if config.liquidation_incentive < LIQUIDATION_INCENTIVE_MIN
        || config.liquidation_incentive > LIQUIDATION_INCENTIVE_MAX
    {
        return Err(RiskParamsError::InvalidLiquidationIncentive);
    }

    Ok(())
}

/// Validate parameter change doesn't exceed maximum allowed change
fn validate_parameter_change(old_value: i128, new_value: i128) -> Result<(), RiskParamsError> {
    let change = if new_value > old_value {
        new_value - old_value
    } else {
        old_value - new_value
    };

    // Calculate maximum allowed change (10% of old value)
    let max_change = (old_value * MAX_PARAMETER_CHANGE_BPS) / BASIS_POINTS_SCALE;

    if change > max_change {
        return Err(RiskParamsError::ParameterChangeTooLarge);
    }

    Ok(())
}

/// Set risk parameters (admin only - caller check should be done by the contract)
///
/// Updates risk parameters with validation and change limits.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `min_collateral_ratio` - New minimum collateral ratio (in basis points)
/// * `liquidation_threshold` - New liquidation threshold (in basis points)
/// * `close_factor` - New close factor (in basis points)
/// * `liquidation_incentive` - New liquidation incentive (in basis points)
///
/// # Returns
/// Returns Ok(()) on success
pub fn set_risk_params(
    env: &Env,
    min_collateral_ratio: Option<i128>,
    liquidation_threshold: Option<i128>,
    close_factor: Option<i128>,
    liquidation_incentive: Option<i128>,
) -> Result<(), RiskParamsError> {
    let mut config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;

    // Update parameters if provided
    if let Some(mcr) = min_collateral_ratio {
        validate_parameter_change(config.min_collateral_ratio, mcr)?;
        config.min_collateral_ratio = mcr;
    }

    if let Some(lt) = liquidation_threshold {
        validate_parameter_change(config.liquidation_threshold, lt)?;
        config.liquidation_threshold = lt;
    }

    if let Some(cf) = close_factor {
        validate_parameter_change(config.close_factor, cf)?;
        config.close_factor = cf;
    }

    if let Some(li) = liquidation_incentive {
        validate_parameter_change(config.liquidation_incentive, li)?;
        config.liquidation_incentive = li;
    }

    // Validate the updated config
    validate_risk_params(&config)?;

    // Update timestamp
    config.last_update = env.ledger().timestamp();

    // Save config as packed single slot (issue #722)
    env.storage()
        .persistent()
        .set(&RiskParamsDataKey::PackedRiskParamsConfig, &pack_risk_params(&config));

    // Emit event
    emit_risk_params_updated_event(env, &config);

    Ok(())
}

/// Emit risk parameters updated event
#[allow(deprecated)]
fn emit_risk_params_updated_event(env: &Env, config: &RiskParams) {
    let topics = (Symbol::new(env, "risk_params_updated"),);
    env.events().publish(topics, config.clone());
}

/// Get minimum collateral ratio
pub fn get_min_collateral_ratio(env: &Env) -> Result<i128, RiskParamsError> {
    let config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;
    Ok(config.min_collateral_ratio)
}

/// Get liquidation threshold
pub fn get_liquidation_threshold(env: &Env) -> Result<i128, RiskParamsError> {
    let config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;
    Ok(config.liquidation_threshold)
}

/// Get close factor
pub fn get_close_factor(env: &Env) -> Result<i128, RiskParamsError> {
    let config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;
    Ok(config.close_factor)
}

/// Get liquidation incentive
pub fn get_liquidation_incentive(env: &Env) -> Result<i128, RiskParamsError> {
    let config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;
    Ok(config.liquidation_incentive)
}

/// Calculate maximum liquidatable amount
///
/// Uses close factor to determine maximum debt that can be liquidated.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `debt_value` - Total debt value (in base units)
///
/// # Returns
/// Maximum amount that can be liquidated
pub fn get_max_liquidatable_amount(env: &Env, debt_value: i128) -> Result<i128, RiskParamsError> {
    let config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;

    // Calculate: debt * close_factor / BASIS_POINTS_SCALE
    let max_amount = (debt_value * config.close_factor)
        .checked_div(BASIS_POINTS_SCALE)
        .ok_or(RiskParamsError::InvalidParameter)?; // Return generic error for overflow since we dropped Overflow variant

    Ok(max_amount)
}

/// Calculate liquidation incentive amount
///
/// Returns the bonus amount for liquidators.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `liquidated_amount` - Amount being liquidated (in base units)
///
/// # Returns
/// Liquidation incentive amount
pub fn get_liquidation_incentive_amount(
    env: &Env,
    liquidated_amount: i128,
) -> Result<i128, RiskParamsError> {
    let config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;

    // Calculate: amount * liquidation_incentive / BASIS_POINTS_SCALE
    let incentive = (liquidated_amount * config.liquidation_incentive)
        .checked_div(BASIS_POINTS_SCALE)
        .ok_or(RiskParamsError::InvalidParameter)?;

    Ok(incentive)
}

/// Require minimum collateral ratio
pub fn require_min_collateral_ratio(
    env: &Env,
    collateral_value: i128,
    debt_value: i128,
) -> Result<(), RiskParamsError> {
    let config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;

    if debt_value == 0 {
        return Ok(());
    }

    let ratio = (collateral_value * BASIS_POINTS_SCALE)
        .checked_div(debt_value)
        .ok_or(RiskParamsError::InvalidParameter)?;

    if ratio < config.min_collateral_ratio {
        return Err(RiskParamsError::InvalidCollateralRatio);
    }

    Ok(())
}

/// Can be liquidated check
pub fn can_be_liquidated(
    env: &Env,
    collateral_value: i128,
    debt_value: i128,
) -> Result<bool, RiskParamsError> {
    let config = get_risk_params(env).ok_or(RiskParamsError::InvalidParameter)?;

    if debt_value == 0 {
        return Ok(false);
    }

    let ratio = (collateral_value * BASIS_POINTS_SCALE)
        .checked_div(debt_value)
        .ok_or(RiskParamsError::InvalidParameter)?;

    Ok(ratio < config.liquidation_threshold)
}
