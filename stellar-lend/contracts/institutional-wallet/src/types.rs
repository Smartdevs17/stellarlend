use soroban_sdk::{contracterror, contracttype, Address, String, Val, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum WalletError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidThreshold = 4,
    InvalidAdmins = 5,
    ProposalNotFound = 6,
    AlreadyVoted = 7,
    ProposalNotActive = 8,
    InsufficientApprovals = 9,
    ExecutionFailed = 10,
    InvalidBatch = 11,
    GuardianAcceptanceRequired = 12,
    GuardianNotAccepted = 13,
    RecoveryNotActive = 14,
    RecoveryAlreadyExists = 15,
    GuardianRotationFailed = 16,
    EmergencyTimeoutActive = 17,
    RecoveryCancelledByOwner = 18,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Config,
    Admins,
    NextProposalId,
    Proposal(u64),
    Approvals(u64),
    AuditTrail(u64),
    Guardians,
    GuardianThreshold,
    RecoveryRequest,
    PendingGuardianInvites,
    GuardianAcceptances(Address),
    LastActivity,
    GuardianApprovals,
    RecoveryCancelRequest,
    RotationProposal,
    RecoveryApprovals,
    PendingGuardianThreshold,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RotationProposal {
    pub new_guardians: Vec<Address>,
    pub new_threshold: u32,
    pub approvals: Vec<Address>,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryRequest {
    pub new_admins: Vec<Address>,
    pub new_threshold: u32,
    pub initiated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultisigConfig {
    pub threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    pub contract: Address,
    pub function: soroban_sdk::Symbol,
    pub args: Vec<Val>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Active,
    Executed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub description: String,
    pub batch: Vec<Transaction>,
    pub status: ProposalStatus,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEntry {
    pub actor: Address,
    pub action: soroban_sdk::Symbol,
    pub timestamp: u64,
}

// Unified error registry: protocol-wide global codes, messages and recovery
// suggestions for every variant (see `stellarlend_errors` and docs/ERROR_HANDLING.md).
stellarlend_errors::impl_contract_error! {
    WalletError => stellarlend_errors::domains::INSTITUTIONAL_WALLET;
    NotInitialized => NotInitialized, "Contract or feature is not initialized";
    AlreadyInitialized => AlreadyInitialized, "Contract or feature is already initialized";
    Unauthorized => Unauthorized, "Caller is not authorized to perform this action";
    InvalidThreshold => InvalidInput, "Invalid threshold";
    InvalidAdmins => InvalidInput, "Invalid admins";
    ProposalNotFound => NotFound, "Proposal not found";
    AlreadyVoted => AlreadyExists, "Already voted";
    ProposalNotActive => InvalidState, "Proposal not active";
    InsufficientApprovals => InvalidState, "Insufficient approvals", RetryLater;
    ExecutionFailed => Internal, "Execution failed";
    InvalidBatch => InvalidInput, "Invalid batch";
    GuardianAcceptanceRequired => InvalidState, "Guardian acceptance required";
    GuardianNotAccepted => InvalidState, "Guardian not accepted";
    RecoveryNotActive => InvalidState, "Recovery not active";
    RecoveryAlreadyExists => AlreadyExists, "Recovery already exists";
    GuardianRotationFailed => Internal, "Guardian rotation failed";
    EmergencyTimeoutActive => Paused, "Emergency timeout active";
    RecoveryCancelledByOwner => InvalidState, "Recovery cancelled by owner";
}
