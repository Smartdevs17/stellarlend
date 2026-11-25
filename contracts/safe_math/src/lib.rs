#![no_std]
//! Single home for overflow-safe arithmetic (issue #1765).
//!
//! * `checked_*` ops return an error on overflow, for paths where overflow
//!   means a bug or an attack and the transaction must fail.
//! * `saturating_*` ops clamp to `i128::MIN`/`MAX`, for paths such as caps and
//!   display values where clamping is the correct behaviour.
//!
//! Division by zero and invalid input are never saturated; they always error.

/// Basis-point divisor (`100% == 10_000 bps`).
pub const BPS_DIVISOR: i128 = 10_000;

/// Errors returned by the arithmetic helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeMathError {
    /// A denominator was zero.
    DivisionByZero,
    /// The operation overflowed or underflowed `i128`.
    Overflow,
    /// An amount, rate or threshold was outside its documented range.
    InvalidInput,
}

/// Calculate `a * b / d` without losing the intermediate product to overflow.
///
/// When `a * b` overflows, the division is performed first and the remainder
/// is added back, keeping truncating semantics identical to the direct form.
pub fn checked_mul_div(a: i128, b: i128, d: i128) -> Result<i128, SafeMathError> {
    if d == 0 {
        return Err(SafeMathError::DivisionByZero);
    }
    match a.checked_mul(b) {
        Some(product) => product.checked_div(d).ok_or(SafeMathError::Overflow),
        None => {
            let quotient = a.checked_div(d).ok_or(SafeMathError::Overflow)?;
            let remainder = a.checked_rem(d).ok_or(SafeMathError::Overflow)?;
            let high = quotient.checked_mul(b).ok_or(SafeMathError::Overflow)?;
            let low = remainder
                .checked_mul(b)
                .and_then(|value| value.checked_div(d))
                .ok_or(SafeMathError::Overflow)?;
            high.checked_add(low).ok_or(SafeMathError::Overflow)
        }
    }
}

/// Apply `bps` basis points to a non-negative `amount`, truncating remainder.
pub fn apply_bps(amount: i128, bps: i128) -> Result<i128, SafeMathError> {
    if amount < 0 || bps < 0 {
        return Err(SafeMathError::InvalidInput);
    }
    checked_mul_div(amount, bps, BPS_DIVISOR)
}

/// Like [`checked_mul_div`], but an overflowing result clamps to
/// `i128::MAX` or `i128::MIN` according to the sign of the true result.
/// Division by zero still errors.
pub fn saturating_mul_div(a: i128, b: i128, d: i128) -> Result<i128, SafeMathError> {
    match checked_mul_div(a, b, d) {
        Err(SafeMathError::Overflow) => {
            // Result is negative iff an odd number of operands are negative.
            let negative = (a < 0) ^ (b < 0) ^ (d < 0);
            Ok(if negative { i128::MIN } else { i128::MAX })
        }
        other => other,
    }
}

/// Like [`apply_bps`], but clamps to `i128::MAX` instead of overflowing.
/// Negative inputs still error.
pub fn saturating_apply_bps(amount: i128, bps: i128) -> Result<i128, SafeMathError> {
    if amount < 0 || bps < 0 {
        return Err(SafeMathError::InvalidInput);
    }
    saturating_mul_div(amount, bps, BPS_DIVISOR)
}

/// `a + b`, clamped to the `i128` range.
pub fn saturating_add(a: i128, b: i128) -> i128 {
    a.saturating_add(b)
}

/// `a - b`, clamped to the `i128` range.
pub fn saturating_sub(a: i128, b: i128) -> i128 {
    a.saturating_sub(b)
}

/// `a * b`, clamped to the `i128` range.
pub fn saturating_mul(a: i128, b: i128) -> i128 {
    a.saturating_mul(b)
}

/// `a - b`, floored at zero (for "remaining capacity" style values).
pub fn saturating_sub_floor(a: i128, b: i128) -> i128 {
    a.saturating_sub(b).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_mul_div_handles_large_products() {
        let a = i128::MAX / 4;
        assert_eq!(checked_mul_div(a, 3, 3), Ok(a));
        assert_eq!(checked_mul_div(a, a, a), Ok(a));
        assert_eq!(checked_mul_div(0, i128::MAX, 7), Ok(0));
    }

    #[test]
    fn checked_mul_div_errors() {
        assert_eq!(checked_mul_div(1, 1, 0), Err(SafeMathError::DivisionByZero));
        assert_eq!(
            checked_mul_div(i128::MAX, 2, 1),
            Err(SafeMathError::Overflow)
        );
        assert_eq!(
            checked_mul_div(i128::MIN, 1, -1),
            Err(SafeMathError::Overflow)
        );
    }

    #[test]
    fn checked_mul_div_truncates() {
        assert_eq!(checked_mul_div(10, 1, 3), Ok(3));
        assert_eq!(checked_mul_div(-10, 1, 3), Ok(-3));
    }

    #[test]
    fn apply_bps_uses_10_000_scale() {
        assert_eq!(apply_bps(20_000, 8_000), Ok(16_000));
        assert_eq!(apply_bps(1, 1), Ok(0));
        assert_eq!(apply_bps(-1, 100), Err(SafeMathError::InvalidInput));
        assert_eq!(apply_bps(100, -1), Err(SafeMathError::InvalidInput));
        assert_eq!(apply_bps(i128::MAX, 20_000), Err(SafeMathError::Overflow));
    }

    #[test]
    fn saturating_mul_div_clamps_by_sign() {
        assert_eq!(saturating_mul_div(i128::MAX, 2, 1), Ok(i128::MAX));
        assert_eq!(saturating_mul_div(i128::MAX, -2, 1), Ok(i128::MIN));
        assert_eq!(saturating_mul_div(i128::MIN, 2, 1), Ok(i128::MIN));
        assert_eq!(saturating_mul_div(i128::MIN, 1, -1), Ok(i128::MAX));
        assert_eq!(saturating_mul_div(i128::MAX, -2, -1), Ok(i128::MAX));
    }

    #[test]
    fn saturating_mul_div_passes_through_non_overflow() {
        assert_eq!(saturating_mul_div(6, 7, 2), Ok(21));
        assert_eq!(
            saturating_mul_div(1, 1, 0),
            Err(SafeMathError::DivisionByZero)
        );
    }

    #[test]
    fn saturating_apply_bps_clamps_and_validates() {
        assert_eq!(saturating_apply_bps(i128::MAX, 20_000), Ok(i128::MAX));
        assert_eq!(saturating_apply_bps(20_000, 8_000), Ok(16_000));
        assert_eq!(
            saturating_apply_bps(-1, 1),
            Err(SafeMathError::InvalidInput)
        );
        assert_eq!(
            saturating_apply_bps(1, -1),
            Err(SafeMathError::InvalidInput)
        );
    }

    #[test]
    fn plain_saturating_ops_hit_bounds() {
        assert_eq!(saturating_add(i128::MAX, 1), i128::MAX);
        assert_eq!(saturating_sub(i128::MIN, 1), i128::MIN);
        assert_eq!(saturating_mul(i128::MAX, 2), i128::MAX);
        assert_eq!(saturating_mul(i128::MIN, 2), i128::MIN);
        assert_eq!(saturating_add(1, 2), 3);
    }

    #[test]
    fn sub_floor_never_goes_negative() {
        assert_eq!(saturating_sub_floor(5, 10), 0);
        assert_eq!(saturating_sub_floor(10, 5), 5);
        assert_eq!(saturating_sub_floor(i128::MIN, 1), 0);
    }
}
