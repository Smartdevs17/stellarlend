use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

/// Price scale used by the lending views (8 decimals, `100_000_000` = 1.0).
pub const ORACLE_PRICE_SCALE: i128 = 100_000_000;

const DEFAULT_PRICE_KEY: Symbol = symbol_short!("default");

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn initialize(
        env: Env,
        admin: Address,
        decimal: u32,
        name: soroban_sdk::String,
        symbol: soroban_sdk::String,
    ) {
        env.storage().instance().set(&"admin", &admin);
        env.storage().instance().set(&"decimal", &decimal);
        env.storage().instance().set(&"name", &name);
        env.storage().instance().set(&"symbol", &symbol);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}

    pub fn mint(env: Env, to: Address, amount: i128) {
        let balance: i128 = env.storage().persistent().get(&to).unwrap_or(0);
        env.storage().persistent().set(&to, &(balance + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        let balance: i128 = env.storage().persistent().get(&from).unwrap_or(0);
        env.storage().persistent().set(&from, &(balance - amount));
    }
}

#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&"admin", &admin);
    }

    pub fn get_price(env: Env, asset: Address) -> i128 {
        env.storage().persistent().get(&asset).unwrap_or(1_000_000)
    }

    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage().persistent().set(&asset, &price);
    }

    pub fn get_last_updated(_env: Env, _asset: Address) -> u64 {
        0
    }
}

/// Oracle exposing the `price(asset) -> i128` interface the lending contract
/// calls. Every asset returns the default price until it is overridden with
/// `set_price`, so one registration covers both flat-price and price-shock
/// tests.
#[contract]
pub struct PriceOracle;

#[contractimpl]
impl PriceOracle {
    pub fn __constructor(env: Env, default_price: i128) {
        env.storage()
            .instance()
            .set(&DEFAULT_PRICE_KEY, &default_price);
    }

    pub fn price(env: Env, asset: Address) -> i128 {
        env.storage().persistent().get(&asset).unwrap_or_else(|| {
            env.storage()
                .instance()
                .get(&DEFAULT_PRICE_KEY)
                .unwrap_or(ORACLE_PRICE_SCALE)
        })
    }

    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage().persistent().set(&asset, &price);
    }

    pub fn set_default_price(env: Env, price: i128) {
        env.storage().instance().set(&DEFAULT_PRICE_KEY, &price);
    }
}

pub fn register_mock_token(env: &Env) -> Address {
    env.register(MockToken, ())
}

pub fn register_mock_oracle(env: &Env) -> Address {
    env.register(MockOracle, ())
}

/// Register a [`PriceOracle`] that quotes `default_price` for every asset.
pub fn register_price_oracle(env: &Env, default_price: i128) -> Address {
    env.register(PriceOracle, (default_price,))
}
