
use soroban_sdk::{Env, Vec, Hash, Address};
use soroban_zks::zkp::{verify_proof, Proof};

#[derive(Debug, Clone)]
pub struct WithdrawalPublicInput {
    pub commitment: Hash,
    pub recipient: Address,
    pub amount: i128,
}

pub fn verify_withdrawal_proof(
    env: &Env,
    verifying_key: &Hash,
    proof: &Proof,
    input: &WithdrawalPublicInput,
) -> bool {
    verify_proof(env, verifying_key, proof, input)
}
