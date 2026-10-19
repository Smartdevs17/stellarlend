use soroban_sdk::{contracttype, Address, Bytes, Env, Symbol, Vec};

use crate::admin::require_admin;

/// Minimum timelock delay (24 hours in ledger timestamps)
const MIN_TIMELOCK_DELAY: u64 = 86400;
/// Maximum timelock delay (30 days)
const MAX_TIMELOCK_DELAY: u64 = 2592000;
/// Grace period after eta before a proposal expires (7 days)
const GRACE_PERIOD: u64 = 604800;

#[contracttype]
#[derive(Clone)]
pub enum TimelockDataKey {
    Delay,
    Proposal(u64),
    ProposalCount,
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
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalStatus {
    Pending,
    Executed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug)]
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

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialize the timelock with the given delay. The delay must be within
/// `[MIN_TIMELOCK_DELAY, MAX_TIMELOCK_DELAY]`. The caller must be the admin.
pub fn initialize_timelock(
    env: &Env,
    admin: &Address,
    delay: u64,
) -> Result<(), TimelockError> {
    require_admin(env, admin).map_err(|_| TimelockError::Unauthorized)?;

    if delay < MIN_TIMELOCK_DELAY || delay > MAX_TIMELOCK_DELAY {
        return Err(TimelockError::InvalidDelay);
    }

    set_delay_storage(env, delay);
    set_proposal_count(env, 0);
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
        function_name,
        call_data,
        eta,
        created_at: now,
        status: ProposalStatus::Pending,
        grace_period_end,
    };

    store_proposal(env, &proposal);
    set_proposal_count(env, id + 1);

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

    let mut proposal =
        load_proposal(env, proposal_id).ok_or(TimelockError::ProposalNotFound)?;

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

    Ok(proposal)
}

/// Cancel a pending proposal. Only the admin can cancel.
pub fn cancel_proposal(
    env: &Env,
    caller: &Address,
    proposal_id: u64,
) -> Result<(), TimelockError> {
    require_admin(env, caller).map_err(|_| TimelockError::Unauthorized)?;

    let mut proposal =
        load_proposal(env, proposal_id).ok_or(TimelockError::ProposalNotFound)?;

    match proposal.status {
        ProposalStatus::Executed => return Err(TimelockError::ProposalAlreadyExecuted),
        ProposalStatus::Cancelled => return Err(TimelockError::ProposalAlreadyCancelled),
        ProposalStatus::Pending => {}
    }

    proposal.status = ProposalStatus::Cancelled;
    store_proposal(env, &proposal);

    Ok(())
}

/// Read proposal details by id.
pub fn get_proposal(
    env: &Env,
    proposal_id: u64,
) -> Result<TimelockProposal, TimelockError> {
    load_proposal(env, proposal_id).ok_or(TimelockError::ProposalNotFound)
}

