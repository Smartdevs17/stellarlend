use super::*;
use soroban_sdk::{testutils::Address as _, Env};

fn setup_test() -> (Env, StealthAddressRegistryClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(StealthAddressRegistry, ());
    let client = StealthAddressRegistryClient::new(&env, &contract_id);

    (env, client)
}

fn generate_key(env: &Env) -> BytesN<32> {
    let arr: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    BytesN::from_array(env, &arr)
}

#[test]
fn test_register_success() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);
    let spend_key = generate_key(&env);
    let view_key = generate_key(&env);

    client.register(&user, &spend_key, &view_key);

    assert!(client.is_registered(&user));
    assert_eq!(client.get_registered_count(), 1);

    let meta = client.get_meta_address(&user).unwrap();
    assert_eq!(meta.spend_public_key, spend_key);
    assert_eq!(meta.view_public_key, view_key);
    assert_eq!(meta.scheme_id, 1);
}

#[test]
fn test_register_duplicate_fails() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);
    let spend_key = generate_key(&env);
    let view_key = generate_key(&env);

    client.register(&user, &spend_key, &view_key);

    let result = client.try_register(&user, &spend_key, &view_key);
    assert_eq!(result, Err(Ok(StealthError::AlreadyRegistered)));
    assert_eq!(client.get_registered_count(), 1);
}

#[test]
fn test_is_registered() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);
    let other = Address::generate(&env);
    let spend_key = generate_key(&env);
    let view_key = generate_key(&env);

    assert!(!client.is_registered(&user));
    assert!(!client.is_registered(&other));

    client.register(&user, &spend_key, &view_key);

    assert!(client.is_registered(&user));
    assert!(!client.is_registered(&other));
}

#[test]
fn test_get_registered_count() {
    let (env, client) = setup_test();
    assert_eq!(client.get_registered_count(), 0);

    for i in 0..5u8 {
        let user = Address::generate(&env);
        let mut spend_arr = [0u8; 32];
        spend_arr[0] = i + 1;
        let spend_key = BytesN::from_array(&env, &spend_arr);
        let mut view_arr = [0u8; 32];
        view_arr[31] = i + 1;
        let view_key = BytesN::from_array(&env, &view_arr);

        client.register(&user, &spend_key, &view_key);
        assert_eq!(client.get_registered_count(), (i as u32) + 1);
    }
}

#[test]
fn test_compute_stealth_address() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);
    let spend_key = generate_key(&env);
    let view_key = generate_key(&env);

    client.register(&user, &spend_key, &view_key);

    let ephemeral_key = {
        let arr: [u8; 32] = [
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0x99, 0x00, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0xab, 0xcd, 0xef, 0x01,
            0x23, 0x45, 0x67, 0x89,
        ];
        BytesN::from_array(&env, &arr)
    };

    let addr = client.compute_stealth_address(&user, &ephemeral_key);
    assert_eq!(addr.ephemeral_public_key, ephemeral_key);
    assert_ne!(addr.stealth_public_key, spend_key);

    let stored = client.get_stealth_address(&user).unwrap();
    assert_eq!(stored.stealth_public_key, addr.stealth_public_key);
}

#[test]
fn test_get_all_recipients() {
    let (env, client) = setup_test();

    let initial = client.get_all_recipients();
    assert_eq!(initial.len(), 0);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let spend_key = generate_key(&env);
    let view_key = generate_key(&env);

    client.register(&user1, &spend_key, &view_key);
    client.register(&user2, &spend_key, &view_key);

    let recipients = client.get_all_recipients();
    assert_eq!(recipients.len(), 2);
    assert_eq!(recipients.get(0).unwrap(), user1);
    assert_eq!(recipients.get(1).unwrap(), user2);
}

#[test]
fn test_get_meta_address_not_registered() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);

    let meta = client.get_meta_address(&user);
    assert!(meta.is_none());
}

