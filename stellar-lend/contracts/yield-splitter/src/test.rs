#![cfg(test)]
use crate::{YieldSplitter, YieldSplitterClient, YieldSplitterError};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, Env,
};

fn setup() -> (Env, Address, YieldSplitterClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(YieldSplitter, ());
    let client = YieldSplitterClient::new(&env, &contract_id);

    client.initialize(&admin);
    (env, admin, client)
}

fn setup_with_tokens() -> (
    Env,
    Address,
    YieldSplitterClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let contract_id = env.register(YieldSplitter, ());
    let underlying = env.register_stellar_asset_contract(admin.clone());
    let principal_token = env.register_stellar_asset_contract(contract_id.clone());
    let yield_token = env.register_stellar_asset_contract(contract_id.clone());
    let client = YieldSplitterClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.register_tokens(&admin, &principal_token, &yield_token);

    (
        env,
        admin,
        client,
        contract_id,
        owner,
        underlying,
        principal_token,
        yield_token,
    )
}

#[test]
fn test_initialize() {
    let (_env, admin, client) = setup();
    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, Some(admin));
}

#[test]
fn test_preview_split() {
    let (_env, _admin, client) = setup();
    let now = _env.ledger().timestamp();
    let maturity = now + 86400 * 365;

    let preview = client.preview_split(&100_000, &maturity);
    assert_eq!(preview.pt_amount, 100_000);
    assert_eq!(preview.yt_amount, 100_000);
    assert!(preview.estimated_yield > 0);
    assert!(preview.time_to_maturity_secs > 0);
}

#[test]
fn test_preview_split_invalid_maturity() {
    let (_env, _admin, client) = setup();
    let now = _env.ledger().timestamp();
    let result = client.try_preview_split(&100_000, &now);
    assert_eq!(result, Err(Ok(YieldSplitterError::InvalidMaturity)));
}

#[test]
fn test_get_yield_accrued_no_position() {
    let (_env, _admin, client) = setup();
    let accrued = client.get_yield_accrued(&1);
    assert_eq!(accrued, 0);
}

#[test]
fn test_get_admin() {
    let (_env, admin, client) = setup();
    let stored = client.get_admin();
    assert_eq!(stored, Some(admin));
}

#[test]
fn test_get_owner_splits_empty() {
    let (_env, _admin, client) = setup();
    let owner = Address::generate(&_env);
    let splits = client.get_owner_splits(&owner);
    assert_eq!(splits.len(), 0);
}

#[test]
fn test_split_requires_and_custodies_underlying() {
    let (env, _admin, client, contract_id, owner, underlying, principal_token, yield_token) =
        setup_with_tokens();
    let amount = 100i128;
    let token = StellarAssetClient::new(&env, &underlying);
    token.mint(&owner, &amount);

    let maturity = env.ledger().timestamp() + 1;
    let position_id = client.split_position(
        &owner,
        &underlying,
        &principal_token,
        &yield_token,
        &amount,
        &maturity,
    );

    let splitter_balance = token.balance(&contract_id);
    assert_eq!(splitter_balance, amount);
    assert_eq!(token.balance(&owner), 0);
    assert_eq!(StellarAssetClient::new(&env, &principal_token).balance(&owner), amount);
    assert_eq!(StellarAssetClient::new(&env, &yield_token).balance(&owner), amount);

    env.ledger().set_timestamp(maturity);
    let returned = client.merge_tokens(&owner, &position_id);
    assert_eq!(returned, amount);
    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(token.balance(&owner), amount);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_split_without_underlying_balance_fails() {
    let (env, _admin, client, _contract_id, owner, underlying, principal_token, yield_token) =
        setup_with_tokens();
    let maturity = env.ledger().timestamp() + 1;

    // The transfer into the splitter must fail before any PT/YT can be minted.
    client.split_position(
        &owner,
        &underlying,
        &principal_token,
        &yield_token,
        &100,
        &maturity,
    );
}

#[test]
fn test_negative_early_redemption_penalty_is_rejected() {
    let (env, _admin, client, _contract_id, owner, underlying, principal_token, yield_token) =
        setup_with_tokens();
    let amount = 100i128;
    let token = StellarAssetClient::new(&env, &underlying);
    token.mint(&owner, &amount);

    let maturity = env.ledger().timestamp() + 100;
    let position_id = client.split_position(
        &owner,
        &underlying,
        &principal_token,
        &yield_token,
        &amount,
        &maturity,
    );

    let result = client.try_merge_before_maturity(&owner, &position_id, &-1);
    assert_eq!(result, Err(Ok(YieldSplitterError::InvalidPenalty)));
}

#[test]
fn test_multi_user_solvency_and_no_principal_drain() {
    let (env, _admin, client, contract_id, _owner, underlying, principal_token, yield_token) =
        setup_with_tokens();
    let token = StellarAssetClient::new(&env, &underlying);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    token.mint(&alice, &100);
    token.mint(&bob, &100);

    let one_year = 86400 * 365;
    let maturity = env.ledger().timestamp() + one_year;

    // Alice and Bob split 100 units each
    let pos_alice = client.split_position(
        &alice,
        &underlying,
        &principal_token,
        &yield_token,
        &100,
        &maturity,
    );
    let pos_bob = client.split_position(
        &bob,
        &underlying,
        &principal_token,
        &yield_token,
        &100,
        &maturity,
    );

    // Contract holds exactly 200 units
    assert_eq!(token.balance(&contract_id), 200);

    // Fast-forward past maturity
    env.ledger().set_timestamp(maturity + 10);

    // Alice redeems her position
    let alice_returned = client.merge_tokens(&alice, &pos_alice);
    assert_eq!(alice_returned, 100);
    assert_eq!(token.balance(&alice), 100);
    assert_eq!(token.balance(&contract_id), 100);

    // Bob can also successfully redeem his full 100 principal without InsufficientBalance
    let bob_returned = client.merge_tokens(&bob, &pos_bob);
    assert_eq!(bob_returned, 100);
    assert_eq!(token.balance(&bob), 100);
    assert_eq!(token.balance(&contract_id), 0);
}

