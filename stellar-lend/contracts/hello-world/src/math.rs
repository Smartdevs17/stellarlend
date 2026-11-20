//! Safe arithmetic for the `hello-world` lending contract.
//!
//! # Why this module exists
//!
//! Soroban contracts run inside a resource-metered host that traps on integer
//! overflow in debug mode and silently wraps in release builds. Neither
//! behaviour is acceptable for financial logic: wrapping creates incorrect
//! balances; trapping is a denial-of-service vector exploitable from any user
//! input that reaches an arithmetic expression.
//!
//! This module centralises every arithmetic primitive used by the contract so
//! that:
//!
//! - all overflow, underflow, and division-by-zero paths return typed errors
//!   instead of panicking or wrapping;
//! - every operation on a user-visible amount emits a structured observability
//!   event so that monitoring infrastructure can detect anomalous computation
//!   patterns in production;
//! - there is a single source of truth for rounding and basis-point
//!   conventions.
//!
//! # Public surface
//!
//! | Function | Description |
//! |---|---|
//! | [`safe_add`] | Checked `i128` addition |
//! | [`safe_sub`] | Checked `i128` subtraction |
//! | [`safe_mul`] | Checked `i128` multiplication |
//! | [`safe_div`] | Checked `i128` division (guards div-by-zero **and** `MIN/-1`) |
//! | [`safe_add_u128`] | Checked `u128` addition |
//! | [`safe_sub_u128`] | Checked `u128` subtraction |
//! | [`safe_mul_u128`] | Checked `u128` multiplication |
//! | [`safe_div_u128`] | Checked `u128` division |
//! | [`bps_of`] | `(amount * bps) / 10_000` |
//! | [`ratio_bps`] | `(numerator * 10_000) / denominator` |
//! | [`checked_mul_div`] | `(a * b) / d` without an Env |
//!
//! # Observability
//!
//! Pass `Some(&env)` to the `env` argument of any function to emit a
//! `("math", "overflow")` or `("math", "div_zero")` event **before** returning
//! an error. These events are picked up by the off-chain indexer (see
//! `api/src/services/eventIndex`) and routed to the anomaly dashboard. Pass
//! `None` for pure-function callers that have no `Env` in scope (e.g. unit
//! tests or wasm-free paths).

use soroban_sdk::{Env, Symbol};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by every arithmetic operation in this module.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MathError {
    /// Addition or multiplication produced a value larger than `i128::MAX` /
    /// `u128::MAX`.
    Overflow,
    /// Subtraction or multiplication produced a value smaller than `i128::MIN`
    /// / `u128::MIN`.
    Underflow,
    /// Division by zero was attempted.
    DivisionByZero,
    /// Square root of a negative number was requested.
    NegativeSqrt,
}

// ---------------------------------------------------------------------------
// Basis-point constants
// ---------------------------------------------------------------------------

/// One hundred percent expressed in basis points.
pub const BPS_DIVISOR: i128 = 10_000;
/// One hundred percent in basis points as a `u128`.
pub const BPS_DIVISOR_U128: u128 = 10_000;

// ---------------------------------------------------------------------------
// Observability helpers (private)
// ---------------------------------------------------------------------------

fn emit_overflow(env: Option<&Env>) {
    if let Some(e) = env {
        e.events().publish(
            (Symbol::new(e, "math"), Symbol::new(e, "overflow")),
            (),
        );
    }
}

fn emit_div_zero(env: Option<&Env>) {
    if let Some(e) = env {
        e.events().publish(
            (Symbol::new(e, "math"), Symbol::new(e, "div_zero")),
            (),
        );
    }
}

fn emit_negative_sqrt(env: Option<&Env>) {
    if let Some(e) = env {
        e.events().publish(
            (Symbol::new(e, "math"), Symbol::new(e, "neg_sqrt")),
            (),
        );
    }
}

// ---------------------------------------------------------------------------
// i128 safe operations
// ---------------------------------------------------------------------------

