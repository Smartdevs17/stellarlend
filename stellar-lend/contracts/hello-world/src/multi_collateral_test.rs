use crate::{
    deposit::{AssetParams, DepositDataKey},
    HelloContract, HelloContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(HelloContract, ());
    let client = HelloContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, admin, contract_id)
}

// ---- Per-Asset Deposit Tracking --------------------------------------------

#[test]
fn test_deposit_records_per_asset_balance() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    // Enable asset
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 7500,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user, &Some(asset.clone()), &1000);

    assert_eq!(client.get_user_asset_collateral(&user, &asset), 1000);
}

#[test]
fn test_deposit_multiple_assets_tracked_independently() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset_a = Address::generate(&env);
    let asset_b = Address::generate(&env);

    env.as_contract(&contract_id, || {
        for asset in [asset_a.clone(), asset_b.clone()] {
            env.storage().persistent().set(
                &DepositDataKey::AssetParams(asset),
                &AssetParams {
                    deposit_enabled: true,
                    collateral_factor: 7500,
                    max_deposit: 0,
                    borrow_fee_bps: 0,
                },
            );
        }
    });

    client.deposit_collateral(&user, &Some(asset_a.clone()), &500);
    client.deposit_collateral(&user, &Some(asset_b.clone()), &300);

    assert_eq!(client.get_user_asset_collateral(&user, &asset_a), 500);
    assert_eq!(client.get_user_asset_collateral(&user, &asset_b), 300);
}

#[test]
fn test_deposit_same_asset_twice_accumulates() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 10000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user, &Some(asset.clone()), &400);
    client.deposit_collateral(&user, &Some(asset.clone()), &600);

    assert_eq!(client.get_user_asset_collateral(&user, &asset), 1000);
}

// ---- Asset List ------------------------------------------------------------

#[test]
fn test_asset_list_populated_on_first_deposit() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 10000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    assert_eq!(client.get_user_asset_list(&user).len(), 0);
    client.deposit_collateral(&user, &Some(asset.clone()), &100);
    assert_eq!(client.get_user_asset_list(&user).len(), 1);
}

#[test]
fn test_asset_list_no_duplicates() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 10000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user, &Some(asset.clone()), &100);
    client.deposit_collateral(&user, &Some(asset.clone()), &200);

    // Should still be 1, not 2
    assert_eq!(client.get_user_asset_list(&user).len(), 1);
}

#[test]
fn test_asset_list_multiple_assets() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset_a = Address::generate(&env);
    let asset_b = Address::generate(&env);
    let asset_c = Address::generate(&env);

    env.as_contract(&contract_id, || {
        for a in [asset_a.clone(), asset_b.clone(), asset_c.clone()] {
            env.storage().persistent().set(
                &DepositDataKey::AssetParams(a),
                &AssetParams {
                    deposit_enabled: true,
                    collateral_factor: 7500,
                    max_deposit: 0,
                    borrow_fee_bps: 0,
                },
            );
        }
    });

    client.deposit_collateral(&user, &Some(asset_a.clone()), &100);
    client.deposit_collateral(&user, &Some(asset_b.clone()), &200);
    client.deposit_collateral(&user, &Some(asset_c.clone()), &300);

    assert_eq!(client.get_user_asset_list(&user).len(), 3);
}

// ---- Withdrawal Per-Asset Tracking -----------------------------------------

#[test]
fn test_withdraw_updates_per_asset_balance() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 10000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user, &Some(asset.clone()), &1000);
    client.withdraw_collateral(&user, &Some(asset.clone()), &400);

    assert_eq!(client.get_user_asset_collateral(&user, &asset), 600);
}

#[test]
fn test_full_withdrawal_removes_asset_from_list() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 10000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user, &Some(asset.clone()), &500);
    assert_eq!(client.get_user_asset_list(&user).len(), 1);

    client.withdraw_collateral(&user, &Some(asset.clone()), &500);
    assert_eq!(client.get_user_asset_list(&user).len(), 0);
    assert_eq!(client.get_user_asset_collateral(&user, &asset), 0);
}

