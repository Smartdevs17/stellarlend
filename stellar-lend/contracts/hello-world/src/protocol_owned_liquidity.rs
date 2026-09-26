//! # Protocol-Owned Liquidity (POL) Management
//!
//! Enables the protocol to own, deploy, and manage liquidity positions from its
//! treasury reserves — improving depth, reducing reliance on external LPs, and
//! generating fee revenue that flows back to the reserve.
//!
//! ## Operations
//! | Function                     | Who can call | Description                            |
//! |------------------------------|--------------|----------------------------------------|
//! | `initialize_pol`             | Admin        | One-time setup                         |
//! | `deploy_pol_liquidity`       | Admin        | Allocate treasury funds to a pool      |
//! | `withdraw_pol_liquidity`     | Admin        | Reclaim funds from a pool              |
//! | `collect_pol_fees`           | Admin        | Sweep accrued LP fees to treasury      |
//! | `get_pol_position`           | Anyone       | Read position for a pool               |
//! | `get_pol_summary`            | Anyone       | Aggregate stats across all pools       |
//!
//! ## Storage
//! - `PolKey::Position(pool)` — per-pool liquidity position
//! - `PolKey::Summary` — aggregate stats
//! - `PolKey::Initialized` — init guard

use soroban_sdk::{contracterror, contracttype, Address, Env, Vec};

use crate::admin::require_admin;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PolError {
    /// Caller is not the protocol admin
    Unauthorized = 1,
    /// Amount must be positive
    InvalidAmount = 2,
    /// Module has already been initialized
    AlreadyInitialized = 3,
    /// Module has not been initialized yet
    NotInitialized = 4,
    /// No position exists for this pool
    PositionNotFound = 5,
    /// Requested withdrawal exceeds deployed amount
    InsufficientDeployed = 6,
    /// Arithmetic overflow
    Overflow = 7,
    /// Maximum number of POL positions reached
    PositionLimitReached = 8,
}

// ─── Types ───────────────────────────────────────────────────────────────────

/// State of a single protocol-owned liquidity position
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PolPosition {
    /// The pool/AMM contract the liquidity is deployed in
    pub pool: Address,
    /// Asset token deployed into the pool
    pub asset: Address,
    /// Amount of `asset` currently deployed
    pub deployed_amount: i128,
    /// Cumulative LP fees collected so far
    pub fees_collected: i128,
    /// Timestamp of the last deployment or withdrawal
    pub last_updated: u64,
    /// Whether this position is currently active
    pub active: bool,
}

/// Aggregate POL statistics across all positions
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PolSummary {
    /// Total value deployed across all pools (sum of `deployed_amount`)
    pub total_deployed: i128,
    /// Total LP fees collected across all pools
    pub total_fees_collected: i128,
    /// Number of active positions
    pub active_positions: u32,
    /// Number of pools ever used
    pub total_positions_created: u32,
}

// ─── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum PolKey {
    /// Init guard
    Initialized,
    /// Per-pool position
    Position(Address),
    /// List of all pool addresses (for iteration)
    PoolList,
    /// Aggregate summary
    Summary,
}

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum number of distinct POL positions
pub const MAX_POL_POSITIONS: u32 = 50;

// ─── Storage Helpers ─────────────────────────────────────────────────────────

fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<PolKey, bool>(&PolKey::Initialized)
        .unwrap_or(false)
}

fn load_position(env: &Env, pool: &Address) -> Option<PolPosition> {
    env.storage()
        .persistent()
        .get::<PolKey, PolPosition>(&PolKey::Position(pool.clone()))
}

fn save_position(env: &Env, position: &PolPosition) {
    env.storage()
        .persistent()
        .set(&PolKey::Position(position.pool.clone()), position);
}

fn load_summary(env: &Env) -> PolSummary {
    env.storage()
        .persistent()
        .get::<PolKey, PolSummary>(&PolKey::Summary)
        .unwrap_or(PolSummary {
            total_deployed: 0,
            total_fees_collected: 0,
            active_positions: 0,
            total_positions_created: 0,
        })
}

