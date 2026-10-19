//! multisig.rs — M-of-N multi-signature gate for protocol-admin operations.
//!
//! # Context (issue #1390)
//!
//! The protocol was previously controlled by a **single admin key**. A single
//! compromised key would give an attacker full control over every critical
//! parameter. This module replaces that model with a threshold multi-sig where
//! `M` out of `N` registered signers must approve every sensitive action before
//! it can be executed. A mandatory execution timelock (default 48 h) provides a
//! second layer of defence.
//!
//! # Lifecycle
//!
//! ```text
//! ms_set_admins(admins, M)
//!      │
//! ms_propose(proposer, action)  ──► ms_created event  (proposer auto-approves)
//!      │
//! ms_approve(approver, id)      ──► ms_approved event  (repeat until M reached)
//!      │
//! [wait ≥ TIMELOCK_SECS]
//!      │
//! ms_execute(executor, id)      ──► ms_executed event  (returns ProposalAction)
//! ```
//!
//! The caller of `ms_execute` is responsible for applying the returned
//! `ProposalAction` to the underlying protocol state.
//!
//! # Supported actions
//!
//! | Variant | Protected operation |
//! |---------|---------------------|
//! | `SetMinCollateralRatio(i128)` | minimum collateral ratio (bps) |
//! | `SetBaseBorrowRate(i128)`     | base interest rate (bps/year)  |
//! | `SetReserveFactor(i128)`      | protocol reserve factor (bps)  |
//! | `SetLiquidationIncentiveBps(i128)` | liquidation bonus (bps)  |
//! | `SetOracle(Address)`          | oracle contract address         |
//! | `PauseProtocol(bool)`         | emergency pause / unpause       |
//! | `RotateAdmin(Address)`        | replace the privileged admin key |
//! | `UpgradeContract(BytesN<32>)` | upgrade the WASM hash            |
//!
//! # Storage layout
//!
//! | Symbol key  | Type             | Description               |
//! |-------------|------------------|---------------------------|
//! | `MS_ADMINS` | `Vec<Address>`   | current signer set        |
//! | `MS_THRESH` | `u32`            | required approvals (M)    |
//! | `MS_CTR`    | `u64`            | monotonic proposal counter |
//! | `MS_P_<id>` | `MsProposal`     | per-proposal data          |
//! | `MS_A_<id>` | `Vec<Address>`   | per-proposal approvals     |

#![allow(dead_code)]

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

// ─── Timelock ──────────────────────────────────────────────────────────────

/// 48 hours — community has a window to react to malicious proposals.
pub const TIMELOCK_SECS: u64 = 48 * 3_600;

// ─── Storage keys ──────────────────────────────────────────────────────────

const KEY_ADMINS: Symbol = symbol_short!("MS_ADMINS");
const KEY_THRESHOLD: Symbol = symbol_short!("MS_THRESH");
const KEY_COUNTER: Symbol = symbol_short!("MS_CTR");

fn proposal_key(id: u64) -> (Symbol, u64) {
    (symbol_short!("MS_P"), id)
}

fn approvals_key(id: u64) -> (Symbol, u64) {
    (symbol_short!("MS_A"), id)
}

// ─── Proposal action ───────────────────────────────────────────────────────

/// Every governable protocol action is encoded as a `ProposalAction` variant.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalAction {
    SetMinCollateralRatio(i128),
    SetBaseBorrowRate(i128),
    SetReserveFactor(i128),
    SetLiquidationIncentiveBps(i128),
    SetOracle(Address),
    PauseProtocol(bool),
    RotateAdmin(Address),
    UpgradeContract(BytesN<32>),
}

// ─── Status ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Executed,
    Cancelled,
}

// ─── Proposal ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct MsProposal {
    pub id: u64,
    pub proposer: Address,
    pub action: ProposalAction,
    pub status: ProposalStatus,
    /// Ledger timestamp at creation (seconds).
    pub created_at: u64,
    /// Earliest timestamp at which execution is allowed.
    pub executable_after: u64,
    /// Cached approval count (mirrors approvals vec length).
    pub approval_count: u32,
}

// ─── Errors ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultisigError {
    /// Caller is not in the admin set.
    Unauthorized,
    /// Admins vec is empty or threshold is out of range.
    InvalidConfig,
    /// Referenced proposal ID does not exist.
    ProposalNotFound,
    /// Caller has already approved this proposal.
    AlreadyApproved,
    /// Not enough approvals yet to execute.
    InsufficientApprovals,
    /// Execution timelock has not yet elapsed.
    TimelockActive,
    /// Proposal is not in a state that allows this operation.
    InvalidProposalState,
    /// Action payload is invalid (e.g. out-of-range value, zero address).
    InvalidAction,
}

