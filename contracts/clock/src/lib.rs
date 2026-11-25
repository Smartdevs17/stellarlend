#![no_std]

use soroban_sdk::Env;

/// A source of Unix time in seconds.
pub trait Clock {
    fn now_seconds(&self) -> u64;
}

/// Clock backed by the current Soroban ledger timestamp.
pub struct LedgerClock<'a> {
    env: &'a Env,
}

impl<'a> LedgerClock<'a> {
    pub fn new(env: &'a Env) -> Self {
        Self { env }
    }
}

impl Clock for LedgerClock<'_> {
    fn now_seconds(&self) -> u64 {
        self.env.ledger().timestamp()
    }
}

/// Read the ledger time through the shared clock abstraction.
pub fn now_seconds(env: &Env) -> u64 {
    LedgerClock::new(env).now_seconds()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_current_ledger_timestamp() {
        let env = Env::default();
        env.ledger().set_timestamp(42);

        assert_eq!(now_seconds(&env), 42);
    }
}