fn save_summary(env: &Env, summary: &PolSummary) {
    env.storage().persistent().set(&PolKey::Summary, summary);
}

fn load_pool_list(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get::<PolKey, Vec<Address>>(&PolKey::PoolList)
        .unwrap_or_else(|| Vec::new(env))
}

fn save_pool_list(env: &Env, list: &Vec<Address>) {
    env.storage().persistent().set(&PolKey::PoolList, list);
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Initialize the POL module (admin-only, one-time).
pub fn initialize_pol(env: &Env, caller: Address) -> Result<(), PolError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| PolError::Unauthorized)?;

    if is_initialized(env) {
        return Err(PolError::AlreadyInitialized);
    }
    env.storage()
        .instance()
        .set(&PolKey::Initialized, &true);
    Ok(())
}

/// Deploy `amount` of `asset` from the protocol treasury into `pool`.
///
/// Creates a new position if one does not exist; increments an existing one.
/// The actual token transfer to the pool is expected to happen outside this
/// function (e.g., through a governance-executed AMM call) — this function
/// records the accounting.
pub fn deploy_pol_liquidity(
    env: &Env,
    caller: Address,
    pool: Address,
    asset: Address,
    amount: i128,
) -> Result<PolPosition, PolError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| PolError::Unauthorized)?;

    if !is_initialized(env) {
        return Err(PolError::NotInitialized);
    }
    if amount <= 0 {
        return Err(PolError::InvalidAmount);
    }

    let now = env.ledger().timestamp();
    let mut summary = load_summary(env);

    let position = match load_position(env, &pool) {
        Some(mut pos) => {
            // Update existing position
            pos.deployed_amount = pos
                .deployed_amount
                .checked_add(amount)
                .ok_or(PolError::Overflow)?;
            pos.last_updated = now;
            if !pos.active {
                pos.active = true;
                summary.active_positions = summary.active_positions.saturating_add(1);
            }
            pos
        }
        None => {
            // New position — check limit
            if summary.total_positions_created >= MAX_POL_POSITIONS {
                return Err(PolError::PositionLimitReached);
            }
            let mut pool_list = load_pool_list(env);
            pool_list.push_back(pool.clone());
            save_pool_list(env, &pool_list);

            summary.active_positions = summary.active_positions.saturating_add(1);
            summary.total_positions_created = summary.total_positions_created.saturating_add(1);

            PolPosition {
                pool: pool.clone(),
                asset: asset.clone(),
                deployed_amount: amount,
                fees_collected: 0,
                last_updated: now,
                active: true,
            }
        }
    };

    summary.total_deployed = summary
        .total_deployed
        .checked_add(amount)
        .ok_or(PolError::Overflow)?;

    save_position(env, &position);
    save_summary(env, &summary);

    env.events().publish(
        (
            soroban_sdk::Symbol::new(env, "pol_deployed"),
            pool.clone(),
        ),
        (asset, amount, now),
    );

    Ok(position)
}

/// Withdraw `amount` of liquidity from `pool` back to the treasury.
///
/// Records the withdrawal in accounting; actual token movement is handled
/// externally via AMM remove-liquidity calls.
pub fn withdraw_pol_liquidity(
    env: &Env,
    caller: Address,
    pool: Address,
    amount: i128,
) -> Result<PolPosition, PolError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| PolError::Unauthorized)?;

    if !is_initialized(env) {
        return Err(PolError::NotInitialized);
    }
    if amount <= 0 {
        return Err(PolError::InvalidAmount);
    }

    let mut position = load_position(env, &pool).ok_or(PolError::PositionNotFound)?;

    if amount > position.deployed_amount {
        return Err(PolError::InsufficientDeployed);
    }

    position.deployed_amount = position
        .deployed_amount
        .checked_sub(amount)
        .ok_or(PolError::Overflow)?;
    position.last_updated = env.ledger().timestamp();

    let mut summary = load_summary(env);
    summary.total_deployed = summary
        .total_deployed
        .checked_sub(amount)
        .ok_or(PolError::Overflow)?;

    if position.deployed_amount == 0 {
        position.active = false;
        summary.active_positions = summary.active_positions.saturating_sub(1);
    }

    save_position(env, &position);
    save_summary(env, &summary);

    env.events().publish(
        (
            soroban_sdk::Symbol::new(env, "pol_withdrawn"),
            pool,
        ),
        (amount, env.ledger().timestamp()),
    );

    Ok(position)
}