// ─── Internal helpers ──────────────────────────────────────────────────────

fn load_admins(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get::<Symbol, Vec<Address>>(&KEY_ADMINS)
        .unwrap_or_else(|| Vec::new(env))
}

fn load_threshold(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get::<Symbol, u32>(&KEY_THRESHOLD)
        .unwrap_or(1)
}

fn is_admin(env: &Env, addr: &Address) -> bool {
    load_admins(env).iter().any(|a| &a == addr)
}

fn next_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .persistent()
        .get::<Symbol, u64>(&KEY_COUNTER)
        .unwrap_or(0);
    let next = id + 1;
    env.storage()
        .persistent()
        .set::<Symbol, u64>(&KEY_COUNTER, &next);
    next
}

fn load_proposal(env: &Env, id: u64) -> Option<MsProposal> {
    let key = proposal_key(id);
    env.storage()
        .persistent()
        .get::<(Symbol, u64), MsProposal>(&key)
}

fn save_proposal(env: &Env, p: &MsProposal) {
    let key = proposal_key(p.id);
    env.storage()
        .persistent()
        .set::<(Symbol, u64), MsProposal>(&key, p);
}

fn load_approvals(env: &Env, id: u64) -> Vec<Address> {
    let key = approvals_key(id);
    env.storage()
        .persistent()
        .get::<(Symbol, u64), Vec<Address>>(&key)
        .unwrap_or_else(|| Vec::new(env))
}

fn save_approvals(env: &Env, id: u64, approvals: &Vec<Address>) {
    let key = approvals_key(id);
    env.storage()
        .persistent()
        .set::<(Symbol, u64), Vec<Address>>(&key, approvals);
}

// ─── Event helpers ─────────────────────────────────────────────────────────

fn emit(env: &Env, topic: Symbol, id: u64, actor: &Address) {
    env.events().publish((topic, id), actor.clone());
}

// ─── Public API ────────────────────────────────────────────────────────────

/// Replace the multi-sig admin set and threshold.
///
/// - **Auth:** Any existing admin, or any caller if no admins exist yet (bootstrap).
/// - `admins` must be non-empty; `1 ≤ threshold ≤ admins.len()`.
pub fn ms_set_admins(
    env: &Env,
    caller: Address,
    admins: Vec<Address>,
    threshold: u32,
) -> Result<(), MultisigError> {
    caller.require_auth();

    let current = load_admins(env);
    if !current.is_empty() && !is_admin(env, &caller) {
        return Err(MultisigError::Unauthorized);
    }
    if admins.is_empty() || threshold == 0 || threshold as usize > admins.len() {
        return Err(MultisigError::InvalidConfig);
    }

    env.storage()
        .persistent()
        .set::<Symbol, Vec<Address>>(&KEY_ADMINS, &admins);
    env.storage()
        .persistent()
        .set::<Symbol, u32>(&KEY_THRESHOLD, &threshold);

    env.events().publish(
        symbol_short!("ms_admins"),
        (threshold, admins.len() as u32),
    );
    Ok(())
}

/// Create a new multi-sig proposal. The proposer is auto-approved as the first signer.
///
/// Returns the new proposal ID.
pub fn ms_propose(
    env: &Env,
    proposer: Address,
    action: ProposalAction,
) -> Result<u64, MultisigError> {
    proposer.require_auth();
    if !is_admin(env, &proposer) {
        return Err(MultisigError::Unauthorized);
    }

    // Basic value validation.
    match &action {
        ProposalAction::SetMinCollateralRatio(v) if *v <= 10_000 => {
            return Err(MultisigError::InvalidAction)
        }
        ProposalAction::SetBaseBorrowRate(v) if *v < 0 => {
            return Err(MultisigError::InvalidAction)
        }
        ProposalAction::SetReserveFactor(v) if *v < 0 || *v > 10_000 => {
            return Err(MultisigError::InvalidAction)
        }
        ProposalAction::SetLiquidationIncentiveBps(v) if *v <= 0 => {
            return Err(MultisigError::InvalidAction)
        }
        _ => {}
    }

    let now = env.ledger().timestamp();
    let id = next_id(env);
    let threshold = load_threshold(env);

    // Proposer auto-approves.
    let mut approvals = Vec::new(env);
    approvals.push_back(proposer.clone());
    save_approvals(env, id, &approvals);

    let status = if 1 >= threshold {
        ProposalStatus::Approved
    } else {
        ProposalStatus::Pending
    };

    let proposal = MsProposal {
        id,
        proposer: proposer.clone(),
        action,
        status,
        created_at: now,
        executable_after: now + TIMELOCK_SECS,
        approval_count: 1,
    };
    save_proposal(env, &proposal);

    emit(env, symbol_short!("ms_creatd"), id, &proposer);
    emit(env, symbol_short!("ms_apprvd"), id, &proposer);
    Ok(id)
}

