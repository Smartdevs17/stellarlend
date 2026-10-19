//! Deterministic test data seeding.
//!
//! [`SeedRng`] is a small SplitMix64 generator so seeded data is identical on
//! every machine without pulling in `rand`. [`Seeder`] uses it to fund users,
//! quote prices and generate amounts on a [`ProtocolFixture`], and records
//! everything it seeded so tests can assert against the seeded totals.
//!
//! The seed comes from `TEST_SEED` when set, so a failing CI run can be
//! reproduced locally with the same data:
//!
//! ```text
//! TEST_SEED=1234 cargo test -p test-utils
//! ```

use soroban_sdk::Address;

use crate::fixtures::ProtocolFixture;

/// Seed used when `TEST_SEED` is not set.
pub const DEFAULT_SEED: u64 = 0x5EED_1E4D;

/// `TEST_SEED` if set and valid, otherwise [`DEFAULT_SEED`].
pub fn seed_from_env() -> u64 {
    std::env::var("TEST_SEED")
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(DEFAULT_SEED)
}

/// SplitMix64: fast, dependency-free and good enough for test data.
#[derive(Clone, Debug)]
pub struct SeedRng {
    state: u64,
}

impl SeedRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn from_env() -> Self {
        Self::new(seed_from_env())
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `min..=max`.
    pub fn range_u64(&mut self, min: u64, max: u64) -> u64 {
        assert!(min <= max, "range_u64: min {} > max {}", min, max);
        let span = max - min;
        if span == u64::MAX {
            return self.next_u64();
        }
        min + self.next_u64() % (span + 1)
    }

    /// Uniform value in `min..=max`. The span must fit in a `u64`, which
    /// covers every realistic token amount.
    pub fn range_i128(&mut self, min: i128, max: i128) -> i128 {
        assert!(min <= max, "range_i128: min {} > max {}", min, max);
        let span = u64::try_from(max - min).expect("range_i128: span must fit in u64");
        min + self.range_u64(0, span) as i128
    }

    pub fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        assert!(denominator > 0, "chance: denominator must be > 0");
        self.next_u64() % denominator < numerator
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        assert!(!items.is_empty(), "pick: empty slice");
        &items[self.range_u64(0, items.len() as u64 - 1) as usize]
    }
}

/// One balance minted by the [`Seeder`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeededBalance {
    pub user: Address,
    pub token: usize,
    pub amount: i128,
}

/// Seeds balances and prices onto a [`ProtocolFixture`] from a [`SeedRng`].
pub struct Seeder<'a> {
    fixture: &'a ProtocolFixture,
    rng: SeedRng,
    balances: Vec<SeededBalance>,
}

impl<'a> Seeder<'a> {
    pub fn new(fixture: &'a ProtocolFixture, seed: u64) -> Self {
        Self {
            fixture,
            rng: SeedRng::new(seed),
            balances: Vec::new(),
        }
    }

    /// Seeder using `TEST_SEED` (or [`DEFAULT_SEED`]).
    pub fn from_env(fixture: &'a ProtocolFixture) -> Self {
        Self::new(fixture, seed_from_env())
    }

    pub fn rng(&mut self) -> &mut SeedRng {
        &mut self.rng
    }

    /// Mint the same `amount` of token `token` to every user.
    pub fn fund_all(&mut self, token: usize, amount: i128) -> &[SeededBalance] {
        let start = self.balances.len();
        let fixture = self.fixture;
        for user in &fixture.users {
            self.mint(user, token, amount);
        }
        &self.balances[start..]
    }

    /// Mint a random amount in `min..=max` of token `token` to every user.
    pub fn fund_random(&mut self, token: usize, min: i128, max: i128) -> &[SeededBalance] {
        let start = self.balances.len();
        let fixture = self.fixture;
        for user in &fixture.users {
            let amount = self.rng.range_i128(min, max);
            self.mint(user, token, amount);
        }
        &self.balances[start..]
    }

    /// Quote a random price in `min..=max` for every token; returns the prices
    /// in token order.
    pub fn random_prices(&mut self, min: i128, max: i128) -> Vec<i128> {
        (0..self.fixture.tokens.len())
            .map(|index| {
                let price = self.rng.range_i128(min, max);
                self.fixture.set_price(index, price);
                price
            })
            .collect()
    }

    /// Random walk of `steps` prices for token `token`, each step moving at
    /// most `max_move_bps` from the previous price. The oracle ends on the
    /// last price; every intermediate price is returned for replay.
    pub fn price_walk(&mut self, token: usize, steps: usize, max_move_bps: i128) -> Vec<i128> {
        let mut price = self.fixture.price(token);
        let mut path = Vec::with_capacity(steps);
        for _ in 0..steps {
            let move_bps = self.rng.range_i128(-max_move_bps, max_move_bps);
            price = (price + price * move_bps / 10_000).max(1);
            path.push(price);
        }
        if let Some(last) = path.last() {
            self.fixture.set_price(token, *last);
        }
        path
    }

    /// `count` random amounts in `min..=max`, e.g. for deposit sequences.
    pub fn amounts(&mut self, count: usize, min: i128, max: i128) -> Vec<i128> {
        (0..count).map(|_| self.rng.range_i128(min, max)).collect()
    }

    /// Everything minted so far, in order.
    pub fn seeded(&self) -> &[SeededBalance] {
        &self.balances
    }

    /// Total minted of token `token`.
    pub fn total_seeded(&self, token: usize) -> i128 {
        self.balances
            .iter()
            .filter(|b| b.token == token)
            .map(|b| b.amount)
            .sum()
    }

    /// Total minted of token `token` to `user`.
    pub fn seeded_for(&self, user: &Address, token: usize) -> i128 {
        self.balances
            .iter()
            .filter(|b| b.token == token && &b.user == user)
            .map(|b| b.amount)
            .sum()
    }

    fn mint(&mut self, user: &Address, token: usize, amount: i128) {
        self.fixture.tokens[token].mint(user, amount);
        self.balances.push(SeededBalance {
            user: user.clone(),
            token,
            amount,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = SeedRng::new(42);
        let mut b = SeedRng::new(42);
        let xs: Vec<u64> = (0..16).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..16).map(|_| b.next_u64()).collect();
        assert_eq!(xs, ys);
        assert_ne!(SeedRng::new(43).next_u64(), xs[0]);
    }

    #[test]
    fn ranges_are_inclusive_and_bounded() {
        let mut rng = SeedRng::new(7);
        let mut seen_min = false;
        let mut seen_max = false;
        for _ in 0..2_000 {
            let v = rng.range_i128(-3, 3);
            assert!((-3..=3).contains(&v));
            seen_min |= v == -3;
            seen_max |= v == 3;
        }
        assert!(seen_min && seen_max, "both bounds should be reachable");
        assert_eq!(rng.range_u64(5, 5), 5);
    }

    #[test]
    fn pick_and_chance_stay_in_bounds() {
        let mut rng = SeedRng::new(9);
        let items = [1, 2, 3];
        for _ in 0..100 {
            assert!(items.contains(rng.pick(&items)));
        }
        assert!(!rng.chance(0, 10));
        assert!(rng.chance(10, 10));
    }
}
