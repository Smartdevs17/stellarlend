//! Policy-based rate-limit configuration.
//!
//! Replaces the hard-coded `default_config(env, op)` dispatch found in
//! `hello-world/src/rate_limiter.rs` with an explicit, layered policy model:
//!
//! ```text
//! default -> per-operation -> per-op+pool override
//! ```
//!
//! Layers are resolved from most-specific to least-specific, so an op+pool override,
//! when present, wins over the per-operation policy, which wins over the default.

use crate::token_bucket;

/// Where a policy value came from — used for configuration UIs and audit logging.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PolicyLayer {
    /// The crate-default baseline.
    Default,
    /// An operation-level override.
    Operation,
    /// An operation+pool override.
    OperationPool,
}

/// A single rate-limit configuration for an operation.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RateLimitConfig {
    /// Refill window in seconds.
    pub window_seconds: u64,
    /// Steady-state calls allowed per window.
    pub max_calls_per_window: u32,
    /// Additional burst capacity on top of `max_calls_per_window`.
    pub burst_calls: u32,
    /// Extra burst granted to whitelisted/high-frequency users.
    pub grace_burst_calls: u32,
}

/// Selects which effective config applies to an (operation, pool) pair.
///
/// This is the policy engine's resolver. Implementations read from their contract's
/// storage; custom selectors can layer governance/whitelist rules on top.
pub trait PolicySelector {
    /// Most-specific override for `(op, pool)`, if configured.
    fn op_pool_limit(&self, op: &str, pool_key: Option<u32>) -> Option<RateLimitConfig>;

    /// Operation-level override for `op`, if configured.
    fn op_limit(&self, op: &str) -> Option<RateLimitConfig>;

    /// Whether `op` is a sensitive operation that should be rate-limited at all.
    fn handles(&self, op: &str) -> bool;
}

/// Default, crate-wide policy for the core StellarLend operations.
///
/// Mirrors the conservative defaults previously encoded in `hello-world`'s
/// `default_config`: borrow and liquidate are the primary targets.
pub struct DefaultPolicy;

impl DefaultPolicy {
    /// The built-in baseline config per operation.
    pub fn config_for(&self, op: &str) -> Option<RateLimitConfig> {
        match op {
            "borrow" => Some(RateLimitConfig {
                window_seconds: 60,
                max_calls_per_window: 5,
                burst_calls: 3,
                grace_burst_calls: 10,
            }),
            "liquidate" => Some(RateLimitConfig {
                window_seconds: 60,
                max_calls_per_window: 10,
                burst_calls: 5,
                grace_burst_calls: 20,
            }),
            "deposit" | "repay" | "withdraw" => Some(RateLimitConfig {
                window_seconds: 60,
                max_calls_per_window: 30,
                burst_calls: 10,
                grace_burst_calls: 0,
            }),
            _ => None,
        }
    }
}

/// Resolves the effective configuration for an (op, pool) given a selector.
///
/// Resolution order: op+pool override → op override → default. Returns `None` when
/// `selector.handles(op)` is false (operation is not rate-limited) or no config is
/// known.
pub fn resolve_config(
    selector: &dyn PolicySelector,
    default: &DefaultPolicy,
    op: &str,
    pool_key: Option<u32>,
) -> Option<(RateLimitConfig, PolicyLayer)> {
    if !selector.handles(op) {
        return None;
    }
    if let Some(cfg) = selector.op_pool_limit(op, pool_key) {
        return Some((cfg, PolicyLayer::OperationPool));
    }
    if let Some(cfg) = selector.op_limit(op) {
        return Some((cfg, PolicyLayer::Operation));
    }
    default
        .config_for(op)
        .map(|cfg| (cfg, PolicyLayer::Default))
}

/// Summary of the resolution, matching what an operator-facing config API would expose.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLimit {
    pub config: RateLimitConfig,
    pub layer: PolicyLayer,
}

