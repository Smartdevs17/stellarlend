#![no_std]

//! Manipulation-resistant TWAP oracle.
//!
//! The oracle keeps a time-weighted average price (TWAP) per asset: every
//! observation credits the *previously observed* price for exactly the time
//! that elapsed since the last observation, so a price that held for an hour
//! weighs a thousand times more than one that printed for a second.
//!
//! On top of the plain average it adds three defences, because a TWAP on its
//! own is still a spot-price oracle with extra steps:
//!
//! 1. **Rejection on ingestion.** [`TwapOracle::record_price`] measures the
//!    deviation of the incoming price against the current TWAP. Beyond
//!    `max_deviation_bps` the observation is dropped: the previously accepted
//!    price keeps accruing and the candidate becomes the reference only if the
//!    attacker can sustain it. A single spot print therefore cannot move the
//!    average, no matter how extreme it is.
//! 2. **Bounded credit after a silence.** An observation that arrives long
//!    after the last one cannot inject the stale price with an unbounded
//!    weight. Once the gap exceeds `max_staleness_secs` the accumulator is
//!    reseeded instead, so an old price cannot be resurrected to drag the
//!    average around.
//! 3. **Auditable, bounded history.** Observations live in a fixed-size ring
//!    buffer, so the audit trail cannot grow without limit while still letting
//!    anybody recompute the average off-chain.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, Vec,
};

const BPS_DENOM: i128 = 10_000;
const DEFAULT_WINDOW_SECS: u64 = 1800;
const DEFAULT_MAX_DEVIATION_BPS: i128 = 500;
const DEFAULT_MIN_SAMPLES: u32 = 3;

/// Largest window governance may configure (24 hours).
const MAX_WINDOW_SECS: u64 = 86_400;

