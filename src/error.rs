
use soroban_sdk::xdr::Hash;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyPoolError {
    InvalidMerkleProof,
    InvalidZKProof,
    NullifierAlreadySpent,
    TokenTransferFailed,
    #[allow(dead_code)]
    __Unused(u32),
}
