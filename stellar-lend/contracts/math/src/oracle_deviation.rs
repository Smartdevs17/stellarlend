//! Oracle price-deviation alerts.
//!
//! Compares an observed oracle price with a reference (e.g. TWAP or a second
//! source) and classifies the absolute deviation against configurable
//! warning / critical thresholds in bps.

use crate::checked::{checked_mul_div, BPS_DIVISOR};
use crate::error::MathError;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum DeviationLevel {
    Normal,
    Warning,
    Critical,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DeviationConfigError {
    /// Thresholds must satisfy `0 < warning <= critical`.
    InvalidThresholds,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct DeviationThresholds {
    pub warning_bps: i128,
    pub critical_bps: i128,
}

impl DeviationThresholds {
    pub fn new(warning_bps: i128, critical_bps: i128) -> Result<Self, DeviationConfigError> {
        if warning_bps <= 0 || critical_bps < warning_bps {
            return Err(DeviationConfigError::InvalidThresholds);
        }
        Ok(Self {
            warning_bps,
            critical_bps,
        })
    }

    pub fn classify(&self, deviation_bps: i128) -> DeviationLevel {
        if deviation_bps >= self.critical_bps {
            DeviationLevel::Critical
        } else if deviation_bps >= self.warning_bps {
            DeviationLevel::Warning
        } else {
            DeviationLevel::Normal
        }
    }
}

/// `|observed - reference| / reference` in bps, rounded down.
///
/// # Errors
/// [`MathError::DivisionByZero`] for a non-positive reference price and
/// [`MathError::Underflow`] for a negative observed price.
pub fn deviation_bps(reference: i128, observed: i128) -> Result<i128, MathError> {
    if reference <= 0 {
        return Err(MathError::DivisionByZero);
    }
    if observed < 0 {
        return Err(MathError::Underflow);
    }
    checked_mul_div((observed - reference).abs(), BPS_DIVISOR, reference)
}

/// Deviation plus its alert level, for emitting as an event or API alert.
pub fn check_deviation(
    reference: i128,
    observed: i128,
    thresholds: &DeviationThresholds,
) -> Result<(i128, DeviationLevel), MathError> {
    let bps = deviation_bps(reference, observed)?;
    Ok((bps, thresholds.classify(bps)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thresholds() -> DeviationThresholds {
        DeviationThresholds::new(200, 500).unwrap() // 2% warn, 5% critical
    }

    #[test]
    fn deviation_is_symmetric_in_direction() {
        assert_eq!(deviation_bps(10_000, 10_300), Ok(300));
        assert_eq!(deviation_bps(10_000, 9_700), Ok(300));
        assert_eq!(deviation_bps(10_000, 10_000), Ok(0));
    }

    #[test]
    fn classifies_at_threshold_boundaries() {
        let t = thresholds();
        assert_eq!(
            check_deviation(10_000, 10_199, &t),
            Ok((199, DeviationLevel::Normal))
        );
        assert_eq!(
            check_deviation(10_000, 10_200, &t),
            Ok((200, DeviationLevel::Warning))
        );
        assert_eq!(
            check_deviation(10_000, 9_500, &t),
            Ok((500, DeviationLevel::Critical))
        );
        assert_eq!(
            check_deviation(10_000, 0, &t),
            Ok((10_000, DeviationLevel::Critical))
        );
    }

    #[test]
    fn rejects_bad_prices_and_config() {
        assert_eq!(deviation_bps(0, 100), Err(MathError::DivisionByZero));
        assert_eq!(deviation_bps(100, -1), Err(MathError::Underflow));
        assert_eq!(
            DeviationThresholds::new(0, 100),
            Err(DeviationConfigError::InvalidThresholds)
        );
        assert_eq!(
            DeviationThresholds::new(500, 200),
            Err(DeviationConfigError::InvalidThresholds)
        );
    }
}
