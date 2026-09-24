#![no_std]
//! # stellarlend-math
//!
//! The protocol's single arithmetic library. Interest-rate curves, liquidation
//! sizing, fixed-point conversion and the checked primitives underneath them all
//! live here so that no two modules can drift apart on a rounding rule.
//!
//! ## Layout
//!
//! | Module | Use it for |
//! |---|---|
//! | [`checked`] | env-free checked `i128` arithmetic for hot paths |
//! | [`rates`] | interest-rate curves, utilization, index accrual |
//! | [`liquidation`] | health factors, penalties, seize amounts, batch priority |
//! | [`lending`] | higher-level lending helpers built on the above |
//! | [`fixed_point`] | WAD/RAY fixed-point arithmetic |
//! | [`mul_div`] | `I256`-backed `mul_div` for intermediates wider than `i128` |
//! | [`compound`], [`exponential`] | compounding and `exp`/`ln` approximations |
//! | [`int128`], [`rounding`], [`precision`] | primitives, rounding modes, precision tracking |
//!
//! ## Choosing between `checked` and `mul_div`
//!
//! [`mul_div::mul_div`] computes through the `I256` host object: exact for any
//! intermediate, but one host call per operation. [`checked::checked_mul_div`]
//! stays in native `i128` and needs no `Env`. Rate curves, health factors and
//! liquidation sizing multiply protocol-scale amounts by basis points, so they
//! use the `checked` path; reach for `mul_div` when an intermediate genuinely
//! exceeds `i128`, such as RAY-scaled (1e27) products.

pub mod checked;
pub mod compound;
pub mod error;
pub mod exponential;
pub mod fixed_point;
pub mod int128;
pub mod lending;
pub mod liquidation;
pub mod mul_div;
pub mod precision;
pub mod rates;
pub mod rounding;

pub use error::MathError;

pub use int128::{
    bps_mul, bps_mul_u128, safe_add, safe_add_u128, safe_div, safe_div_u128, safe_mul,
    safe_mul_u128, safe_pow, safe_sqrt, safe_sqrt_u128, safe_sub, safe_sub_u128, unchecked_add,
    unchecked_add_u128, unchecked_div, unchecked_mul, unchecked_mul_u128, unchecked_sub,
    unchecked_sub_u128,
};

pub use fixed_point::{
    bps_ratio, fp_add, fp_div, fp_mul, fp_pow, fp_sqrt, fp_sub, ray_div, ray_mul, ray_to_wad,
    simple_interest, wad_to_ray, HALF_WAD, RAY, SECONDS_PER_YEAR, WAD,
};

pub use compound::{compound_interest, compound_interest_continuous};
pub use exponential::{wad_exp, wad_ln};
pub use mul_div::{
    mul_div, mul_div_ceil, mul_div_floor, mul_div_round_up, mul_div_with_rounding,
    mul_div_with_safety,
};
pub use precision::{PrecisionLoss, PrecisionTracker};
pub use rounding::{round_down, round_nearest, round_up, RoundingMode};

pub use lending::{
    accrue_interest, calculate_utilization, collateral_ratio_bps,
    compound_interest as lending_compound_interest, health_factor_bps, is_liquidatable,
    liquidation_bonus, ltv_bps, max_borrow_amount, max_liquidatable, seize_amount,
    validate_collateral_ratio, InterestRateModel, BPS_DIVISOR,
};

pub use checked::{
    apply_bps, checked_mul_div, clamp, ratio_bps, BPS_DIVISOR as CHECKED_BPS_DIVISOR,
};

pub use rates::{
    accrue_index, apply_rate_bounds, dual_slope_rate, exponential_rate, index_growth_factor, jump_rate, kink_rate,
    linear_rate, supply_rate_from_reserve_factor, supply_rate_from_spread, utilization_bps,
    RateCurve, RateModelKind,
};

pub use liquidation::{
    collateral_value_in_debt, debt_value_in_collateral, dynamic_penalty_bps,
    health_factor_bps as position_health_factor_bps, incentive_amount,
    is_liquidatable as position_is_liquidatable, is_profitable, max_repayable, priority_score,
    seize_amount as seize_with_incentive, split_proceeds, LiquidationSplit, MAX_PENALTY_BPS,
};
