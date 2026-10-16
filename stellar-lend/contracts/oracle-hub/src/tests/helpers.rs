//! Shared test harness for the Oracle Hub.
//!
//! Provides `TestEnv`, a registered hub with governance/admin, a client
//! accessor, a mock pull-based provider contract, and small utilities used by
//! every suite.

use crate::types::{FeedMode, FeedPriority, ProviderPrice};
use crate::{OracleHubContract, OracleHubContractClient};
use soroban_sdk::testutils::{Address as _, Events, MockAuth, MockAuthInvoke};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Bytes, Env, IntoVal, Symbol, TryFromVal, Val,
    Vec,
};

/// A wired-up hub instance plus its actor addresses.
pub struct TestEnv {
    pub env: Env,
    pub contract_id: Address,
    pub governance: Address,
    pub admin: Address,
}

/// Deploy and initialize a fresh hub.
pub fn setup() -> TestEnv {
    let env = Env::default();
    let governance = Address::generate(&env);
    let admin = Address::generate(&env);
    let contract_id = env.register(OracleHubContract, ());
    let client = OracleHubContractClient::new(&env, &contract_id);
    env.mock_auths(&[
        MockAuth {
            address: &governance,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: (&governance, &admin).into_val(&env),
                sub_invokes: &[],
            },
        },
        MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: (&governance, &admin).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);
    client.initialize(&governance, &admin);
    TestEnv {
        env,
        contract_id,
        governance,
        admin,
    }
}

/// Client handle for the deployed hub.
pub fn client(te: &TestEnv) -> OracleHubContractClient<'_> {
    OracleHubContractClient::new(&te.env, &te.contract_id)
}

/// Build a distinct asset identifier from a label.
pub fn mk_asset(env: &Env, name: &str) -> Bytes {
    Bytes::from_slice(env, name.as_bytes())
}

/// Authorize everything for the remainder of the test (governance + reporters).
pub fn allow_all(te: &TestEnv) {
    te.env.mock_all_auths();
}

/// Register a push feed slot with default weight.
pub fn register_push_feed(
    te: &TestEnv,
    asset: &Bytes,
    oracle: &Address,
    priority: &FeedPriority,
    stale: u64,
) {
    allow_all(te);
    client(te).register_feed(asset, oracle, priority, &stale, &FeedMode::Push, &10000);
}

/// Push a price as `oracle` for `asset`.
pub fn report(te: &TestEnv, asset: &Bytes, oracle: &Address, price: i128, priority: &FeedPriority) {
    te.env.mock_auths(&[MockAuth {
        address: oracle,
        invoke: &MockAuthInvoke {
            contract: &te.contract_id,
            fn_name: "report_price",
            args: (asset, &price, &100u32, priority).into_val(&te.env),
            sub_invokes: &[],
        },
    }]);
    client(te).report_price(asset, &price, &100, priority);
}

/// Register `n` push feeds (one per slot, from `Primary` upwards) and report
/// `prices[i]` on slot `i`. Returns the oracle addresses in slot order.
pub fn register_and_report(te: &TestEnv, asset: &Bytes, prices: &[i128]) -> Vec<Address> {
    let mut oracles: Vec<Address> = Vec::new(&te.env);
    for (i, price) in prices.iter().enumerate() {
        let oracle = Address::generate(&te.env);
        let slot = slot(i);
        register_push_feed(te, asset, &oracle, &slot, 3600);
        report(te, asset, &oracle, *price, &slot);
        oracles.push_back(oracle);
    }
    oracles
}

/// The `i`-th feed slot.
pub fn slot(i: usize) -> FeedPriority {
    match i {
        0 => FeedPriority::Primary,
        1 => FeedPriority::Secondary,
        2 => FeedPriority::Fallback,
        3 => FeedPriority::Quaternary,
        _ => FeedPriority::Quinary,
    }
}

/// Whether an event with the given name was published for `asset`.
///
/// `#[contractevent]` publishes the snake-cased struct name as the first
/// topic and every `#[topic]` field after it, so the asset id appears among
/// the remaining topics of asset-scoped events.
pub fn has_event(te: &TestEnv, name: &str, asset: &Bytes) -> bool {
    let name_val: Val = Symbol::new(&te.env, name).into_val(&te.env);
    let asset_val: Val = asset.clone().into_val(&te.env);
    let expected_name = ScVal::try_from_val(&te.env, &name_val).unwrap();
    let expected_asset = ScVal::try_from_val(&te.env, &asset_val).unwrap();
    te.env.events().all().events().iter().any(|event| {
        let ContractEventBody::V0(body) = &event.body;
        body.topics.first() == Some(&expected_name) && body.topics.contains(&expected_asset)
    })
}

// ── Mock pull-based provider ────────────────────────────────────────────────

/// An external contract that implements the `PriceProvider` interface by
/// returning whatever price was stored for the asset.
#[contract]
pub struct MockProvider;

#[contracttype]
#[derive(Clone)]
pub enum MockProviderKey {
    Price(Bytes),
}

#[contractimpl]
impl MockProvider {
    /// Configure the price the provider returns for an asset.
    pub fn set_price(env: Env, asset: Bytes, price: i128, confidence: u32) {
        env.storage().instance().set(
            &MockProviderKey::Price(asset.clone()),
            &ProviderPrice {
                price,
                decimals: 8,
                timestamp: env.ledger().timestamp(),
                confidence,
            },
        );
    }

    /// Clear any configured price (subsequent pulls return zero).
    pub fn clear_price(env: Env, asset: Bytes) {
        env.storage()
            .instance()
            .remove(&MockProviderKey::Price(asset));
    }

    /// Configure a price quoted at a precision other than the canonical 8.
    pub fn set_scaled_price(env: Env, asset: Bytes, price: i128, decimals: u32, confidence: u32) {
        env.storage().instance().set(
            &MockProviderKey::Price(asset.clone()),
            &ProviderPrice {
                price,
                decimals,
                timestamp: env.ledger().timestamp(),
                confidence,
            },
        );
    }

    pub fn get_price(env: Env, asset: Bytes) -> ProviderPrice {
        env.storage()
            .instance()
            .get(&MockProviderKey::Price(asset))
            .unwrap_or(ProviderPrice {
                price: 0,
                decimals: 8,
                timestamp: env.ledger().timestamp(),
                confidence: 0,
            })
    }
}

/// Register a pull feed backed by the mock provider and return the provider id.
pub fn register_mock_provider(te: &TestEnv, asset: &Bytes, priority: &FeedPriority) -> Address {
    let provider_id = te.env.register(MockProvider, ());
    allow_all(te);
    client(te).register_feed(
        asset,
        &provider_id,
        priority,
        &3600,
        &FeedMode::Pull,
        &10000,
    );
    provider_id
}
