use crate::error::MathError;
use crate::int128::safe_sqrt;
use soroban_sdk::{Env, I256};

pub const WAD: i128 = 1_000_000_000_000_000_000;
pub const RAY: i128 = 1_000_000_000_000_000_000_000_000_000;
pub const HALF_WAD: i128 = 500_000_000_000_000_000;
pub const SECONDS_PER_YEAR: u64 = 31_536_000;

// ── FixedPoint<W> wrapper type ────────────────────────────────────────────────
//
// Issue #1598: Replace raw `i128` arithmetic with a typed wrapper so callers
// cannot accidentally mix WAD and RAY values, or raw token amounts with
// scaled prices, without an explicit conversion.
//
// ## Design
//
// `FixedPoint<W>` is a newtype over `i128` parameterised by a const `W: i128`
// that carries the scale factor (WAD = 1e18, RAY = 1e27, BPS = 10_000, …).
// The scale is part of the type and never stored at run time, so the memory
// layout is identical to a bare `i128`.
//
// All arithmetic that requires widening goes through `I256` (one `Env` call per
// op), consistent with the rest of this module.  Addition and subtraction stay
// in `i128` via `safe_add` / `safe_sub`.
//
// The `From<FixedPoint<W>> for i128` impl intentionally exposes the inner value
// so hot paths in contracts can drop back to raw arithmetic when needed.

/// A fixed-point number with scale `W`.
///
/// # Type-level scale
///
/// | Alias          | `W`   | Use it for                              |
/// |----------------|-------|------------------------------------------|
/// | [`WadFP`]      | 1e18  | most per-unit fractions and prices       |
/// | [`RayFP`]      | 1e27  | cumulative interest indices              |
/// | [`BpsFP`]      | 10000 | basis-points ratios (rates, factors)     |
///
/// # Arithmetic
///
/// `add` / `sub` are scale-preserving and checked.
/// `mul` / `div` produce a result at the same scale by intermediate widening.
/// Mixing scales requires an explicit [`FixedPoint::to_scale`] conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedPoint<const W: i128>(i128);

/// WAD-scaled fixed-point (1e18).
pub type WadFP = FixedPoint<WAD>;
/// RAY-scaled fixed-point (1e27).
pub type RayFP = FixedPoint<RAY>;
/// Basis-points fixed-point (1e4 = 10 000).
pub type BpsFP = FixedPoint<10_000>;

impl<const W: i128> FixedPoint<W> {
    /// Construct from a raw scaled integer.
    ///
    /// The caller is responsible for the value being expressed in scale `W`.
    #[inline]
    pub const fn from_raw(raw: i128) -> Self {
        Self(raw)
    }

    /// Construct from an integer by multiplying by the scale.
    ///
    /// `FixedPoint::<WAD>::from_int(1)` → `1e18` (represents 1.0 in WAD).
    pub fn from_int(integer: i128) -> Result<Self, MathError> {
        integer
            .checked_mul(W)
            .map(Self)
            .ok_or(MathError::Overflow)
    }

    /// Raw inner value (already multiplied by the scale).
    #[inline]
    pub const fn raw(self) -> i128 {
        self.0
    }

    /// Integer part (truncates toward zero).
    #[inline]
    pub const fn floor(self) -> i128 {
        self.0 / W
    }

    /// `true` if the value is zero.
    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// `true` if the value is strictly positive.
    #[inline]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// Checked addition, preserving scale `W`.
    pub fn add(self, rhs: Self) -> Result<Self, MathError> {
        crate::int128::safe_add(self.0, rhs.0).map(Self)
    }

    /// Checked subtraction, preserving scale `W`.
    pub fn sub(self, rhs: Self) -> Result<Self, MathError> {
        crate::int128::safe_sub(self.0, rhs.0).map(Self)
    }

    /// Multiply two `FixedPoint<W>` values, returning a `FixedPoint<W>`.
    ///
    /// Uses `I256` for the intermediate product to avoid overflow on large
    /// WAD-scaled values, then divides by `W` to restore the scale.
    pub fn mul(self, env: &Env, rhs: Self) -> Result<Self, MathError> {
        let a = I256::from_i128(env, self.0);
        let b = I256::from_i128(env, rhs.0);
        let w = I256::from_i128(env, W);
        let product = a.mul(&b).div(&w);
        product.to_i128().map(Self).ok_or(MathError::Overflow)
    }

