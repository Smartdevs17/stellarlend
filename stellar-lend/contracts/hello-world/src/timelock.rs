use soroban_sdk::{contracttype, Address, Bytes, Env, Symbol, Vec};

use crate::admin::require_admin;

/// Minimum timelock delay (24 hours in ledger timestamps)
const MIN_TIMELOCK_DELAY: u64 = 86400;
/// Maximum timelock delay (30 days)
const MAX_TIMELOCK_DELAY: u64 = 2592000;
/// Grace period after eta before a proposal expires (7 days)
const GRACE_PERIOD: u64 = 604800;

// ---------------------------------------------------------------------------
// Error / status types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum TimelockDataKey {
    Delay,
    Proposal(u64),
    ProposalCount,
    /// Tracks the last proposal-id that satisfied an `update_delay` execution,
    /// so it cannot be reused across multiple direct `update_delay` calls.
    LastDelayProposalId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TimelockError {
    Unauthorized,
    InvalidDelay,
    ProposalNotFound,
    ProposalNotReady,
    ProposalExpired,
    ProposalAlreadyExecuted,
    ProposalAlreadyCancelled,
    /// `initialize_timelock` was called after a delay was already configured.
    AlreadyInitialized,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalStatus {
    Pending,
    Executed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TimelockProposal {
    pub id: u64,
    pub proposer: Address,
    pub function_name: Symbol,
    pub call_data: Vec<Bytes>,
    pub eta: u64,
    pub created_at: u64,
    pub status: ProposalStatus,
    pub grace_period_end: u64,
}

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn is_initialized(env: &Env) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&TimelockDataKey::Delay)
        .is_some()
}

fn get_delay(env: &Env) -> Option<u64> {
    env.storage().persistent().get(&TimelockDataKey::Delay)
}

fn set_delay_storage(env: &Env, delay: u64) {
    env.storage()
        .persistent()
        .set(&TimelockDataKey::Delay, &delay);
}

fn get_proposal_count(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&TimelockDataKey::ProposalCount)
        .unwrap_or(0u64)
}

fn set_proposal_count(env: &Env, count: u64) {
    env.storage()
        .persistent()
        .set(&TimelockDataKey::ProposalCount, &count);
}

fn store_proposal(env: &Env, proposal: &TimelockProposal) {
    env.storage()
        .persistent()
        .set(&TimelockDataKey::Proposal(proposal.id), proposal);
}

fn load_proposal(env: &Env, proposal_id: u64) -> Option<TimelockProposal> {
    env.storage()
        .persistent()
        .get(&TimelockDataKey::Proposal(proposal_id))
}

/// Return the proposal-id of the most recently consumed `update_delay`
/// execution ticket, if any. The id is consumed at most once per call to
/// [`update_delay`].
fn get_last_delay_proposal_id(env: &Env) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&TimelockDataKey::LastDelayProposalId)
}

