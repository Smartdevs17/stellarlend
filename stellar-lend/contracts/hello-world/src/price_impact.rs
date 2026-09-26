//! # Price Impact Calculator for Large Trades
//!
//! Computes the estimated price impact of a trade against the protocol's
//! liquidity pools using a constant-product AMM model approximation.
//!
//! ## Formula
//! For a trade of `amount_in` against a pool of `reserve_in` / `reserve_out`
//! the constant-product formula gives:
//!
//! ```text
//! amount_out = (reserve_out * amount_in) / (reserve_in + amount_in)
//! price_impact_bps = (1 - amount_out / (reserve_out * amount_in / reserve_in)) * 10_000
//! ```
//!
//! Equivalently:
//! ```text
//! mid_price = reserve_out / reserve_in
//! exec_price = amount_out / amount_in
//! price_impact_bps = (mid_price - exec_price) / mid_price * 10_000
//! ```
//!
//! ## Safeguards
//! - Returns `Err(ExcessiveImpact)` when impact exceeds `max_impact_bps`.
//! - All arithmetic uses checked operations to prevent overflow.
//! - Inputs are validated before computation.

use soroban_sdk::{contracterror, contracttype, Address, Env};

// ─── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PriceImpactError {
    /// Input amount must be positive
    InvalidAmount = 1,
    /// Pool reserves must be positive
    InvalidReserves = 2,
    /// Arithmetic overflow
    Overflow = 3,
    /// Computed price impact exceeds the caller-specified maximum
    ExcessiveImpact = 4,
    /// Pool has insufficient output reserve for the trade
    InsufficientLiquidity = 5,
}

// ─── Types ───────────────────────────────────────────────────────────────────

/// Parameters for a single-leg trade price-impact query
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TradeParams {
    /// Input amount (in the smallest unit of `token_in`)
    pub amount_in: i128,
    /// Pool's current reserve of the input token
    pub reserve_in: i128,
    /// Pool's current reserve of the output token
    pub reserve_out: i128,
    /// Maximum acceptable price impact in basis points (0 = no check)
    pub max_impact_bps: i128,
}

/// Result of a price impact calculation
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceImpactResult {
    /// Estimated output amount after the trade
    pub amount_out: i128,
    /// Price impact in basis points (1 bps = 0.01%)
    pub impact_bps: i128,
    /// Effective execution price (amount_out per amount_in, scaled by BPS_SCALE)
    pub exec_price_scaled: i128,
    /// Mid price before the trade (scaled by BPS_SCALE)
    pub mid_price_scaled: i128,
}

/// Storage key for pool liquidity snapshots
#[contracttype]
#[derive(Clone)]
pub enum PriceImpactKey {
    /// (token_in, token_out) → (reserve_in, reserve_out)
    PoolReserves(Address, Address),
}

/// Scale factor for price arithmetic (avoids floating point)
pub const BPS_SCALE: i128 = 10_000;

// ─── Core calculation (pure, no storage) ─────────────────────────────────────

