//! Fixture management and data seeding against real Soroban contracts.

use soroban_sdk::{testutils::Address as _, Address};
use test_utils::{
    register_price_oracle, snapshotless_env, PriceOracleClient, ProtocolFixture, Seeder, TestEnv,
    TokenFixture, ORACLE_PRICE_SCALE,
};

#[test]
fn builder_defaults() {
    let f = ProtocolFixture::builder().build();
    assert_eq!(f.users.len(), 2);
    assert_eq!(f.tokens.len(), 1);
    assert_eq!(f.price(0), ORACLE_PRICE_SCALE);
    assert_eq!(f.token(0).balance(f.user(0)), 0);
}

#[test]
fn builder_seeds_users_tokens_prices_and_time() {
    let f = ProtocolFixture::builder()
        .users(4)
        .tokens(3)
        .default_price(2 * ORACLE_PRICE_SCALE)
        .token_price(1, ORACLE_PRICE_SCALE / 2)
        .initial_balance(1_000)
        .timestamp(1_700_000_000)
        .build();

    assert_eq!(f.users.len(), 4);
    assert_eq!(f.tokens.len(), 3);
    assert_eq!(f.price(0), 2 * ORACLE_PRICE_SCALE);
    assert_eq!(f.price(1), ORACLE_PRICE_SCALE / 2);
    assert_eq!(f.price(2), 2 * ORACLE_PRICE_SCALE);
    for token in &f.tokens {
        for user in &f.users {
            assert_eq!(token.balance(user), 1_000);
        }
    }
    assert_eq!(f.env.ledger().timestamp(), 1_700_000_000);
    f.advance_time(60);
    assert_eq!(f.env.ledger().timestamp(), 1_700_000_060);
}

#[test]
fn tokens_are_real_stellar_asset_contracts() {
    let f = ProtocolFixture::builder().initial_balance(500).build();
    let token = f.token(0);
    token.client().transfer(f.user(0), f.user(1), &200);
    assert_eq!(token.balance(f.user(0)), 300);
    assert_eq!(token.balance(f.user(1)), 700);
    assert_eq!(token.admin_client().admin(), f.admin);
}

#[test]
fn price_oracle_quotes_default_until_overridden() {
    let env = snapshotless_env();
    let oracle = PriceOracleClient::new(&env, &register_price_oracle(&env, 7));
    let (a, b) = (Address::generate(&env), Address::generate(&env));
    assert_eq!(oracle.price(&a), 7);
    oracle.set_price(&a, &11);
    assert_eq!(oracle.price(&a), 11);
    assert_eq!(oracle.price(&b), 7);
    oracle.set_default_price(&9);
    assert_eq!(oracle.price(&b), 9);
    assert_eq!(oracle.price(&a), 11);
}

#[test]
fn test_env_builders_compose() {
    let mut t = TestEnv::snapshotless()
        .with_timestamp(100)
        .with_ledger_sequence(5)
        .with_unlimited_budget();
    let users = t.generate_users(3);
    assert_eq!(users.len(), 3);
    assert_eq!(t.users.len(), 3);
    t.advance_time(10);
    t.advance_ledger(2);
    assert_eq!(t.env.ledger().timestamp(), 110);
    assert_eq!(t.env.ledger().sequence(), 7);

    // A fixture can be layered on an env the test already owns.
    let f = ProtocolFixture::builder()
        .with_env(t.env.clone())
        .users(1)
        .build();
    let token = TokenFixture::deploy(&f.env, &f.admin);
    token.mint(f.user(0), 42);
    assert_eq!(token.balance(f.user(0)), 42);
}

#[test]
fn seeding_is_deterministic_per_seed() {
    let seeded_amounts = |seed: u64| {
        let f = ProtocolFixture::builder().users(5).tokens(2).build();
        let mut seeder = Seeder::new(&f, seed);
        seeder.fund_random(0, 1_000, 50_000);
        seeder.fund_random(1, 10, 20);
        let prices = seeder.random_prices(ORACLE_PRICE_SCALE / 2, 2 * ORACLE_PRICE_SCALE);
        let amounts: Vec<i128> = seeder.seeded().iter().map(|b| b.amount).collect();
        (amounts, prices)
    };
    assert_eq!(seeded_amounts(1), seeded_amounts(1));
    assert_ne!(seeded_amounts(1), seeded_amounts(2));
}

#[test]
fn seeder_mints_and_records_what_it_seeded() {
    let f = ProtocolFixture::builder().users(3).tokens(2).build();
    let mut seeder = Seeder::new(&f, 99);

    seeder.fund_all(0, 1_000);
    let random = seeder.fund_random(1, 100, 200).to_vec();
    for b in &random {
        assert!((100..=200).contains(&b.amount));
        assert_eq!(f.token(1).balance(&b.user), b.amount);
    }

    assert_eq!(seeder.total_seeded(0), 3_000);
    assert_eq!(
        seeder.total_seeded(1),
        random.iter().map(|b| b.amount).sum::<i128>()
    );
    for user in &f.users {
        assert_eq!(seeder.seeded_for(user, 0), f.token(0).balance(user));
        assert_eq!(seeder.seeded_for(user, 1), f.token(1).balance(user));
    }
}

#[test]
fn price_walk_stays_positive_and_bounded() {
    let f = ProtocolFixture::builder().build();
    let mut seeder = Seeder::new(&f, 5);
    let start = f.price(0);
    let path = seeder.price_walk(0, 50, 500);
    assert_eq!(path.len(), 50);
    let mut prev = start;
    for p in &path {
        assert!(*p > 0);
        assert!((p - prev).abs() <= prev * 500 / 10_000 + 1);
        prev = *p;
    }
    assert_eq!(f.price(0), *path.last().unwrap());

    let amounts = seeder.amounts(10, 1, 3);
    assert!(amounts.iter().all(|a| (1..=3).contains(a)));
}
