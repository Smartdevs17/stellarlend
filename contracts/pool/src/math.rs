//! Pool arithmetic, backed by the shared `stellarlend-safe-math` crate.
//!
//! Pool code imports checked and saturating helpers from here instead of
//! hand-rolling overflow handling.

pub use stellarlend_safe_math::{
    apply_bps, checked_mul_div, saturating_add, saturating_apply_bps, saturating_mul,
    saturating_mul_div, saturating_sub, saturating_sub_floor, SafeMathError,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexports_behave_like_safe_math() {
        assert_eq!(apply_bps(20_000, 8_000), Ok(16_000));
        assert_eq!(saturating_apply_bps(i128::MAX, 20_000), Ok(i128::MAX));
        assert_eq!(checked_mul_div(1, 1, 0), Err(SafeMathError::DivisionByZero));
    }
}
