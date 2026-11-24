//! Oracle adapter layer (issue #1762).
//!
//! The adapter is a trait around a price source. A source can be a registered
//! Soroban contract ([`ContractPriceSource`]) or an in-memory mock, so callers
//! bind the implementation once and test the surrounding logic without a
//! network. Validation, staleness checks, fallback and telemetry live in the
//! adapter, not in each source.

use soroban_sdk::{contractclient, contracttype, symbol_short, Address, Env, Symbol};

/// A normalized price quote returned by any oracle source.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceQuote {
    /// Asset the price refers to.
    pub asset: Symbol,
    /// Price in `decimals` units.
    pub price: i128,
    /// Number of decimals in `price`.
    pub decimals: u32,
    /// Ledger timestamp of the observation.
    pub timestamp: u64,
    /// Confidence in basis points (`0..=10_000`).
    pub confidence: u32,
}

/// Contract interface any price source may implement.
#[contractclient(name = "PriceFeedClient")]
pub trait PriceFeed {
    /// Return the latest quote for `asset`.
    fn get_price(env: Env, asset: Symbol) -> PriceQuote;
}

/// Errors shared by every binding of the adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterError {
    /// The source rejected the request or is unreachable.
    SourceUnavailable,
    /// The quote failed a structural check.
    InvalidQuote,
    /// The quote is older than the configured bound.
    StaleQuote { age: u64, max_age: u64 },
    /// No source was available for a fallback attempt.
    NoSource,
}

impl core::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SourceUnavailable => f.write_str("oracle source unavailable"),
            Self::InvalidQuote => f.write_str("oracle returned an invalid quote"),
            Self::StaleQuote { age, max_age } => {
                write!(f, "oracle quote is {age}s old, maximum is {max_age}s")
            }
            Self::NoSource => f.write_str("no oracle source configured"),
        }
    }
}

/// Which source produced a quote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceKind {
    Primary,
    Fallback,
}

impl SourceKind {
    /// Event topic for this source.
    pub fn topic(self) -> Symbol {
        match self {
            Self::Primary => symbol_short!("primary"),
            Self::Fallback => symbol_short!("fallback"),
        }
    }
}

/// A price source that can be swapped for a mock in tests.
pub trait PriceSource {
    /// Fetch a quote for `asset`.
    fn fetch(&self, env: &Env, asset: &Symbol) -> Result<PriceQuote, AdapterError>;
}

/// Binding to a registered contract implementing [`PriceFeed`].
#[derive(Clone)]
pub struct ContractPriceSource {
    /// Address of the price feed contract.
    pub address: Address,
}

impl PriceSource for ContractPriceSource {
    fn fetch(&self, env: &Env, asset: &Symbol) -> Result<PriceQuote, AdapterError> {
        match PriceFeedClient::new(env, &self.address).try_get_price(asset) {
            Ok(Ok(quote)) => Ok(quote),
            _ => Err(AdapterError::SourceUnavailable),
        }
    }
}

/// Observability hooks for user-visible adapter decisions.
pub trait AdapterTelemetry {
    /// A quote was accepted.
    fn success(&self, env: &Env, asset: &Symbol, quote: &PriceQuote, source: SourceKind);
    /// The primary source failed and a fallback was attempted.
    fn fallback(&self, env: &Env, asset: &Symbol, reason: &AdapterError);
    /// Every source failed.
    fn failure(&self, env: &Env, asset: &Symbol, reason: &AdapterError);
}

/// Telemetry that emits Soroban contract events.
#[derive(Clone, Copy, Default)]
pub struct SorobanTelemetry;

#[allow(deprecated)]
impl AdapterTelemetry for SorobanTelemetry {
    fn success(&self, env: &Env, asset: &Symbol, quote: &PriceQuote, source: SourceKind) {
        env.events().publish(
            (symbol_short!("oa_ok"), source.topic()),
            (asset.clone(), quote.price, quote.timestamp),
        );
    }

    fn fallback(&self, env: &Env, asset: &Symbol, reason: &AdapterError) {
        env.events().publish(
            (symbol_short!("oa_fb"), asset.clone()),
            reason.to_string(env),
        );
    }

    fn failure(&self, env: &Env, asset: &Symbol, reason: &AdapterError) {
        env.events().publish(
            (symbol_short!("oa_err"), asset.clone()),
            reason.to_string(env),
        );
    }
}