/// Checked `i128` addition.
///
/// Returns [`MathError::Overflow`] if `a + b > i128::MAX` and
/// [`MathError::Underflow`] if `a + b < i128::MIN`.
#[inline]
pub fn safe_add(env: Option<&Env>, a: i128, b: i128) -> Result<i128, MathError> {
    match a.checked_add(b) {
        Some(v) => Ok(v),
        None => {
            emit_overflow(env);
            Err(if b > 0 {
                MathError::Overflow
            } else {
                MathError::Underflow
            })
        }
    }
}

/// Checked `i128` subtraction.
///
/// Returns [`MathError::Underflow`] if `a - b < i128::MIN` and
/// [`MathError::Overflow`] if `a - b > i128::MAX`.
#[inline]
pub fn safe_sub(env: Option<&Env>, a: i128, b: i128) -> Result<i128, MathError> {
    match a.checked_sub(b) {
        Some(v) => Ok(v),
        None => {
            emit_overflow(env);
            Err(if b > 0 {
                MathError::Underflow
            } else {
                MathError::Overflow
            })
        }
    }
}

/// Checked `i128` multiplication.
///
/// Returns [`MathError::Overflow`] or [`MathError::Underflow`] on out-of-range
/// results.
#[inline]
pub fn safe_mul(env: Option<&Env>, a: i128, b: i128) -> Result<i128, MathError> {
    match a.checked_mul(b) {
        Some(v) => Ok(v),
        None => {
            emit_overflow(env);
            Err(if (a > 0 && b > 0) || (a < 0 && b < 0) {
                MathError::Overflow
            } else {
                MathError::Underflow
            })
        }
    }
}

/// Checked `i128` division.
///
/// Guards two distinct failure modes that `checked_div` alone does not cover:
///
/// 1. **Division by zero** — `b == 0`.
/// 2. **MIN / -1 overflow** — `i128::MIN / -1` would produce `i128::MAX + 1`,
///    which does not fit in `i128`. Rust's `checked_div` returns `None` for
///    this case; we surface it as [`MathError::Overflow`].
#[inline]
pub fn safe_div(env: Option<&Env>, a: i128, b: i128) -> Result<i128, MathError> {
    if b == 0 {
        emit_div_zero(env);
        return Err(MathError::DivisionByZero);
    }
    match a.checked_div(b) {
        Some(v) => Ok(v),
        None => {
            // The only remaining case is i128::MIN / -1.
            emit_overflow(env);
            Err(MathError::Overflow)
        }
    }
}

/// Integer square root of a non-negative `i128`.
///
/// Returns the floor of the true square root. Returns
/// [`MathError::NegativeSqrt`] for negative inputs.
pub fn safe_sqrt(env: Option<&Env>, n: i128) -> Result<i128, MathError> {
    if n < 0 {
        emit_negative_sqrt(env);
        return Err(MathError::NegativeSqrt);
    }
    if n < 4 {
        return Ok(if n == 0 { 0 } else { 1 });
    }
    let mut x = n / 2;
    loop {
        let next = (x + n / x) / 2;
        if next >= x {
            break;
        }
        x = next;
    }
    Ok(x)
}

// ---------------------------------------------------------------------------
// u128 safe operations
// ---------------------------------------------------------------------------

/// Checked `u128` addition.
#[inline]
pub fn safe_add_u128(env: Option<&Env>, a: u128, b: u128) -> Result<u128, MathError> {
    match a.checked_add(b) {
        Some(v) => Ok(v),
        None => {
            emit_overflow(env);
            Err(MathError::Overflow)
        }
    }
}

/// Checked `u128` subtraction.
#[inline]
pub fn safe_sub_u128(env: Option<&Env>, a: u128, b: u128) -> Result<u128, MathError> {
    match a.checked_sub(b) {
        Some(v) => Ok(v),
        None => {
            emit_overflow(env);
            Err(MathError::Underflow)
        }
    }
}