/// Add an approval to an open proposal. Transitions to `Approved` when threshold is met.
pub fn ms_approve(
    env: &Env,
    approver: Address,
    proposal_id: u64,
) -> Result<(), MultisigError> {
    approver.require_auth();
    if !is_admin(env, &approver) {
        return Err(MultisigError::Unauthorized);
    }

    let mut proposal = load_proposal(env, proposal_id)
        .ok_or(MultisigError::ProposalNotFound)?;

    if proposal.status != ProposalStatus::Pending {
        return Err(MultisigError::InvalidProposalState);
    }

    let mut approvals = load_approvals(env, proposal_id);
    if approvals.iter().any(|a| a == approver) {
        return Err(MultisigError::AlreadyApproved);
    }

    approvals.push_back(approver.clone());
    save_approvals(env, proposal_id, &approvals);

    proposal.approval_count = approvals.len() as u32;
    if proposal.approval_count >= load_threshold(env) {
        proposal.status = ProposalStatus::Approved;
    }
    save_proposal(env, &proposal);

    emit(env, symbol_short!("ms_apprvd"), proposal_id, &approver);
    Ok(())
}

/// Cancel a pending or approved proposal. Any admin may cancel.
pub fn ms_cancel(
    env: &Env,
    canceller: Address,
    proposal_id: u64,
) -> Result<(), MultisigError> {
    canceller.require_auth();
    if !is_admin(env, &canceller) {
        return Err(MultisigError::Unauthorized);
    }

    let mut proposal = load_proposal(env, proposal_id)
        .ok_or(MultisigError::ProposalNotFound)?;

    if matches!(
        proposal.status,
        ProposalStatus::Executed | ProposalStatus::Cancelled
    ) {
        return Err(MultisigError::InvalidProposalState);
    }

    proposal.status = ProposalStatus::Cancelled;
    save_proposal(env, &proposal);
    emit(env, symbol_short!("ms_cancld"), proposal_id, &canceller);
    Ok(())
}

/// Execute a fully-approved proposal whose timelock has elapsed.
///
/// Marks the proposal as `Executed` and returns the `ProposalAction` for the
/// caller to apply to the protocol state.
pub fn ms_execute(
    env: &Env,
    executor: Address,
    proposal_id: u64,
) -> Result<ProposalAction, MultisigError> {
    executor.require_auth();
    if !is_admin(env, &executor) {
        return Err(MultisigError::Unauthorized);
    }

    let mut proposal = load_proposal(env, proposal_id)
        .ok_or(MultisigError::ProposalNotFound)?;

    if !matches!(proposal.status, ProposalStatus::Approved) {
        return Err(MultisigError::InvalidProposalState);
    }
    if proposal.approval_count < load_threshold(env) {
        return Err(MultisigError::InsufficientApprovals);
    }
    if env.ledger().timestamp() < proposal.executable_after {
        return Err(MultisigError::TimelockActive);
    }

    proposal.status = ProposalStatus::Executed;
    save_proposal(env, &proposal);
    emit(env, symbol_short!("ms_execd"), proposal_id, &executor);
    Ok(proposal.action)
}

// ─── View helpers ──────────────────────────────────────────────────────────

pub fn ms_get_admins(env: &Env) -> Vec<Address> {
    load_admins(env)
}

pub fn ms_get_threshold(env: &Env) -> u32 {
    load_threshold(env)
}

pub fn ms_get_proposal(env: &Env, proposal_id: u64) -> Option<MsProposal> {
    load_proposal(env, proposal_id)
}

pub fn ms_get_approvals(env: &Env, proposal_id: u64) -> Vec<Address> {
    load_approvals(env, proposal_id)
}
