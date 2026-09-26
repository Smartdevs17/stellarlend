//! # Oracle Upgrade Mechanism with Time Delay
//!
//! Implements a time-delayed oracle upgrade mechanism that prevents immediate
//! oracle swaps, giving users time to react before a new price source takes effect.
//!
//! ## Flow
//! 1. Admin calls `propose_oracle_upgrade` — stores a pending upgrade with an ETA.
//! 2. After the delay passes, admin calls `execute_oracle_upgrade` to apply it.
//! 3. Admin may call `cancel_oracle_upgrade` at any time to abort.
//!
//! ## Security
//! - Only admin can propose, execute, or cancel.
//! - Minimum delay: 1 hour (configurable, max 7 days).
//! - Executing before the ETA is rejected.
//! - An expired proposal (past grace period) cannot be executed.

use soroban_sdk::{contracterror, contracttype, Address, Env};

use crate::admin::require_admin;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum upgrade delay: 1 hour
pub const MIN_ORACLE_UPGRADE_DELAY: u64 = 3_600;
/// Maximum upgrade delay: 7 days
pub const MAX_ORACLE_UPGRADE_DELAY: u64 = 604_800;
/// Grace period after ETA during which the upgrade can still execute: 2 days
pub const ORACLE_UPGRADE_GRACE_PERIOD: u64 = 172_800;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleUpgradeError {
    /// Caller is not the protocol admin
    Unauthorized = 1,
    /// Delay is outside the allowed range
    InvalidDelay = 2,
    /// No pending upgrade found
    NoPendingUpgrade = 3,
    /// Upgrade delay has not yet elapsed
    UpgradeNotReady = 4,
    /// Upgrade window has expired (past ETA + grace period)
    UpgradeExpired = 5,
    /// An upgrade is already pending; cancel it first
    UpgradeAlreadyPending = 6,
}

// ─── Types ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum OracleUpgradeStatus {
    Pending,
    Executed,
    Cancelled,
}

/// A pending oracle upgrade proposal
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleUpgradeProposal {
    /// Asset whose oracle is being replaced
    pub asset: Address,
    /// The new oracle address to apply
    pub new_oracle: Address,
    /// The proposing admin
    pub proposer: Address,
    /// Earliest timestamp at which the upgrade may execute
    pub eta: u64,
    /// When this proposal was created
    pub created_at: u64,
    /// Deadline after which the proposal expires
    pub expires_at: u64,
    /// Current status
    pub status: OracleUpgradeStatus,
}

// ─── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum OracleUpgradeKey {
    /// Pending proposal for a given asset
    Proposal(Address),
    /// Configured upgrade delay (seconds)
    Delay,
}

// ─── Storage helpers ─────────────────────────────────────────────────────────

fn get_delay(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get::<OracleUpgradeKey, u64>(&OracleUpgradeKey::Delay)
        .unwrap_or(MIN_ORACLE_UPGRADE_DELAY)
}

fn set_delay_storage(env: &Env, delay: u64) {
    env.storage()
        .persistent()
        .set(&OracleUpgradeKey::Delay, &delay);
}

fn get_proposal(env: &Env, asset: &Address) -> Option<OracleUpgradeProposal> {
    env.storage()
        .persistent()
        .get::<OracleUpgradeKey, OracleUpgradeProposal>(&OracleUpgradeKey::Proposal(asset.clone()))
}

fn set_proposal(env: &Env, proposal: &OracleUpgradeProposal) {
    env.storage()
        .persistent()
        .set(&OracleUpgradeKey::Proposal(proposal.asset.clone()), proposal);
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Set the oracle upgrade delay (admin-only).
///
/// Must be within `[MIN_ORACLE_UPGRADE_DELAY, MAX_ORACLE_UPGRADE_DELAY]`.
pub fn set_oracle_upgrade_delay(
    env: &Env,
    caller: Address,
    delay: u64,
) -> Result<(), OracleUpgradeError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| OracleUpgradeError::Unauthorized)?;

    if delay < MIN_ORACLE_UPGRADE_DELAY || delay > MAX_ORACLE_UPGRADE_DELAY {
        return Err(OracleUpgradeError::InvalidDelay);
    }
    set_delay_storage(env, delay);
    Ok(())
}

/// Returns the configured upgrade delay in seconds.
pub fn get_oracle_upgrade_delay(env: &Env) -> u64 {
    get_delay(env)
}