/// Convenience wrapper returning a [`ResolvedLimit`].
pub fn resolve(
    selector: &dyn PolicySelector,
    default: &DefaultPolicy,
    op: &str,
    pool_key: Option<u32>,
) -> Option<ResolvedLimit> {
    resolve_config(selector, default, op, pool_key)
        .map(|(config, layer)| ResolvedLimit { config, layer })
}

impl ResolvedLimit {
    /// Effective token capacity for this resolved config (no grace).
    pub fn capacity(&self) -> i128 {
        token_bucket::capacity_tokens(self.config.max_calls_per_window, self.config.burst_calls, 0)
    }

    /// Effective refill rate (scaled tokens/sec).
    pub fn refill_per_second(&self) -> Result<i128, token_bucket::TokenBucketError> {
        token_bucket::refill_per_second(
            self.config.window_seconds,
            self.config.max_calls_per_window,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    struct TestSelector {
        op_pool: Option<(String, u32, RateLimitConfig)>,
        op: Option<(String, RateLimitConfig)>,
        handled: Vec<String>,
    }

    impl PolicySelector for TestSelector {
        fn op_pool_limit(&self, op: &str, pool_key: Option<u32>) -> Option<RateLimitConfig> {
            match &self.op_pool {
                Some((o, p, cfg)) if o == op && Some(*p) == pool_key => Some(*cfg),
                _ => None,
            }
        }
        fn op_limit(&self, op: &str) -> Option<RateLimitConfig> {
            self.op
                .as_ref()
                .filter(|(o, _)| o == op)
                .map(|(_, cfg)| *cfg)
        }
        fn handles(&self, op: &str) -> bool {
            self.handled.iter().any(|h| h == op)
        }
    }

    fn cfg(win: u64, max: u32, burst: u32) -> RateLimitConfig {
        RateLimitConfig {
            window_seconds: win,
            max_calls_per_window: max,
            burst_calls: burst,
            grace_burst_calls: 0,
        }
    }

    #[test]
    fn op_pool_override_wins() {
        let sel = TestSelector {
            op_pool: Some(("borrow".into(), 7, cfg(30, 2, 1))),
            op: Some(("borrow".into(), cfg(60, 5, 3))),
            handled: vec!["borrow".into()],
        };
        let (limit, layer) = resolve_config(&sel, &DefaultPolicy, "borrow", Some(7)).unwrap();
        assert_eq!(layer, PolicyLayer::OperationPool);
        assert_eq!(limit.max_calls_per_window, 2);
    }

    #[test]
    fn op_override_over_default() {
        let sel = TestSelector {
            op_pool: None,
            op: Some(("borrow".into(), cfg(60, 9, 4))),
            handled: vec!["borrow".into()],
        };
        let (limit, layer) = resolve_config(&sel, &DefaultPolicy, "borrow", None).unwrap();
        assert_eq!(layer, PolicyLayer::Operation);
        assert_eq!(limit.max_calls_per_window, 9);
    }

    #[test]
    fn falls_back_to_default_and_layer_tracks_it() {
        let sel = TestSelector {
            op_pool: None,
            op: None,
            handled: vec!["borrow".into()],
        };
        let (limit, layer) = resolve_config(&sel, &DefaultPolicy, "borrow", None).unwrap();
        assert_eq!(layer, PolicyLayer::Default);
        assert_eq!(limit.max_calls_per_window, 5);
    }

    #[test]
    fn unhandled_op_returns_none() {
        let sel = TestSelector {
            op_pool: None,
            op: None,
            handled: vec!["borrow".into()],
        };
        assert!(resolve_config(&sel, &DefaultPolicy, "unknown_op", None).is_none());
    }

    #[test]
    fn resolved_limit_exposes_math() {
        let sel = TestSelector {
            op_pool: None,
            op: None,
            handled: vec!["borrow".into()],
        };
        let limit = resolve(&sel, &DefaultPolicy, "borrow", None).unwrap();
        assert_eq!(limit.capacity(), token_bucket::capacity_tokens(5, 3, 0));
        assert!(limit.refill_per_second().is_ok());
    }
}
