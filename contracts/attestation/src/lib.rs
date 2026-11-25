#![no_std]
use soroban_sdk::{contract, contractimpl, contracterror, Address, BytesN, Env, Symbol, symbol_short};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AttestationError {
    InvalidSigner = 1,
    AttestationExpired = 2,
    InvalidPayloadHash = 3,
}

const ATTESTATION_SIGNER: Symbol = symbol_short!("SIGNER");

#[contract]
pub struct PortfolioAttestation;

#[contractimpl]
impl PortfolioAttestation {
    pub fn init(env: Env, signer: Address) {
        signer.require_auth();
        env.storage().instance().set(&ATTESTATION_SIGNER, &signer);
    }

    pub fn verify_and_record_attestation(
        env: Env,
        borrower: Address,
        portfolio_hash: BytesN<32>,
        timestamp: u64
    ) -> Result<bool, AttestationError> {
        borrower.require_auth();

        let current_time = env.ledger().timestamp();
        if current_time > timestamp + 86400 {
            return Err(AttestationError::AttestationExpired);
        }

        // Store latest attestation for borrower
        let key = (symbol_short!("PORTFOLIO"), borrower.clone());
        env.storage().persistent().set(&key, &portfolio_hash);

        env.events().publish((symbol_short!("ATTEST"), borrower), (portfolio_hash, timestamp));
        Ok(true)
    }
}
