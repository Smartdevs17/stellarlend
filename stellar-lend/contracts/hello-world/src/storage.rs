use soroban_sdk::{contracttype, Address, Vec};

#[derive(Clone)]
#[contracttype]
pub enum GovernanceDataKey {
    Admin,
    Config,
    NextProposalId,
    MultisigConfig,
    MultisigAdmins,
    MultisigThreshold,
    GuardianConfig,
    Guardians,
    GuardianThreshold,

    Proposal(u64),
    Vote(u64, Address),
    ProposalApprovals(u64),
    UserProposals(Address, u64),

    RecoveryRequest,
    RecoveryApprovals,

    // Flash loan attack protection keys
    /// Snapshot of a voter's token balance at proposal creation time
    VotePowerSnapshot(u64, Address),
    /// Lock record preventing token transfers during active vote
    VoteLock(Address),
    /// Delegation record: delegator -> delegatee
    DelegationRecord(Address),
    /// Governance analytics for attack detection
    GovernanceAnalytics,
    /// Tracks how many proposals an address has created in the current window
    ProposalCreationCount(Address),
    /// Timestamp of the last proposal creation window start for an address
    ProposalWindowStart(Address),
}

#[derive(Clone)]
#[contracttype]
pub struct GuardianConfig {
    pub guardians: Vec<Address>,
    pub threshold: u32,
}
