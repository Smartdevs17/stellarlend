//! Fixture constants and builders.
//!
//! Constants (`AmountFixtures`, `TimeFixtures`, ...) cover plain values.
//! [`ProtocolFixture`] covers deployed state: an env with an admin, users,
//! Stellar Asset Contract tokens and a [`PriceOracle`], built once through
//! [`ProtocolFixtureBuilder`] and shared by every test that needs it.

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

use crate::environment::{advance_time, snapshotless_env};
use crate::mock_contracts::{register_price_oracle, PriceOracleClient, ORACLE_PRICE_SCALE};

pub const MIN_AMOUNT: i128 = 100;
pub const MAX_AMOUNT: i128 = 1_000_000_000;
pub const LARGE_CEILING: i128 = 100_000_000_000;
pub const DEFAULT_PRICE: i128 = 1_000_000;

pub const DEFAULT_COLLATERAL_FACTOR: i128 = 7500;
pub const DEFAULT_LIQUIDATION_THRESHOLD: i128 = 8000;
pub const DEFAULT_RESERVE_FACTOR: i128 = 1000;

pub const MAX_BPS: u64 = 10_000;

pub struct AmountFixtures;

impl AmountFixtures {
    pub const ZERO: i128 = 0;
    pub const SMALL: i128 = 1_000;
    pub const MEDIUM: i128 = 100_000;
    pub const LARGE: i128 = 10_000_000;
    pub const HUGE: i128 = 1_000_000_000;
}

pub struct TimeFixtures;

impl TimeFixtures {
    pub const MINUTE: u64 = 60;
    pub const HOUR: u64 = 3_600;
    pub const DAY: u64 = 86_400;
    pub const WEEK: u64 = 604_800;
    pub const MONTH: u64 = 2_592_000;
    pub const YEAR: u64 = 31_536_000;
}

pub struct RateFixtures;

impl RateFixtures {
    pub const ZERO_PERCENT: i128 = 0;
    pub const ONE_PERCENT: i128 = 100;
    pub const FIVE_PERCENT: i128 = 500;
    pub const TEN_PERCENT: i128 = 1_000;
    pub const FIFTY_PERCENT: i128 = 5_000;
    pub const HUNDRED_PERCENT: i128 = 10_000;
}

pub struct AssetConfigFixture {
    pub collateral_factor: i128,
    pub liquidation_threshold: i128,
    pub reserve_factor: i128,
    pub max_supply: i128,
    pub max_borrow: i128,
    pub price: i128,
}

impl Default for AssetConfigFixture {
    fn default() -> Self {
        Self {
            collateral_factor: DEFAULT_COLLATERAL_FACTOR,
            liquidation_threshold: DEFAULT_LIQUIDATION_THRESHOLD,
            reserve_factor: DEFAULT_RESERVE_FACTOR,
            max_supply: MAX_AMOUNT * 10,
            max_borrow: MAX_AMOUNT * 5,
            price: DEFAULT_PRICE,
        }
    }
}

impl AssetConfigFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_collateral_factor(mut self, factor: i128) -> Self {
        self.collateral_factor = factor;
        self
    }

    pub fn with_liquidation_threshold(mut self, threshold: i128) -> Self {
        self.liquidation_threshold = threshold;
        self
    }

    pub fn with_reserve_factor(mut self, factor: i128) -> Self {
        self.reserve_factor = factor;
        self
    }

    pub fn with_price(mut self, price: i128) -> Self {
        self.price = price;
        self
    }

    pub fn conservative() -> Self {
        Self {
            collateral_factor: 5000,
            liquidation_threshold: 6000,
            reserve_factor: 2000,
            max_supply: MAX_AMOUNT,
            max_borrow: MAX_AMOUNT / 2,
            price: DEFAULT_PRICE,
        }
    }

    pub fn aggressive() -> Self {
        Self {
            collateral_factor: 9000,
            liquidation_threshold: 9500,
            reserve_factor: 500,
            max_supply: MAX_AMOUNT * 100,
            max_borrow: MAX_AMOUNT * 90,
            price: DEFAULT_PRICE,
        }
    }
}

/// A Stellar Asset Contract deployed for a test, with its issuing admin.
#[derive(Clone)]
pub struct TokenFixture {
    pub env: Env,
    pub address: Address,
    pub admin: Address,
}

impl TokenFixture {
    pub fn deploy(env: &Env, admin: &Address) -> Self {
        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        Self {
            env: env.clone(),
            address: sac.address(),
            admin: admin.clone(),
        }
    }

