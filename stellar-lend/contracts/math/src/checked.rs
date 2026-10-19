//! Environment-free checked arithmetic for hot paths.
//!
//! [`crate::mul_div`] routes through the `I256` host object, which is correct for
//! very wide intermediates but costs a host call per operation. The lending hot
//! paths — interest-rate curve evaluation, liquidation sizing, health factors —
//! only ever multiply protocol-scale amounts by basis points, so they fit in
//! `i128` with room to spare.
//!
//! The helpers here stay in native `i128` and never touch the `Env`, which keeps
//! them usable from pure functions (and from tests without a host). When a
//! product would overflow, [`checked_mul_div`] retries with a
//! divide-first decomposition rather than failing immediately, so accuracy is
//! preserved for large operands without paying for `I256` on every call.

use crate::error::MathError;

/// Basis-point divisor: 10,000 bps = 100%.
pub const BPS_DIVISOR: i128 = 10_000;

/// `(a * b) / d` with overflow and division-by-zero checks, without a host call.
///
/// Truncates toward zero, matching `i128` division.
///
/// # Errors
/// - [`MathError::DivisionByZero`] when `d == 0`.
/// - [`MathError::Overflow`] when the result cannot be represented in `i128`.
///
/// # Example
/// ```
/// use stellarlend_math::checked::checked_mul_div;
/// assert_eq!(checked_mul_div(1_000, 250, 10_000).unwrap(), 25);
/// ```
pub fn checked_mul_div(a: i128, b: i128, d: i128) -> Result<i128, MathError> {
    if d == 0 {
        return Err(MathError::DivisionByZero);
    }
    match a.checked_mul(b) {
        Some(product) => product.checked_div(d).ok_or(MathError::Overflow),
        // `a * b` does not fit in i128: split `a` into quotient and remainder so
        // the wide part of the product is divided before it is multiplied.
        None => {
            let quotient = a / d;
            let remainder = a % d;
            let high = quotient.checked_mul(b).ok_or(MathError::Overflow)?;
            let low = remainder
                .checked_mul(b)
                .ok_or(MathError::Overflow)?
                .checked_div(d)
                .ok_or(MathError::Overflow)?;
            high.checked_add(low).ok_or(MathError::Overflow)
        }
    }
}

/// `(a * bps) / 10_000` — the basis-point case of [`checked_mul_div`].
///
/// # Example
/// ```
/// use stellarlend_math::checked::apply_bps;
/// // 5% of 2,000
/// assert_eq!(apply_bps(2_000, 500).unwrap(), 100);
/// ```
pub fn apply_bps(amount: i128, bps: i128) -> Result<i128, MathError> {
    checked_mul_div(amount, bps, BPS_DIVISOR)
}

/// `(numerator * 10_000) / denominator` — a ratio expressed in basis points.
///
/// Returns `Ok(0)` when `denominator == 0`, which is the convention the lending
/// modules use for "no exposure yet" (zero deposits, zero collateral).
pub fn ratio_bps(numerator: i128, denominator: i128) -> Result<i128, MathError> {
    if denominator == 0 {
        return Ok(0);
    }
    checked_mul_div(numerator, BPS_DIVISOR, denominator)
}

/// Checked addition.
pub fn checked_add(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

/// Checked subtraction.
pub fn checked_sub(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_sub(b).ok_or(MathError::Underflow)
}

/// Checked multiplication.
pub fn checked_mul(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_mul(b).ok_or(MathError::Overflow)
}

/// Checked division.
pub fn checked_div(a: i128, b: i128) -> Result<i128, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero);
    }
    a.checked_div(b).ok_or(MathError::Overflow)
}

/// Clamps `value` into `[min, max]`.
///
/// Returns `min` when the bounds are inverted, so a misconfigured floor/ceiling
/// pair can never widen a bound instead of narrowing it.
pub fn clamp(value: i128, min: i128, max: i128) -> i128 {
    if min > max {
        return min;
    }
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_basic() {
        assert_eq!(checked_mul_div(100, 200, 10).unwrap(), 2_000);
        assert_eq!(checked_mul_div(7, 3, 2).unwrap(), 10);
    }

    #[test]
    fn mul_div_truncates_toward_zero() {
        assert_eq!(checked_mul_div(10, 1, 3).unwrap(), 3);
        assert_eq!(checked_mul_div(-10, 1, 3).unwrap(), -3);
    }

    #[test]
    fn mul_div_zero_denominator() {
        assert_eq!(checked_mul_div(1, 1, 0), Err(MathError::DivisionByZero));
    }

    #[test]
    fn mul_div_zero_operands() {
        assert_eq!(checked_mul_div(0, i128::MAX, 7).unwrap(), 0);
        assert_eq!(checked_mul_div(i128::MAX, 0, 7).unwrap(), 0);
    }

    #[test]
    fn mul_div_falls_back_on_wide_product() {
        // a * b overflows i128, but (a * b) / d is representable.
        let a = i128::MAX / 2;
        let result = checked_mul_div(a, 4, 8).unwrap();
        assert!((result - a / 2).abs() <= 1, "got {result}");
    }

    #[test]
    fn mul_div_reports_unrepresentable_result() {
        assert_eq!(
            checked_mul_div(i128::MAX, i128::MAX, 1),
            Err(MathError::Overflow)
        );
    }

    #[test]
    fn mul_div_negative_denominator() {
        assert_eq!(checked_mul_div(100, 200, -10).unwrap(), -2_000);
    }

    #[test]
    fn apply_bps_percentages() {
        assert_eq!(apply_bps(2_000, 500).unwrap(), 100);
        assert_eq!(apply_bps(2_000, 0).unwrap(), 0);
        assert_eq!(apply_bps(2_000, BPS_DIVISOR).unwrap(), 2_000);
    }

    #[test]
    fn ratio_bps_cases() {
        assert_eq!(ratio_bps(50, 100).unwrap(), 5_000);
        assert_eq!(ratio_bps(0, 100).unwrap(), 0);
        assert_eq!(ratio_bps(100, 0).unwrap(), 0);
        assert_eq!(ratio_bps(3, 7).unwrap(), 4_285);
    }

    #[test]
    fn checked_ops_report_their_own_errors() {
        assert_eq!(checked_add(i128::MAX, 1), Err(MathError::Overflow));
        assert_eq!(checked_sub(i128::MIN, 1), Err(MathError::Underflow));
        assert_eq!(checked_mul(i128::MAX, 2), Err(MathError::Overflow));
        assert_eq!(checked_div(1, 0), Err(MathError::DivisionByZero));
        assert_eq!(checked_add(1, 2).unwrap(), 3);
        assert_eq!(checked_sub(3, 1).unwrap(), 2);
        assert_eq!(checked_mul(3, 4).unwrap(), 12);
        assert_eq!(checked_div(12, 4).unwrap(), 3);
    }

    #[test]
    fn clamp_bounds() {
        assert_eq!(clamp(5, 0, 10), 5);
        assert_eq!(clamp(-1, 0, 10), 0);
        assert_eq!(clamp(11, 0, 10), 10);
        assert_eq!(clamp(5, 0, 0), 0);
        // Inverted bounds resolve to the floor.
        assert_eq!(clamp(5, 10, 0), 10);
    }
}
