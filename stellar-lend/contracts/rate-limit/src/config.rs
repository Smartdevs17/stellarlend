//! Rate-limit configuration API.
//!
//! Provides lossless encoding/decoding between the library's in-memory
//! [`crate::policy::RateLimitConfig`] and a packed `u128` (or a verbose field tuple) so
//! a configuration endpoint can read/write policies compactly and deterministically.
//!
//! ## Packed layout
//!
//! ```text
//! bits 0..32   window_seconds (u32)
//! bits 32..64  max_calls_per_window (u32)
//! bits 64..96  burst_calls (u32)
//! bits 96..128 grace_burst_calls (u32)
//! ```

use crate::policy::RateLimitConfig;

/// Pack a [`RateLimitConfig`] into a compact `u128` for storage / transport.
pub fn pack_config(cfg: &RateLimitConfig) -> u128 {
    (cfg.window_seconds as u128)
        | ((cfg.max_calls_per_window as u128) << 32)
        | ((cfg.burst_calls as u128) << 64)
        | ((cfg.grace_burst_calls as u128) << 96)
}

/// Decode a [`RateLimitConfig`] produced by [`pack_config`].
pub fn unpack_config(packed: u128) -> RateLimitConfig {
    const MASK: u128 = 0xFFFF_FFFF;
    RateLimitConfig {
        window_seconds: (packed & MASK) as u64,
        max_calls_per_window: ((packed >> 32) & MASK) as u32,
        burst_calls: ((packed >> 64) & MASK) as u32,
        grace_burst_calls: ((packed >> 96) & MASK) as u32,
    }
}

/// A textual policy descriptor suitable for a configuration REST endpoint.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PolicyDescriptor {
    pub op: &'static str,
    pub config: RateLimitConfig,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_round_trips() {
        let cfg = RateLimitConfig {
            window_seconds: 60,
            max_calls_per_window: 5,
            burst_calls: 3,
            grace_burst_calls: 10,
        };
        assert_eq!(unpack_config(pack_config(&cfg)), cfg);
    }

    #[test]
    fn pack_is_deterministic() {
        let cfg = RateLimitConfig {
            window_seconds: 30,
            max_calls_per_window: 2,
            burst_calls: 1,
            grace_burst_calls: 0,
        };
        assert_eq!(pack_config(&cfg), pack_config(&cfg));
    }
}
