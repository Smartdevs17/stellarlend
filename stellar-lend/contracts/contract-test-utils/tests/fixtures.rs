use contract_test_utils::{register_price_oracle, snapshotless_env, ProtocolFixture, PriceOracleClient, ORACLE_PRICE_SCALE};
use soroban_sdk::testutils::Address as _;

#[test]
fn protocol_fixture_builds_users_tokens_and_oracle() {
    let fixture = ProtocolFixture::builder()
        .users(2)
        .tokens(1)
        .initial_balance(1_000)
        .default_price(ORACLE_PRICE_SCALE)
        .build();

    assert_eq!(fixture.users.len(), 2);
    assert_eq!(fixture.tokens.len(), 1);
    assert_eq!(fixture.token(0).balance(fixture.user(0)), 1_000);
    assert_eq!(fixture.price(0), ORACLE_PRICE_SCALE);
}

#[test]
fn price_oracle_fixture_can_be_registered_standalone() {
    let env = snapshotless_env();
    let oracle = PriceOracleClient::new(&env, &register_price_oracle(&env, 7));
    let asset = soroban_sdk::Address::generate(&env);
    assert_eq!(oracle.price(&asset), 7);
    oracle.set_price(&asset, &11);
    assert_eq!(oracle.price(&asset), 11);
}