/// Checked `u128` multiplication.
#[inline]
pub fn safe_mul_u128(env: Option<&Env>, a: u128, b: u128) -> Result<u128, MathError> {
    match a.checked_mul(b) {
        Some(v) => Ok(v),
        None => {
            emit_overflow(env);
            Err(MathError::Overflow)
        }
    }
}

/// Checked `u128` division.
#[inline]
pub fn safe_div_u128(env: Option<&Env>, a: u128, b: u128) -> Result<u128, MathError> {
    if b == 0 {
        emit_div_zero(env);
        return Err(MathError::DivisionByZero);
    }
    Ok(a / b)
}

// ---------------------------------------------------------------------------
// Composite helpers
// ---------------------------------------------------------------------------

/// `(amount * bps) / 10_000` — the basis-point share of `amount`.
///
/// # Example
///
/// ```
/// // 5% of 2,000 tokens = 100
/// assert_eq!(bps_of(None, 2_000, 500).unwrap(), 100i128);
/// ```
pub fn bps_of(env: Option<&Env>, amount: i128, bps: i128) -> Result<i128, MathError> {
    let product = safe_mul(env, amount, bps)?;
    safe_div(env, product, BPS_DIVISOR)
}

/// `(numerator * 10_000) / denominator` — a ratio expressed in basis points.
///
/// Returns `Ok(0)` when `denominator == 0` to represent "no exposure yet"
/// (zero deposits, zero collateral). This matches the convention used by the
/// wider math library.
pub fn ratio_bps(env: Option<&Env>, numerator: i128, denominator: i128) -> Result<i128, MathError> {
    if denominator == 0 {
        return Ok(0);
    }
    let product = safe_mul(env, numerator, BPS_DIVISOR)?;
    safe_div(env, product, denominator)
}

