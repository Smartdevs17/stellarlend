//! # Pending-Liquidation Dutch Auction with Configurable Decay
//!
//! When a position becomes undercollateralized and enters liquidation, instead
//! of immediately executing a fixed-discount liquidation, the collateral is
//! placed into a Dutch auction. The auction price starts at the oracle price
//! and decays linearly over a configurable duration, incentivizing liquidators
//! to act quickly for a better discount while giving the position time to
//! recover.
//!
//! ## Design
//! - Admin configures the auction duration and decay rate (max discount at end).
//! - When a liquidation is triggered, an auction is created for the collateral.
//! - The auction price decays linearly from the start price to the end price.
//! - Liquidators buy collateral at the current auction price.
//! - The auction can be cancelled if the position becomes healthy again.
//! - Every state change emits an event for observability.

use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Symbol};

use crate::admin;

// ── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum DutchAuctionError {
    Unauthorized = 1,
    InvalidDuration = 2,
    InvalidDecay = 3,
    AuctionNotFound = 4,
    AuctionExpired = 5,
    AuctionCancelled = 6,
    InvalidAmount = 7,
    InsufficientPayment = 8,
    Overflow = 9,
    PositionHealthy = 10,
    AuctionAlreadyExists = 11,
}

// ── Types ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum AuctionDataKey {
    /// Auction configuration.
    Config,
    /// Active auction by ID.
    Auction(u64),
    /// Counter for auction IDs.
    AuctionCounter,
    /// Pending liquidation position.
    PendingLiquidation(Address),
}

/// Configuration for the Dutch auction.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuctionConfig {
    /// Duration of the auction in seconds.
    pub duration: u64,
    /// Maximum discount at the end of the auction in basis points (e.g., 2000 = 20%).
    pub max_decay_bps: i128,
}

/// A pending liquidation Dutch auction.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAuction {
    pub id: u64,
    pub borrower: Address,
    pub collateral_amount: i128,
    pub start_price: i128,
    pub end_price: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub config: AuctionConfig,
    pub status: AuctionStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuctionStatus {
    Active,
    Settled,
    Cancelled,
}

// ── Admin: Configuration ────────────────────────────────────────────────────

/// Set the Dutch auction configuration.
pub fn set_auction_config(
    env: &Env,
    admin: Address,
    config: AuctionConfig,
) -> Result<(), DutchAuctionError> {
    admin::require_admin(env, &admin).map_err(|_| DutchAuctionError::Unauthorized)?;
    if config.duration == 0 {
        return Err(DutchAuctionError::InvalidDuration);
    }
    if config.max_decay_bps < 0 || config.max_decay_bps > 10000 {
        return Err(DutchAuctionError::InvalidDecay);
    }
    env.storage()
        .persistent()
        .set(&AuctionDataKey::Config, &config);
    env.events().publish(
        (Symbol::new(env, "auction_config_set"), admin),
        (config.duration, config.max_decay_bps),
    );
    Ok(())
}

/// Get the current auction configuration.
pub fn get_auction_config(env: &Env) -> AuctionConfig {
    env.storage()
        .persistent()
        .get(&AuctionDataKey::Config)
        .unwrap_or(AuctionConfig {
            duration: 3600,
            max_decay_bps: 2000,
        })
}

// ── Auction lifecycle ───────────────────────────────────────────────────────

/// Create a pending-liquidation Dutch auction for a borrower's collateral.
///
/// The auction price starts at `start_price` and decays linearly to `end_price`
/// over the configured duration. `end_price` = start_price * (1 - max_decay_bps/10000).
pub fn create_auction(
    env: &Env,
    caller: Address,
    borrower: Address,
    collateral_amount: i128,
    start_price: i128,
) -> Result<PendingAuction, DutchAuctionError> {
    caller.require_auth();
    if collateral_amount <= 0 || start_price <= 0 {
        return Err(DutchAuctionError::InvalidAmount);
    }

    // Check no existing auction for this borrower.
    let existing_key = AuctionDataKey::PendingLiquidation(borrower.clone());
    if env.storage().persistent().get::<_, u64>(&existing_key).is_some() {
        return Err(DutchAuctionError::AuctionAlreadyExists);
    }

    let config = get_auction_config(env);
    let now = env.ledger().timestamp();
    let end_time = now.checked_add(config.duration).ok_or(DutchAuctionError::Overflow)?;

    let decay = start_price
        .checked_mul(config.max_decay_bps)
        .and_then(|v| v.checked_div(10000))
        .ok_or(DutchAuctionError::Overflow)?;
    let end_price = start_price.checked_sub(decay).ok_or(DutchAuctionError::Overflow)?;

    let counter: u64 = env
        .storage()
        .persistent()
        .get(&AuctionDataKey::AuctionCounter)
        .unwrap_or(0);
    let id = counter.checked_add(1).ok_or(DutchAuctionError::Overflow)?;
    env.storage()
        .persistent()
        .set(&AuctionDataKey::AuctionCounter, &id);

    let auction = PendingAuction {
        id,
        borrower: borrower.clone(),
        collateral_amount,
        start_price,
        end_price,
        start_time: now,
        end_time,
        config,
        status: AuctionStatus::Active,
    };

    env.storage()
        .persistent()
        .set(&AuctionDataKey::Auction(id), &auction);
    env.storage()
        .persistent()
        .set(&existing_key, &id);

    env.events().publish(
        (Symbol::new(env, "auction_created"), borrower, id),
        (collateral_amount, start_price, end_time),
    );
    Ok(auction)
}