/// Observations retained per asset. The ring is fixed so the audit trail and
/// the write cost per observation both stay constant.
const MAX_OBSERVATIONS: u32 = 32;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TwapOracleError {
    Unauthorized = 1,
    InvalidConfig = 2,
    InsufficientSamples = 3,
    PriceManipulationDetected = 4,
    Overflow = 5,
    AlreadyInitialized = 6,
    NotInitialized = 7,
    InvalidPrice = 8,
    Stale = 9,
    RingOverflow = 10,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TwapConfig {
    pub admin: Address,
    /// Length of the averaging window. `get_twap` only returns a usable value
    /// once the accumulator covers this much time.
    pub window_secs: u64,
    /// Deviation from the TWAP beyond which an observation is rejected.
    pub max_deviation_bps: i128,
    /// Observations that must have been accepted before the TWAP is served
    /// without a fallback flag.
    pub min_samples: u32,
    /// Longest gap an observation may bridge. Beyond it the accumulator is
    /// reseeded instead of crediting the stale price.
    pub max_staleness_secs: u64,
    pub initialized: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceObservation {
    pub price: i128,
    pub timestamp: u64,
    pub block: u32,
    /// Time the *previous* price was credited for by this observation.
    pub weighted_seconds: u64,
    /// False when the price was rejected as manipulated.
    pub accepted: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TwapAccumulator {
    /// Sum of `price * seconds`, i.e. the numerator of the average.
    pub price_sum: i128,
    /// Seconds the numerator covers. The denominator of the average.
    pub total_time: u64,
    /// `price_sum / total_time`, the current time-weighted average.
    pub twap: i128,
    /// Observations that were accepted into the average.
    pub sample_count: u32,
    /// Observations that were rejected as manipulated.
    pub rejected_count: u32,
    /// True while at least one rejection is unacknowledged, i.e. the feed is
    /// under attack and consumers should tighten their risk parameters.
    pub manipulation_pending: bool,
    /// Timestamp the accumulator was last updated at.
    pub last_update: u64,
    /// Price currently accruing.
    pub last_price: i128,
}

/// Manipulation-resistant view of an asset.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TwapResult {
    pub twap: i128,
    pub spot_price: i128,
    pub deviation_bps: i128,
    pub manipulation_detected: bool,
    pub used_fallback: bool,
    pub sample_count: u32,
    /// False once the newest observation is older than
    /// [`TwapConfig::max_staleness_secs`].
    pub fresh: bool,
    /// Seconds of history the average covers.
    pub coverage_secs: u64,
    pub window_secs: u64,
    pub window_coverage_bps: i128,
    pub rejected_count: u32,
}

/// Read-only health snapshot of an asset's TWAP.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TwapHealth {
    pub twap: i128,
    pub spot_price: i128,
    pub sample_count: u32,
    pub rejected_count: u32,
    pub coverage_secs: u64,
    pub window_secs: u64,
    pub last_update: u64,
    pub age_secs: u64,
    pub fresh: bool,
    pub manipulation_pending: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum TwapKey {
    Config,
    Accumulator(Address),
    Observation(Address, u32),
    /// Next slot to write in the ring buffer.
    ObservationCursor(Address),
    /// Number of live entries in the ring buffer.
    ObservationLen(Address),
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceRecordedEvent {
    #[topic]
    pub asset: Address,
    pub price: i128,
    pub twap: i128,
    pub weighted_seconds: u64,
    pub sample_count: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceRejectedEvent {
    #[topic]
    pub asset: Address,
    pub price: i128,
    pub twap: i128,
    pub deviation_bps: i128,
    pub max_deviation_bps: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccumulatorReseededEvent {
    #[topic]
    pub asset: Address,
    #[topic]
    pub reason: soroban_sdk::Symbol,
    pub price: i128,
    pub stale_secs: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceForceRecordedEvent {
    #[topic]
    pub asset: Address,
    #[topic]
    pub operator: Address,
    pub price: i128,
    pub twap: i128,
    pub deviation_bps: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigUpdatedEvent {
    #[topic]
    pub admin: Address,
    pub window_secs: u64,
    pub max_deviation_bps: i128,
    pub min_samples: u32,
    pub max_staleness_secs: u64,
}

#[contract]
pub struct TwapOracle;

#[contractimpl]
impl TwapOracle {
    /// Seed the configuration. Defaults: 30 minute window, 5 % deviation band,
    /// three samples, and a staleness limit of twice the window.
    pub fn initialize(env: Env, admin: Address) -> Result<(), TwapOracleError> {
        if env.storage().instance().has(&TwapKey::Config) {
            return Err(TwapOracleError::AlreadyInitialized);
        }
        admin.require_auth();

        let config = TwapConfig {
            admin: admin.clone(),
            window_secs: DEFAULT_WINDOW_SECS,
            max_deviation_bps: DEFAULT_MAX_DEVIATION_BPS,
            min_samples: DEFAULT_MIN_SAMPLES,
            max_staleness_secs: DEFAULT_WINDOW_SECS * 2,
            initialized: true,
        };

        env.storage().instance().set(&TwapKey::Config, &config);
        ConfigUpdatedEvent {
            admin,
            window_secs: config.window_secs,
            max_deviation_bps: config.max_deviation_bps,
            min_samples: config.min_samples,
            max_staleness_secs: config.max_staleness_secs,
        }
        .publish(&env);
        Ok(())
    }

    /// Governance entry point. `max_staleness_secs` of `0` derives the limit
    /// from the window (twice the window).
    pub fn set_config(
        env: Env,
        admin: Address,
        window_secs: u64,
        max_deviation_bps: i128,
        min_samples: u32,
        max_staleness_secs: u64,
    ) -> Result<(), TwapOracleError> {
        admin.require_auth();

        let config: TwapConfig = env
            .storage()
            .instance()
            .get(&TwapKey::Config)
            .ok_or(TwapOracleError::NotInitialized)?;

        if admin != config.admin {
            return Err(TwapOracleError::Unauthorized);
        }

        let staleness = if max_staleness_secs == 0 {
            window_secs * 2
        } else {
            max_staleness_secs
        };

        if window_secs == 0
            || window_secs > MAX_WINDOW_SECS
            || max_deviation_bps <= 0
            || max_deviation_bps > BPS_DENOM
            || min_samples == 0
            || min_samples as u64 > MAX_OBSERVATIONS as u64
            || staleness < window_secs
            || staleness > MAX_WINDOW_SECS * 2
        {
            return Err(TwapOracleError::InvalidConfig);
        }

        let updated = TwapConfig {
            admin: config.admin.clone(),
            window_secs,
            max_deviation_bps,
            min_samples,
            max_staleness_secs: staleness,
            initialized: true,
        };

        env.storage().instance().set(&TwapKey::Config, &updated);
        ConfigUpdatedEvent {
            admin: config.admin,
            window_secs,
            max_deviation_bps,
            min_samples,
            max_staleness_secs: staleness,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_config(env: Env) -> Option<TwapConfig> {
        env.storage().instance().get(&TwapKey::Config)
    }

    /// Record a spot price.
    ///
    /// The observation is credited for the time that elapsed since the previous
    /// one, then becomes the price that accrues going forward. A price beyond
    /// the deviation band is rejected: the accumulator keeps the last accepted
    /// price accruing, the rejection is counted, and an event is published.
    /// Returns `Ok(false)` for an accepted price and `Ok(true)` when the
    /// candidate was rejected as manipulated.
    pub fn record_price(env: Env, asset: Address, price: i128) -> Result<bool, TwapOracleError> {
        let config: TwapConfig = env
            .storage()
            .instance()
            .get(&TwapKey::Config)
            .ok_or(TwapOracleError::NotInitialized)?;

        config.admin.require_auth();

        if price <= 0 {
            return Err(TwapOracleError::InvalidPrice);
        }

        let now = env.ledger().timestamp();
        let mut acc: TwapAccumulator = env
            .storage()
            .persistent()
            .get(&TwapKey::Accumulator(asset.clone()))
            .unwrap_or(TwapAccumulator {
                price_sum: 0,
                total_time: 0,
                twap: 0,
                sample_count: 0,
                rejected_count: 0,
                manipulation_pending: false,
                last_update: 0,
                last_price: 0,
            });

        // Manipulation check against the current average. The average itself
        // only moves with time-weighted evidence, so it cannot be talked up
        // by a single print.
        let deviation = if acc.twap > 0 {
            deviation_bps(price, acc.twap)
        } else {
            0
        };
        let accepted = acc.sample_count == 0 || deviation <= config.max_deviation_bps;

        let weighted_seconds = if accepted {
            credit(&env, &mut acc, price, now, &config, &asset)
        } else {
            // A rejected observation still marks the time as covered by the
            // price that was already accepted, so the average cannot be frozen
            // out of the window by a stream of garbage.
            bridge_elapsed(&env, &mut acc, now, &config, &asset)
        };

        if !accepted {
            acc.rejected_count = acc.rejected_count.saturating_add(1);
            acc.manipulation_pending = true;
        }

        env.storage()
            .persistent()
            .set(&TwapKey::Accumulator(asset.clone()), &acc);
        write_observation(
            &env,
            &asset,
            PriceObservation {
                price,
                timestamp: now,
                block: env.ledger().sequence(),
                weighted_seconds,
                accepted,
            },
        );

        if accepted {
            PriceRecordedEvent {
                asset: asset.clone(),
                price,
                twap: acc.twap,
                weighted_seconds,
                sample_count: acc.sample_count,
            }
            .publish(&env);
        } else {
            PriceRejectedEvent {
                asset: asset.clone(),
                price,
                twap: acc.twap,
                deviation_bps: deviation,
                max_deviation_bps: config.max_deviation_bps,
            }
            .publish(&env);
        }

        Ok(accepted)
    }

    /// Record a price that the deviation check would reject.
    ///
    /// The escape hatch for a genuine repricing: the operator is recorded on
    /// chain, and the candidate is seeded as a fresh observation instead of
    /// being blended into the old average, so history is not silently
    /// rewritten.
    pub fn force_record_price(
        env: Env,
        asset: Address,
        price: i128,
    ) -> Result<(), TwapOracleError> {
        let config: TwapConfig = env
            .storage()
            .instance()
            .get(&TwapKey::Config)
            .ok_or(TwapOracleError::NotInitialized)?;

        config.admin.require_auth();
        if price <= 0 {
            return Err(TwapOracleError::InvalidPrice);
        }

        let now = env.ledger().timestamp();
        let previous_twap: i128 = env
            .storage()
            .persistent()
            .get(&TwapKey::Accumulator(asset.clone()))
            .map(|acc: TwapAccumulator| acc.twap)
            .unwrap_or(0);
        let deviation = if previous_twap > 0 {
            deviation_bps(price, previous_twap)
        } else {
            0
        };

        let acc = TwapAccumulator {
            price_sum: price,
            total_time: 1,
            twap: price,
            sample_count: 1,
            rejected_count: 0,
            manipulation_pending: false,
            last_update: now,
            last_price: price,
        };
        let weighted_seconds = acc.total_time;
        env.storage()
            .persistent()
            .set(&TwapKey::Accumulator(asset.clone()), &acc);
        write_observation(
            &env,
            &asset,
            PriceObservation {
                price,
                timestamp: now,
                block: env.ledger().sequence(),
                weighted_seconds,
                accepted: true,
            },
        );

        AccumulatorReseededEvent {
            asset: asset.clone(),
            reason: soroban_sdk::Symbol::new(&env, "forced"),
            price,
            stale_secs: 0,
        }
        .publish(&env);
        PriceForceRecordedEvent {
            asset,
            operator: config.admin,
            price,
            twap: acc.twap,
            deviation_bps: deviation,
        }
        .publish(&env);
        Ok(())
    }

    /// The manipulation-resistant price for an asset.
    ///
    /// `used_fallback` is set when the average is not yet trustworthy: too few
    /// samples, less time covered than the window asks for, a stale feed, or a
    /// rejected observation that nobody has looked at. `get_liquidation_price`
    /// refuses to fall back to spot in those cases.
    pub fn get_twap(env: Env, asset: Address) -> TwapResult {
        let config: TwapConfig = match env.storage().instance().get(&TwapKey::Config) {
            Some(c) => c,
            None => return empty_result(0, 0),
        };
        let acc: TwapAccumulator = match env
            .storage()
            .persistent()
            .get(&TwapKey::Accumulator(asset.clone()))
        {
            Some(a) => a,
            None => return empty_result(config.window_secs, 0),
        };

        Self::summarize(&env, &config, &acc)
    }

    /// TWAP for liquidation, or the spot price when the average is unusable.
    ///
    /// A stale or manipulated average is not silently replaced by spot: the
    /// caller sees `used_fallback` and decides. Callers that cannot accept a
    /// spot price should use `get_twap` and require `used_fallback == false`.
    pub fn get_liquidation_price(env: Env, asset: Address) -> TwapResult {
        let result = Self::get_twap(env, asset);

        if result.used_fallback {
            TwapResult {
                twap: result.spot_price,
                ..result
            }
        } else {
            result
        }
    }

    /// Reject a spot price that deviates from the TWAP.
    ///
    /// The counterpart of `record_price` for consumers that receive a spot
    /// price out of band (a DEX, an AMM, a lending market's own feed).
    pub fn check_deviation(
        env: Env,
        asset: Address,
        spot_price: i128,
    ) -> Result<TwapResult, TwapOracleError> {
        let config: TwapConfig = env
            .storage()
            .instance()
            .get(&TwapKey::Config)
            .ok_or(TwapOracleError::NotInitialized)?;

        let acc: TwapAccumulator = env
            .storage()
            .persistent()
            .get(&TwapKey::Accumulator(asset.clone()))
            .ok_or(TwapOracleError::InsufficientSamples)?;

        if acc.sample_count < config.min_samples {
            return Err(TwapOracleError::InsufficientSamples);
        }

        if acc.twap <= 0 || spot_price <= 0 {
            return Err(TwapOracleError::InvalidPrice);
        }

        let now = env.ledger().timestamp();
        let age = now.saturating_sub(acc.last_update);
        if age > config.max_staleness_secs {
            return Err(TwapOracleError::Stale);
        }

        let deviation_bps = deviation_bps(spot_price, acc.twap);
        if deviation_bps > config.max_deviation_bps {
            return Err(TwapOracleError::PriceManipulationDetected);
        }

        let mut result = Self::summarize(&env, &config, &acc);
        result.spot_price = spot_price;
        result.deviation_bps = deviation_bps;
        result.manipulation_detected = false;
        result.used_fallback = false;
        Ok(result)
    }

    /// Health snapshot for keepers and monitors.
    pub fn get_health(env: Env, asset: Address) -> Option<TwapHealth> {
        let config: TwapConfig = env.storage().instance().get(&TwapKey::Config)?;
        let acc: TwapAccumulator = env
            .storage()
            .persistent()
            .get(&TwapKey::Accumulator(asset.clone()))?;
        let now = env.ledger().timestamp();
        let age = now.saturating_sub(acc.last_update);
        Some(TwapHealth {
            twap: acc.twap,
            spot_price: acc.last_price,
            sample_count: acc.sample_count,
            rejected_count: acc.rejected_count,
            coverage_secs: acc.total_time,
            window_secs: config.window_secs,
            last_update: acc.last_update,
            age_secs: age,
            fresh: age <= config.max_staleness_secs,
            manipulation_pending: acc.manipulation_pending,
        })
    }

    /// The raw accumulator, for integrators that need the exact numerator and
    /// denominator.
    pub fn get_accumulator(env: Env, asset: Address) -> Option<TwapAccumulator> {
        env.storage()
            .persistent()
            .get(&TwapKey::Accumulator(asset.clone()))
    }

    /// Observations in chronological order, oldest first, capped at
    /// [`MAX_OBSERVATIONS`].
    pub fn get_observations(env: Env, asset: Address) -> Vec<PriceObservation> {
        let mut out = Vec::new(&env);
        let len: u32 = env
            .storage()
            .persistent()
            .get(&TwapKey::ObservationLen(asset.clone()))
            .unwrap_or(0);
        let cursor: u32 = env
            .storage()
            .persistent()
            .get(&TwapKey::ObservationCursor(asset.clone()))
            .unwrap_or(0);

        // The cursor points at the next write, so the oldest entry is the one
        // `len` slots behind it.
        let start = (cursor + MAX_OBSERVATIONS - len) % MAX_OBSERVATIONS;
        let mut i = 0u32;
        while i < len {
            let idx = (start + i) % MAX_OBSERVATIONS;
            if let Some(obs) = env
                .storage()
                .persistent()
                .get(&TwapKey::Observation(asset.clone(), idx))
            {
                out.push_back(obs);
            }
            i += 1;
        }
        out
    }

    /// Drop an asset's history. Governance use, e.g. after a token migration.
    pub fn reset_asset(env: Env, asset: Address) -> Result<(), TwapOracleError> {
        let config: TwapConfig = env
            .storage()
            .instance()
            .get(&TwapKey::Config)
            .ok_or(TwapOracleError::NotInitialized)?;
        config.admin.require_auth();

        env.storage()
            .persistent()
            .remove(&TwapKey::Accumulator(asset.clone()));
        env.storage()
            .persistent()
            .remove(&TwapKey::ObservationCursor(asset.clone()));
        env.storage()
            .persistent()
            .remove(&TwapKey::ObservationLen(asset.clone()));
        let mut i = 0u32;
        while i < MAX_OBSERVATIONS {
            env.storage()
                .persistent()
                .remove(&TwapKey::Observation(asset.clone(), i));
            i += 1;
        }

        AccumulatorReseededEvent {
            asset,
            reason: soroban_sdk::Symbol::new(&env, "reset"),
            price: 0,
            stale_secs: 0,
        }
        .publish(&env);
        Ok(())
    }

    // ── Internals exposed for the contract interface ───────────────────────

    /// Exposed so `get_twap` and `check_deviation` agree on what "usable"
    /// means.
    #[doc(hidden)]
    pub fn summarize(env: &Env, config: &TwapConfig, acc: &TwapAccumulator) -> TwapResult {
        let now = env.ledger().timestamp();
        let age = now.saturating_sub(acc.last_update);
        let fresh = age <= config.max_staleness_secs;
        let coverage = acc.total_time;
        let window_coverage_bps = if config.window_secs == 0 {
            0
        } else {
            (coverage as i128).saturating_mul(BPS_DENOM) / config.window_secs as i128
        };

        let spot_price = acc.last_price;
        let deviation = if acc.twap > 0 && spot_price > 0 {
            deviation_bps(spot_price, acc.twap)
        } else {
            0
        };

        let too_few_samples = acc.sample_count < config.min_samples;
        let thin_window = coverage < config.window_secs;
        // A rejected observation means the feed is being pushed around; the
        // average is still resistant, but consumers should know.
        let attacked = acc.manipulation_pending;

        TwapResult {
            twap: acc.twap,
            spot_price,
            deviation_bps: deviation,
            manipulation_detected: attacked || deviation > config.max_deviation_bps,
            used_fallback: acc.twap <= 0 || too_few_samples || thin_window || !fresh || attacked,
            sample_count: acc.sample_count,
            fresh,
            coverage_secs: coverage,
            window_secs: config.window_secs,
            window_coverage_bps,
            rejected_count: acc.rejected_count,
        }
    }
}

/// Deviation in basis points of `price` from `reference`.
fn deviation_bps(price: i128, reference: i128) -> i128 {
    if reference <= 0 || price <= 0 {
        return 0;
    }
    let diff = if price > reference {
        price - reference
    } else {
        reference - price
    };
    diff.saturating_mul(BPS_DENOM) / reference
}

/// Credit the previous price for the elapsed time, then adopt `price`.
///
/// Returns the number of seconds the accepted price was weighted by.
fn credit(
    env: &Env,
    acc: &mut TwapAccumulator,
    price: i128,
    now: u64,
    config: &TwapConfig,
    asset: &Address,
) -> u64 {
    if acc.sample_count == 0 {
        // Seed: the price is treated as having been in effect for one second
        // so that the average is defined immediately, without giving it
        // influence over anything that happened before it existed.
        acc.price_sum = price;
        acc.total_time = 1;
        acc.twap = price;
        acc.sample_count = 1;
        acc.rejected_count = 0;
        acc.manipulation_pending = false;
        acc.last_update = now;
        acc.last_price = price;
        return 1;
    }

    let elapsed = now.saturating_sub(acc.last_update);
    if elapsed == 0 {
        // Same ledger second: there is no time to weight, but the observation
        // is still a sample and the freshest price available.
        acc.sample_count = acc.sample_count.saturating_add(1);
        acc.manipulation_pending = false;
        acc.last_price = price;
        return 0;
    }

    if elapsed > config.max_staleness_secs {
        // The gap is too long to attribute to the old price. Reseed instead of
        // crediting a stale quote with an unbounded weight.
        acc.price_sum = price;
        acc.total_time = 1;
        acc.twap = price;
        acc.sample_count = 1;
        acc.rejected_count = 0;
        acc.manipulation_pending = false;
        acc.last_update = now;
        acc.last_price = price;
        AccumulatorReseededEvent {
            asset: asset.clone(),
            reason: soroban_sdk::Symbol::new(env, "stale"),
            price,
            stale_secs: elapsed,
        }
        .publish(env);
        return 1;
    }

    // The heart of a TWAP: the previous price owned the interval, so it is
    // credited for exactly the time it was in effect.
    let weighted = acc.last_price.saturating_mul(elapsed as i128);
    acc.price_sum = acc.price_sum.saturating_add(weighted);
    acc.total_time = acc.total_time.saturating_add(elapsed);
    acc.sample_count = acc.sample_count.saturating_add(1);
    // The feed answered with a price inside the band, so whatever was pushing
    // it around has stopped.
    acc.manipulation_pending = false;
    acc.last_update = now;
    acc.last_price = price;
    if acc.total_time > 0 {
        acc.twap = acc.price_sum / acc.total_time as i128;
    }
    elapsed
}

/// Credit a rejected observation's interval to the price already accepted, so
/// repeated garbage cannot stop the window from filling.
fn bridge_elapsed(
    env: &Env,
    acc: &mut TwapAccumulator,
    now: u64,
    config: &TwapConfig,
    asset: &Address,
) -> u64 {
    if acc.sample_count == 0 {
        acc.last_update = now;
        acc.last_price = 0;
        return 0;
    }
    let elapsed = now.saturating_sub(acc.last_update);
    if elapsed == 0 {
        return 0;
    }
    if elapsed > config.max_staleness_secs {
        acc.last_update = now;
        AccumulatorReseededEvent {
            asset: asset.clone(),
            reason: soroban_sdk::Symbol::new(env, "stale"),
            price: acc.last_price,
            stale_secs: elapsed,
        }
        .publish(env);
        return 0;
    }
    let weighted = acc.last_price.saturating_mul(elapsed as i128);
    acc.price_sum = acc.price_sum.saturating_add(weighted);
    acc.total_time = acc.total_time.saturating_add(elapsed);
    acc.last_update = now;
    if acc.total_time > 0 {
        acc.twap = acc.price_sum / acc.total_time as i128;
    }
    elapsed
}

/// Persist an observation into the fixed-size ring buffer.
fn write_observation(env: &Env, asset: &Address, obs: PriceObservation) {
    let cursor: u32 = env
        .storage()
        .persistent()
        .get(&TwapKey::ObservationCursor(asset.clone()))
        .unwrap_or(0);
    let len: u32 = env
        .storage()
        .persistent()
        .get(&TwapKey::ObservationLen(asset.clone()))
        .unwrap_or(0);

    env.storage()
        .persistent()
        .set(&TwapKey::Observation(asset.clone(), cursor), &obs);
    env.storage().persistent().set(
        &TwapKey::ObservationCursor(asset.clone()),
        &((cursor + 1) % MAX_OBSERVATIONS),
    );
    if len < MAX_OBSERVATIONS {
        env.storage()
            .persistent()
            .set(&TwapKey::ObservationLen(asset.clone()), &(len + 1));
    }
}

/// Empty result for an unconfigured contract or an asset without history.
fn empty_result(window_secs: u64, coverage_secs: u64) -> TwapResult {
    TwapResult {
        twap: 0,
        spot_price: 0,
        deviation_bps: 0,
        manipulation_detected: false,
        used_fallback: true,
        sample_count: 0,
        fresh: false,
        coverage_secs,
        window_secs,
        window_coverage_bps: 0,
        rejected_count: 0,
    }
}

#[cfg(test)]
mod tests;