#[test]
fn test_deterministic_stealth_address() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);
    let spend_key = generate_key(&env);
    let view_key = generate_key(&env);

    client.register(&user, &spend_key, &view_key);

    let ephemeral_key = generate_key(&env);

    let addr1 = client.compute_stealth_address(&user, &ephemeral_key);
    let addr2 = client.compute_stealth_address(&user, &ephemeral_key);

    assert_eq!(addr1.stealth_public_key, addr2.stealth_public_key);
    assert_eq!(addr1.view_tag, addr2.view_tag);
}

#[test]
fn test_register_multiple_users() {
    let (env, client) = setup_test();

    for i in 0u8..15u8 {
        let user = Address::generate(&env);
        let mut spend_arr = [0u8; 32];
        spend_arr[0] = i + 1;
        let spend_key = BytesN::from_array(&env, &spend_arr);
        let mut view_arr = [0u8; 32];
        view_arr[31] = i + 1;
        let view_key = BytesN::from_array(&env, &view_arr);

        client.register(&user, &spend_key, &view_key);
        assert!(client.is_registered(&user));
    }

    assert_eq!(client.get_registered_count(), 15);
}

#[test]
fn test_invalid_public_key_rejected_on_registration() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);
    let zero_key = BytesN::from_array(&env, &[0u8; 32]);
    let valid_key = generate_key(&env);

    // Spend key is zero
    let res1 = client.try_register(&user, &zero_key, &valid_key);
    assert_eq!(res1, Err(Ok(StealthError::InvalidPublicKey)));

    // View key is zero
    let res2 = client.try_register(&user, &valid_key, &zero_key);
    assert_eq!(res2, Err(Ok(StealthError::InvalidPublicKey)));

    // Both zero
    let res3 = client.try_register(&user, &zero_key, &zero_key);
    assert_eq!(res3, Err(Ok(StealthError::InvalidPublicKey)));
}

#[test]
fn test_invalid_ephemeral_key_rejected_on_compute() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);
    let spend_key = generate_key(&env);
    let view_key = generate_key(&env);
    client.register(&user, &spend_key, &view_key);

    let zero_key = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_compute_stealth_address(&user, &zero_key);
    assert_eq!(res, Err(Ok(StealthError::InvalidPublicKey)));
}

#[test]
fn test_stealth_address_persisted_by_key_no_overwrite() {
    let (env, client) = setup_test();
    let user = Address::generate(&env);
    let spend_key = generate_key(&env);
    let view_key = generate_key(&env);
    client.register(&user, &spend_key, &view_key);

    // First sender computes stealth address with ephemeral_1
    let mut eph1_arr = [0x11u8; 32];
    eph1_arr[0] = 0xaa;
    let eph1 = BytesN::from_array(&env, &eph1_arr);
    let addr1 = client.compute_stealth_address(&user, &eph1);

    // Second sender computes stealth address with ephemeral_2
    let mut eph2_arr = [0x22u8; 32];
    eph2_arr[0] = 0xbb;
    let eph2 = BytesN::from_array(&env, &eph2_arr);
    let addr2 = client.compute_stealth_address(&user, &eph2);

    assert_ne!(addr1.stealth_public_key, addr2.stealth_public_key);

    // Both addresses are safely preserved and retrievable by their unique stealth public key
    let retrieved1 = client.get_stealth_address_by_key(&addr1.stealth_public_key).unwrap();
    assert_eq!(retrieved1.ephemeral_public_key, eph1);

    let retrieved2 = client.get_stealth_address_by_key(&addr2.stealth_public_key).unwrap();
    assert_eq!(retrieved2.ephemeral_public_key, eph2);

    // Latest stealth address for user points to addr2
    let latest = client.get_stealth_address(&user).unwrap();
    assert_eq!(latest.stealth_public_key, addr2.stealth_public_key);
}