/// Get the current auction price at a given timestamp.
///
/// Price decays linearly from `start_price` to `end_price` over the duration.
pub fn get_current_price(env: &Env, auction_id: u64) -> Result<i128, DutchAuctionError> {
    let auction = get_auction(env, auction_id)?;
    if auction.status != AuctionStatus::Active {
        return Err(DutchAuctionError::AuctionNotFound);
    }
    let now = env.ledger().timestamp();
    if now >= auction.end_time {
        return Ok(auction.end_price);
    }
    if now <= auction.start_time {
        return Ok(auction.start_price);
    }

    let elapsed = now - auction.start_time;
    let duration = auction.end_time - auction.start_time;
    let price_diff = auction
        .start_price
        .checked_sub(auction.end_price)
        .ok_or(DutchAuctionError::Overflow)?;
    let decay = price_diff
        .checked_mul(elapsed as i128)
        .and_then(|v| v.checked_div(duration as i128))
        .ok_or(DutchAuctionError::Overflow)?;
    auction
        .start_price
        .checked_sub(decay)
        .ok_or(DutchAuctionError::Overflow)
}

/// Buy collateral from an active auction at the current price.
///
/// Returns the amount of collateral purchased and the price paid.
pub fn buy_auction(
    env: &Env,
    buyer: Address,
    auction_id: u64,
    collateral_amount: i128,
) -> Result<(i128, i128), DutchAuctionError> {
    buyer.require_auth();
    let mut auction = get_auction(env, auction_id)?;
    if auction.status != AuctionStatus::Active {
        return Err(DutchAuctionError::AuctionNotFound);
    }
    let now = env.ledger().timestamp();
    if now > auction.end_time {
        return Err(DutchAuctionError::AuctionExpired);
    }
    if collateral_amount <= 0 || collateral_amount > auction.collateral_amount {
        return Err(DutchAuctionError::InvalidAmount);
    }

    let price = get_current_price(env, auction_id)?;
    let payment = price
        .checked_mul(collateral_amount)
        .ok_or(DutchAuctionError::Overflow)?;

    // Update auction.
    auction.collateral_amount -= collateral_amount;
    if auction.collateral_amount == 0 {
        auction.status = AuctionStatus::Settled;
    }
    env.storage()
        .persistent()
        .set(&AuctionDataKey::Auction(auction_id), &auction);

    // Clear pending liquidation if fully settled.
    if auction.status == AuctionStatus::Settled {
        env.storage()
            .persistent()
            .remove(&AuctionDataKey::PendingLiquidation(auction.borrower.clone()));
    }

    env.events().publish(
        (Symbol::new(env, "auction_bought"), buyer, auction_id),
        (collateral_amount, price, payment),
    );
    Ok((collateral_amount, payment))
}

/// Cancel an auction (e.g., position became healthy again).
pub fn cancel_auction(
    env: &Env,
    caller: Address,
    auction_id: u64,
) -> Result<(), DutchAuctionError> {
    caller.require_auth();
    let mut auction = get_auction(env, auction_id)?;
    if auction.status != AuctionStatus::Active {
        return Err(DutchAuctionError::AuctionNotFound);
    }
    auction.status = AuctionStatus::Cancelled;
    env.storage()
        .persistent()
        .set(&AuctionDataKey::Auction(auction_id), &auction);
    env.storage()
        .persistent()
        .remove(&AuctionDataKey::PendingLiquidation(auction.borrower.clone()));

    env.events().publish(
        (Symbol::new(env, "auction_cancelled"), caller, auction_id),
        (),
    );
    Ok(())
}

/// Get an auction by ID.
pub fn get_auction(env: &Env, auction_id: u64) -> Result<PendingAuction, DutchAuctionError> {
    env.storage()
        .persistent()
        .get(&AuctionDataKey::Auction(auction_id))
        .ok_or(DutchAuctionError::AuctionNotFound)
}

