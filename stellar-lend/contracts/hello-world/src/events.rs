//! #1606: Unified event emission — one macro instead of duplicated helpers.
use soroban_sdk::{Env, IntoVal, Symbol, Val, Vec};

/// Emit a contract event with up to 4 topic values and one data value.
/// Replaces the scattered ad-hoc `env.events().publish(...)` call sites.
///
/// Usage:
///   emit!(env, "topic1", "topic2"; data_value);
#[macro_export]
macro_rules! emit {
    ($env:expr, $($topic:expr),+ ; $data:expr) => {{
        let mut topics = soroban_sdk::Vec::new($env);
        $(topics.push_back(soroban_sdk::Symbol::new($env, $topic));)+
        $env.events().publish(topics, $data);
    }};
}

/// Convenience: emit a named event with a single (key, value) data pair.
pub fn emit_event(env: &Env, name: &str, key: &str, val: impl IntoVal<Env, Val>) {
    let topic = Symbol::new(env, name);
    let mut topics = Vec::new(env);
    topics.push_back(topic);
    env.events().publish(topics, (Symbol::new(env, key), val));
}