    pub fn client(&self) -> TokenClient<'_> {
        TokenClient::new(&self.env, &self.address)
    }

    pub fn admin_client(&self) -> StellarAssetClient<'_> {
        StellarAssetClient::new(&self.env, &self.address)
    }

    pub fn mint(&self, to: &Address, amount: i128) {
        self.admin_client().mint(to, &amount);
    }

    pub fn balance(&self, of: &Address) -> i128 {
        self.client().balance(of)
    }
}

/// Deployed protocol-agnostic state shared by contract suites: users, tokens
/// and an oracle. Contract-specific fixtures (e.g. the lending suites'
/// `Fixture`) register their contract on `env` and reuse the rest.
pub struct ProtocolFixture {
    pub env: Env,
    pub admin: Address,
    pub users: Vec<Address>,
    pub tokens: Vec<TokenFixture>,
    pub oracle: Address,
}

impl ProtocolFixture {
    pub fn builder() -> ProtocolFixtureBuilder {
        ProtocolFixtureBuilder::default()
    }

    pub fn user(&self, index: usize) -> &Address {
        &self.users[index]
    }

    pub fn token(&self, index: usize) -> &TokenFixture {
        &self.tokens[index]
    }

    pub fn oracle_client(&self) -> PriceOracleClient<'_> {
        PriceOracleClient::new(&self.env, &self.oracle)
    }

    /// Quote `price` (8 decimals) for token `index`.
    pub fn set_price(&self, index: usize, price: i128) {
        self.oracle_client()
            .set_price(&self.tokens[index].address, &price);
    }

    pub fn price(&self, index: usize) -> i128 {
        self.oracle_client().price(&self.tokens[index].address)
    }

    pub fn advance_time(&self, seconds: u64) {
        advance_time(&self.env, seconds);
    }
}

/// Builder for [`ProtocolFixture`]. Defaults: snapshot-free env with all
/// auths mocked, two users, one token, flat 1.0 prices, no balances.
pub struct ProtocolFixtureBuilder {
    env: Option<Env>,
    users: usize,
    tokens: usize,
    default_price: i128,
    prices: Vec<(usize, i128)>,
    initial_balance: i128,
    timestamp: Option<u64>,
    unlimited_budget: bool,
}

impl Default for ProtocolFixtureBuilder {
    fn default() -> Self {
        Self {
            env: None,
            users: 2,
            tokens: 1,
            default_price: ORACLE_PRICE_SCALE,
            prices: Vec::new(),
            initial_balance: 0,
            timestamp: None,
            unlimited_budget: false,
        }
    }
}

impl ProtocolFixtureBuilder {
    /// Build on an existing env instead of a fresh snapshot-free one.
    pub fn with_env(mut self, env: Env) -> Self {
        self.env = Some(env);
        self
    }

    pub fn users(mut self, count: usize) -> Self {
        self.users = count;
        self
    }

    pub fn tokens(mut self, count: usize) -> Self {
        self.tokens = count;
        self
    }

    /// Price quoted for every token without an explicit price.
    pub fn default_price(mut self, price: i128) -> Self {
        self.default_price = price;
        self
    }

    pub fn token_price(mut self, index: usize, price: i128) -> Self {
        self.prices.push((index, price));
        self
    }

    /// Mint `amount` of every token to every user.
    pub fn initial_balance(mut self, amount: i128) -> Self {
        self.initial_balance = amount;
        self
    }

    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    pub fn unlimited_budget(mut self) -> Self {
        self.unlimited_budget = true;
        self
    }

    pub fn build(self) -> ProtocolFixture {
        let env = self.env.unwrap_or_else(snapshotless_env);
        env.mock_all_auths();
        if self.unlimited_budget {
            env.cost_estimate().budget().reset_unlimited();
        }
        if let Some(timestamp) = self.timestamp {
            env.ledger().with_mut(|li| li.timestamp = timestamp);
        }

        let admin = Address::generate(&env);
        let users: Vec<Address> = (0..self.users).map(|_| Address::generate(&env)).collect();
        let tokens: Vec<TokenFixture> = (0..self.tokens)
            .map(|_| TokenFixture::deploy(&env, &admin))
            .collect();
        let oracle = register_price_oracle(&env, self.default_price);

        let fixture = ProtocolFixture {
            env,
            admin,
            users,
            tokens,
            oracle,
        };
        for (index, price) in self.prices {
            fixture.set_price(index, price);
        }
        if self.initial_balance > 0 {
            for token in &fixture.tokens {
                for user in &fixture.users {
                    token.mint(user, self.initial_balance);
                }
            }
        }
        fixture
    }
}
