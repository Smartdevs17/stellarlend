use super::*;
use soroban_sdk::{testutils::Address as _, Env};

fn setup_env() -> (Env, Address, PrivacyPoolClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let stealth_contract_id = env.register(stealth_address::StealthAddressRegistry, ());

    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);

    client.initialize(&admin, &asset, &20, &false);

    let user = Address::generate(&env);

    let spend_key = BytesN::from_array(&env, &[1u8; 32]);
    let view_key = BytesN::from_array(&env, &[2u8; 32]);

    let stealth_client =
        stealth_address::StealthAddressRegistryClient::new(&env, &stealth_contract_id);
    stealth_client.register(&user, &spend_key, &view_key);

    (env, stealth_contract_id, client, user)
}

fn make_commitment(env: &Env, seed: u8) -> BytesN<32> {
    let mut arr = [0u8; 32];
    arr[0] = seed;
    arr[31] = seed;
    BytesN::from_array(env, &arr)
}

fn make_ephemeral_key(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[4u8; 32])
}

fn make_nullifier(env: &Env, seed: u8) -> BytesN<32> {
    let mut arr = [0u8; 32];
    arr[0] = seed.wrapping_add(0x80);
    arr[31] = seed.wrapping_add(0x80);
    BytesN::from_array(env, &arr)
}

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);

    let result = client.try_initialize(&admin, &asset, &20, &false);
    assert!(result.is_ok());

    let config = client.get_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.asset, asset);
    assert_eq!(config.tree_depth, 20);
    assert_eq!(config.min_anonymity_set, 10);
}

#[test]
fn test_initialize_duplicate_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);

    client.initialize(&admin, &asset, &20, &false);

    let result = client.try_initialize(&admin, &asset, &20, &false);
    assert_eq!(result, Err(Ok(PrivacyPoolError::AlreadyInitialized)));
}

#[test]
fn test_initialize_invalid_depth_too_small() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);

    let result = client.try_initialize(&admin, &asset, &9, &false);
    assert_eq!(result, Err(Ok(PrivacyPoolError::InvalidProof)));
}

#[test]
fn test_initialize_invalid_depth_too_large() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);

    let result = client.try_initialize(&admin, &asset, &33, &false);
    assert_eq!(result, Err(Ok(PrivacyPoolError::InvalidProof)));
}

#[test]
fn test_shielded_deposit() {
    let (env, stealth_id, client, user) = setup_env();
    let commitment = make_commitment(&env, 1);
    let ephemeral_key = make_ephemeral_key(&env);

    let leaf_index =
        client.shielded_deposit(&commitment, &1000, &stealth_id, &user, &ephemeral_key);
    assert_eq!(leaf_index, 0);

    assert_eq!(client.get_total_deposits(), 1000);

    let note = client.get_commitment(&0).unwrap();
    assert_eq!(note.commitment, commitment);
    assert_eq!(note.amount, 1000);
}

#[test]
fn test_deposit_zero_amount_fails() {
    let (env, stealth_id, client, user) = setup_env();
    let commitment = make_commitment(&env, 1);
    let ephemeral_key = make_ephemeral_key(&env);

    let result = client.try_shielded_deposit(&commitment, &0, &stealth_id, &user, &ephemeral_key);
    assert_eq!(result, Err(Ok(PrivacyPoolError::InvalidAmount)));
}

#[test]
fn test_deposit_negative_amount_fails() {
    let (env, stealth_id, client, user) = setup_env();
    let commitment = make_commitment(&env, 1);
    let ephemeral_key = make_ephemeral_key(&env);

    let result =
        client.try_shielded_deposit(&commitment, &(-100), &stealth_id, &user, &ephemeral_key);
    assert_eq!(result, Err(Ok(PrivacyPoolError::InvalidAmount)));
}

#[test]
fn test_deposit_when_paused() {
    let (env, stealth_id, client, user) = setup_env();
    let commitment = make_commitment(&env, 1);
    let ephemeral_key = make_ephemeral_key(&env);

    let admin = client.get_config().admin;
    client.set_pause_deposit(&admin, &true);

    let result =
        client.try_shielded_deposit(&commitment, &1000, &stealth_id, &user, &ephemeral_key);
    assert_eq!(result, Err(Ok(PrivacyPoolError::DepositPaused)));
}