/// Calculate price impact for a trade defined by `params`.
///
/// This is a pure function — no storage reads or writes.  It can be called
/// from both contract entrypoints and off-chain simulation.
pub fn calculate_price_impact(
    params: &TradeParams,
) -> Result<PriceImpactResult, PriceImpactError> {
    if params.amount_in <= 0 {
        return Err(PriceImpactError::InvalidAmount);
    }
    if params.reserve_in <= 0 || params.reserve_out <= 0 {
        return Err(PriceImpactError::InvalidReserves);
    }

    // Constant-product formula: amount_out = reserve_out * amount_in / (reserve_in + amount_in)
    let denominator = params
        .reserve_in
        .checked_add(params.amount_in)
        .ok_or(PriceImpactError::Overflow)?;

    let amount_out = params
        .reserve_out
        .checked_mul(params.amount_in)
        .ok_or(PriceImpactError::Overflow)?
        .checked_div(denominator)
        .ok_or(PriceImpactError::Overflow)?;

    if amount_out <= 0 {
        return Err(PriceImpactError::InsufficientLiquidity);
    }
    if amount_out >= params.reserve_out {
        return Err(PriceImpactError::InsufficientLiquidity);
    }

    // Mid price = reserve_out / reserve_in  (scaled by BPS_SCALE)
    let mid_price_scaled = params
        .reserve_out
        .checked_mul(BPS_SCALE)
        .ok_or(PriceImpactError::Overflow)?
        .checked_div(params.reserve_in)
        .ok_or(PriceImpactError::Overflow)?;

    // Execution price = amount_out / amount_in  (scaled by BPS_SCALE)
    let exec_price_scaled = amount_out
        .checked_mul(BPS_SCALE)
        .ok_or(PriceImpactError::Overflow)?
        .checked_div(params.amount_in)
        .ok_or(PriceImpactError::Overflow)?;

    // Price impact = (mid_price - exec_price) / mid_price * BPS_SCALE
    let impact_bps = if mid_price_scaled > exec_price_scaled {
        mid_price_scaled
            .checked_sub(exec_price_scaled)
            .ok_or(PriceImpactError::Overflow)?
            .checked_mul(BPS_SCALE)
            .ok_or(PriceImpactError::Overflow)?
            .checked_div(mid_price_scaled)
            .ok_or(PriceImpactError::Overflow)?
    } else {
        0
    };

    if params.max_impact_bps > 0 && impact_bps > params.max_impact_bps {
        return Err(PriceImpactError::ExcessiveImpact);
    }

    Ok(PriceImpactResult {
        amount_out,
        impact_bps,
        exec_price_scaled,
        mid_price_scaled,
    })
}

// ─── Storage-backed pool helpers ─────────────────────────────────────────────

/// Persist a pool's reserve snapshot (called from AMM liquidity management).
pub fn update_pool_reserves(
    env: &Env,
    token_in: Address,
    token_out: Address,
    reserve_in: i128,
    reserve_out: i128,
) {
    let key = PriceImpactKey::PoolReserves(token_in, token_out);
    env.storage()
        .persistent()
        .set(&key, &(reserve_in, reserve_out));
}

/// Read the stored reserves for a (token_in, token_out) pool.
///
/// Returns `None` if no snapshot has been stored yet.
pub fn get_pool_reserves(
    env: &Env,
    token_in: &Address,
    token_out: &Address,
) -> Option<(i128, i128)> {
    let key = PriceImpactKey::PoolReserves(token_in.clone(), token_out.clone());
    env.storage()
        .persistent()
        .get::<PriceImpactKey, (i128, i128)>(&key)
}

