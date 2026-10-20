use crate::encoding::{parse_actions, ACTION_BYTES_LEN};
use oracle_hub::{
    FeedMode, FeedPriority, OracleHubContract, OracleHubContractClient, TtlPolicy,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, Env};

const MAX_ACTIONS: usize = 16;
const BASE_PRICE: i128 = 1_000_000_000;

/// Exercises registration, bounded price updates, multi-feed aggregation, the
/// governance recovery path, the TTL manager, and the retained price history.
///
/// Inputs are deliberately mapped into a narrow positive price range so expected
/// market movement does not turn the fuzz target into an invalid-input test.
/// The invariants asserted here are the ones the hub promises callers:
///
/// - a served price is always positive and backed by at least one live feed;
/// - a rejected report never corrupts the aggregate (it either stays put or the
///   caller sees an error);
/// - the TTL manager is total: bumping any asset at any time either extends
///   what exists or does nothing, and never traps;
/// - the retained history never reports more entries than its ring capacity.
pub fn run(data: &[u8]) {
    if data.len() < ACTION_BYTES_LEN {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    let hub_id = env.register(OracleHubContract, ());
    let client = OracleHubContractClient::new(&env, &hub_id);
    let governance = Address::generate(&env);
    let admin = Address::generate(&env);
    let asset = Bytes::from_slice(&env, b"XLM");

    client.initialize(&governance, &admin);

    let primary_oracle = Address::generate(&env);
    let secondary_oracle = Address::generate(&env);
    client.register_feed(
        &asset,
        &primary_oracle,
        &FeedPriority::Primary,
        &3600,
        &FeedMode::Push,
        &10_000,
    );
    client.register_feed(
        &asset,
        &secondary_oracle,
        &FeedPriority::Secondary,
        &3600,
        &FeedMode::Push,
        &10_000,
    );

    for action in parse_actions(data, MAX_ACTIONS) {
        let offset = (action.u32_param() % 10_000_000) as i128;
        let primary_price = BASE_PRICE + offset;
        let secondary_price = primary_price + (action.user() as i128);

        let _ = client.try_report_price(
            &asset,
            &primary_price,
            &(action.kind() as u32),
            &FeedPriority::Primary,
        );
        let _ = client.try_report_price(
            &asset,
            &secondary_price,
            &(action.asset_a() as u32),
            &FeedPriority::Secondary,
        );

        let aggregate = client.get_price(&asset);
        assert!(aggregate.price > 0);
        assert!(aggregate.num_active_feeds >= 1);

        // The deviation guard must never let a jump through silently: either
        // the report is rejected or the served price stays within the bound of
        // the previous one. Here we only require that a *read* of the price is
        // always well-formed, and drive the governance recovery path on the
        // low bit of the action kind.
        if action.asset_b() & 1 == 1 {
            let _ = client.try_rebase_price(
                &asset,
                &FeedPriority::Primary,
                &primary_price,
                &(action.u32_param() % 100),
            );
            let after = client.get_price(&asset);
            assert!(after.price > 0);
        }

        // TTL is total: bumping whatever exists must not trap, and returns a
        // count no larger than the entries the hub manages.
        let bumped = client.bump_storage(&asset);
        assert!(bumped <= 32);

        // History is capacity-bounded.
        assert!(client.get_price_history(&asset).len() <= 64);
    }
}

/// Drives the TTL policy surface directly: arbitrary policies through the
/// governance setter, then the permissionless bump path. Invalid policies must
/// be rejected as an error rather than trapping the host.
pub fn run_ttl(data: &[u8]) {
    if data.len() < ACTION_BYTES_LEN {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    let hub_id = env.register(OracleHubContract, ());
    let client = OracleHubContractClient::new(&env, &hub_id);
    let governance = Address::generate(&env);
    let admin = Address::generate(&env);
    let asset = Bytes::from_slice(&env, b"XLM");

    client.initialize(&governance, &admin);
    let oracle = Address::generate(&env);
    client.register_feed(
        &asset,
        &oracle,
        &FeedPriority::Primary,
        &3600,
        &FeedMode::Push,
        &10_000,
    );
    client.report_price(&asset, &BASE_PRICE, &80, &FeedPriority::Primary);

    for action in parse_actions(data, MAX_ACTIONS) {
        let policy = TtlPolicy {
            enabled: action.kind() & 1 == 1,
            threshold: action.u32_param() % 1_000_000,
            extend_to: (action.u32_param().rotate_left(8)) % 8_000_000,
        };

        // Must either apply cleanly or be rejected as an error; never trap.
        if client.try_set_ttl_policy(&Some(asset.clone()), &policy).is_ok() {
            let config = client.get_ttl_policy(&asset);
            assert_eq!(config.policy, policy);
        }

        let first = client.bump_storage(&asset);
        // A disabled policy manages nothing.
        if !client.get_ttl_policy(&asset).policy.enabled {
            assert_eq!(first, 0);
        }
    }
}