#[test]
fn test_multiple_deposits() {
    let (env, stealth_id, client, user) = setup_env();
    let ephemeral_key = make_ephemeral_key(&env);

    for i in 0..15 {
        let commitment = make_commitment(&env, (i + 10) as u8);
        let leaf_index = client.shielded_deposit(
            &commitment,
            &(1000 * (i as i128 + 1)),
            &stealth_id,
            &user,
            &ephemeral_key,
        );
        assert_eq!(leaf_index, i);
    }

    assert_eq!(client.get_total_deposits(), 1000 * 120);
    assert_eq!(client.get_anonymity_set_size(), 10);

    for i in 0..15 {
        let note = client.get_commitment(&i).unwrap();
        assert_eq!(note.amount, 1000 * (i as i128 + 1));
    }
}

#[test]
fn test_anonymity_set_tracking() {
    let (env, stealth_id, client, user) = setup_env();
    let ephemeral_key = make_ephemeral_key(&env);

    assert_eq!(client.get_anonymity_set_size(), 0);

    for i in 0..10 {
        let commitment = make_commitment(&env, (i + 20) as u8);
        client.shielded_deposit(&commitment, &500, &stealth_id, &user, &ephemeral_key);
    }

    assert_eq!(client.get_anonymity_set_size(), 10);

    let commitment = make_commitment(&env, 30);
    client.shielded_deposit(&commitment, &500, &stealth_id, &user, &ephemeral_key);
    assert_eq!(client.get_anonymity_set_size(), 10);
}

#[test]
fn test_pause_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let other = Address::generate(&env);

    client.initialize(&admin, &asset, &20, &false);

    let result = client.try_set_pause_deposit(&other, &true);
    assert_eq!(result, Err(Ok(PrivacyPoolError::Unauthorized)));
}

#[test]
fn test_set_pause_withdraw() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);

    client.initialize(&admin, &asset, &20, &false);

    let config = client.get_config();
    assert!(!config.withdraw_paused);

    client.set_pause_withdraw(&admin, &true);
    assert!(client.get_config().withdraw_paused);

    client.set_pause_withdraw(&admin, &false);
    assert!(!client.get_config().withdraw_paused);
}

#[test]
fn test_set_compliance_required() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);

    client.initialize(&admin, &asset, &20, &false);
    assert!(!client.get_config().compliance_required);

    client.set_compliance_required(&admin, &true);
    assert!(client.get_config().compliance_required);
}

#[test]
fn test_nullifier_tracking() {
    let (env, _, client, _) = setup_env();

    let nullifier = make_nullifier(&env, 1);
    assert!(!client.is_nullifier_used(&nullifier));
}

#[test]
fn test_withdraw_insufficient_anonymity_set() {
    let (env, stealth_id, client, user) = setup_env();
    let commitment = make_commitment(&env, 1);
    let ephemeral_key = make_ephemeral_key(&env);

    client.shielded_deposit(&commitment, &1000, &stealth_id, &user, &ephemeral_key);

    let proof = WithdrawalProof {
        nullifier: make_nullifier(&env, 1),
        recipient: user.clone(),
        amount: 500,
        merkle_root: client.get_merkle_root(),
        path_indices: {
            let mut v = Vec::new(&env);
            for _ in 0..20 {
                v.push_back(0u32);
            }
            v
        },
        siblings: {
            let mut v = Vec::new(&env);
            let zero = BytesN::from_array(&env, &[0u8; 32]);
            for _ in 0..20 {
                v.push_back(zero.clone());
            }
            v
        },
    };

    let result = client.try_shielded_withdraw(&proof, &None);
    assert_eq!(result, Err(Ok(PrivacyPoolError::InsufficientAnonymitySet)));
}

#[test]
fn test_get_merkle_root_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);

    client.initialize(&admin, &asset, &20, &false);
    let root = client.get_merkle_root();
    assert_eq!(root, BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
fn test_get_total_withdrawals_zero() {
    let (_env, _, client, _) = setup_env();
    assert_eq!(client.get_total_withdrawals(), 0);
}

#[test]
fn test_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PrivacyPool, ());
    let client = PrivacyPoolClient::new(&env, &contract_id);

    let result = client.try_get_config();
    assert_eq!(result, Err(Ok(PrivacyPoolError::NotInitialized)));
}

#[test]
fn test_shielded_deposit_generates_stealth_address() {
    let (env, stealth_id, client, user) = setup_env();
    let commitment = make_commitment(&env, 42);
    let ephemeral_key = make_ephemeral_key(&env);

    let leaf_index =
        client.shielded_deposit(&commitment, &5000, &stealth_id, &user, &ephemeral_key);
    assert_eq!(leaf_index, 0);
}