/// Update the timelock delay. If a delay is already configured this change
/// must itself go through the timelock (the caller is expected to have
/// previously proposed and executed a delay-change proposal). For bootstrap
/// when no delay is set yet, the admin can set it directly.
pub fn update_delay(
    env: &Env,
    caller: &Address,
    new_delay: u64,
) -> Result<(), TimelockError> {
    require_admin(env, caller).map_err(|_| TimelockError::Unauthorized)?;

    if new_delay < MIN_TIMELOCK_DELAY || new_delay > MAX_TIMELOCK_DELAY {
        return Err(TimelockError::InvalidDelay);
    }

    // If no delay is set yet (bootstrap), allow direct update.
    // Otherwise the caller must go through the timelock: they should call
    // `propose` with function_name = "update_delay", wait for eta, call
    // `execute_proposal`, and then invoke this function. We enforce the
    // timelock by checking that a matching executed proposal exists whose
    // eta has passed.
    if let Some(_existing_delay) = get_delay(env) {
        // Non-bootstrap path: require an executed proposal whose
        // function_name is "update_delay" and whose eta has passed.
        let count = get_proposal_count(env);
        let mut found = false;
        // Walk backwards to find the most recent matching executed proposal.
        let mut i = count;
        while i > 0 {
            i -= 1;
            if let Some(p) = load_proposal(env, i) {
                if p.status == ProposalStatus::Executed
                    && p.function_name == Symbol::new(env, "update_delay")
                {
                    let now = env.ledger().timestamp();
                    if now >= p.eta {
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found {
            return Err(TimelockError::ProposalNotReady);
        }
    }

    set_delay_storage(env, new_delay);
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

    /// Helper: create an env with mock auth, generate an admin address, and
    /// register it in admin storage.
    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        // Bootstrap admin directly in storage (no existing admin).
        set_admin(&env, admin.clone(), None).unwrap();
        (env, admin)
    }

    // -----------------------------------------------------------------------
    // initialize_timelock
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize_timelock_success() {
        let (env, admin) = setup();
        let delay = 86400u64; // 24 hours
        assert!(initialize_timelock(&env, &admin, delay).is_ok());
        assert_eq!(get_delay(&env), Some(delay));
        assert_eq!(get_proposal_count(&env), 0);
    }

    #[test]
    fn test_initialize_timelock_delay_too_short() {
        let (env, admin) = setup();
        let result = initialize_timelock(&env, &admin, 100);
        assert_eq!(result, Err(TimelockError::InvalidDelay));
    }

    #[test]
    fn test_initialize_timelock_delay_too_long() {
        let (env, admin) = setup();
        let result = initialize_timelock(&env, &admin, MAX_TIMELOCK_DELAY + 1);
        assert_eq!(result, Err(TimelockError::InvalidDelay));
    }

    #[test]
    fn test_initialize_timelock_unauthorized() {
        let (env, _admin) = setup();
        let rando = Address::generate(&env);
        let result = initialize_timelock(&env, &rando, 86400);
        assert_eq!(result, Err(TimelockError::Unauthorized));
    }

    // -----------------------------------------------------------------------
    // propose
    // -----------------------------------------------------------------------

    #[test]
    fn test_propose_success() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

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
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let fn_name = Symbol::new(&env, "action");
        let cd: Vec<Bytes> = Vec::new(&env);

        let id0 = propose(&env, &admin, fn_name.clone(), cd.clone()).unwrap();
        let id1 = propose(&env, &admin, fn_name.clone(), cd.clone()).unwrap();
        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
    }

    #[test]
    fn test_propose_unauthorized() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let rando = Address::generate(&env);
        let result = propose(&env, &rando, Symbol::new(&env, "x"), Vec::new(&env));
        assert_eq!(result, Err(TimelockError::Unauthorized));
    }

    // -----------------------------------------------------------------------
    // execute_proposal
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_proposal_success() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        // Advance time past eta
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });

        let executed = execute_proposal(&env, &admin, id).unwrap();
        assert_eq!(executed.status, ProposalStatus::Executed);

        // Verify storage
        let p = get_proposal(&env, id).unwrap();
        assert_eq!(p.status, ProposalStatus::Executed);
    }

    #[test]
    fn test_execute_proposal_too_early() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalNotReady));
    }

    #[test]
    fn test_execute_proposal_expired() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        // Advance time past grace period
        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + GRACE_PERIOD + 1;
        });

        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalExpired));
    }

    #[test]
    fn test_execute_proposal_already_executed() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });

        execute_proposal(&env, &admin, id).unwrap();

        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalAlreadyExecuted));
    }

    #[test]
    fn test_execute_proposal_not_found() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let result = execute_proposal(&env, &admin, 999);
        assert_eq!(result, Err(TimelockError::ProposalNotFound));
    }

    #[test]
    fn test_execute_proposal_unauthorized() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

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
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        cancel_proposal(&env, &admin, id).unwrap();

        let p = get_proposal(&env, id).unwrap();
        assert_eq!(p.status, ProposalStatus::Cancelled);
    }

    #[test]
    fn test_cancel_proposal_already_cancelled() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        cancel_proposal(&env, &admin, id).unwrap();

        let result = cancel_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalAlreadyCancelled));
    }

    #[test]
    fn test_cancel_proposal_already_executed() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        env.ledger().with_mut(|li| {
            li.timestamp += 86400 + 1;
        });

        execute_proposal(&env, &admin, id).unwrap();

        let result = cancel_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalAlreadyExecuted));
    }

    #[test]
    fn test_cancel_proposal_not_found() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

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
        // No delay set yet => admin can set directly.
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
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        // Delay is already set; a direct update without a matching executed
        // proposal should fail.
        let result = update_delay(&env, &admin, 172800);
        assert_eq!(result, Err(TimelockError::ProposalNotReady));
    }

    #[test]
    fn test_update_delay_through_timelock() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

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
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        let p = get_proposal(&env, id).unwrap();

        // Set timestamp to exactly eta
        env.ledger().with_mut(|li| {
            li.timestamp = p.eta;
        });

        let result = execute_proposal(&env, &admin, id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_at_exact_grace_period_end() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id = propose(
            &env,
            &admin,
            Symbol::new(&env, "action"),
            Vec::new(&env),
        )
        .unwrap();

        let p = get_proposal(&env, id).unwrap();

        // Set timestamp to exactly grace_period_end (should be expired)
        env.ledger().with_mut(|li| {
            li.timestamp = p.grace_period_end;
        });

        let result = execute_proposal(&env, &admin, id);
        assert_eq!(result, Err(TimelockError::ProposalExpired));
    }

    #[test]
    fn test_cancel_does_not_affect_other_proposals() {
        let (env, admin) = setup();
        initialize_timelock(&env, &admin, 86400).unwrap();

        let id0 = propose(
            &env,
            &admin,
            Symbol::new(&env, "a"),
            Vec::new(&env),
        )
        .unwrap();

        let id1 = propose(
            &env,
            &admin,
            Symbol::new(&env, "b"),
            Vec::new(&env),
        )
        .unwrap();

        cancel_proposal(&env, &admin, id0).unwrap();

        let p0 = get_proposal(&env, id0).unwrap();
        let p1 = get_proposal(&env, id1).unwrap();
        assert_eq!(p0.status, ProposalStatus::Cancelled);
        assert_eq!(p1.status, ProposalStatus::Pending);
    }
}
