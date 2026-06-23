use soroban_sdk::{contracterror, contracttype, Address, BytesN, String, Vec, Val};

/// Role-based authorization levels for institutional wallet operations.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Copy)]
pub enum WalletRole {
    /// Full access: manage signers, thresholds, roles, and spending limits
    Admin = 0,
    /// Can propose and approve transactions
    Approver = 1,
    /// Can execute approved proposals
    Executor = 2,
    /// Read-only access to wallet state
    Viewer = 3,
}

/// Per-asset daily spending limit configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailySpendingLimit {
    /// Maximum amount that can be spent per day for this asset (0 = unlimited)
    pub daily_limit: i128,
}

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
    /// Spending limit exceeded for the current day
    SpendingLimitExceeded = 19,
    /// Role assignment not found
    RoleNotFound = 20,
    /// Insufficient role for operation
    InsufficientRole = 21,
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
    /// Role assignment per admin: RoleAssignments(Address) -> WalletRole
    RoleAssignments(Address),
    /// Daily spending limit per asset: DailySpending(asset Address, day u64) -> i128
    DailySpending(Address, u64),
    /// Spending limit config per asset: SpendingLimitConfig(Address) -> DailySpendingLimit
    SpendingLimitConfig(Address),
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
    /// Default daily spending limit for all assets (0 = unlimited)
    pub default_daily_spend_limit: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    pub contract: Address,
    pub function: soroban_sdk::Symbol,
    pub args: Vec<Val>,
    /// Optional spending amount for daily limit enforcement (0 = no limit check)
    pub spend_amount: i128,
    /// Asset to check spending limit against (only used when spend_amount > 0)
    pub spend_asset: Option<Address>,
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