/// Convenience wrapper: load reserves from storage and call `calculate_price_impact`.
///
/// Returns `Err(InvalidReserves)` if no pool snapshot exists.
pub fn calculate_price_impact_for_pool(
    env: &Env,
    token_in: &Address,
    token_out: &Address,
    amount_in: i128,
    max_impact_bps: i128,
) -> Result<PriceImpactResult, PriceImpactError> {
    let (reserve_in, reserve_out) =
        get_pool_reserves(env, token_in, token_out).ok_or(PriceImpactError::InvalidReserves)?;

    calculate_price_impact(&TradeParams {
        amount_in,
        reserve_in,
        reserve_out,
        max_impact_bps,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn make_params(amount_in: i128, reserve_in: i128, reserve_out: i128) -> TradeParams {
        TradeParams {
            amount_in,
            reserve_in,
            reserve_out,
            max_impact_bps: 0,
        }
    }

    #[test]
    fn test_small_trade_low_impact() {
        // 1% of pool size → small impact
        let result = calculate_price_impact(&make_params(100, 10_000, 10_000)).unwrap();
        // impact should be well below 200 bps (2%)
        assert!(result.impact_bps < 200, "impact={}", result.impact_bps);
        assert!(result.amount_out > 0);
    }

    #[test]
    fn test_large_trade_high_impact() {
        // 50% of pool → significant impact
        let result = calculate_price_impact(&make_params(5_000, 10_000, 10_000)).unwrap();
        // constant-product: out = 10_000 * 5_000 / 15_000 = 3_333
        assert_eq!(result.amount_out, 3_333);
        // impact > 15%
        assert!(result.impact_bps > 1500, "impact={}", result.impact_bps);
    }

    #[test]
    fn test_max_impact_guard_blocks_excessive_trade() {
        let params = TradeParams {
            amount_in: 5_000,
            reserve_in: 10_000,
            reserve_out: 10_000,
            max_impact_bps: 500, // 5% max
        };
        // large trade would exceed 5%
        let result = calculate_price_impact(&params);
        assert_eq!(result, Err(PriceImpactError::ExcessiveImpact));
    }

    #[test]
    fn test_max_impact_guard_allows_small_trade() {
        let params = TradeParams {
            amount_in: 100,
            reserve_in: 1_000_000,
            reserve_out: 1_000_000,
            max_impact_bps: 100, // 1% max — tiny trade should pass
        };
        let result = calculate_price_impact(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_zero_amount_rejected() {
        let result = calculate_price_impact(&make_params(0, 10_000, 10_000));
        assert_eq!(result, Err(PriceImpactError::InvalidAmount));
    }

    #[test]
    fn test_zero_reserves_rejected() {
        let result = calculate_price_impact(&make_params(100, 0, 10_000));
        assert_eq!(result, Err(PriceImpactError::InvalidReserves));

        let result2 = calculate_price_impact(&make_params(100, 10_000, 0));
        assert_eq!(result2, Err(PriceImpactError::InvalidReserves));
    }

    #[test]
    fn test_mid_price_equals_reserves_ratio() {
        // reserve_out / reserve_in = 2, so mid_price_scaled = 2 * BPS_SCALE = 20_000
        let result = calculate_price_impact(&make_params(100, 5_000, 10_000)).unwrap();
        assert_eq!(result.mid_price_scaled, 2 * BPS_SCALE);
    }

    #[test]
    fn test_pool_reserves_storage() {
        let env = Env::default();
        env.mock_all_auths();
        let tok_a = Address::generate(&env);
        let tok_b = Address::generate(&env);

        // Nothing stored yet
        assert!(get_pool_reserves(&env, &tok_a, &tok_b).is_none());

        update_pool_reserves(&env, tok_a.clone(), tok_b.clone(), 100_000, 200_000);

        let (r_in, r_out) = get_pool_reserves(&env, &tok_a, &tok_b).unwrap();
        assert_eq!(r_in, 100_000);
        assert_eq!(r_out, 200_000);
    }

    #[test]
    fn test_calculate_price_impact_for_pool() {
        let env = Env::default();
        env.mock_all_auths();
        let tok_a = Address::generate(&env);
        let tok_b = Address::generate(&env);

        update_pool_reserves(&env, tok_a.clone(), tok_b.clone(), 1_000_000, 1_000_000);

        let result =
            calculate_price_impact_for_pool(&env, &tok_a, &tok_b, 1_000, 0).unwrap();
        assert!(result.amount_out > 0);
        assert!(result.impact_bps < 200);
    }

    #[test]
    fn test_calculate_price_impact_for_pool_missing_reserves() {
        let env = Env::default();
        env.mock_all_auths();
        let tok_a = Address::generate(&env);
        let tok_b = Address::generate(&env);

        let result = calculate_price_impact_for_pool(&env, &tok_a, &tok_b, 1_000, 0);
        assert_eq!(result, Err(PriceImpactError::InvalidReserves));
    }

    #[test]
    fn test_insufficient_liquidity_when_trade_drains_pool() {
        // Attempting to trade more than the pool has available
        let result = calculate_price_impact(&make_params(1_000_000, 1_000, 1_000));
        assert!(matches!(
            result,
            Err(PriceImpactError::InsufficientLiquidity)
        ));
    }

    #[test]
    fn test_negative_amount_rejected() {
        let result = calculate_price_impact(&make_params(-1, 10_000, 10_000));
        assert_eq!(result, Err(PriceImpactError::InvalidAmount));
    }
}
