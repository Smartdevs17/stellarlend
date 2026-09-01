//! Shared test harness for the Oracle Hub.
//!
//! Provides `TestEnv`, a registered hub with governance/admin, a client
//! accessor, a mock pull-based provider contract, and small utilities used by
//! every suite.

use crate::types::{FeedMode, FeedPriority, ProviderPrice};
use crate::{OracleHubContract, OracleHubContractClient};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, IntoVal};

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