fn set_last_delay_proposal_id(env: &Env, id: u64) {
    env.storage()
        .persistent()
        .set(&TimelockDataKey::LastDelayProposalId, &id);
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

/// Topics emitted by all timelock state changes. The first topic is always a
/// stable module tag so off-chain indexers can subscribe to a single prefix.
mod topic {
    pub const MODULE: &str = "timelock";
    pub const INITIALIZED: &str = "initialized";
    pub const PROPOSED: &str = "proposed";
    pub const EXECUTED: &str = "executed";
    pub const CANCELLED: &str = "cancelled";
    pub const DELAY_UPDATED: &str = "delay_updated";
}

fn emit_initialized(env: &Env, delay: u64) {
    env.events().publish(
        (
            Symbol::new(env, topic::MODULE),
            Symbol::new(env, topic::INITIALIZED),
        ),
        delay,
    );
}

fn emit_proposed(env: &Env, id: u64, eta: u64, function_name: &Symbol) {
    env.events().publish(
        (
            Symbol::new(env, topic::MODULE),
            Symbol::new(env, topic::PROPOSED),
            function_name.clone(),
        ),
        (id, eta),
    );
}

fn emit_executed(env: &Env, id: u64) {
    env.events().publish(
        (
            Symbol::new(env, topic::MODULE),
            Symbol::new(env, topic::EXECUTED),
        ),
        id,
    );
}

fn emit_cancelled(env: &Env, id: u64) {
    env.events().publish(
        (
            Symbol::new(env, topic::MODULE),
            Symbol::new(env, topic::CANCELLED),
        ),
        id,
    );
}

fn emit_delay_updated(env: &Env, old_delay: u64, new_delay: u64, via_proposal_id: Option<u64>) {
    env.events().publish(
        (
            Symbol::new(env, topic::MODULE),
            Symbol::new(env, topic::DELAY_UPDATED),
        ),
        (old_delay, new_delay, via_proposal_id.unwrap_or(u64::MAX)),
    );
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialize the timelock with the given delay. The delay must be within
/// `[MIN_TIMELOCK_DELAY, MAX_TIMELOCK_DELAY]`. The caller must be the admin.
///
/// # Security
/// Can only be called **once**. A second call returns
/// [`TimelockError::AlreadyInitialized`] regardless of who calls it, preventing
/// a privileged account from silently resetting the proposal counter and
/// thereby voiding pending proposals.
pub fn initialize_timelock(env: &Env, admin: &Address, delay: u64) -> Result<(), TimelockError> {
    // Re-initialization guard: reject if a delay has ever been stored.
    if is_initialized(env) {
        return Err(TimelockError::AlreadyInitialized);
    }

    require_admin(env, admin).map_err(|_| TimelockError::Unauthorized)?;

    if delay < MIN_TIMELOCK_DELAY || delay > MAX_TIMELOCK_DELAY {
        return Err(TimelockError::InvalidDelay);
    }

    set_delay_storage(env, delay);
    set_proposal_count(env, 0);
    emit_initialized(env, delay);
    Ok(())
}

/// Create a new timelock proposal. Only the admin can propose.
/// Returns the new proposal id. Sets `eta = now + delay`.
pub fn propose(
    env: &Env,
    caller: &Address,
    function_name: Symbol,
    call_data: Vec<Bytes>,
) -> Result<u64, TimelockError> {
    require_admin(env, caller).map_err(|_| TimelockError::Unauthorized)?;

    let delay = get_delay(env).ok_or(TimelockError::InvalidDelay)?;
    let now = env.ledger().timestamp();
    let eta = now + delay;
    let grace_period_end = eta + GRACE_PERIOD;

    let id = get_proposal_count(env);

    let proposal = TimelockProposal {
        id,
        proposer: caller.clone(),
        function_name: function_name.clone(),
        call_data,
        eta,
        created_at: now,
        status: ProposalStatus::Pending,
        grace_period_end,
    };

    store_proposal(env, &proposal);
    set_proposal_count(env, id + 1);
    emit_proposed(env, id, eta, &function_name);

    Ok(id)
}

/// Execute a pending proposal. The current timestamp must be `>= eta` and
/// `< grace_period_end`. Only the admin can execute.
pub fn execute_proposal(
    env: &Env,
    caller: &Address,
    proposal_id: u64,
) -> Result<TimelockProposal, TimelockError> {
    require_admin(env, caller).map_err(|_| TimelockError::Unauthorized)?;

    let mut proposal = load_proposal(env, proposal_id).ok_or(TimelockError::ProposalNotFound)?;

    match proposal.status {
        ProposalStatus::Executed => return Err(TimelockError::ProposalAlreadyExecuted),
        ProposalStatus::Cancelled => return Err(TimelockError::ProposalAlreadyCancelled),
        ProposalStatus::Pending => {}
    }

    let now = env.ledger().timestamp();

    if now < proposal.eta {
        return Err(TimelockError::ProposalNotReady);
    }

    if now >= proposal.grace_period_end {
        return Err(TimelockError::ProposalExpired);
    }

    proposal.status = ProposalStatus::Executed;
    store_proposal(env, &proposal);
    emit_executed(env, proposal_id);

    Ok(proposal)
}

/// Cancel a pending proposal. Only the admin can cancel.
pub fn cancel_proposal(env: &Env, caller: &Address, proposal_id: u64) -> Result<(), TimelockError> {
    require_admin(env, caller).map_err(|_| TimelockError::Unauthorized)?;

    let mut proposal = load_proposal(env, proposal_id).ok_or(TimelockError::ProposalNotFound)?;

    match proposal.status {
        ProposalStatus::Executed => return Err(TimelockError::ProposalAlreadyExecuted),
        ProposalStatus::Cancelled => return Err(TimelockError::ProposalAlreadyCancelled),
        ProposalStatus::Pending => {}
    }

    proposal.status = ProposalStatus::Cancelled;
    store_proposal(env, &proposal);
    emit_cancelled(env, proposal_id);

    Ok(())
}

/// Read proposal details by id.
pub fn get_proposal(env: &Env, proposal_id: u64) -> Result<TimelockProposal, TimelockError> {
    load_proposal(env, proposal_id).ok_or(TimelockError::ProposalNotFound)
}

/// Update the timelock delay.
///
/// **Bootstrap path** (no delay configured yet): the admin can set the delay
/// directly. This is the one-time setup window before `initialize_timelock` is
/// called.
///
/// **Post-init path**: a matching `update_delay` proposal must have been
/// proposed, waited through its own delay, and executed **before** calling this
/// function. The proposal id is then recorded as consumed so the same executed
/// ticket cannot be replayed to perform a second, unsanctioned delay change.
///
/// # Security
/// Each executed `update_delay` proposal is tracked by id in
/// `LastDelayProposalId`. On the post-init path we walk proposals backwards to
/// find the most recent *un-consumed* executed `update_delay` ticket. If no
/// such ticket exists, the call is rejected.
pub fn update_delay(env: &Env, caller: &Address, new_delay: u64) -> Result<(), TimelockError> {
    require_admin(env, caller).map_err(|_| TimelockError::Unauthorized)?;

    if new_delay < MIN_TIMELOCK_DELAY || new_delay > MAX_TIMELOCK_DELAY {
        return Err(TimelockError::InvalidDelay);
    }

    let old_delay_opt = get_delay(env);

    if let Some(old_delay) = old_delay_opt {
        // Post-init: require an un-consumed executed `update_delay` proposal.
        let last_consumed = get_last_delay_proposal_id(env);
        let count = get_proposal_count(env);
        let mut matched_id: Option<u64> = None;

        // Walk backwards to find the most recent executed `update_delay`
        // proposal whose id has not been consumed yet.
        let mut i = count;
        while i > 0 {
            i -= 1;
            if let Some(ref consumed_id) = last_consumed {
                if i == *consumed_id {
                    // This ticket was already consumed; skip past it — do not
                    // let it satisfy a second update.
                    continue;
                }
            }
            if let Some(p) = load_proposal(env, i) {
                if p.status == ProposalStatus::Executed
                    && p.function_name == Symbol::new(env, "update_delay")
                {
                    let now = env.ledger().timestamp();
                    if now >= p.eta {
                        matched_id = Some(i);
                        break;
                    }
                }
            }
        }

        let ticket_id = matched_id.ok_or(TimelockError::ProposalNotReady)?;

        // Consume the ticket so it cannot satisfy a future call.
        set_last_delay_proposal_id(env, ticket_id);
        set_delay_storage(env, new_delay);
        emit_delay_updated(env, old_delay, new_delay, Some(ticket_id));
    } else {
        // Bootstrap: no delay set yet, allow direct update.
        set_delay_storage(env, new_delay);
        emit_delay_updated(env, 0, new_delay, None);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::set_admin;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Bytes, Env, Symbol, Vec};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Create an env with mock auth, generate an admin address, and register
    /// it in admin storage.
    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        set_admin(&env, admin.clone(), None).unwrap();
        (env, admin)
    }

    fn setup_initialized(delay: u64) -> (Env, Address) {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, delay).unwrap();
        (env, admin)
    }

    // -----------------------------------------------------------------------
    // initialize_timelock
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize_timelock_success() {
        let (env, admin) = setup();
        let delay = 86400u64;
        assert!(initialize_timelock(&env, &admin, delay).is_ok());
        assert_eq!(get_delay(&env), Some(delay));
        assert_eq!(get_proposal_count(&env), 0);
    }

    #[test]
    fn test_initialize_timelock_delay_too_short() {
        let (env, admin) = setup();
        assert_eq!(
            initialize_timelock(&env, &admin, 100),
            Err(TimelockError::InvalidDelay)
        );
    }

    #[test]
    fn test_initialize_timelock_delay_too_long() {
        let (env, admin) = setup();
        assert_eq!(
            initialize_timelock(&env, &admin, MAX_TIMELOCK_DELAY + 1),
            Err(TimelockError::InvalidDelay)
        );
    }

    #[test]
    fn test_initialize_timelock_unauthorized() {
        let (env, _admin) = setup();
        let rando = Address::generate(&env);
        // Delay check comes before admin check only if already initialized;
        // here no delay is set so admin check fires first.
        let result = initialize_timelock(&env, &rando, 86400);
        assert_eq!(result, Err(TimelockError::Unauthorized));
    }

    /// Re-initialization must be rejected even by the legitimate admin.
    #[test]
    fn test_initialize_timelock_already_initialized() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();
        // Second call — same admin, valid delay — must fail.
        let result = initialize_timelock(&env, &admin, 172800);
        assert_eq!(result, Err(TimelockError::AlreadyInitialized));
        // Original delay must be unchanged.
        assert_eq!(get_delay(&env), Some(86400));
    }

    /// A hostile non-admin cannot reset the timelock after initialization.
    #[test]
    fn test_initialize_timelock_hostile_reinit_rejected() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        // A fresh address pretending to be admin — the guard fires before the
        // admin check, so the error is AlreadyInitialized, not Unauthorized.
        let attacker = Address::generate(&env);
        let result = initialize_timelock(&env, &attacker, 86400);
        assert_eq!(result, Err(TimelockError::AlreadyInitialized));

        // Proposal counter and delay remain intact.
        assert_eq!(get_delay(&env), Some(86400));
        assert_eq!(get_proposal_count(&env), 0);
    }

    // -----------------------------------------------------------------------
    // propose
    // -----------------------------------------------------------------------

    #[test]
    fn test_propose_success() {
        let (env, admin) = setup_initialized(86400);

        let fn_name = Symbol::new(&env, "set_rate");
        let call_data: Vec<Bytes> = Vec::new(&env);

        let id = propose(&env, &admin, fn_name.clone(), call_data).unwrap();
        assert_eq!(id, 0);

        let p = get_proposal(&env, id).unwrap();
        assert_eq!(p.function_name, fn_name);
        assert_eq!(p.status, ProposalStatus::Pending);
        assert_eq!(p.eta, p.created_at + 86400);
        assert_eq!(p.grace_period_end, p.eta + GRACE_PERIOD);
    }

    #[test]
    fn test_propose_increments_id() {
        let (env, admin) = setup_initialized(86400);

        let fn_name = Symbol::new(&env, "action");
        let cd: Vec<Bytes> = Vec::new(&env);

        let id0 = propose(&env, &admin, fn_name.clone(), cd.clone()).unwrap();
        let id1 = propose(&env, &admin, fn_name.clone(), cd).unwrap();
        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
    }

    #[test]
    fn test_propose_unauthorized() {
        let (env, admin) = setup_initialized(86400);
        let rando = Address::generate(&env);
        let result = propose(&env, &rando, Symbol::new(&env, "x"), Vec::new(&env));
        assert_eq!(result, Err(TimelockError::Unauthorized));
    }

    // -----------------------------------------------------------------------
    // execute_proposal
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_proposal_success() {
        let (env, admin) = setup_initialized(86400);

        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();

        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });

        let executed = execute_proposal(&env, &admin, id).unwrap();
        assert_eq!(executed.status, ProposalStatus::Executed);

        let p = get_proposal(&env, id).unwrap();
        assert_eq!(p.status, ProposalStatus::Executed);
    }

    #[test]
    fn test_execute_proposal_too_early() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalNotReady));
    }

    #[test]
    fn test_execute_proposal_expired() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + GRACE_PERIOD + 1;
        });
        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalExpired));
    }

    #[test]
    fn test_execute_proposal_already_executed() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });
        execute_proposal(&env, &admin, id).unwrap();
        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalAlreadyExecuted));
    }

    #[test]
    fn test_execute_proposal_not_found() {
        let (env, admin) = setup_initialized(86400);
        let result = execute_proposal(&env, &admin, 999);
        assert_eq!(result, Err(TimelockError::ProposalNotFound));
    }

    #[test]
    fn test_execute_proposal_unauthorized() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });
        let rando = Address::generate(&env);
        let result = execute_proposal(&env, &rando, id);
        assert_eq!(result, Err(TimelockError::Unauthorized));
    }

    // -----------------------------------------------------------------------
    // cancel_proposal
    // -----------------------------------------------------------------------

    #[test]
    fn test_cancel_proposal_success() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        cancel_proposal(&env, &admin, id).unwrap();
        let p = get_proposal(&env, id).unwrap();
        assert_eq!(p.status, ProposalStatus::Cancelled);
    }

    #[test]
    fn test_cancel_proposal_already_cancelled() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        cancel_proposal(&env, &admin, id).unwrap();
        let result = cancel_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalAlreadyCancelled));
    }

    #[test]
    fn test_cancel_proposal_already_executed() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });
        execute_proposal(&env, &admin, id).unwrap();
        let result = cancel_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalAlreadyExecuted));
    }

    #[test]
    fn test_cancel_proposal_not_found() {
        let (env, admin) = setup_initialized(86400);
        let result = cancel_proposal(&env, &admin, 42);
        assert_eq!(result, Err(TimelockError::ProposalNotFound));
    }

    // -----------------------------------------------------------------------
    // get_proposal
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_proposal_not_found() {
        let (env, _admin) = setup();
        let result = get_proposal(&env, 0);
        assert_eq!(result, Err(TimelockError::ProposalNotFound));
    }

    // -----------------------------------------------------------------------
    // update_delay
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_delay_bootstrap() {
        // No delay set yet — admin can set directly.
        let (env, admin) = setup();
        let result = update_delay(&env, &admin, 172800);
        assert!(result.is_ok());
        assert_eq!(get_delay(&env), Some(172800));
    }

    #[test]
    fn test_update_delay_invalid() {
        let (env, admin) = setup();
        assert_eq!(
            update_delay(&env, &admin, 10),
            Err(TimelockError::InvalidDelay)
        );
        assert_eq!(
            update_delay(&env, &admin, MAX_TIMELOCK_DELAY + 1),
            Err(TimelockError::InvalidDelay)
        );
    }

    #[test]
    fn test_update_delay_requires_timelock_when_already_set() {
        let (env, admin) = setup_initialized(86400);
        // A direct update without a matching executed proposal must fail.
        let result = update_delay(&env, &admin, 172800);
        assert_eq!(result, Err(TimelockError::ProposalNotReady));
    }

    #[test]
    fn test_update_delay_through_timelock() {
        let (env, admin) = setup_initialized(86400);

        // 1. Propose a delay change.
        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "update_delay"),
            Vec::new(&env),
        )
        .unwrap();

        // 2. Wait for eta.
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });

        // 3. Execute the proposal.
        execute_proposal(&env, &admin, id).unwrap();

        // 4. Now update_delay should succeed.
        let result = update_delay(&env, &admin, 172800);
        assert!(result.is_ok());
        assert_eq!(get_delay(&env), Some(172800));
    }

    /// A second `update_delay` call with the **same** executed proposal must
    /// be rejected — the ticket is consumed on first use.
    #[test]
    fn test_update_delay_ticket_cannot_be_replayed() {
        let (env, admin) = setup_initialized(86400);

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "update_delay"),
            Vec::new(&env),
        )
        .unwrap();

        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });

        execute_proposal(&env, &admin, id).unwrap();

        // First use: succeeds.
        update_delay(&env, &admin, 172800).unwrap();

        // Second use with the same ticket: must fail.
        let result = update_delay(&env, &admin, 86400);
        assert_eq!(result, Err(TimelockError::ProposalNotReady));
    }

    /// A second ticket (a new executed proposal) allows a second delay update.
    #[test]
    fn test_update_delay_second_ticket_allowed() {
        let (env, admin) = setup_initialized(86400);

        // --- Ticket 1 ---
        let id0 = propose(
            &env,
            &admin,
            Symbol::new(&env, "update_delay"),
            Vec::new(&env),
        )
        .unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });
        execute_proposal(&env, &admin, id0).unwrap();
        update_delay(&env, &admin, 172800).unwrap();
        assert_eq!(get_delay(&env), Some(172800));

        // --- Ticket 2 ---
        let id1 = propose(
            &env,
            &admin,
            Symbol::new(&env, "update_delay"),
            Vec::new(&env),
        )
        .unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp += 172800 + 1;
        });
        execute_proposal(&env, &admin, id1).unwrap();
        update_delay(&env, &admin, 259200).unwrap();
        assert_eq!(get_delay(&env), Some(259200));
    }

    #[test]
    fn test_update_delay_unauthorized() {
        let (env, _admin) = setup();
        let rando = Address::generate(&env);
        let result = update_delay(&env, &rando, 86400);
        assert_eq!(result, Err(TimelockError::Unauthorized));
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_at_exact_eta() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        let p = get_proposal(&env, id).unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp = p.eta;
        });
        assert!(execute_proposal(&env, &admin, id).is_ok());
    }

    #[test]
    fn test_execute_at_exact_grace_period_end_is_expired() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        let p = get_proposal(&env, id).unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp = p.grace_period_end;
        });
        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalExpired));
    }

    #[test]
    fn test_cancel_does_not_affect_other_proposals() {
        let (env, admin) = setup_initialized(86400);
        let id0 = propose(&env, &admin, Symbol::new(&env, "a"), Vec::new(&env)).unwrap();
        let id1 = propose(&env, &admin, Symbol::new(&env, "b"), Vec::new(&env)).unwrap();
        cancel_proposal(&env, &admin, id0).unwrap();
        let p0 = get_proposal(&env, id0).unwrap();
        let p1 = get_proposal(&env, id1).unwrap();
        assert_eq!(p0.status, ProposalStatus::Cancelled);
        assert_eq!(p1.status, ProposalStatus::Pending);
    }

    /// Minimum-boundary delay must be accepted.
    #[test]
    fn test_initialize_at_min_delay() {
        let (env, admin) = setup();
        assert!(initialize_timelock(&env, &admin, MIN_TIMELOCK_DELAY).is_ok());
    }

    /// Maximum-boundary delay must be accepted.
    #[test]
    fn test_initialize_at_max_delay() {
        let (env, admin) = setup();
        assert!(initialize_timelock(&env, &admin, MAX_TIMELOCK_DELAY).is_ok());
    }

    /// Proposal id one above the last invalid delay is rejected.
    #[test]
    fn test_initialize_one_above_max_delay_rejected() {
        let (env, admin) = setup();
        assert_eq!(
            initialize_timelock(&env, &admin, MAX_TIMELOCK_DELAY + 1),
            Err(TimelockError::InvalidDelay)
        );
    }

    /// A cancelled proposal cannot be re-executed.
    #[test]
    fn test_execute_cancelled_proposal_rejected() {
        let (env, admin) = setup_initialized(86400);
        let id = propose(&env, &admin, Symbol::new(&env, "action"), Vec::new(&env)).unwrap();
        cancel_proposal(&env, &admin, id).unwrap();
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });
        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalAlreadyCancelled));
    }
}