#[test]
fn test_partial_withdrawal_keeps_asset_in_list() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 10000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user, &Some(asset.clone()), &500);
    client.withdraw_collateral(&user, &Some(asset.clone()), &200);

    assert_eq!(client.get_user_asset_list(&user).len(), 1);
    assert_eq!(client.get_user_asset_collateral(&user, &asset), 300);
}

// ---- Total Collateral Value ------------------------------------------------

#[test]
fn test_total_collateral_value_zero_for_legacy_user() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    // No multi-asset deposits — legacy user
    assert_eq!(client.get_user_total_collateral_value(&user), 0);
}

#[test]
fn test_total_collateral_value_single_asset_no_oracle() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    // Set 75% collateral factor, no oracle price (falls back to 1:1 = 10^8)
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 7500, // 75%
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user, &Some(asset.clone()), &1000);

    // value = 1000 * 1_00_000_000 / 1_00_000_000 * 7500 / 10000 = 750
    let total = client.get_user_total_collateral_value(&user);
    assert_eq!(total, 750);
}

#[test]
fn test_total_collateral_value_two_assets() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset_a = Address::generate(&env);
    let asset_b = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // asset_a: 100% collateral factor, no oracle (1:1)
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset_a.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 10000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
        // asset_b: 50% collateral factor, no oracle (1:1)
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset_b.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 5000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user, &Some(asset_a.clone()), &2000);
    client.deposit_collateral(&user, &Some(asset_b.clone()), &1000);

    // asset_a: 2000 * 10000 / 10000 = 2000
    // asset_b: 1000 * 5000 / 10000 = 500
    // total = 2500
    let total = client.get_user_total_collateral_value(&user);
    assert_eq!(total, 2500);
}

// ---- Borrow Health Factor with Multi-Asset ---------------------------------

#[test]
fn test_borrow_allowed_using_multi_asset_collateral() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let collateral_a = Address::generate(&env);
    let collateral_b = Address::generate(&env);
    let borrow_asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Each collateral asset: 100% factor
        for asset in [
            collateral_a.clone(),
            collateral_b.clone(),
            borrow_asset.clone(),
        ] {
            env.storage().persistent().set(
                &DepositDataKey::AssetParams(asset),
                &AssetParams {
                    deposit_enabled: true,
                    collateral_factor: 10000,
                    max_deposit: 0,
                    borrow_fee_bps: 0,
                },
            );
        }
    });

    // Deposit 5000 in asset_a and 5000 in asset_b = 10000 total collateral value
    client.deposit_collateral(&user, &Some(collateral_a.clone()), &5000);
    client.deposit_collateral(&user, &Some(collateral_b.clone()), &5000);

    // Min collateral ratio is 110% by default.
    // Max borrow ≈ 10000 * 10000 / 11000 ≈ 9090
    // Borrowing 5000 should be well within limit
    let debt = client.borrow_asset(&user, &Some(borrow_asset), &5000);
    assert!(
        debt > 0,
        "Borrow should succeed with multi-asset collateral"
    );
}

#[test]
fn test_per_asset_view_unrelated_to_other_user() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DepositDataKey::AssetParams(asset.clone()),
            &AssetParams {
                deposit_enabled: true,
                collateral_factor: 10000,
                max_deposit: 0,
                borrow_fee_bps: 0,
            },
        );
    });

    client.deposit_collateral(&user_a, &Some(asset.clone()), &1000);

    // user_b has no deposit in this asset
    assert_eq!(client.get_user_asset_collateral(&user_b, &asset), 0);
    assert_eq!(client.get_user_asset_list(&user_b).len(), 0);
}

#[test]
fn test_zero_balance_asset_not_in_list_by_default() {
    let (env, _admin, contract_id) = setup();
    let client = HelloContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let asset = Address::generate(&env);

    // No deposit made
    assert_eq!(client.get_user_asset_collateral(&user, &asset), 0);
    assert_eq!(client.get_user_asset_list(&user).len(), 0);
}
