//! Error logging helpers.
//!
//! Soroban has no structured logger in `no_std` without extra runtimes; these helpers
//! centralize *how* errors are annotated so every contract emits a consistent,
//! greppable `Symbol` topic plus a human-readable description payload. Deployers can
//! swap the underlying emission to `env.events()` or a custom logger without touching
//! call sites.

use soroban_sdk::{Env, String, Symbol, Vec};

use crate::IntoError;

/// Emits an environment event carrying the normalized error category and a short
/// description string. This is the recommended cross-contract logging primitive.
///
/// The event topics are `["log_error", category, source]` and the data payload is the
/// `source` description. Keeping topics as `Symbol`s makes them indexable by off-chain
/// indexers.
///
/// Note: uses the free-function `env.events().publish` form (same legacy path as
/// `common/message_bus.rs`) because `#[contractevent]` requires a concrete
/// `#[contractimpl]` context that a reusable no_std utility crate doesn't own.
#[allow(deprecated, clippy::needless_pass_by_value)]
pub fn log_error<E: IntoError>(env: &Env, source: &str, error: E) {
    let category = error.into_core().tag(env);
    let mut topics = Vec::new(env);
    topics.push_back(Symbol::new(env, "log_error"));
    topics.push_back(category);
    topics.push_back(Symbol::new(env, source));

    env.events().publish(topics, String::from_str(env, source));
}

/// Like [`log_error`] but tags each entry with a caller-supplied `tag` symbol, useful
/// for grouping several modules under one analytics key.
#[allow(deprecated, clippy::needless_pass_by_value)]
pub fn log_error_with_tag<E: IntoError>(env: &Env, tag: Symbol, source: &str, error: E) {
    let category = error.into_core().tag(env);
    let mut topics = Vec::new(env);
    topics.push_back(tag);
    topics.push_back(category);
    topics.push_back(Symbol::new(env, source));

    env.events().publish(topics, String::from_str(env, source));
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::CoreError;

    struct DummyError;
    impl IntoError for DummyError {
        fn into_core(self) -> CoreError {
            CoreError::Overflow
        }
    }

    #[test]
    fn log_error_publishes_event() {
        // Publishing events outside a contract context is legal in tests; we just
        // verify the call does not panic. This exercises the emit path.
        let env = Env::default();
        log_error(&env, "borrow", DummyError);
        log_error_with_tag(&env, Symbol::new(&env, "lending"), "borrow", DummyError);
        let _ = env;
    }
}
