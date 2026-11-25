//! Collateral haircut calculation by asset volatility bucket (issue #1742).
//!
//! Applies risk-adjusted discounts to posted collateral values based on
//! calibrated volatility tiers. Higher volatility assets receive steeper haircuts
//! to insulate the lending pool from sudden collateral value erosion before
//! liquidations can settle.

#![no_std]

/// Basis points scale (10,000 bps = 100%).
pub const BPS_SCALE: i128 = 10_000;

/// Asset volatility classification bucket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum VolatilityBucket {
    /// Ultra-stable assets (e.g. fiat-backed cash equivalents, stablecoins).
    Low = 0,
    /// Moderately volatile blue-chip crypto assets (e.g. XLM, BTC, ETH).
    Medium = 1,
    /// High-volatility governance, synthetic, or long-tail assets.
    High = 2,
    /// Unrated or speculative high-beta assets.
    Extreme = 3,
}

impl VolatilityBucket {
    /// Default haircut in basis points applied to collateral in this bucket.
    pub const fn default_haircut_bps(&self) -> i128 {
        match self {
            Self::Low => 500,       // 5.0% haircut (95% collateral value)
            Self::Medium => 1_500,  // 15.0% haircut (85% collateral value)
            Self::High => 3_000,    // 30.0% haircut (70% collateral value)
            Self::Extreme => 5_000, // 50.0% haircut (50% collateral value)
        }
    }
}

/// Applies a collateral haircut to a raw asset valuation.
///
/// # Arguments
/// * `raw_value` - The nominal valuation in pool quote units.
/// * `haircut_bps` - Haircut in basis points (0..=10,000).
///
/// Returns the effective risk-adjusted collateral value.
pub fn apply_haircut(raw_value: i128, haircut_bps: i128) -> Result<i128, &'static str> {
    if raw_value < 0 {
        return Err("raw_value must be non-negative");
    }
    if haircut_bps < 0 || haircut_bps > BPS_SCALE {
        return Err("haircut_bps must be between 0 and 10,000");
    }
    let effective_bps = BPS_SCALE - haircut_bps;
    Ok((raw_value * effective_bps) / BPS_SCALE)
}

/// Computes the effective collateral value given a nominal value and asset volatility tier.
pub fn compute_bucketed_value(raw_value: i128, bucket: VolatilityBucket) -> i128 {
    apply_haircut(raw_value, bucket.default_haircut_bps()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_low_volatility_haircut() {
        let raw = 100_000;
        let effective = compute_bucketed_value(raw, VolatilityBucket::Low);
        assert_eq!(effective, 95_000);
    }

    #[test]
    fn test_medium_volatility_haircut() {
        let raw = 100_000;
        let effective = compute_bucketed_value(raw, VolatilityBucket::Medium);
        assert_eq!(effective, 85_000);
    }

    #[test]
    fn test_high_volatility_haircut() {
        let raw = 100_000;
        let effective = compute_bucketed_value(raw, VolatilityBucket::High);
        assert_eq!(effective, 70_000);
    }

    #[test]
    fn test_extreme_volatility_haircut() {
        let raw = 100_000;
        let effective = compute_bucketed_value(raw, VolatilityBucket::Extreme);
        assert_eq!(effective, 50_000);
    }

    #[test]
    fn test_invalid_haircut_bps() {
        assert!(apply_haircut(100, -1).is_err());
        assert!(apply_haircut(100, 10_001).is_err());
        assert!(apply_haircut(-50, 1000).is_err());
    }
}