    /// Divide two `FixedPoint<W>` values, returning a `FixedPoint<W>`.
    ///
    /// Uses `I256` to avoid losing precision in the numerator shift.
    pub fn div(self, env: &Env, rhs: Self) -> Result<Self, MathError> {
        if rhs.0 == 0 {
            return Err(MathError::DivisionByZero);
        }
        let a = I256::from_i128(env, self.0);
        let w = I256::from_i128(env, W);
        let b = I256::from_i128(env, rhs.0);
        let result = a.mul(&w).div(&b);
        result.to_i128().map(Self).ok_or(MathError::Overflow)
    }

    /// Multiply a `FixedPoint<W>` by a plain `i128` scalar (not scaled).
    ///
    /// Useful for multiplying a WAD price by a raw token amount.
    pub fn scale_by(self, scalar: i128) -> Result<Self, MathError> {
        self.0
            .checked_mul(scalar)
            .map(Self)
            .ok_or(MathError::Overflow)
    }

    /// Convert this value from scale `W` to scale `V`.
    ///
    /// Performs the conversion as `(raw * V) / W` using `I256` to avoid
    /// intermediate overflow.  The result is truncated (floor toward zero).
    pub fn to_scale<const V: i128>(&self, env: &Env) -> Result<FixedPoint<V>, MathError> {
        if V == W {
            return Ok(FixedPoint::<V>(self.0));
        }
        let raw = I256::from_i128(env, self.0);
        let v = I256::from_i128(env, V);
        let w = I256::from_i128(env, W);
        let result = raw.mul(&v).div(&w);
        result
            .to_i128()
            .map(FixedPoint::<V>)
            .ok_or(MathError::Overflow)
    }

    /// Square root, returning a `FixedPoint<W>`.
    ///
    /// For WAD: `sqrt(4e18)` → `2e18`.
    pub fn sqrt(self, env: &Env) -> Result<Self, MathError> {
        fp_sqrt(env, self.0).map(Self)
    }

    /// Raise to a non-negative integer power using square-and-multiply.
    pub fn pow(self, env: &Env, exp: u32) -> Result<Self, MathError> {
        fp_pow(env, self.0, exp).map(Self)
    }
}

impl<const W: i128> From<FixedPoint<W>> for i128 {
    #[inline]
    fn from(fp: FixedPoint<W>) -> i128 {
        fp.0
    }
}

impl<const W: i128> core::fmt::Display for FixedPoint<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Show as "integer.fraction" at scale W.
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let w = W.unsigned_abs() as u128;
        let integer = abs / w;
        let frac = abs % w;
        write!(f, "{sign}{integer}.{frac:0>18}")
    }
}

// ── WAD operations ───────────────────────────────────────────────────────────

pub fn fp_mul(env: &Env, a: i128, b: i128) -> Result<i128, MathError> {
    let a256 = I256::from_i128(env, a);
    let b256 = I256::from_i128(env, b);
    let wad256 = I256::from_i128(env, WAD);
    let product = a256.mul(&b256);
    let result = product.div(&wad256);
    result.to_i128().ok_or(MathError::Overflow)
}

pub fn fp_div(env: &Env, a: i128, b: i128) -> Result<i128, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero);
    }
    let a256 = I256::from_i128(env, a);
    let wad256 = I256::from_i128(env, WAD);
    let b256 = I256::from_i128(env, b);
    let numerator = a256.mul(&wad256);
    let result = numerator.div(&b256);
    result.to_i128().ok_or(MathError::Overflow)
}

#[inline]
pub fn fp_add(a: i128, b: i128) -> Result<i128, MathError> {
    crate::int128::safe_add(a, b)
}

#[inline]
pub fn fp_sub(a: i128, b: i128) -> Result<i128, MathError> {
    crate::int128::safe_sub(a, b)
}

pub fn fp_sqrt(env: &Env, a: i128) -> Result<i128, MathError> {
    if a < 0 {
        return Err(MathError::NegativeSqrt);
    }
    if a == 0 {
        return Ok(0);
    }
    let a256 = I256::from_i128(env, a);
    let wad256 = I256::from_i128(env, WAD);
    let scaled = a256.mul(&wad256);
    let scaled_i128 = scaled.to_i128();
    if let Some(s) = scaled_i128 {
        safe_sqrt(s)
    } else {
        i256_isqrt(env, scaled)
    }
}

