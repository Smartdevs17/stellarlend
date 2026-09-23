#![no_std]

use soroban_sdk::{contracttype, Env};
use stellarlend_safe_math::{bps_mul, safe_add, safe_div, safe_mul, MathError, WAD};

/// Fixed-point scale used by the cumulative interest index.
pub const INTEREST_INDEX_SCALE: i128 = WAD;

#[contracttype]
#[derive(Clone)]
enum InterestCacheKey {
    GlobalIndex,
}

/// Persisted cumulative interest state.
///
/// Positions only need to retain the index observed at their last mutation.
/// Accrual then advances this global index for the elapsed segment instead of
/// recomputing every position's full history.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestIndexCache {
    pub index: i128,
    pub last_update: u64,
    pub last_ledger: u32,
    pub rate_bps: i128,
}

impl InterestIndexCache {
    fn fresh(env: &Env, rate_bps: i128) -> Self {
        Self {
            index: INTEREST_INDEX_SCALE,
            last_update: env.ledger().timestamp(),
            last_ledger: env.ledger().sequence(),
            rate_bps,
        }
    }
}

pub struct InterestRateModel {
    pub base_rate: i128,
    pub slope1: i128,
    pub slope2: i128,
    pub optimal_utilization: i128,
}

impl InterestRateModel {
    /// Variable-slope borrow rate in basis points.
    ///
    /// Below kink:  `base_rate + utilization × slope1 / 10 000`
    /// Above kink:  `base_rate + kink × slope1 / 10 000 + excess × slope2 / 10 000`
    pub fn calculate_borrow_rate(&self, utilization: i128) -> Result<i128, MathError> {
        if utilization <= self.optimal_utilization {
            let inc = safe_mul(utilization, self.slope1).and_then(|v| safe_div(v, 10_000))?;
            safe_add(self.base_rate, inc)
        } else {
            let excess = safe_add(utilization, -self.optimal_utilization)?;
            let kink_component = safe_mul(self.optimal_utilization, self.slope1)
                .and_then(|v| safe_div(v, 10_000))?;
            let excess_component =
                safe_mul(excess, self.slope2).and_then(|v| safe_div(v, 10_000))?;
            safe_add(self.base_rate, kink_component).and_then(|v| safe_add(v, excess_component))
        }
    }

    /// Supply rate: `borrow_rate × (10 000 − reserve_factor) / 10 000 × utilization / 10 000`
    pub fn calculate_supply_rate(
        &self,
        borrow_rate: i128,
        utilization: i128,
        reserve_factor: i128,
    ) -> Result<i128, MathError> {
        let net_factor = safe_add(10_000, -reserve_factor)?;
        let rate_to_pool = safe_mul(borrow_rate, net_factor).and_then(|v| safe_div(v, 10_000))?;
        safe_mul(rate_to_pool, utilization).and_then(|v| safe_div(v, 10_000))
    }
}

/// Utilization rate in basis points: `total_borrows × 10 000 / total_supply`.
pub fn calculate_utilization(total_borrows: i128, total_supply: i128) -> Result<i128, MathError> {
    if total_supply == 0 {
        return Ok(0);
    }
    safe_mul(total_borrows, 10_000).and_then(|v| safe_div(v, total_supply))
}

/// Simple interest via I256 intermediates: `principal × rate × elapsed / (SPY × 10 000)`.
///
/// Replaces the old `unwrap_or(0)` implementation which silently returned 0
/// on overflow.  Now returns `Err(MathError::Overflow)` for very large inputs.
pub fn accrue_interest(
    env: &Env,
    principal: i128,
    rate: i128,
    time_elapsed: u64,
) -> Result<i128, MathError> {
    if time_elapsed == 0 {
        return Ok(0);
    }
    stellarlend_safe_math::simple_interest(env, principal, rate, time_elapsed)
}

/// Load the persisted cumulative index, or return a fresh in-memory index.
pub fn get_interest_cache(env: &Env, initial_rate_bps: i128) -> InterestIndexCache {
    env.storage()
        .persistent()
        .get(&InterestCacheKey::GlobalIndex)
        .unwrap_or_else(|| InterestIndexCache::fresh(env, initial_rate_bps))
}

/// Advance and persist the cumulative index for only the newly elapsed time.
///
/// The elapsed segment is charged at the previously cached rate. The supplied
/// `current_rate_bps` becomes the rate for the next segment, so rate changes do
/// not retroactively affect already elapsed time. Repeated calls in the same
/// ledger with an unchanged rate perform no storage write.
pub fn update_interest_cache(
    env: &Env,
    current_rate_bps: i128,
) -> Result<InterestIndexCache, MathError> {
    let stored: Option<InterestIndexCache> = env
        .storage()
        .persistent()
        .get(&InterestCacheKey::GlobalIndex);
    let mut cache = stored
        .clone()
        .unwrap_or_else(|| InterestIndexCache::fresh(env, current_rate_bps));
    let now = env.ledger().timestamp();
    let ledger = env.ledger().sequence();
    let mut changed = stored.is_none();

    // Never rewind the index if ledger time moves backwards during a reorg.
    if ledger != cache.last_ledger && now > cache.last_update {
        let elapsed = now - cache.last_update;
        let delta = accrue_interest(env, cache.index, cache.rate_bps, elapsed)?;
        cache.index = safe_add(cache.index, delta)?;
        cache.last_update = now;
        cache.last_ledger = ledger;
        changed = true;
    }

    if cache.rate_bps != current_rate_bps {
        cache.rate_bps = current_rate_bps;
        changed = true;
    }

    if changed {
        env.storage()
            .persistent()
            .set(&InterestCacheKey::GlobalIndex, &cache);
    }

    Ok(cache)
}