#[test]
fn test_dual_key_derivation_respects_view_key() {
    let (env, client) = setup_test();
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    let spend_key = generate_key(&env);

    let view_a_arr = [0x33u8; 32];
    let view_key_a = BytesN::from_array(&env, &view_a_arr);

    let view_b_arr = [0x44u8; 32];
    let view_key_b = BytesN::from_array(&env, &view_b_arr);

    // Both users share the same spend public key, but have distinct view public keys
    client.register(&user_a, &spend_key, &view_key_a);
    client.register(&user_b, &spend_key, &view_key_b);

    let ephemeral_key = generate_key(&env);

    let addr_a = client.compute_stealth_address(&user_a, &ephemeral_key);
    let addr_b = client.compute_stealth_address(&user_b, &ephemeral_key);

    // In a correct DKSAP implementation, shared secret and view tag depend on the view key!
    // Since view keys differ, view tags and derived stealth keys MUST differ.
    assert_ne!(addr_a.view_tag, addr_b.view_tag);
    assert_ne!(addr_a.stealth_public_key, addr_b.stealth_public_key);
}

#[test]
fn test_verify_view_tag() {
    let (env, client) = setup_test();
    let view_key = generate_key(&env);
    let mut eph_arr = [0x77u8; 32];
    eph_arr[0] = 0x99;
    let eph_key = BytesN::from_array(&env, &eph_arr);

    // User registers and computes stealth address
    let user = Address::generate(&env);
    let spend_key = generate_key(&env);
    client.register(&user, &spend_key, &view_key);

    let addr = client.compute_stealth_address(&user, &eph_key);

    // Verification succeeds with correct tag
    let ok = client.verify_view_tag(&view_key, &eph_key, &addr.view_tag);
    assert!(ok);

    // Verification fails with tampered view tag
    let mut bad_tag_arr = [0x00u8; 16];
    bad_tag_arr[0] = 0xff;
    let bad_tag = BytesN::from_array(&env, &bad_tag_arr);
    let err = client.try_verify_view_tag(&view_key, &eph_key, &bad_tag);
    assert_eq!(err, Err(Ok(StealthError::InvalidViewTag)));

    // Verification fails with zero public key
    let zero_key = BytesN::from_array(&env, &[0u8; 32]);
    let err_zero = client.try_verify_view_tag(&zero_key, &eph_key, &addr.view_tag);
    assert_eq!(err_zero, Err(Ok(StealthError::InvalidPublicKey)));
}

#[test]
fn test_recipients_pagination() {
    let (env, client) = setup_test();

    let mut users = soroban_sdk::Vec::new(&env);
    for i in 0u8..6u8 {
        let user = Address::generate(&env);
        let mut spend_arr = [0u8; 32];
        spend_arr[0] = i + 1;
        let spend_key = BytesN::from_array(&env, &spend_arr);
        let mut view_arr = [0u8; 32];
        view_arr[31] = i + 1;
        let view_key = BytesN::from_array(&env, &view_arr);

        client.register(&user, &spend_key, &view_key);
        users.push_back(user);
    }

    // Page 1: start=0, limit=2
    let page1 = client.get_recipients_page(&0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap(), users.get(0).unwrap());
    assert_eq!(page1.get(1).unwrap(), users.get(1).unwrap());

    // Page 2: start=2, limit=2
    let page2 = client.get_recipients_page(&2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap(), users.get(2).unwrap());
    assert_eq!(page2.get(1).unwrap(), users.get(3).unwrap());

    // Page 3: start=4, limit=10 (limit exceeds remaining)
    let page3 = client.get_recipients_page(&4, &10);
    assert_eq!(page3.len(), 2);
    assert_eq!(page3.get(0).unwrap(), users.get(4).unwrap());
    assert_eq!(page3.get(1).unwrap(), users.get(5).unwrap());

    // Out of bounds: start=10, limit=5
    let empty = client.get_recipients_page(&10, &5);
    assert_eq!(empty.len(), 0);

    // Zero limit
    let zero_lim = client.get_recipients_page(&0, &0);
    assert_eq!(zero_lim.len(), 0);
}