/// No-op telemetry for callers that do not emit events.
#[derive(Clone, Copy, Default)]
pub struct NoopTelemetry;

impl AdapterTelemetry for NoopTelemetry {
    fn success(&self, _env: &Env, _asset: &Symbol, _quote: &PriceQuote, _source: SourceKind) {}
    fn fallback(&self, _env: &Env, _asset: &Symbol, _reason: &AdapterError) {}
    fn failure(&self, _env: &Env, _asset: &Symbol, _reason: &AdapterError) {}
}

impl AdapterError {
    /// Convert to a Soroban string for event data.
    fn to_string(&self, env: &Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(
            env,
            match self {
                Self::SourceUnavailable => "source unavailable",
                Self::InvalidQuote => "invalid quote",
                Self::StaleQuote { .. } => "stale quote",
                Self::NoSource => "no source",
            },
        )
    }
}

/// Validates and normalizes prices from one or two interchangeable sources.
pub struct OracleAdapter<P: PriceSource, F: PriceSource = P, T: AdapterTelemetry = NoopTelemetry> {
    primary: P,
    fallback: Option<F>,
    telemetry: T,
    max_staleness: Option<u64>,
}

impl<P: PriceSource, F: PriceSource, T: AdapterTelemetry> OracleAdapter<P, F, T> {
    /// Bind a primary source, an optional fallback and telemetry hooks.
    pub fn new(primary: P, fallback: Option<F>, telemetry: T) -> Self {
        Self {
            primary,
            fallback,
            telemetry,
            max_staleness: None,
        }
    }

    /// Reject quotes older than `max_age` seconds.
    pub fn with_max_staleness(mut self, max_age: u64) -> Self {
        self.max_staleness = Some(max_age);
        self
    }

    /// Return the best available validated quote.
    pub fn latest(&self, env: &Env, asset: &Symbol) -> Result<PriceQuote, AdapterError> {
        match self.primary.fetch(env, asset) {
            Ok(quote) => {
                self.validate(env, &quote)?;
                self.telemetry
                    .success(env, asset, &quote, SourceKind::Primary);
                return Ok(quote);
            }
            Err(reason) => {
                self.telemetry.fallback(env, asset, &reason);
                match self.fallback.as_ref() {
                    Some(source) => match source.fetch(env, asset) {
                        Ok(quote) => {
                            self.validate(env, &quote)?;
                            self.telemetry
                                .success(env, asset, &quote, SourceKind::Fallback);
                            return Ok(quote);
                        }
                        Err(reason) => {
                            self.telemetry.failure(env, asset, &reason);
                            return Err(reason);
                        }
                    },
                    None => {
                        self.telemetry.failure(env, asset, &reason);
                        return Err(reason);
                    }
                }
            }
        }
    }