/// Record LP fee income for `pool` and accumulate into the summary.
///
/// In production this would be called after harvesting fees from the AMM.
pub fn collect_pol_fees(
    env: &Env,
    caller: Address,
    pool: Address,
    fee_amount: i128,
) -> Result<PolPosition, PolError> {
    caller.require_auth();
    require_admin(env, &caller).map_err(|_| PolError::Unauthorized)?;

    if !is_initialized(env) {
        return Err(PolError::NotInitialized);
    }
    if fee_amount <= 0 {
        return Err(PolError::InvalidAmount);
    }

    let mut position = load_position(env, &pool).ok_or(PolError::PositionNotFound)?;

    position.fees_collected = position
        .fees_collected
        .checked_add(fee_amount)
        .ok_or(PolError::Overflow)?;
    position.last_updated = env.ledger().timestamp();

    let mut summary = load_summary(env);
    summary.total_fees_collected = summary
        .total_fees_collected
        .checked_add(fee_amount)
        .ok_or(PolError::Overflow)?;

    save_position(env, &position);
    save_summary(env, &summary);

    env.events().publish(
        (
            soroban_sdk::Symbol::new(env, "pol_fees_collected"),
            pool,
        ),
        (fee_amount, env.ledger().timestamp()),
    );

    Ok(position)
}

/// Return the current POL position for `pool`, if any.
pub fn get_pol_position(env: &Env, pool: &Address) -> Option<PolPosition> {
    load_position(env, pool)
}

/// Return the aggregate POL summary.
pub fn get_pol_summary(env: &Env) -> PolSummary {
    load_summary(env)
}

