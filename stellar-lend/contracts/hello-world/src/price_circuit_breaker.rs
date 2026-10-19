//! # Price Circuit Breaker for Extreme Price Movements
//!
//! Extends the oracle module with an asset-level circuit breaker that
//! automatically halts price acceptance when a price deviates beyond a
//! configurable threshold from the last accepted reference price.
//!
//! ## Design
//! - Each asset has an independent `PriceCircuitBreakerState`.
//! - On every price update attempt the deviation from `last_safe_price` is
//!   checked; if it exceeds `threshold_bps` the breaker trips.
//! - While tripped, `get_price_with_breaker` returns an error.
//! - The breaker resets automatically after `cooldown_seconds` OR when the
//!   admin explicitly resets it via `reset_price_circuit_breaker`.
//!
//! ## Storage
//! All state is stored under `PriceCircuitBreakerKey::State(asset)` in
//! persistent storage — one entry per asset.

use soroban_sdk::{contracterror, contracttype, Address, Env};

use crate::admin::require_admin;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Default price deviation threshold: 20% (2000 bps)
pub const DEFAULT_PRICE_CB_THRESHOLD_BPS: i128 = 2000;
/// Minimum threshold: 1% (100 bps)
pub const MIN_PRICE_CB_THRESHOLD_BPS: i128 = 100;
/// Maximum threshold: 50% (5000 bps)
pub const MAX_PRICE_CB_THRESHOLD_BPS: i128 = 5000;
/// Default cooldown: 1 hour
pub const DEFAULT_PRICE_CB_COOLDOWN: u64 = 3_600;
/// Maximum cooldown: 24 hours
pub const MAX_PRICE_CB_COOLDOWN: u64 = 86_400;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PriceCircuitBreakerError {
    /// Caller is not the protocol admin
    Unauthorized = 1,
    /// Price circuit breaker is currently open for this asset
    CircuitOpen = 2,
    /// Price deviation exceeded the configured threshold
    DeviationExceeded = 3,
    /// Invalid configuration parameter
    InvalidConfig = 4,
    /// Arithmetic overflow
    Overflow = 5,
}

// ─── Types ───────────────────────────────────────────────────────────────────

/// Per-asset circuit breaker configuration
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceCircuitBreakerConfig {
    /// Maximum allowed deviation from last safe price (basis points)
    pub threshold_bps: i128,
    /// How long the breaker stays open after tripping (seconds)
    pub cooldown_seconds: u64,
    /// Whether the breaker is globally enabled for this asset
    pub enabled: bool,
}

impl Default for PriceCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            threshold_bps: DEFAULT_PRICE_CB_THRESHOLD_BPS,
            cooldown_seconds: DEFAULT_PRICE_CB_COOLDOWN,
            enabled: true,
        }
    }
}

/// Per-asset circuit breaker runtime state
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceCircuitBreakerState {
    /// Last accepted safe price
    pub last_safe_price: i128,
    /// If non-zero and in the future, the breaker is open
    pub open_until: u64,
    /// Timestamp of the last trip
    pub last_trip_at: u64,
    /// Cumulative trip count
    pub trip_count: u32,
    /// Price that triggered the last trip
    pub last_trigger_price: i128,
    /// Deviation bps that triggered the last trip
    pub last_deviation_bps: i128,
}

impl Default for PriceCircuitBreakerState {
    fn default() -> Self {
        Self {
            last_safe_price: 0,
            open_until: 0,
            last_trip_at: 0,
            trip_count: 0,
            last_trigger_price: 0,
            last_deviation_bps: 0,
        }
    }
}

// ─── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum PriceCircuitBreakerKey {
    /// Per-asset runtime state
    State(Address),
    /// Per-asset configuration
    Config(Address),
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn load_state(env: &Env, asset: &Address) -> PriceCircuitBreakerState {
    env.storage()
        .persistent()
        .get::<PriceCircuitBreakerKey, PriceCircuitBreakerState>(
            &PriceCircuitBreakerKey::State(asset.clone()),
        )
        .unwrap_or_default()
}

fn save_state(env: &Env, asset: &Address, state: &PriceCircuitBreakerState) {
    env.storage()
        .persistent()
        .set(&PriceCircuitBreakerKey::State(asset.clone()), state);
}

fn load_config(env: &Env, asset: &Address) -> PriceCircuitBreakerConfig {
    env.storage()
        .persistent()
        .get::<PriceCircuitBreakerKey, PriceCircuitBreakerConfig>(
            &PriceCircuitBreakerKey::Config(asset.clone()),
        )
        .unwrap_or_default()
}