/// Propose a time-delayed oracle upgrade for `asset` to `new_oracle`.
///
/// Only one pending proposal per asset is allowed at a time.
pub fn propose_oracle_upgrade(
    env: &Env,
    caller: Address,
    asset: Address,
    new_oracle: Address,
) -> Result<OracleUpgradeProposal, OracleUpgradeError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| OracleUpgradeError::Unauthorized)?;

    // Reject if there is already a pending proposal for this asset
    if let Some(existing) = get_proposal(env, &asset) {
        if existing.status == OracleUpgradeStatus::Pending {
            return Err(OracleUpgradeError::UpgradeAlreadyPending);
        }
    }

    let delay = get_delay(env);
    let now = env.ledger().timestamp();
    let eta = now.saturating_add(delay);
    let expires_at = eta.saturating_add(ORACLE_UPGRADE_GRACE_PERIOD);

    let proposal = OracleUpgradeProposal {
        asset: asset.clone(),
        new_oracle: new_oracle.clone(),
        proposer: caller.clone(),
        eta,
        created_at: now,
        expires_at,
        status: OracleUpgradeStatus::Pending,
    };

    set_proposal(env, &proposal);

    env.events().publish(
        (
            soroban_sdk::Symbol::new(env, "oracle_upgrade_proposed"),
            asset,
        ),
        (new_oracle, eta),
    );

    Ok(proposal)
}

/// Execute a previously proposed oracle upgrade for `asset`.
///
/// Fails if no pending proposal exists, if the ETA has not passed, or if
/// the proposal has expired.
pub fn execute_oracle_upgrade(
    env: &Env,
    caller: Address,
    asset: Address,
) -> Result<Address, OracleUpgradeError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| OracleUpgradeError::Unauthorized)?;

    let mut proposal = get_proposal(env, &asset).ok_or(OracleUpgradeError::NoPendingUpgrade)?;

    if proposal.status != OracleUpgradeStatus::Pending {
        return Err(OracleUpgradeError::NoPendingUpgrade);
    }

    let now = env.ledger().timestamp();

    if now < proposal.eta {
        return Err(OracleUpgradeError::UpgradeNotReady);
    }
    if now > proposal.expires_at {
        return Err(OracleUpgradeError::UpgradeExpired);
    }

    // Apply the oracle upgrade: emit the event and mark the proposal as executed.
    // The protocol integration layer (governance executor or off-chain indexer)
    // reads this event and calls set_primary_oracle on the oracle module.
    proposal.status = OracleUpgradeStatus::Executed;
    set_proposal(env, &proposal);

    env.events().publish(
        (
            soroban_sdk::Symbol::new(env, "oracle_upgrade_executed"),
            asset,
        ),
        proposal.new_oracle.clone(),
    );

    Ok(proposal.new_oracle)
}

/// Cancel a pending oracle upgrade for `asset`.
pub fn cancel_oracle_upgrade(
    env: &Env,
    caller: Address,
    asset: Address,
) -> Result<(), OracleUpgradeError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| OracleUpgradeError::Unauthorized)?;

    let mut proposal = get_proposal(env, &asset).ok_or(OracleUpgradeError::NoPendingUpgrade)?;

    if proposal.status != OracleUpgradeStatus::Pending {
        return Err(OracleUpgradeError::NoPendingUpgrade);
    }

    proposal.status = OracleUpgradeStatus::Cancelled;
    set_proposal(env, &proposal);

    env.events().publish(
        (
            soroban_sdk::Symbol::new(env, "oracle_upgrade_cancelled"),
            asset,
        ),
        caller,
    );

    Ok(())
}

