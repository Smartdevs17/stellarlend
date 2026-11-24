//! Mock oracle bindings for the trait-based adapter layer (issue #1762).
//!
//! Two bindings are provided: [`MockPriceSource`] for pure unit tests and
//! [`MockOracleContract`] for tests that need a real registered contract behind
//! [`ContractPriceSource`]. Both use the same `PriceQuote` type, so swapping a
//! mock for a production feed is a single binding change.

use oracle_adapter::{AdapterError, PriceQuote, PriceSource};
use soroban_sdk::Symbol;
use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, symbol_short, Env, Map, Vec,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MockOracleError {
    InjectedFailure = 1,
    UnknownAsset = 2,
}

/// In-memory source for adapter unit tests.
#[derive(Clone)]
pub struct MockPriceSource {
    quotes: Vec<(Symbol, PriceQuote)>,
    fail: bool,
}

impl MockPriceSource {
    /// Create an empty source.
    pub fn new(env: &Env) -> Self {
        Self {
            quotes: Vec::new(env),
            fail: false,
        }
    }

    /// Configure the quote returned for `asset`.
    pub fn set_price(&mut self, asset: Symbol, quote: PriceQuote) {
        for index in 0..self.quotes.len() {
            let (key, _) = self.quotes.get(index).unwrap();
            if key == asset {
                self.quotes.set(index, (asset, quote));
                return;
            }
        }
        self.quotes.push_back((asset, quote));
    }

    /// Make every fetch fail, simulating an unreachable feed.
    pub fn set_failure(&mut self, fail: bool) {
        self.fail = fail;
    }

    /// Number of configured quotes.
    pub fn quote_count(&self) -> u32 {
        self.quotes.len()
    }
}

impl PriceSource for MockPriceSource {
    fn fetch(&self, _env: &Env, asset: &Symbol) -> Result<PriceQuote, AdapterError> {
        if self.fail {
            return Err(AdapterError::SourceUnavailable);
        }
        self.quotes
            .iter()
            .find(|(key, _)| key == asset)
            .map(|(_, quote)| quote.clone())
            .ok_or(AdapterError::SourceUnavailable)
    }
}

/// Configurable mock contract implementing the `PriceFeed` interface.
#[contract]
pub struct MockOracleContract;

#[contractimpl]
impl MockOracleContract {
    /// Set up the mock with failure injection disabled.
    pub fn initialize(env: Env) {
        env.storage().instance().set(&symbol_short!("fail"), &false);
        env.storage().instance().set(
            &symbol_short!("quotes"),
            &Map::<Symbol, PriceQuote>::new(&env),
        );
    }

    /// Configure a quote for an asset.
    pub fn set_price(env: Env, asset: Symbol, quote: PriceQuote) {
        let mut quotes: Map<Symbol, PriceQuote> = env
            .storage()
            .instance()
            .get(&symbol_short!("quotes"))
            .unwrap();
        quotes.set(asset, quote);
        env.storage()
            .instance()
            .set(&symbol_short!("quotes"), &quotes);
    }

    /// Inject a failure for every subsequent `get_price` call.
    pub fn set_failure(env: Env, fail: bool) {
        env.storage().instance().set(&symbol_short!("fail"), &fail);
    }

    /// Return the configured quote, or panic with an injected error.
    pub fn get_price(env: Env, asset: Symbol) -> PriceQuote {
        if env
            .storage()
            .instance()
            .get(&symbol_short!("fail"))
            .unwrap_or(false)
        {
            panic_with_error!(&env, MockOracleError::InjectedFailure);
        }
        let quotes: Map<Symbol, PriceQuote> = env
            .storage()
            .instance()
            .get(&symbol_short!("quotes"))
            .unwrap();
        quotes
            .get(asset)
            .unwrap_or_else(|| panic_with_error!(&env, MockOracleError::UnknownAsset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_adapter::{ContractPriceSource, NoopTelemetry, OracleAdapter};

    fn quote(env: &Env, price: i128) -> PriceQuote {
        PriceQuote {
            asset: Symbol::new(env, "XLM"),
            price,
            decimals: 7,
            timestamp: 900,
            confidence: 100,
        }
    }

    #[test]
    fn in_memory_mock_feeds_the_adapter() {
        let env = Env::default();
        let asset = Symbol::new(&env, "XLM");
        let mut source = MockPriceSource::new(&env);
        assert_eq!(
            source.fetch(&env, &asset),
            Err(AdapterError::SourceUnavailable)
        );

        source.set_price(asset.clone(), quote(&env, 123));
        assert_eq!(source.quote_count(), 1);
        let adapter = OracleAdapter::new(source, None::<MockPriceSource>, NoopTelemetry);
        assert_eq!(adapter.latest(&env, &asset).unwrap().price, 123);
    }

    #[test]
    fn in_memory_mock_failure_is_reported() {
        let env = Env::default();
        let asset = Symbol::new(&env, "XLM");
        let mut source = MockPriceSource::new(&env);
        source.set_price(asset.clone(), quote(&env, 123));
        source.set_failure(true);
        let adapter = OracleAdapter::new(source, None::<MockPriceSource>, NoopTelemetry);
        assert_eq!(
            adapter.latest(&env, &asset),
            Err(AdapterError::SourceUnavailable)
        );
    }

    #[test]
    fn registered_mock_contract_binds_through_the_client() {
        let env = Env::default();
        let asset = Symbol::new(&env, "XLM");
        let id = env.register(MockOracleContract, ());
        let client = MockOracleContractClient::new(&env, &id);
        client.initialize();
        client.set_price(&asset, &quote(&env, 456));

        let adapter = OracleAdapter::new(
            ContractPriceSource {
                address: id.clone(),
            },
            None::<MockPriceSource>,
            NoopTelemetry,
        );
        assert_eq!(adapter.latest(&env, &asset).unwrap().price, 456);

        client.set_failure(&true);
        assert_eq!(
            adapter.latest(&env, &asset),
            Err(AdapterError::SourceUnavailable)
        );
    }

    #[test]
    fn mock_contract_falls_back_to_an_in_memory_source() {
        let env = Env::default();
        let asset = Symbol::new(&env, "XLM");
        let id = env.register(MockOracleContract, ());
        let client = MockOracleContractClient::new(&env, &id);
        client.initialize();
        client.set_failure(&true);

        let mut fallback = MockPriceSource::new(&env);
        fallback.set_price(asset.clone(), quote(&env, 789));
        let adapter = OracleAdapter::new(
            ContractPriceSource { address: id },
            Some(fallback),
            NoopTelemetry,
        );
        assert_eq!(adapter.latest(&env, &asset).unwrap().price, 789);
    }
}