fn compute_deviation_bps(
    reference: i128,
    candidate: i128,
) -> Result<i128, PriceCircuitBreakerError> {
    if reference <= 0 || candidate <= 0 {
        return Ok(0);
    }
    let diff = if candidate > reference {
        candidate
            .checked_sub(reference)
            .ok_or(PriceCircuitBreakerError::Overflow)?
    } else {
        reference
            .checked_sub(candidate)
            .ok_or(PriceCircuitBreakerError::Overflow)?
    };
    diff.checked_mul(10_000)
        .ok_or(PriceCircuitBreakerError::Overflow)?
        .checked_div(reference)
        .ok_or(PriceCircuitBreakerError::Overflow)
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Configure the price circuit breaker for `asset` (admin-only).
pub fn configure_price_circuit_breaker(
    env: &Env,
    caller: Address,
    asset: Address,
    config: PriceCircuitBreakerConfig,
) -> Result<(), PriceCircuitBreakerError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| PriceCircuitBreakerError::Unauthorized)?;

    if config.threshold_bps < MIN_PRICE_CB_THRESHOLD_BPS
        || config.threshold_bps > MAX_PRICE_CB_THRESHOLD_BPS
    {
        return Err(PriceCircuitBreakerError::InvalidConfig);
    }
    if config.cooldown_seconds > MAX_PRICE_CB_COOLDOWN {
        return Err(PriceCircuitBreakerError::InvalidConfig);
    }

    env.storage().persistent().set(
        &PriceCircuitBreakerKey::Config(asset.clone()),
        &config,
    );
    Ok(())
}

/// Check and record a candidate price against the circuit breaker for `asset`.
///
/// - If the breaker is already open, returns `Err(CircuitOpen)`.
/// - If the deviation exceeds the threshold, trips the breaker and returns
///   `Err(DeviationExceeded)`.
/// - Otherwise records the price as the new safe baseline and returns `Ok(())`.
///
/// Call this inside `update_price_feed` before persisting the new price.
pub fn check_price_circuit_breaker(
    env: &Env,
    asset: &Address,
    candidate_price: i128,
) -> Result<(), PriceCircuitBreakerError> {
    let config = load_config(env, asset);
    if !config.enabled {
        return Ok(());
    }

    let mut state = load_state(env, asset);
    let now = env.ledger().timestamp();

    // Auto-reset after cooldown
    if state.open_until > 0 && now >= state.open_until {
        state.open_until = 0;
        save_state(env, asset, &state);
    }

    // Still open?
    if state.open_until > 0 {
        return Err(PriceCircuitBreakerError::CircuitOpen);
    }

    // First price sets the baseline — no deviation to check
    if state.last_safe_price <= 0 {
        state.last_safe_price = candidate_price;
        save_state(env, asset, &state);
        return Ok(());
    }

    let deviation_bps = compute_deviation_bps(state.last_safe_price, candidate_price)?;

    if deviation_bps > config.threshold_bps {
        // Trip the breaker
        state.open_until = now.saturating_add(config.cooldown_seconds);
        state.last_trip_at = now;
        state.trip_count = state.trip_count.saturating_add(1);
        state.last_trigger_price = candidate_price;
        state.last_deviation_bps = deviation_bps;
        save_state(env, asset, &state);

        env.events().publish(
            (
                soroban_sdk::Symbol::new(env, "price_cb_tripped"),
                asset.clone(),
            ),
            (deviation_bps, config.threshold_bps, state.open_until),
        );

        return Err(PriceCircuitBreakerError::DeviationExceeded);
    }

    // Price is safe — update baseline
    state.last_safe_price = candidate_price;
    save_state(env, asset, &state);
    Ok(())
}

/// Manually reset the circuit breaker for `asset` (admin-only).
///
/// Sets `open_until = 0` and updates `last_safe_price` to `new_reference`.
pub fn reset_price_circuit_breaker(
    env: &Env,
    caller: Address,
    asset: Address,
    new_reference_price: i128,
) -> Result<(), PriceCircuitBreakerError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| PriceCircuitBreakerError::Unauthorized)?;

    let mut state = load_state(env, &asset);
    state.open_until = 0;
    if new_reference_price > 0 {
        state.last_safe_price = new_reference_price;
    }
    save_state(env, &asset, &state);

    env.events().publish(
        (
            soroban_sdk::Symbol::new(env, "price_cb_reset"),
            asset,
        ),
        (caller, new_reference_price),
    );
    Ok(())
}

/// Check whether the circuit breaker is currently open for `asset`.
pub fn is_price_circuit_breaker_open(env: &Env, asset: &Address) -> bool {
    let state = load_state(env, asset);
    if state.open_until == 0 {
        return false;
    }
    env.ledger().timestamp() < state.open_until
}

/// Return the current runtime state for `asset`.
pub fn get_price_circuit_breaker_state(
    env: &Env,
    asset: &Address,
) -> PriceCircuitBreakerState {
    load_state(env, asset)
}