pub fn fp_pow(env: &Env, base: i128, exp: u32) -> Result<i128, MathError> {
    if exp == 0 {
        return Ok(WAD);
    }
    let mut result = WAD;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 {
            result = fp_mul(env, result, b)?;
        }
        e >>= 1;
        if e > 0 {
            b = fp_mul(env, b, b)?;
        }
    }
    Ok(result)
}

// ── RAY operations ───────────────────────────────────────────────────────────

pub fn ray_mul(env: &Env, a: i128, b: i128) -> Result<i128, MathError> {
    let a256 = I256::from_i128(env, a);
    let b256 = I256::from_i128(env, b);
    let ray256 = I256::from_i128(env, RAY);
    let product = a256.mul(&b256);
    let result = product.div(&ray256);
    result.to_i128().ok_or(MathError::Overflow)
}

pub fn ray_div(env: &Env, a: i128, b: i128) -> Result<i128, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero);
    }
    let a256 = I256::from_i128(env, a);
    let ray256 = I256::from_i128(env, RAY);
    let b256 = I256::from_i128(env, b);
    let numerator = a256.mul(&ray256);
    let result = numerator.div(&b256);
    result.to_i128().ok_or(MathError::Overflow)
}

pub fn ray_to_wad(env: &Env, a: i128) -> Result<i128, MathError> {
    if a == 0 {
        return Ok(0);
    }
    let a256 = I256::from_i128(env, a);
    let factor = I256::from_i128(env, 10i128.pow(9));
    let result = a256.div(&factor);
    result.to_i128().ok_or(MathError::Overflow)
}

pub fn wad_to_ray(env: &Env, a: i128) -> Result<i128, MathError> {
    if a == 0 {
        return Ok(0);
    }
    let a256 = I256::from_i128(env, a);
    let factor = I256::from_i128(env, 10i128.pow(9));
    let result = a256.mul(&factor);
    result.to_i128().ok_or(MathError::Overflow)
}

// ── Interest-rate helpers ────────────────────────────────────────────────────

pub fn simple_interest(
    env: &Env,
    principal: i128,
    rate_bps: i128,
    elapsed_secs: u64,
) -> Result<i128, MathError> {
    if elapsed_secs == 0 || principal == 0 || rate_bps == 0 {
        return Ok(0);
    }
    let p256 = I256::from_i128(env, principal);
    let r256 = I256::from_i128(env, rate_bps);
    let t256 = I256::from_i128(env, elapsed_secs as i128);
    let bps256 = I256::from_i128(env, 10_000);
    let spy256 = I256::from_i128(env, SECONDS_PER_YEAR as i128);
    let result = p256.mul(&r256).mul(&t256).div(&bps256).div(&spy256);
    result.to_i128().ok_or(MathError::Overflow)
}

pub fn bps_ratio(env: &Env, numerator: i128, denominator: i128) -> Result<i128, MathError> {
    if denominator == 0 {
        return Err(MathError::DivisionByZero);
    }
    let n256 = I256::from_i128(env, numerator);
    let bps256 = I256::from_i128(env, 10_000);
    let d256 = I256::from_i128(env, denominator);
    let result = n256.mul(&bps256).div(&d256);
    result.to_i128().ok_or(MathError::Overflow)
}

