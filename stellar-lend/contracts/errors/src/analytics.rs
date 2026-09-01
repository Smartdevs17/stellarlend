//! Error analytics — lightweight, `no_std` helper for categorizing and counting
//! errors so off-chain dashboards can aggregate failures across contracts.

use soroban_sdk::{Env, Symbol, Vec};

use crate::IntoError;

/// Accumulates per-category error counters.
///
/// This type is intentionally storage-agnostic: it is meant to be used as an
/// intermediate accumulator (e.g. in tests or off-chain processing), or persisted by
/// a caller that owns the concrete storage keys.
#[derive(Clone)]
pub struct ErrorAnalytics {
    env: Env,
    /// `Vec<(category, count)>` running tally.
    counts: Vec<(Symbol, u32)>,
}

impl ErrorAnalytics {
    /// Starts an empty tally, retaining `env` so categories can be encoded later.
    pub fn new(env: &Env) -> Self {
        Self {
            env: env.clone(),
            counts: Vec::new(env),
        }
    }

    /// Records one occurrence, incrementing the counter for `category`.
    pub fn record<E: IntoError>(&mut self, error: E) {
        let category = error.into_core().tag(&self.env);
        self.bump(category);
    }

    /// Records an occurrence already normalized to a `Symbol` category.
    pub fn record_tag(&mut self, category: Symbol) {
        self.bump(category);
    }

    /// Returns the count observed for `category` (0 if never seen).
    pub fn count(&self, category: Symbol) -> u32 {
        for (cat, n) in self.counts.iter() {
            if cat == category {
                return n;
            }
        }
        0
    }

    /// Builds a ready-to-emit `Vec<(Symbol, u32)>` snapshot.
    pub fn snapshot(&self) -> Vec<(Symbol, u32)> {
        self.counts.clone()
    }

    fn bump(&mut self, category: Symbol) {
        for i in 0..self.counts.len() {
            let (cat, n) = &mut self.counts.get(i).unwrap();
            if *cat == category {
                *n += 1;
                self.counts.set(i, (cat.clone(), *n));
                return;
            }
        }
        self.counts.push_back((category, 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreError;

    struct DummyError;
    impl IntoError for DummyError {
        fn into_core(self) -> CoreError {
            CoreError::Unauthorized
        }
    }

    #[test]
    fn analytics_counts_per_category() {
        let env = Env::default();
        let mut a = ErrorAnalytics::new(&env);
        let unauthorized = CoreError::Unauthorized.tag(&env);
        let overflow = CoreError::Overflow.tag(&env);

        a.record(DummyError);
        a.record(CoreError::Overflow);
        a.record(DummyError);

        assert_eq!(a.count(unauthorized), 2);
        assert_eq!(a.count(overflow), 1);
        assert_eq!(a.count(CoreError::NotFound.tag(&env)), 0);
        assert_eq!(a.snapshot().len(), 2);
    }
}
