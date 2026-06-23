use soroban_sdk::{Address, Env, Vec};
use crate::types::{DailySpendingLimit, DataKey, MultisigConfig, Proposal, AuditEntry, WalletError, WalletRole};

pub fn get_config(env: &Env) -> Result<MultisigConfig, WalletError> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(WalletError::NotInitialized)
}

pub fn set_config(env: &Env, config: &MultisigConfig) {
    env.storage().instance().set(&DataKey::Config, config);
}

pub fn get_admins(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Admins)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_admins(env: &Env, admins: &Vec<Address>) {
    env.storage().instance().set(&DataKey::Admins, admins);
}

pub fn get_next_proposal_id(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::NextProposalId)
        .unwrap_or(0)
}

pub fn increment_proposal_id(env: &Env) -> u64 {
    let id = get_next_proposal_id(env);
    env.storage().instance().set(&DataKey::NextProposalId, &(id + 1));
    id
}

pub fn get_proposal(env: &Env, id: u64) -> Option<Proposal> {
    env.storage().persistent().get(&DataKey::Proposal(id))
}

pub fn set_proposal(env: &Env, id: u64, proposal: &Proposal) {
    env.storage().persistent().set(&DataKey::Proposal(id), proposal);
}

pub fn get_approvals(env: &Env, id: u64) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Approvals(id))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_approvals(env: &Env, id: u64, approvals: &Vec<Address>) {
    env.storage().persistent().set(&DataKey::Approvals(id), approvals);
}

pub fn add_audit_entry(env: &Env, id: u64, entry: AuditEntry) {
    let mut trail: Vec<AuditEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::AuditTrail(id))
        .unwrap_or_else(|| Vec::new(env));
    trail.push_back(entry);
    env.storage().persistent().set(&DataKey::AuditTrail(id), &trail);
}

pub fn get_audit_trail(env: &Env, id: u64) -> Vec<AuditEntry> {
    env.storage()
        .persistent()
        .get(&DataKey::AuditTrail(id))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn get_guardians(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Guardians)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_guardians(env: &Env, guardians: &Vec<Address>) {
    env.storage().instance().set(&DataKey::Guardians, guardians);
}

pub fn get_guardian_threshold(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::GuardianThreshold).unwrap_or(0)
}

pub fn set_guardian_threshold(env: &Env, threshold: u32) {
    env.storage().instance().set(&DataKey::GuardianThreshold, &threshold);
}

pub fn get_recovery_request(env: &Env) -> Option<crate::types::RecoveryRequest> {
    env.storage().instance().get(&DataKey::RecoveryRequest)
}

pub fn set_recovery_request(env: &Env, request: Option<crate::types::RecoveryRequest>) {
    match request {
        Some(r) => env.storage().instance().set(&DataKey::RecoveryRequest, &r),
        None => env.storage().instance().remove(&DataKey::RecoveryRequest),
    }
}

pub fn get_last_activity(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::LastActivity).unwrap_or(0)
}

pub fn set_last_activity(env: &Env, timestamp: u64) {
    env.storage().instance().set(&DataKey::LastActivity, &timestamp);
}

pub fn get_pending_guardian_invites(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::PendingGuardianInvites)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_pending_guardian_invites(env: &Env, guardians: &Vec<Address>) {
    env.storage().instance().set(&DataKey::PendingGuardianInvites, guardians);
}

pub fn get_guardian_acceptance(env: &Env, guardian: &Address) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::GuardianAcceptances(guardian.clone()))
        .unwrap_or(false)
}

pub fn set_guardian_acceptance(env: &Env, guardian: Address, accepted: bool) {
    env.storage().instance().set(&DataKey::GuardianAcceptances(guardian), &accepted);
}

// ─── Role-Based Authorization ─────────────────────────────────────────────

/// Get the role assigned to an address. Returns `None` if address has no role.
pub fn get_role(env: &Env, addr: &Address) -> Option<WalletRole> {
    env.storage().instance().get(&DataKey::RoleAssignments(addr.clone()))
}

/// Set a role for an address.
pub fn set_role(env: &Env, addr: &Address, role: WalletRole) {
    env.storage().instance().set(&DataKey::RoleAssignments(addr.clone()), &role);
}

/// Remove a role assignment for an address.
pub fn remove_role(env: &Env, addr: &Address) {
    env.storage().instance().remove(&DataKey::RoleAssignments(addr.clone()));
}

/// Remove daily spending limit config for an asset.
pub fn remove_spending_limit_config(env: &Env, asset: &Address) {
    env.storage().instance().remove(&DataKey::SpendingLimitConfig(asset.clone()));
}

// ─── Daily Spending Limits ────────────────────────────────────────────────

const SECONDS_PER_DAY: u64 = 86400;

/// Get the day number for a given timestamp.
fn day_number(timestamp: u64) -> u64 {
    timestamp / SECONDS_PER_DAY
}

/// Get the configured daily spending limit for an asset.
pub fn get_spending_limit_config(env: &Env, asset: &Address) -> Option<DailySpendingLimit> {
    env.storage().instance().get(&DataKey::SpendingLimitConfig(asset.clone()))
}

/// Set the daily spending limit configuration for an asset.
pub fn set_spending_limit_config(env: &Env, asset: &Address, limit: &DailySpendingLimit) {
    env.storage().instance().set(&DataKey::SpendingLimitConfig(asset.clone()), limit);
}

/// Clear today's spending tracking for an asset (used on limit removal).
pub fn clear_daily_spend_tracking(env: &Env, asset: &Address) {
    let now = env.ledger().timestamp();
    let current_day = now / SECONDS_PER_DAY;
    env.storage().instance().remove(&DataKey::DailySpending(asset.clone(), current_day));
}

/// Check if spending `amount` for `asset` would exceed the daily limit.
/// If within limit, updates the spent-today tracking and returns Ok.
/// If over limit, returns Err(SpendingLimitExceeded).
pub fn check_and_record_daily_spend(
    env: &Env,
    asset: &Address,
    amount: i128,
) -> Result<(), WalletError> {
    if amount <= 0 {
        return Ok(()); // Nothing to check
    }

    let daily_limit = match get_spending_limit_config(env, asset) {
        Some(cfg) => cfg.daily_limit,
        None => {
            // No per-asset limit; check default from multisig config
            let config = get_config(env)?;
            config.default_daily_spend_limit
        }
    };

    if daily_limit == 0 {
        return Ok(()); // Unlimited
    }

    let now = env.ledger().timestamp();
    let current_day = now / SECONDS_PER_DAY;

    let spent_today: i128 = env
        .storage()
        .instance()
        .get(&DataKey::DailySpending(asset.clone(), current_day))
        .unwrap_or(0);

    let new_total = spent_today
        .checked_add(amount)
        .ok_or(WalletError::ExecutionFailed)?;

    if new_total > daily_limit {
        return Err(WalletError::SpendingLimitExceeded);
    }

    env.storage()
        .instance()
        .set(&DataKey::DailySpending(asset.clone(), current_day), &new_total);

    Ok(())
}