/// `(a * b) / d` with overflow and division-by-zero checks, without a host
/// call.
///
/// When `a * b` overflows `i128` the function retries with a split
/// decomposition — `(a / d) * b + (a % d) * b / d` — which preserves accuracy
/// for large-but-representable results at the cost of one additional division.
///
/// Returns [`MathError::DivisionByZero`] when `d == 0` and
/// [`MathError::Overflow`] when the result cannot be represented in `i128`.
pub fn checked_mul_div(env: Option<&Env>, a: i128, b: i128, d: i128) -> Result<i128, MathError> {
    if d == 0 {
        emit_div_zero(env);
        return Err(MathError::DivisionByZero);
    }
    match a.checked_mul(b) {
        Some(product) => product.checked_div(d).ok_or_else(|| {
            emit_overflow(env);
            MathError::Overflow
        }),
        None => {
            // Split decomposition: avoids a host I256 call while handling
            // most practically-occurring large-but-representable products.
            let quotient = a / d;
            let remainder = a % d;
            let high = quotient.checked_mul(b).ok_or_else(|| {
                emit_overflow(env);
                MathError::Overflow
            })?;
            let low_product = remainder.checked_mul(b).ok_or_else(|| {
                emit_overflow(env);
                MathError::Overflow
            })?;
            let low = low_product.checked_div(d).ok_or_else(|| {
                emit_overflow(env);
                MathError::Overflow
            })?;
            high.checked_add(low).ok_or_else(|| {
                emit_overflow(env);
                MathError::Overflow
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    // No-env shorthand used throughout — observability hooks are covered in a
    // separate section.
    fn env() -> Env {
        Env::default()
    }

    // -----------------------------------------------------------------------
    // safe_add
    // -----------------------------------------------------------------------

    #[test]
    fn add_normal() {
        assert_eq!(safe_add(None, 3, 4), Ok(7));
        assert_eq!(safe_add(None, -3, 4), Ok(1));
        assert_eq!(safe_add(None, 0, i128::MAX), Ok(i128::MAX));
        assert_eq!(safe_add(None, 0, i128::MIN), Ok(i128::MIN));
    }

    #[test]
    fn add_overflow() {
        assert_eq!(safe_add(None, i128::MAX, 1), Err(MathError::Overflow));
        assert_eq!(safe_add(None, i128::MAX, i128::MAX), Err(MathError::Overflow));
    }

    #[test]
    fn add_underflow() {
        assert_eq!(safe_add(None, i128::MIN, -1), Err(MathError::Underflow));
    }

    // -----------------------------------------------------------------------
    // safe_sub
    // -----------------------------------------------------------------------

    #[test]
    fn sub_normal() {
        assert_eq!(safe_sub(None, 10, 3), Ok(7));
        assert_eq!(safe_sub(None, i128::MIN, 0), Ok(i128::MIN));
    }

    #[test]
    fn sub_underflow() {
        assert_eq!(safe_sub(None, i128::MIN, 1), Err(MathError::Underflow));
    }

    #[test]
    fn sub_overflow() {
        assert_eq!(safe_sub(None, i128::MAX, -1), Err(MathError::Overflow));
    }

    // -----------------------------------------------------------------------
    // safe_mul
    // -----------------------------------------------------------------------

    #[test]
    fn mul_normal() {
        assert_eq!(safe_mul(None, 6, 7), Ok(42));
        assert_eq!(safe_mul(None, -3, 4), Ok(-12));
        assert_eq!(safe_mul(None, 0, i128::MAX), Ok(0));
    }

    #[test]
    fn mul_overflow() {
        assert_eq!(safe_mul(None, i128::MAX, 2), Err(MathError::Overflow));
        // MIN * -1 also overflows (would be MAX + 1)
        assert_eq!(safe_mul(None, i128::MIN, -1), Err(MathError::Overflow));
    }

    #[test]
    fn mul_underflow() {
        assert_eq!(safe_mul(None, i128::MAX, -2), Err(MathError::Underflow));
    }

    // -----------------------------------------------------------------------
    // safe_div
    // -----------------------------------------------------------------------

    #[test]
    fn div_normal() {
        assert_eq!(safe_div(None, 10, 3), Ok(3));
        assert_eq!(safe_div(None, -10, 3), Ok(-3));
        assert_eq!(safe_div(None, 0, 7), Ok(0));
    }

    #[test]
    fn div_by_zero() {
        assert_eq!(safe_div(None, 42, 0), Err(MathError::DivisionByZero));
        assert_eq!(safe_div(None, 0, 0), Err(MathError::DivisionByZero));
        assert_eq!(safe_div(None, i128::MIN, 0), Err(MathError::DivisionByZero));
    }

    /// `i128::MIN / -1` would produce `i128::MAX + 1`; this is the unique
    /// overflow case in integer division and must be caught explicitly.
    #[test]
    fn div_min_neg_one_overflow() {
        assert_eq!(safe_div(None, i128::MIN, -1), Err(MathError::Overflow));
    }

    #[test]
    fn div_self_is_one() {
        assert_eq!(safe_div(None, i128::MAX, i128::MAX), Ok(1));
        assert_eq!(safe_div(None, i128::MIN, i128::MIN), Ok(1));
    }

    // -----------------------------------------------------------------------
    // safe_sqrt
    // -----------------------------------------------------------------------

    #[test]
    fn sqrt_normal() {
        assert_eq!(safe_sqrt(None, 0), Ok(0));
        assert_eq!(safe_sqrt(None, 1), Ok(1));
        assert_eq!(safe_sqrt(None, 4), Ok(2));
        assert_eq!(safe_sqrt(None, 16), Ok(4));
        assert_eq!(safe_sqrt(None, 2), Ok(1));
    }

    #[test]
    fn sqrt_negative() {
        assert_eq!(safe_sqrt(None, -1), Err(MathError::NegativeSqrt));
    }

    #[test]
    fn sqrt_large() {
        let r = safe_sqrt(None, i128::MAX).unwrap();
        // r² <= MAX and (r+1)² > MAX
        assert!((r as u128).saturating_mul(r as u128) <= i128::MAX as u128);
    }

    #[test]
    fn sqrt_perfect_squares() {
        for n in 0i128..=100 {
            let sq = n * n;
            assert_eq!(safe_sqrt(None, sq), Ok(n), "sqrt({sq}) should be {n}");
        }
    }

    // -----------------------------------------------------------------------
    // u128 operations
    // -----------------------------------------------------------------------

    #[test]
    fn u128_add_normal() {
        assert_eq!(safe_add_u128(None, 10, 20), Ok(30));
    }

    #[test]
    fn u128_add_overflow() {
        assert_eq!(safe_add_u128(None, u128::MAX, 1), Err(MathError::Overflow));
    }

    #[test]
    fn u128_sub_normal() {
        assert_eq!(safe_sub_u128(None, 10, 3), Ok(7));
        assert_eq!(safe_sub_u128(None, 10, 10), Ok(0));
    }

    #[test]
    fn u128_sub_underflow() {
        assert_eq!(safe_sub_u128(None, 0, 1), Err(MathError::Underflow));
    }

    #[test]
    fn u128_mul_normal() {
        assert_eq!(safe_mul_u128(None, 6, 7), Ok(42));
        assert_eq!(safe_mul_u128(None, 0, u128::MAX), Ok(0));
    }

    #[test]
    fn u128_mul_overflow() {
        assert_eq!(safe_mul_u128(None, u128::MAX, 2), Err(MathError::Overflow));
    }

    #[test]
    fn u128_div_normal() {
        assert_eq!(safe_div_u128(None, 10, 3), Ok(3));
    }

    #[test]
    fn u128_div_zero() {
        assert_eq!(safe_div_u128(None, 10, 0), Err(MathError::DivisionByZero));
    }

    // -----------------------------------------------------------------------
    // bps_of
    // -----------------------------------------------------------------------

    #[test]
    fn bps_of_normal() {
        assert_eq!(bps_of(None, 1_000_000, 100), Ok(10_000)); // 1% of 1M
        assert_eq!(bps_of(None, 10_000, 10_000), Ok(10_000)); // 100% of 10k
        assert_eq!(bps_of(None, 0, 9_999), Ok(0));
    }

    #[test]
    fn bps_of_overflow() {
        // amount * bps overflows i128
        assert_eq!(bps_of(None, i128::MAX, 2), Err(MathError::Overflow));
    }

    // -----------------------------------------------------------------------
    // ratio_bps
    // -----------------------------------------------------------------------

    #[test]
    fn ratio_bps_normal() {
        assert_eq!(ratio_bps(None, 50, 100), Ok(5_000));
        assert_eq!(ratio_bps(None, 100, 100), Ok(10_000));
        assert_eq!(ratio_bps(None, 0, 100), Ok(0));
    }

    #[test]
    fn ratio_bps_zero_denominator_returns_zero() {
        // Convention: no-exposure case must not error.
        assert_eq!(ratio_bps(None, 999, 0), Ok(0));
    }

    #[test]
    fn ratio_bps_overflow() {
        // numerator * BPS_DIVISOR overflows
        assert_eq!(ratio_bps(None, i128::MAX, 1), Err(MathError::Overflow));
    }

    // -----------------------------------------------------------------------
    // checked_mul_div
    // -----------------------------------------------------------------------

    #[test]
    fn mul_div_basic() {
        assert_eq!(checked_mul_div(None, 100, 200, 10), Ok(2_000));
        assert_eq!(checked_mul_div(None, 7, 3, 2), Ok(10));
    }

    #[test]
    fn mul_div_truncates_toward_zero() {
        assert_eq!(checked_mul_div(None, 10, 1, 3), Ok(3));
        assert_eq!(checked_mul_div(None, -10, 1, 3), Ok(-3));
    }

    #[test]
    fn mul_div_zero_denominator() {
        assert_eq!(
            checked_mul_div(None, 1, 1, 0),
            Err(MathError::DivisionByZero)
        );
    }

    #[test]
    fn mul_div_zero_operands() {
        assert_eq!(checked_mul_div(None, 0, i128::MAX, 7), Ok(0));
        assert_eq!(checked_mul_div(None, i128::MAX, 0, 7), Ok(0));
    }

    #[test]
    fn mul_div_fallback_on_wide_product() {
        // a * b overflows i128 but (a * b) / d is representable.
        let a = i128::MAX / 2;
        let result = checked_mul_div(None, a, 4, 8).unwrap();
        assert!((result - a / 2).abs() <= 1, "got {result}");
    }

    #[test]
    fn mul_div_unrepresentable_result() {
        assert_eq!(
            checked_mul_div(None, i128::MAX, i128::MAX, 1),
            Err(MathError::Overflow)
        );
    }

    #[test]
    fn mul_div_negative_denominator() {
        assert_eq!(checked_mul_div(None, 100, 200, -10), Ok(-2_000));
    }

    // -----------------------------------------------------------------------
    // Observability: events are emitted before error return
    // -----------------------------------------------------------------------

    #[test]
    fn overflow_event_emitted_on_add_overflow() {
        use soroban_sdk::testutils::Events;

        let e = env();
        let result = safe_add(Some(&e), i128::MAX, 1);
        assert_eq!(result, Err(MathError::Overflow));

        let events = e.events().all();
        assert!(
            !events.is_empty(),
            "expected at least one event on overflow"
        );
    }

    #[test]
    fn div_zero_event_emitted() {
        use soroban_sdk::testutils::Events;

        let e = env();
        let result = safe_div(Some(&e), 42, 0);
        assert_eq!(result, Err(MathError::DivisionByZero));

        let events = e.events().all();
        assert!(!events.is_empty(), "expected at least one event on div/0");
    }

    #[test]
    fn neg_sqrt_event_emitted() {
        use soroban_sdk::testutils::Events;

        let e = env();
        let result = safe_sqrt(Some(&e), -5);
        assert_eq!(result, Err(MathError::NegativeSqrt));

        let events = e.events().all();
        assert!(!events.is_empty(), "expected at least one event on neg sqrt");
    }

    #[test]
    fn no_event_on_success() {
        use soroban_sdk::testutils::Events;

        let e = env();
        safe_add(Some(&e), 1, 1).unwrap();
        // No error → no anomaly event expected.
        // (The events vec may contain other contract events but we only check
        // that our add did not publish an error event when successful.)
        // A successful add publishes nothing at all.
        assert!(e.events().all().is_empty());
    }

    // -----------------------------------------------------------------------
    // Property-style table checks
    // -----------------------------------------------------------------------

    #[test]
    fn safe_add_matches_checked_add_for_boundary_set() {
        let samples: &[i128] = &[
            0,
            1,
            -1,
            i128::MAX,
            i128::MIN,
            i128::MAX / 2,
            i128::MIN / 2,
            42,
            -42,
        ];
        for &a in samples {
            for &b in samples {
                let expected = a.checked_add(b);
                assert_eq!(
                    safe_add(None, a, b).ok(),
                    expected,
                    "safe_add({a}, {b})"
                );
            }
        }
    }

    #[test]
    fn safe_mul_matches_checked_mul_for_boundary_set() {
        let samples: &[i128] = &[
            0,
            1,
            -1,
            2,
            -2,
            i128::MAX,
            i128::MIN,
            i128::MAX / 2,
            i128::MIN / 2,
        ];
        for &a in samples {
            for &b in samples {
                let expected = a.checked_mul(b);
                assert_eq!(
                    safe_mul(None, a, b).ok(),
                    expected,
                    "safe_mul({a}, {b})"
                );
            }
        }
    }

    #[test]
    fn safe_div_matches_checked_div_for_nonzero_divisors() {
        let samples: &[i128] = &[1, -1, 2, -2, i128::MAX, i128::MIN, 100, -100];
        let divisors: &[i128] = &[1, -1, 2, -2, 3, i128::MAX];
        for &a in samples {
            for &b in divisors {
                let expected = a.checked_div(b);
                assert_eq!(
                    safe_div(None, a, b).ok(),
                    expected,
                    "safe_div({a}, {b})"
                );
            }
        }
    }
}