/// Return the list of all pool addresses that have had POL positions.
pub fn get_pol_pool_list(env: &Env) -> Vec<Address> {
    load_pool_list(env)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deposit::DepositDataKey;
    use soroban_sdk::{testutils::Address as _, Address, Env};

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
    fn test_initialize_pol() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();
        // Second call should fail
        let result = initialize_pol(&env, admin.clone());
        assert_eq!(result, Err(PolError::AlreadyInitialized));
    }

    #[test]
    fn test_deploy_liquidity_creates_position() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool = Address::generate(&env);
        let asset = Address::generate(&env);

        let position =
            deploy_pol_liquidity(&env, admin.clone(), pool.clone(), asset.clone(), 100_000)
                .unwrap();

        assert_eq!(position.deployed_amount, 100_000);
        assert!(position.active);

        let summary = get_pol_summary(&env);
        assert_eq!(summary.total_deployed, 100_000);
        assert_eq!(summary.active_positions, 1);
        assert_eq!(summary.total_positions_created, 1);
    }

    #[test]
    fn test_deploy_increments_existing_position() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool = Address::generate(&env);
        let asset = Address::generate(&env);

        deploy_pol_liquidity(&env, admin.clone(), pool.clone(), asset.clone(), 50_000).unwrap();
        deploy_pol_liquidity(&env, admin.clone(), pool.clone(), asset.clone(), 50_000).unwrap();

        let pos = get_pol_position(&env, &pool).unwrap();
        assert_eq!(pos.deployed_amount, 100_000);

        let summary = get_pol_summary(&env);
        assert_eq!(summary.total_deployed, 100_000);
        assert_eq!(summary.total_positions_created, 1); // still one pool
    }

    #[test]
    fn test_withdraw_liquidity() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool = Address::generate(&env);
        let asset = Address::generate(&env);

        deploy_pol_liquidity(&env, admin.clone(), pool.clone(), asset.clone(), 100_000).unwrap();
        let pos = withdraw_pol_liquidity(&env, admin.clone(), pool.clone(), 40_000).unwrap();

        assert_eq!(pos.deployed_amount, 60_000);
        assert!(pos.active);

        let summary = get_pol_summary(&env);
        assert_eq!(summary.total_deployed, 60_000);
        assert_eq!(summary.active_positions, 1);
    }

    #[test]
    fn test_full_withdrawal_deactivates_position() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool = Address::generate(&env);
        let asset = Address::generate(&env);

        deploy_pol_liquidity(&env, admin.clone(), pool.clone(), asset.clone(), 100_000).unwrap();
        let pos = withdraw_pol_liquidity(&env, admin.clone(), pool.clone(), 100_000).unwrap();

        assert_eq!(pos.deployed_amount, 0);
        assert!(!pos.active);

        let summary = get_pol_summary(&env);
        assert_eq!(summary.active_positions, 0);
        assert_eq!(summary.total_deployed, 0);
    }

    #[test]
    fn test_withdraw_excess_rejected() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool = Address::generate(&env);
        let asset = Address::generate(&env);

        deploy_pol_liquidity(&env, admin.clone(), pool.clone(), asset.clone(), 50_000).unwrap();
        let result = withdraw_pol_liquidity(&env, admin.clone(), pool.clone(), 100_000);
        assert_eq!(result, Err(PolError::InsufficientDeployed));
    }

    #[test]
    fn test_collect_fees() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool = Address::generate(&env);
        let asset = Address::generate(&env);

        deploy_pol_liquidity(&env, admin.clone(), pool.clone(), asset.clone(), 100_000).unwrap();
        let pos = collect_pol_fees(&env, admin.clone(), pool.clone(), 1_500).unwrap();
        assert_eq!(pos.fees_collected, 1_500);

        let summary = get_pol_summary(&env);
        assert_eq!(summary.total_fees_collected, 1_500);
    }

    #[test]
    fn test_collect_fees_nonexistent_pool_rejected() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool = Address::generate(&env);
        let result = collect_pol_fees(&env, admin.clone(), pool, 100);
        assert_eq!(result, Err(PolError::PositionNotFound));
    }

    #[test]
    fn test_unauthorized_operations_rejected() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let stranger = Address::generate(&env);
        let pool = Address::generate(&env);
        let asset = Address::generate(&env);

        let r1 = deploy_pol_liquidity(&env, stranger.clone(), pool.clone(), asset.clone(), 1000);
        assert_eq!(r1, Err(PolError::Unauthorized));

        let r2 = withdraw_pol_liquidity(&env, stranger.clone(), pool.clone(), 100);
        assert_eq!(r2, Err(PolError::Unauthorized));

        let r3 = collect_pol_fees(&env, stranger.clone(), pool.clone(), 10);
        assert_eq!(r3, Err(PolError::Unauthorized));
    }

    #[test]
    fn test_deploy_before_init_rejected() {
        let (env, admin) = setup();
        let pool = Address::generate(&env);
        let asset = Address::generate(&env);
        let result = deploy_pol_liquidity(&env, admin, pool, asset, 100_000);
        assert_eq!(result, Err(PolError::NotInitialized));
    }

    #[test]
    fn test_zero_amount_rejected() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool = Address::generate(&env);
        let asset = Address::generate(&env);

        let result = deploy_pol_liquidity(&env, admin.clone(), pool.clone(), asset.clone(), 0);
        assert_eq!(result, Err(PolError::InvalidAmount));
    }

    #[test]
    fn test_pool_list_tracks_pools() {
        let (env, admin) = setup();
        initialize_pol(&env, admin.clone()).unwrap();

        let pool1 = Address::generate(&env);
        let pool2 = Address::generate(&env);
        let asset = Address::generate(&env);

        deploy_pol_liquidity(&env, admin.clone(), pool1.clone(), asset.clone(), 1000).unwrap();
        deploy_pol_liquidity(&env, admin.clone(), pool2.clone(), asset.clone(), 2000).unwrap();

        let list = get_pol_pool_list(&env);
        assert_eq!(list.len(), 2);
    }
}