    fn validate(&self, env: &Env, quote: &PriceQuote) -> Result<(), AdapterError> {
        if quote.price <= 0
            || quote.timestamp == 0
            || quote.decimals > 18
            || quote.confidence > 10_000
        {
            return Err(AdapterError::InvalidQuote);
        }
        if let Some(max_age) = self.max_staleness {
            let age = env.ledger().timestamp().saturating_sub(quote.timestamp);
            if age > max_age {
                return Err(AdapterError::StaleQuote { age, max_age });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;
    use soroban_sdk::testutils::Ledger;

    #[derive(Clone)]
    struct StubSource {
        quote: Option<PriceQuote>,
    }

    impl StubSource {
        fn quote(price: i128, timestamp: u64) -> Self {
            Self {
                quote: Some(PriceQuote {
                    asset: Symbol::new(&Env::default(), "XLM"),
                    price,
                    decimals: 7,
                    timestamp,
                    confidence: 100,
                }),
            }
        }

        fn failing() -> Self {
            Self { quote: None }
        }
    }

    impl PriceSource for StubSource {
        fn fetch(&self, _env: &Env, _asset: &Symbol) -> Result<PriceQuote, AdapterError> {
            self.quote.clone().ok_or(AdapterError::SourceUnavailable)
        }
    }

    #[derive(Clone, Copy)]
    struct CountingTelemetry<'a> {
        success: &'a Cell<u32>,
        fallback: &'a Cell<u32>,
        failure: &'a Cell<u32>,
    }

    impl AdapterTelemetry for CountingTelemetry<'_> {
        fn success(&self, _env: &Env, _asset: &Symbol, _quote: &PriceQuote, _source: SourceKind) {
            self.success.set(self.success.get() + 1);
        }
        fn fallback(&self, _env: &Env, _asset: &Symbol, _reason: &AdapterError) {
            self.fallback.set(self.fallback.get() + 1);
        }
        fn failure(&self, _env: &Env, _asset: &Symbol, _reason: &AdapterError) {
            self.failure.set(self.failure.get() + 1);
        }
    }

    fn counters() -> (Cell<u32>, Cell<u32>, Cell<u32>) {
        (Cell::new(0), Cell::new(0), Cell::new(0))
    }

    fn env() -> Env {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        env
    }

    #[test]
    fn primary_quote_is_validated_and_reported() {
        let env = env();
        let asset = Symbol::new(&env, "XLM");
        let (success, fallback, failure) = counters();
        let telemetry = CountingTelemetry {
            success: &success,
            fallback: &fallback,
            failure: &failure,
        };
        let adapter =
            OracleAdapter::new(StubSource::quote(100, 990), None::<StubSource>, telemetry);

        let quote = adapter.latest(&env, &asset).unwrap();
        assert_eq!(quote.price, 100);
        assert_eq!(success.get(), 1);
        assert_eq!(fallback.get(), 0);
        assert_eq!(failure.get(), 0);
    }

    #[test]
    fn fallback_source_is_used_when_primary_fails() {
        let env = env();
        let asset = Symbol::new(&env, "XLM");
        let (success, fallback, failure) = counters();
        let telemetry = CountingTelemetry {
            success: &success,
            fallback: &fallback,
            failure: &failure,
        };
        let adapter = OracleAdapter::new(
            StubSource::failing(),
            Some(StubSource::quote(250, 995)),
            telemetry,
        );

        let quote = adapter.latest(&env, &asset).unwrap();
        assert_eq!(quote.price, 250);
        assert_eq!(fallback.get(), 1);
        assert_eq!(success.get(), 1);
        assert_eq!(failure.get(), 0);
    }

    #[test]
    fn all_sources_failing_is_actionable_and_records_failure() {
        let env = env();
        let asset = Symbol::new(&env, "XLM");
        let (success, fallback, failure) = counters();
        let telemetry = CountingTelemetry {
            success: &success,
            fallback: &fallback,
            failure: &failure,
        };
        let adapter = OracleAdapter::new(
            StubSource::failing(),
            Some(StubSource::failing()),
            telemetry,
        );

        assert_eq!(
            adapter.latest(&env, &asset),
            Err(AdapterError::SourceUnavailable)
        );
        assert_eq!(failure.get(), 1);
        assert_eq!(success.get(), 0);
    }

    #[test]
    fn invalid_quotes_are_rejected_without_success_telemetry() {
        let env = env();
        let asset = Symbol::new(&env, "XLM");
        let (success, fallback, failure) = counters();
        let telemetry = CountingTelemetry {
            success: &success,
            fallback: &fallback,
            failure: &failure,
        };
        let adapter = OracleAdapter::new(StubSource::quote(0, 990), None::<StubSource>, telemetry);

        assert_eq!(
            adapter.latest(&env, &asset),
            Err(AdapterError::InvalidQuote)
        );
        assert_eq!(success.get(), 0);
        assert_eq!(fallback.get(), 0);
    }

    #[test]
    fn staleness_boundary_is_enforced() {
        let env = env();
        let asset = Symbol::new(&env, "XLM");
        let adapter = OracleAdapter::new(
            StubSource::quote(100, 900),
            None::<StubSource>,
            NoopTelemetry,
        )
        .with_max_staleness(100);
        assert_eq!(adapter.latest(&env, &asset).unwrap().price, 100);

        let adapter = OracleAdapter::new(
            StubSource::quote(100, 899),
            None::<StubSource>,
            NoopTelemetry,
        )
        .with_max_staleness(100);
        assert_eq!(
            adapter.latest(&env, &asset),
            Err(AdapterError::StaleQuote {
                age: 101,
                max_age: 100
            })
        );
    }
}