/// Return the pending upgrade proposal for `asset`, if any.
pub fn get_oracle_upgrade_proposal(
    env: &Env,
    asset: &Address,
) -> Option<OracleUpgradeProposal> {
    get_proposal(env, asset)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deposit::DepositDataKey;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage()
            .persistent()
            .set(&DepositDataKey::Admin, &admin);
        (env, admin)
    }

    #[test]
    fn test_propose_and_execute_oracle_upgrade() {
        let (env, admin) = setup();
        let asset = Address::generate(&env);
        let new_oracle = Address::generate(&env);

        // Set a delay of 2 hours
        set_oracle_upgrade_delay(&env, admin.clone(), 7200).unwrap();
        assert_eq!(get_oracle_upgrade_delay(&env), 7200);

        // Propose
        let proposal =
            propose_oracle_upgrade(&env, admin.clone(), asset.clone(), new_oracle.clone()).unwrap();
        assert_eq!(proposal.status, OracleUpgradeStatus::Pending);
        assert_eq!(proposal.new_oracle, new_oracle);

        // Trying to execute immediately should fail
        let result = execute_oracle_upgrade(&env, admin.clone(), asset.clone());
        assert_eq!(result, Err(OracleUpgradeError::UpgradeNotReady));

        // Advance ledger past ETA
        env.ledger().with_mut(|li| li.timestamp += 7201);

        // Now execution should succeed
        let applied = execute_oracle_upgrade(&env, admin.clone(), asset.clone()).unwrap();
        assert_eq!(applied, new_oracle);

        // Proposal is now executed — cannot execute again
        let result2 = execute_oracle_upgrade(&env, admin.clone(), asset.clone());
        assert_eq!(result2, Err(OracleUpgradeError::NoPendingUpgrade));
    }

    #[test]
    fn test_cancel_oracle_upgrade() {
        let (env, admin) = setup();
        let asset = Address::generate(&env);
        let new_oracle = Address::generate(&env);

        propose_oracle_upgrade(&env, admin.clone(), asset.clone(), new_oracle.clone()).unwrap();
        cancel_oracle_upgrade(&env, admin.clone(), asset.clone()).unwrap();

        let proposal = get_oracle_upgrade_proposal(&env, &asset).unwrap();
        assert_eq!(proposal.status, OracleUpgradeStatus::Cancelled);

        // Cancelled proposal cannot be executed
        let result = execute_oracle_upgrade(&env, admin.clone(), asset.clone());
        assert_eq!(result, Err(OracleUpgradeError::NoPendingUpgrade));
    }

    #[test]
    fn test_cannot_propose_while_one_is_pending() {
        let (env, admin) = setup();
        let asset = Address::generate(&env);
        let oracle_a = Address::generate(&env);
        let oracle_b = Address::generate(&env);

        propose_oracle_upgrade(&env, admin.clone(), asset.clone(), oracle_a.clone()).unwrap();

        let result =
            propose_oracle_upgrade(&env, admin.clone(), asset.clone(), oracle_b.clone());
        assert_eq!(result, Err(OracleUpgradeError::UpgradeAlreadyPending));
    }

    #[test]
    fn test_upgrade_expires_after_grace_period() {
        let (env, admin) = setup();
        let asset = Address::generate(&env);
        let new_oracle = Address::generate(&env);

        propose_oracle_upgrade(&env, admin.clone(), asset.clone(), new_oracle.clone()).unwrap();

        // Advance past ETA + grace period
        env.ledger().with_mut(|li| {
            li.timestamp += MIN_ORACLE_UPGRADE_DELAY + ORACLE_UPGRADE_GRACE_PERIOD + 1
        });

        let result = execute_oracle_upgrade(&env, admin.clone(), asset.clone());
        assert_eq!(result, Err(OracleUpgradeError::UpgradeExpired));
    }

    #[test]
    fn test_unauthorized_propose_rejected() {
        let (env, _admin) = setup();
        let stranger = Address::generate(&env);
        let asset = Address::generate(&env);
        let new_oracle = Address::generate(&env);

        let result = propose_oracle_upgrade(&env, stranger, asset, new_oracle);
        assert_eq!(result, Err(OracleUpgradeError::Unauthorized));
    }

    #[test]
    fn test_invalid_delay_rejected() {
        let (env, admin) = setup();

        // Too short
        let result = set_oracle_upgrade_delay(&env, admin.clone(), 100);
        assert_eq!(result, Err(OracleUpgradeError::InvalidDelay));

        // Too long
        let result = set_oracle_upgrade_delay(&env, admin.clone(), MAX_ORACLE_UPGRADE_DELAY + 1);
        assert_eq!(result, Err(OracleUpgradeError::InvalidDelay));
    }

    #[test]
    fn test_get_upgrade_proposal_returns_none_when_not_set() {
        let (env, _admin) = setup();
        let asset = Address::generate(&env);
        assert!(get_oracle_upgrade_proposal(&env, &asset).is_none());
    }
}