fn i256_isqrt(env: &Env, n: I256) -> Result<i128, MathError> {
    let zero = I256::from_i128(env, 0);
    let two = I256::from_i128(env, 2);
    if n == zero {
        return Ok(0);
    }
    let mut x = n.div(&two);
    let mut y = x.add(&n.div(&x)).div(&two);
    while y < x {
        x = y.clone();
        y = x.add(&n.div(&x)).div(&two);
    }
    x.to_i128().ok_or(MathError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    fn env() -> Env {
        Env::default()
    }

    #[test]
    fn fp_mul_unit() {
        let e = env();
        assert_eq!(fp_mul(&e, WAD, WAD), Ok(WAD));
        let one_half = WAD + WAD / 2;
        assert_eq!(fp_mul(&e, one_half, 2 * WAD), Ok(3 * WAD));
    }

    #[test]
    fn fp_mul_zero() {
        let e = env();
        assert_eq!(fp_mul(&e, 0, WAD), Ok(0));
    }

    #[test]
    fn fp_mul_overflow_result() {
        let e = env();
        assert!(fp_mul(&e, i128::MAX, i128::MAX).is_err());
    }

    #[test]
    fn fp_div_unit() {
        let e = env();
        assert_eq!(fp_div(&e, 3 * WAD, 2 * WAD), Ok(WAD + WAD / 2));
        assert_eq!(fp_div(&e, WAD, WAD), Ok(WAD));
    }

    #[test]
    fn fp_div_by_zero() {
        let e = env();
        assert_eq!(fp_div(&e, WAD, 0), Err(MathError::DivisionByZero));
    }

    #[test]
    fn fp_sqrt_unit() {
        let e = env();
        assert_eq!(fp_sqrt(&e, WAD), Ok(WAD));
        assert_eq!(fp_sqrt(&e, 4 * WAD), Ok(2 * WAD));
        assert_eq!(fp_sqrt(&e, 0), Ok(0));
    }

    #[test]
    fn fp_sqrt_floor_property() {
        let e = env();
        let inputs: &[i128] = &[WAD, 2 * WAD, 3 * WAD, 100 * WAD, 1_000_000 * WAD];
        for &a in inputs {
            let r = fp_sqrt(&e, a).unwrap();
            let r_sq = fp_mul(&e, r, r).unwrap();
            assert!(r_sq <= a, "fp_sqrt floor fail: r²={r_sq} > a={a}");
        }
    }

    #[test]
    fn fp_sqrt_negative() {
        let e = env();
        assert_eq!(fp_sqrt(&e, -1), Err(MathError::NegativeSqrt));
    }

    #[test]
    fn fp_pow_zero_exp() {
        let e = env();
        assert_eq!(fp_pow(&e, 2 * WAD, 0), Ok(WAD));
    }

    #[test]
    fn fp_pow_square() {
        let e = env();
        assert_eq!(fp_pow(&e, 2 * WAD, 2), Ok(4 * WAD));
        let one_five = WAD + WAD / 2;
        let two_twenty_five = 2 * WAD + WAD / 4;
        assert_eq!(fp_pow(&e, one_five, 2), Ok(two_twenty_five));
    }

    #[test]
    fn fp_pow_exponent_one() {
        let e = env();
        assert_eq!(fp_pow(&e, 5 * WAD, 1), Ok(5 * WAD));
    }

    #[test]
    fn ray_mul_normal() {
        let e = env();
        assert_eq!(ray_mul(&e, RAY, RAY), Ok(RAY));
    }

    #[test]
    fn ray_div_normal() {
        let e = env();
        assert_eq!(ray_div(&e, RAY, RAY), Ok(RAY));
    }

    #[test]
    fn ray_div_by_zero() {
        let e = env();
        assert_eq!(ray_div(&e, RAY, 0), Err(MathError::DivisionByZero));
    }

    #[test]
    fn ray_wad_conversion_round_trip() {
        let e = env();
        let wad_val = WAD;
        let ray_val = wad_to_ray(&e, wad_val).unwrap();
        let back = ray_to_wad(&e, ray_val).unwrap();
        assert_eq!(wad_val, back);
    }

    #[test]
    fn ray_wad_conversion_zero() {
        let e = env();
        assert_eq!(ray_to_wad(&e, 0), Ok(0));
        assert_eq!(wad_to_ray(&e, 0), Ok(0));
    }

    #[test]
    fn simple_interest_annual() {
        let e = env();
        let interest = simple_interest(&e, 100_000, 500, SECONDS_PER_YEAR).unwrap();
        assert_eq!(interest, 5_000);
    }

    #[test]
    fn simple_interest_zero_elapsed() {
        let e = env();
        assert_eq!(simple_interest(&e, 1_000_000, 500, 0), Ok(0));
    }

    #[test]
    fn simple_interest_large_principal() {
        let e = env();
        let principal = 1_000_000_000_000_000_000_000_000_000_000i128;
        let interest = simple_interest(&e, principal, 500, SECONDS_PER_YEAR);
        assert!(interest.is_ok());
        assert!(interest.unwrap() > 10_000_000_000_000_000_000_000_000_000i128);
    }

    #[test]
    fn bps_ratio_normal() {
        let e = env();
        assert_eq!(bps_ratio(&e, 5_000, 10_000), Ok(5_000));
        assert_eq!(bps_ratio(&e, 10_000, 10_000), Ok(10_000));
    }

    #[test]
    fn bps_ratio_div_zero() {
        let e = env();
        assert_eq!(bps_ratio(&e, 1_000, 0), Err(MathError::DivisionByZero));
    }

    #[test]
    fn fp_sqrt_precision_boundary() {
        let e = env();
        let just_under = WAD - 1;
        let result = fp_sqrt(&e, just_under).unwrap();
        assert!(result <= WAD);
    }

    // ── FixedPoint<W> wrapper tests ───────────────────────────────────────────

    #[test]
    fn fixed_point_from_int_round_trip() {
        let one = WadFP::from_int(1).unwrap();
        assert_eq!(one.raw(), WAD);
        assert_eq!(one.floor(), 1);
    }

    #[test]
    fn fixed_point_add() {
        let a = WadFP::from_raw(WAD);
        let b = WadFP::from_raw(WAD / 2);
        let sum = a.add(b).unwrap();
        assert_eq!(sum.raw(), WAD + WAD / 2);
    }

    #[test]
    fn fixed_point_sub() {
        let a = WadFP::from_raw(3 * WAD);
        let b = WadFP::from_raw(WAD);
        assert_eq!(a.sub(b).unwrap().raw(), 2 * WAD);
    }

    #[test]
    fn fixed_point_mul_identity() {
        let e = env();
        let a = WadFP::from_raw(3 * WAD);
        let one = WadFP::from_raw(WAD);
        assert_eq!(a.mul(&e, one).unwrap().raw(), 3 * WAD);
    }

    #[test]
    fn fixed_point_mul_half() {
        let e = env();
        let a = WadFP::from_raw(4 * WAD);
        let half = WadFP::from_raw(WAD / 2);
        assert_eq!(a.mul(&e, half).unwrap().raw(), 2 * WAD);
    }

    #[test]
    fn fixed_point_div_identity() {
        let e = env();
        let a = WadFP::from_raw(3 * WAD);
        let one = WadFP::from_raw(WAD);
        assert_eq!(a.div(&e, one).unwrap().raw(), 3 * WAD);
    }

    #[test]
    fn fixed_point_div_by_zero() {
        let e = env();
        let a = WadFP::from_raw(WAD);
        let zero = WadFP::from_raw(0);
        assert_eq!(a.div(&e, zero), Err(MathError::DivisionByZero));
    }

    #[test]
    fn fixed_point_scale_by() {
        let a = WadFP::from_raw(WAD);
        assert_eq!(a.scale_by(5).unwrap().raw(), 5 * WAD);
    }

    #[test]
    fn fixed_point_to_scale_wad_to_bps() {
        let e = env();
        // 1.0 in WAD → 10_000 in BPS
        let one_wad = WadFP::from_raw(WAD);
        let one_bps: BpsFP = one_wad.to_scale::<10_000>(&e).unwrap();
        assert_eq!(one_bps.raw(), 10_000);
    }

    #[test]
    fn fixed_point_sqrt() {
        let e = env();
        let four = WadFP::from_raw(4 * WAD);
        assert_eq!(four.sqrt(&e).unwrap().raw(), 2 * WAD);
    }

    #[test]
    fn fixed_point_pow_square() {
        let e = env();
        let two = WadFP::from_raw(2 * WAD);
        assert_eq!(two.pow(&e, 2).unwrap().raw(), 4 * WAD);
    }

    #[test]
    fn fixed_point_pow_zero() {
        let e = env();
        let two = WadFP::from_raw(2 * WAD);
        // x^0 = 1.0
        assert_eq!(two.pow(&e, 0).unwrap().raw(), WAD);
    }

    #[test]
    fn fixed_point_is_zero() {
        assert!(WadFP::from_raw(0).is_zero());
        assert!(!WadFP::from_raw(1).is_zero());
    }

    #[test]
    fn fixed_point_is_positive() {
        assert!(WadFP::from_raw(1).is_positive());
        assert!(!WadFP::from_raw(0).is_positive());
        assert!(!WadFP::from_raw(-1).is_positive());
    }

    #[test]
    fn fixed_point_into_i128() {
        let fp = WadFP::from_raw(42);
        let raw: i128 = fp.into();
        assert_eq!(raw, 42);
    }

    #[test]
    fn fixed_point_ray_mul_identity() {
        let e = env();
        let a = RayFP::from_raw(RAY);
        let b = RayFP::from_raw(RAY);
        assert_eq!(a.mul(&e, b).unwrap().raw(), RAY);
    }

    #[test]
    fn fixed_point_mixed_scale_via_to_scale() {
        let e = env();
        // Convert 2.0 WAD → RAY scale
        let two_wad = WadFP::from_raw(2 * WAD);
        let two_ray: RayFP = two_wad.to_scale::<{ RAY }>(&e).unwrap();
        assert_eq!(two_ray.raw(), 2 * RAY);
    }
}
