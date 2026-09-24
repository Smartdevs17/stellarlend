
use soroban_sdk::{contractimport, Env, Symbol, Vec, contracttype, Address, contractimpl, token, panic};
use soroban_sdk::xdr::{self, Hash, MerklePath};
use soroban_zks::zkp::{verify_proof, Proof, PublicInput};

#[contractimport]
pub mod token {
    soroban_sdk::contractimport!(file = "token.rs");
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitmentNote {
    pub commitment: Hash,
    pub amount: i128,
    pub leaf_index: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShieldedWithdrawProof {
    pub commitment: Hash,
    pub nullifier: Hash,
    pub recipient: Address,
    pub amount: i128,
    pub siblings: Vec<Hash>,
    pub path_indices: Vec<u32>,
    pub zk_proof: Proof,
}

#[contractimpl]
pub trait PrivacyPoolClient {
    pub fn shielded_withdraw(
        env: Env,
        config: &PrivacyPoolConfig,
        proof: ShieldedWithdrawProof,
    ) -> Result<(), PrivacyPoolError>;
}

#[contractimpl]
impl PrivacyPoolClient for PrivacyPool {
    pub fn shielded_withdraw(
        env: &Env,
        config: &PrivacyPoolConfig,
        proof: ShieldedWithdrawProof,
    ) -> Result<(), PrivacyPoolError> {
        // 1. Verify Merkle inclusion proof
        let valid_merkle = verify_merkle_proof(
            env,
            config,
            &proof.commitment,
            &proof.siblings,
            &proof.path_indices,
        );
        if !valid_merkle {
            return Err(PrivacyPoolError::InvalidMerkleProof);
        }

        // 2. Verify ZKP for nullifier derivation and amount binding
        let public_input = PublicInput {
            commitment: proof.commitment,
            recipient: proof.recipient,
            amount: proof.amount,
        };

        let valid_zkp = verify_proof(
            env,
            &config.zk_verifying_key,
            &proof.zk_proof,
            &public_input,
        );
        if !valid_zkp {
            return Err(PrivacyPoolError::InvalidZKProof);
        }

        // 3. Verify nullifier hasn't been spent
        let nullifier_key = PrivacyPoolDataKey::Nullifier(proof.nullifier);
        if env.storage().persistent().has(&nullifier_key) {
            return Err(PrivacyPoolError::NullifierAlreadySpent);
        }

        // 4. Mark nullifier as spent
        env.storage().persistent().set(&nullifier_key, &true);

        // 5. Update pool statistics
        update_total_withdrawals(env, proof.amount);

        // 6. Transfer tokens to recipient
        let token_client = token::Client::new(env, &config.asset);
        token_client.transfer(
            &env.current_contract_address(),
            &proof.recipient,
            &proof.amount,
        )?;

        Ok(())
    }
}