/// Get the pending auction ID for a borrower, if any.
pub fn get_pending_auction_id(env: &Env, borrower: &Address) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&AuctionDataKey::PendingLiquidation(borrower.clone()))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{contract, contractimpl};

    #[contract]
    struct AuctionTestContract;

    #[contractimpl]
    impl AuctionTestContract {}

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, AuctionTestContract);
        env.as_contract(&contract_id, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
        });
        (env, admin, contract_id)
    }

    #[test]
    fn auction_price_decays_linearly() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_auction_config(
                &env,
                admin,
                AuctionConfig {
                    duration: 1000,
                    max_decay_bps: 2000, // 20% max discount
                },
            )
            .unwrap();

            let caller = Address::generate(&env);
            let borrower = Address::generate(&env);
            let auction = create_auction(&env, caller, borrower, 100, 1000).unwrap();
            assert_eq!(auction.start_price, 1000);
            assert_eq!(auction.end_price, 800); // 20% discount

            // At start, price = start_price.
            assert_eq!(get_current_price(&env, auction.id).unwrap(), 1000);

            // At midpoint, price should be ~900.
            env.ledger().with_mut(|l| l.timestamp = 600);
            let mid_price = get_current_price(&env, auction.id).unwrap();
            assert_eq!(mid_price, 900);

            // At end, price = end_price.
            env.ledger().with_mut(|l| l.timestamp = 1100);
            assert_eq!(get_current_price(&env, auction.id).unwrap(), 800);
        });
    }

    #[test]
    fn buy_auction_at_current_price() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_auction_config(
                &env,
                admin,
                AuctionConfig {
                    duration: 1000,
                    max_decay_bps: 2000,
                },
            )
            .unwrap();

            let caller = Address::generate(&env);
            let borrower = Address::generate(&env);
            let buyer = Address::generate(&env);
            create_auction(&env, caller, borrower, 100, 1000).unwrap();

            // Buy at start price.
            let (amount, payment) = buy_auction(&env, buyer, 1, 50).unwrap();
            assert_eq!(amount, 50);
            assert_eq!(payment, 50000); // 50 * 1000

            // Remaining collateral.
            let auction = get_auction(&env, 1).unwrap();
            assert_eq!(auction.collateral_amount, 50);
        });
    }

    #[test]
    fn auction_settles_when_fully_bought() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_auction_config(
                &env,
                admin,
                AuctionConfig {
                    duration: 1000,
                    max_decay_bps: 2000,
                },
            )
            .unwrap();

            let caller = Address::generate(&env);
            let borrower = Address::generate(&env);
            let buyer = Address::generate(&env);
            create_auction(&env, caller, borrower, 100, 1000).unwrap();

            buy_auction(&env, buyer, 1, 100).unwrap();
            let auction = get_auction(&env, 1).unwrap();
            assert_eq!(auction.status, AuctionStatus::Settled);
            assert!(get_pending_auction_id(&env, &borrower).is_none());
        });
    }

    #[test]
    fn cancel_auction_restores_position() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_auction_config(
                &env,
                admin,
                AuctionConfig {
                    duration: 1000,
                    max_decay_bps: 2000,
                },
            )
            .unwrap();

            let caller = Address::generate(&env);
            let borrower = Address::generate(&env);
            create_auction(&env, caller, borrower, 100, 1000).unwrap();
            assert!(get_pending_auction_id(&env, &borrower).is_some());

            cancel_auction(&env, caller, 1).unwrap();
            let auction = get_auction(&env, 1).unwrap();
            assert_eq!(auction.status, AuctionStatus::Cancelled);
            assert!(get_pending_auction_id(&env, &borrower).is_none());
        });
    }

    #[test]
    fn duplicate_auction_rejected() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_auction_config(
                &env,
                admin,
                AuctionConfig {
                    duration: 1000,
                    max_decay_bps: 2000,
                },
            )
            .unwrap();

            let caller = Address::generate(&env);
            let borrower = Address::generate(&env);
            create_auction(&env, caller.clone(), borrower.clone(), 100, 1000).unwrap();
            assert_eq!(
                create_auction(&env, caller, borrower, 100, 1000),
                Err(DutchAuctionError::AuctionAlreadyExists)
            );
        });
    }

    #[test]
    fn expired_auction_rejects_buy() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_auction_config(
                &env,
                admin,
                AuctionConfig {
                    duration: 100,
                    max_decay_bps: 2000,
                },
            )
            .unwrap();

            let caller = Address::generate(&env);
            let borrower = Address::generate(&env);
            let buyer = Address::generate(&env);
            create_auction(&env, caller, borrower, 100, 1000).unwrap();

            env.ledger().with_mut(|l| l.timestamp = 500);
            assert_eq!(
                buy_auction(&env, buyer, 1, 10),
                Err(DutchAuctionError::AuctionExpired)
            );
        });
    }
}