/// Return the configuration for `asset`.
pub fn get_price_circuit_breaker_config(
    env: &Env,
    asset: &Address,
) -> PriceCircuitBreakerConfig {
    load_config(env, asset)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deposit::DepositDataKey;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let asset = Address::generate(&env);
        env.storage()
            .persistent()
            .set(&DepositDataKey::Admin, &admin);
        (env, admin, asset)
    }

    #[test]
    fn test_first_price_sets_baseline() {
        let (env, _admin, asset) = setup();
        let result = check_price_circuit_breaker(&env, &asset, 1_000_000);
        assert!(result.is_ok());
        let state = get_price_circuit_breaker_state(&env, &asset);
        assert_eq!(state.last_safe_price, 1_000_000);
        assert_eq!(state.open_until, 0);
    }

    #[test]
    fn test_small_price_change_passes() {
        let (env, _admin, asset) = setup();
        // Set baseline
        check_price_circuit_breaker(&env, &asset, 1_000_000).unwrap();
        // 5% increase — within default 20% threshold
        let result = check_price_circuit_breaker(&env, &asset, 1_050_000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_large_price_change_trips_breaker() {
        let (env, _admin, asset) = setup();
        check_price_circuit_breaker(&env, &asset, 1_000_000).unwrap();
        // 30% drop — exceeds default 20% threshold
        let result = check_price_circuit_breaker(&env, &asset, 700_000);
        assert_eq!(result, Err(PriceCircuitBreakerError::DeviationExceeded));

        // Now the breaker is open
        assert!(is_price_circuit_breaker_open(&env, &asset));
        let state = get_price_circuit_breaker_state(&env, &asset);
        assert_eq!(state.trip_count, 1);
    }

    #[test]
    fn test_open_breaker_blocks_further_updates() {
        let (env, _admin, asset) = setup();
        check_price_circuit_breaker(&env, &asset, 1_000_000).unwrap();
        // Trip
        let _ = check_price_circuit_breaker(&env, &asset, 500_000);
        // Even a safe price should be blocked
        let result = check_price_circuit_breaker(&env, &asset, 1_000_000);
        assert_eq!(result, Err(PriceCircuitBreakerError::CircuitOpen));
    }

    #[test]
    fn test_breaker_auto_resets_after_cooldown() {
        let (env, _admin, asset) = setup();
        check_price_circuit_breaker(&env, &asset, 1_000_000).unwrap();
        let _ = check_price_circuit_breaker(&env, &asset, 500_000); // trip

        // Advance past cooldown
        env.ledger()
            .with_mut(|li| li.timestamp += DEFAULT_PRICE_CB_COOLDOWN + 1);

        // Should be allowed again
        let result = check_price_circuit_breaker(&env, &asset, 1_000_000);
        assert!(result.is_ok());
        assert!(!is_price_circuit_breaker_open(&env, &asset));
    }

    #[test]
    fn test_admin_reset_clears_breaker() {
        let (env, admin, asset) = setup();
        check_price_circuit_breaker(&env, &asset, 1_000_000).unwrap();
        let _ = check_price_circuit_breaker(&env, &asset, 300_000); // trip

        assert!(is_price_circuit_breaker_open(&env, &asset));

        reset_price_circuit_breaker(&env, admin.clone(), asset.clone(), 1_000_000).unwrap();
        assert!(!is_price_circuit_breaker_open(&env, &asset));
        let state = get_price_circuit_breaker_state(&env, &asset);
        assert_eq!(state.last_safe_price, 1_000_000);
    }

    #[test]
    fn test_unauthorized_reset_rejected() {
        let (env, _admin, asset) = setup();
        let stranger = Address::generate(&env);
        let result = reset_price_circuit_breaker(&env, stranger, asset, 1_000_000);
        assert_eq!(result, Err(PriceCircuitBreakerError::Unauthorized));
    }

    #[test]
    fn test_configure_custom_threshold() {
        let (env, admin, asset) = setup();
        let config = PriceCircuitBreakerConfig {
            threshold_bps: 500, // 5%
            cooldown_seconds: 600,
            enabled: true,
        };
        configure_price_circuit_breaker(&env, admin.clone(), asset.clone(), config.clone())
            .unwrap();
        let loaded = get_price_circuit_breaker_config(&env, &asset);
        assert_eq!(loaded.threshold_bps, 500);
        assert_eq!(loaded.cooldown_seconds, 600);
    }

    #[test]
    fn test_disabled_breaker_always_passes() {
        let (env, admin, asset) = setup();
        let config = PriceCircuitBreakerConfig {
            threshold_bps: 100,
            cooldown_seconds: 3600,
            enabled: false,
        };
        configure_price_circuit_breaker(&env, admin.clone(), asset.clone(), config).unwrap();
        // Set baseline
        check_price_circuit_breaker(&env, &asset, 1_000_000).unwrap();
        // 90% crash — would normally trip but breaker is disabled
        let result = check_price_circuit_breaker(&env, &asset, 100_000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_config_rejected() {
        let (env, admin, asset) = setup();
        // Threshold too low
        let bad = PriceCircuitBreakerConfig {
            threshold_bps: 50,
            cooldown_seconds: 3600,
            enabled: true,
        };
        assert_eq!(
            configure_price_circuit_breaker(&env, admin.clone(), asset.clone(), bad),
            Err(PriceCircuitBreakerError::InvalidConfig)
        );
        // Cooldown too high
        let bad2 = PriceCircuitBreakerConfig {
            threshold_bps: 2000,
            cooldown_seconds: MAX_PRICE_CB_COOLDOWN + 1,
            enabled: true,
        };
        assert_eq!(
            configure_price_circuit_breaker(&env, admin.clone(), asset.clone(), bad2),
            Err(PriceCircuitBreakerError::InvalidConfig)
        );
    }
}
