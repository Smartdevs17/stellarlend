
use soroban_sdk::{testutils::Address as _, Env, Symbol, Vec, Hash, Address};
use soroban_zks::zkp::{Proof, generate_proof, PublicInput};

#[test]
fn test_valid_shielded_withdrawal() {
    let env = Env::default();
    let config = PrivacyPoolConfig {
        asset: Address::generate(&env),
        zk_verifying_key: Hash::from_seed(&env, b"zk_verifying_key"),
        ..Default::default()
    };

    let valid_proof = generate_valid_proof(&env, &config);
    let result = PrivacyPool::shielded_withdraw(&env, &config, valid_proof);
    assert!(result.is_ok());
}

#[test]
fn test_invalid_zk_proof_rejection() {
    let env = Env::default();
    let config = PrivacyPoolConfig {
        asset: Address::generate(&env),
        zk_verifying_key: Hash::from_seed(&env, b"zk_verifying_key"),
        ..Default::default()
    };

    let mut invalid_proof = generate_valid_proof(&env, &config);
    invalid_proof.zk_proof = Proof::default(); // Invalid proof

    let result = PrivacyPool::shielded_withdraw(&env, &config, invalid_proof);
    assert_eq!(result, Err(PrivacyPoolError::InvalidZKProof));
}

#[test]
fn test_nullifier_spend_prevention() {
    let env = Env::default();
    let config = PrivacyPoolConfig {
        asset: Address::generate(&env),
        zk_verifying_key: Hash::from_seed(&env, b"zk_verifying_key"),
        ..Default::default()
    };

    let proof = generate_valid_proof(&env, &config);
    PrivacyPool::shielded_withdraw(&env, &config, proof.clone()).unwrap();

    let result = PrivacyPool::shielded_withdraw(&env, &config, proof);
    assert_eq!(result, Err(PrivacyPoolError::NullifierAlreadySpent));
}

fn generate_valid_proof(env: &Env, config: &PrivacyPoolConfig) -> ShieldedWithdrawProof {
    ShieldedWithdrawProof {
        commitment: Hash::from_seed(env, b"valid_commitment"),
        nullifier: Hash::from_seed(env, b"valid_nullifier"),
        recipient: Address::generate(env),
        amount: 100,
        siblings: vec![Hash::from_seed(env, b"sibling1")],
        path_indices: vec![0],
        zk_proof: generate_proof(
            env,
            &config.zk_verifying_key,
            &PublicInput {
                commitment: Hash::from_seed(env, b"valid_commitment"),
                recipient: Address::generate(env),
                amount: 100,
            },
        ),
    }
}
