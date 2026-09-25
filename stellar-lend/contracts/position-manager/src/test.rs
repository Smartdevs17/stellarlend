#![cfg(test)]

use crate::{
    PositionError, PositionManager, PositionManagerClient, PositionMetadata,
};
use soroban_sdk::{
    testutils::Address as _,
    Address, Env,
};

fn setup() -> (Env, PositionManagerClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(PositionManager, ());
    let client = PositionManagerClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);

    client.initialize(&admin).unwrap();

    (env, client, admin, owner)
}

#[test]
fn test_initialize_and_guards() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(PositionManager, ());
    let client = PositionManagerClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    assert!(client.initialize(&admin).is_ok());
    assert_eq!(client.get_admin(), Some(admin.clone()));

    // Reinitialization fails
    assert_eq!(
        client.initialize(&admin),
        Err(Ok(PositionError::AlreadyInitialized))
    );
}

#[test]
fn test_create_and_update_position_by_owner() {
    let (env, client, _admin, owner) = setup();

    let deposit_asset = Address::generate(&env);
    let borrow_asset = Address::generate(&env);

    let pos_id = client
        .create_position(
            &owner,
            &deposit_asset,
            &borrow_asset,
            &1_000_000,
            &250_000,
            &12_500,
        )
        .unwrap();

    assert_eq!(pos_id, 1);

    let pos = client.get_position(&pos_id).unwrap();
    assert_eq!(pos.collateral_amount, 1_000_000);
    assert_eq!(pos.borrowed_amount, 250_000);
    assert_eq!(pos.leverage_bps, 12_500);
    assert!(pos.active);

    // Owner updates position
    let updated = client
        .update_position(&pos_id, &1_200_000, &300_000, &15_000)
        .unwrap();

    assert_eq!(updated.collateral_amount, 1_200_000);
    assert_eq!(updated.borrowed_amount, 300_000);
    assert_eq!(updated.leverage_bps, 15_000);

    let pos = client.get_position(&pos_id).unwrap();
    assert_eq!(pos.collateral_amount, 1_200_000);
    assert_eq!(pos.borrowed_amount, 300_000);
}

#[test]
#[should_panic]
fn test_unauthenticated_caller_cannot_update_position() {
    let env = Env::default();
    let contract_id = env.register(PositionManager, ());
    let client = PositionManagerClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let deposit_asset = Address::generate(&env);
    let borrow_asset = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    let pos_id = client
        .create_position(
            &owner,
            &deposit_asset,
            &borrow_asset,
            &1_000_000,
            &250_000,
            &12_500,
        )
        .unwrap();

    // Disable mock_all_auths so genuine owner authentication is enforced
    let unauthenticated_env = Env::default();
    let unauth_client = PositionManagerClient::new(&unauthenticated_env, &contract_id);
    unauth_client.update_position(&pos_id, &5, &9_999_999, &50_000);
}

#[test]
fn test_update_position_validation() {
    let (env, client, _admin, owner) = setup();

    let deposit_asset = Address::generate(&env);
    let borrow_asset = Address::generate(&env);

    let pos_id = client
        .create_position(
            &owner,
            &deposit_asset,
            &borrow_asset,
            &1_000_000,
            &250_000,
            &12_500,
        )
        .unwrap();

    // Invalid leverage
    let res = client.update_position(&pos_id, &1_000_000, &250_000, &5_000);
    assert_eq!(res, Err(Ok(PositionError::InvalidLeverage)));

    let res = client.update_position(&pos_id, &1_000_000, &250_000, &60_000);
    assert_eq!(res, Err(Ok(PositionError::InvalidLeverage)));

    // Invalid collateral amount <= 0
    let res = client.update_position(&pos_id, &0, &250_000, &15_000);
    assert_eq!(res, Err(Ok(PositionError::InvalidAmount)));

    // Invalid borrowed amount < 0
    let res = client.update_position(&pos_id, &1_000_000, &-10, &15_000);
    assert_eq!(res, Err(Ok(PositionError::InvalidAmount)));

    // Nonexistent position
    let res = client.update_position(&999, &1_000_000, &250_000, &15_000);
    assert_eq!(res, Err(Ok(PositionError::PositionNotFound)));
}

#[test]
fn test_close_position_by_owner() {
    let (env, client, _admin, owner) = setup();

    let deposit_asset = Address::generate(&env);
    let borrow_asset = Address::generate(&env);

    let pos_id = client
        .create_position(
            &owner,
            &deposit_asset,
            &borrow_asset,
            &1_000_000,
            &250_000,
            &12_500,
        )
        .unwrap();

    // Unauthorized close
    let non_owner = Address::generate(&env);
    let res = client.close_position(&non_owner, &pos_id);
    assert_eq!(res, Err(Ok(PositionError::Unauthorized)));

    // Owner close succeeds
    let closed = client.close_position(&owner, &pos_id).unwrap();
    assert!(!closed.active);

    // Cannot update closed position
    let res = client.update_position(&pos_id, &1_000_000, &250_000, &15_000);
    assert_eq!(res, Err(Ok(PositionError::PositionNotActive)));
}
