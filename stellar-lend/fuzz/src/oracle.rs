use crate::encoding::{parse_actions, ACTION_BYTES_LEN};
use oracle_hub::{FeedMode, FeedPriority, OracleHubContract, OracleHubContractClient};
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

const MAX_ACTIONS: usize = 16;
const BASE_PRICE: i128 = 1_000_000_000;

/// Exercises registration, bounded price updates, and multi-feed aggregation.
///
/// Inputs are deliberately mapped into a narrow positive price range so expected
/// market movement does not turn the fuzz target into an invalid-input test.
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

        client.report_price(
            &asset,
            &primary_price,
            &(action.kind() as u32),
            &FeedPriority::Primary,
        );
        client.report_price(
            &asset,
            &secondary_price,
            &(action.asset_a() as u32),
            &FeedPriority::Secondary,
        );

        let aggregate = client.get_price(&asset);
        assert!(aggregate.price > 0);
        assert!(aggregate.num_active_feeds >= 1);
    }
}