/// Preview the up-to-date index without writing to storage.
pub fn preview_interest_index(env: &Env, current_rate_bps: i128) -> Result<i128, MathError> {
    let cache = get_interest_cache(env, current_rate_bps);
    let now = env.ledger().timestamp();
    if now <= cache.last_update {
        return Ok(cache.index);
    }

    let delta = accrue_interest(env, cache.index, cache.rate_bps, now - cache.last_update)?;
    safe_add(cache.index, delta)
}

/// Calculate a position's accrued interest from two cumulative-index snapshots.
pub fn interest_from_index(
    principal: i128,
    entry_index: i128,
    current_index: i128,
) -> Result<i128, MathError> {
    if principal <= 0 || entry_index <= 0 || current_index <= entry_index {
        return Ok(0);
    }
    let growth = safe_add(current_index, -entry_index)?;
    safe_mul(principal, growth).and_then(|value| safe_div(value, entry_index))
}

/// Compound interest over discrete periods using bps_mul for each step.
pub fn compound_interest(principal: i128, rate: i128, periods: u64) -> Result<i128, MathError> {
    let mut result = principal;
    for _ in 0..periods {
        let interest = bps_mul(result, rate)?;
        result = safe_add(result, interest)?;
    }
    safe_add(result, -principal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Ledger as _, Env};

    #[contract]
    struct CacheTestContract;

    #[contractimpl]
    impl CacheTestContract {
        pub fn initialize(_env: Env) {}
    }

    #[test]
    fn test_utilization_calculation() {
        assert_eq!(calculate_utilization(50_000, 100_000), Ok(5_000));
        assert_eq!(calculate_utilization(80_000, 100_000), Ok(8_000));
        assert_eq!(calculate_utilization(0, 100_000), Ok(0));
        assert_eq!(calculate_utilization(100_000, 0), Ok(0));
    }

    #[test]
    fn test_interest_rate_model_below_kink() {
        let model = InterestRateModel {
            base_rate: 200,
            slope1: 400,
            slope2: 6_000,
            optimal_utilization: 8_000,
        };

        let rate_at_50 = model.calculate_borrow_rate(5_000).unwrap();
        assert!(rate_at_50 > model.base_rate);

        let rate_at_90 = model.calculate_borrow_rate(9_000).unwrap();
        assert!(rate_at_90 > rate_at_50);
    }

    #[test]
    fn test_accrue_interest_annual() {
        let env = Env::default();
        let interest =
            accrue_interest(&env, 100_000, 500, stellarlend_safe_math::SECONDS_PER_YEAR).unwrap();
        assert_eq!(interest, 5_000);
    }

    #[test]
    fn test_accrue_interest_zero_elapsed() {
        let env = Env::default();
        assert_eq!(accrue_interest(&env, 1_000_000, 500, 0), Ok(0));
    }

    #[test]
    fn test_utilization_overflow_is_err() {
        // total_borrows near MAX: safe_mul(MAX, 10_000) overflows.
        let result = calculate_utilization(i128::MAX, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_borrow_rate_overflow_inputs_err() {
        let model = InterestRateModel {
            base_rate: i128::MAX,
            slope1: i128::MAX,
            slope2: i128::MAX,
            optimal_utilization: 8_000,
        };
        assert!(model.calculate_borrow_rate(5_000).is_err());
    }

    #[test]
    fn test_supply_rate_zero_pool() {
        let model = InterestRateModel {
            base_rate: 200,
            slope1: 400,
            slope2: 6_000,
            optimal_utilization: 8_000,
        };
        // reserve_factor = 10_000 → net_factor = 0 → supply rate = 0.
        let rate = model.calculate_supply_rate(500, 5_000, 10_000).unwrap();
        assert_eq!(rate, 0);
    }

    #[test]
    fn test_interest_from_index() {
        let grown_index = INTEREST_INDEX_SCALE + INTEREST_INDEX_SCALE / 20;
        assert_eq!(
            interest_from_index(1_000_000, INTEREST_INDEX_SCALE, grown_index),
            Ok(50_000)
        );
    }

    #[test]
    fn test_cache_updates_incrementally_and_switches_rate() {
        let env = Env::default();
        let contract_id = env.register(CacheTestContract, ());

        env.as_contract(&contract_id, || {
            let initial = update_interest_cache(&env, 500).unwrap();
            assert_eq!(initial.index, INTEREST_INDEX_SCALE);

            env.ledger().set_sequence_number(2);
            env.ledger()
                .set_timestamp(stellarlend_safe_math::SECONDS_PER_YEAR);
            let accrued = update_interest_cache(&env, 1_000).unwrap();
            assert_eq!(
                accrued.index,
                INTEREST_INDEX_SCALE + INTEREST_INDEX_SCALE * 5 / 100
            );
            assert_eq!(accrued.rate_bps, 1_000);

            // Same-ledger calls reuse the cached value.
            assert_eq!(update_interest_cache(&env, 1_000).unwrap(), accrued);
        });
    }

    #[test]
    fn test_preview_does_not_persist() {
        let env = Env::default();
        let contract_id = env.register(CacheTestContract, ());

        env.as_contract(&contract_id, || {
            update_interest_cache(&env, 500).unwrap();
            env.ledger().set_sequence_number(2);
            env.ledger()
                .set_timestamp(stellarlend_safe_math::SECONDS_PER_YEAR);

            let preview = preview_interest_index(&env, 500).unwrap();
            assert_eq!(
                preview,
                INTEREST_INDEX_SCALE + INTEREST_INDEX_SCALE * 5 / 100
            );
            assert_eq!(
                get_interest_cache(&env, 500).index,
                INTEREST_INDEX_SCALE
            );
        });
    }
}
