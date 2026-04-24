use soroban_sdk::{contracttype, Address, Bytes, String, Symbol, Vec};

// ========================================================================
// Proposal Types
// ========================================================================

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum ProposalStatus {
    Pending,
    Active,
    Succeeded,
    Defeated,
    Expired,
    Queued,
    Executed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum VoteType {
    For,
    Against,
    Abstain,
}

/// Proposal type for protocol parameter changes
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum ProposalType {
    /// Change minimum collateral ratio
    MinCollateralRatio(i128),
    /// Change risk parameters (min_cr, liq_threshold, close_factor, liq_incentive)
    RiskParams(Option<i128>, Option<i128>, Option<i128>, Option<i128>),
    /// Pause/unpause operation
    PauseSwitch(Symbol, bool),
    /// Emergency pause
    EmergencyPause(bool),
    /// Generic action for future extensions
    GenericAction(Action),
    /// Change interest rate configuration
    InterestRateConfig(InterestRateParams),
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct InterestRateParams {
    pub base_rate_bps: Option<i128>,
    pub kink_utilization_bps: Option<i128>,
    pub multiplier_bps: Option<i128>,
    pub jump_multiplier_bps: Option<i128>,
    pub rate_floor_bps: Option<i128>,
    pub rate_ceiling_bps: Option<i128>,
    pub spread_bps: Option<i128>,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub proposal_type: ProposalType,
    pub description: String,
    pub status: ProposalStatus,
    pub start_time: u64,
    pub end_time: u64,
    pub execution_time: Option<u64>,
    pub voting_threshold: i128, // In basis points (e.g., 5000 = 50%)
    pub for_votes: i128,
    pub against_votes: i128,
    pub abstain_votes: i128,
    pub total_voting_power: i128,
    pub created_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct VoteInfo {
    pub voter: Address,
    pub proposal_id: u64,
    pub vote_type: VoteType,
    pub voting_power: i128,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ProposalOutcome {
    pub proposal_id: u64,
    pub succeeded: bool,
    pub for_votes: i128,
    pub against_votes: i128,
    pub abstain_votes: i128,
    pub quorum_reached: bool,
    pub quorum_required: i128,
}

/// Asset status for carbon credit or tokenized assets
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum AssetStatus {
    Issued,
    Listed,
    Retired,
    Invalidated,
}

// ========================================================================
// Governance Configuration
// ========================================================================

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct GovernanceConfig {
    pub voting_period: u64,             // Duration in seconds
    pub execution_delay: u64,           // Delay before execution
    pub quorum_bps: u32,                // Quorum in basis points
    pub proposal_threshold: i128,       // Min tokens to create proposal
    pub vote_token: Address,            // Token used for voting
    pub timelock_duration: u64,         // Max time before expiration
    pub default_voting_threshold: i128, // Default 50% in basis points
}

// ========================================================================
// Multisig Types
// ========================================================================

#[derive(Clone, Debug)]
#[contracttype]
pub struct MultisigConfig {
    pub admins: Vec<Address>,
    pub threshold: u32,
}

// ========================================================================
// Social Recovery Types
// ========================================================================

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct RecoveryRequest {
    pub old_admin: Address,
    pub new_admin: Address,
    pub initiator: Address,
    pub initiated_at: u64,
    pub expires_at: u64,
}

// ========================================================================
// Action Type (for generic execution)
// ========================================================================

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Action {
    pub target: Address,
    pub method: Symbol,
    pub args: Vec<Bytes>,
    pub value: i128,
}

// ========================================================================
// Flash Loan Attack Protection Types
// ========================================================================

/// Snapshot of a voter's token balance taken at proposal creation time.
/// Voting power is always derived from this snapshot, not the live balance,
/// preventing flash loan attacks where tokens are borrowed to inflate votes.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct VotePowerSnapshot {
    /// The proposal this snapshot belongs to
    pub proposal_id: u64,
    /// The voter whose balance was snapshotted
    pub voter: Address,
    /// Token balance at snapshot time (= voting power)
    pub balance: i128,
    /// Ledger timestamp when the snapshot was taken
    pub snapshot_time: u64,
}

/// Vote lock record: prevents a voter from transferring governance tokens
/// while they have an active vote in a live proposal.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct VoteLock {
    /// The locked voter
    pub voter: Address,
    /// Timestamp when the lock expires (= proposal end_time)
    pub locked_until: u64,
    /// Amount of tokens locked
    pub locked_amount: i128,
    /// The proposal that triggered this lock
    pub proposal_id: u64,
}

/// Delegation record: tracks who an address has delegated its vote to.
/// Delegation must be set at least `DELEGATION_DEADLINE` seconds before
/// a proposal is created to be valid for that proposal.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct DelegationRecord {
    /// The address that delegated its vote
    pub delegator: Address,
    /// The address receiving the delegated vote power
    pub delegatee: Address,
    /// When the delegation was established
    pub delegated_at: u64,
    /// Depth in the delegation chain (capped at MAX_DELEGATION_DEPTH)
    pub depth: u32,
}

/// Governance analytics for attack detection.
/// Tracks suspicious patterns such as sudden large vote swings.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct GovernanceAnalytics {
    /// Total proposals created
    pub total_proposals: u64,
    /// Total votes cast
    pub total_votes: u64,
    /// Number of proposals that triggered the suspicious-activity flag
    pub suspicious_proposals: u32,
    /// Timestamp of the last suspicious event
    pub last_suspicious_at: u64,
    /// Largest single-voter power seen (for anomaly detection)
    pub max_single_voter_power: i128,
}

// ========================================================================
// Constants
// ========================================================================

pub const BASIS_POINTS_SCALE: i128 = 10_000; // 100% = 10,000 basis points
pub const DEFAULT_VOTING_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days
pub const DEFAULT_EXECUTION_DELAY: u64 = 2 * 24 * 60 * 60; // 2 days
pub const DEFAULT_QUORUM_BPS: u32 = 4_000; // 40% default quorum
pub const DEFAULT_VOTING_THRESHOLD: i128 = 5_000; // 50% default threshold
pub const DEFAULT_TIMELOCK_DURATION: u64 = 7 * 24 * 60 * 60; // 7 days
pub const DEFAULT_RECOVERY_PERIOD: u64 = 3 * 24 * 60 * 60; // 3 days
pub const MIN_TIMELOCK_DELAY: u64 = 24 * 60 * 60; // 24 hours

// Flash loan attack protection constants
pub const VOTE_LOCK_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days - tokens locked during voting
pub const DELEGATION_DEADLINE: u64 = 24 * 60 * 60; // 24 hours before proposal submission
pub const MAX_DELEGATION_DEPTH: u32 = 3; // Prevent delegation chains
pub const PROPOSAL_RATE_LIMIT: u32 = 5; // Max proposals per address per window
pub const PROPOSAL_RATE_WINDOW: u64 = 24 * 60 * 60; // 24 hour window for rate limiting
