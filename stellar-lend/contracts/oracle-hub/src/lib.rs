#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, Map, Vec};

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PriceFeed {
    pub asset: Bytes,
    pub oracle_address: Address,
    pub priority: u32,
    pub enabled: bool,
    pub stale_threshold_seconds: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PricePoint {
    pub asset: Bytes,
    pub price: i128,
    pub timestamp: u64,
    pub confidence: u32,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct FeedStatus {
    pub asset: Bytes,
    pub status: u32,
    pub last_update: u64,
    pub is_stale: bool,
}

const FEED_STATUS_ACTIVE: u32 = 0;
const FEED_STATUS_STALE: u32 = 1;
const FEED_STATUS_DISABLED: u32 = 2;

#[contract]
pub struct OracleHubContract;

#[contractimpl]
impl OracleHubContract {
    /// Initialize the oracle hub with governance.
    pub fn initialize(env: Env, governance: Address, admin: Address) {
        env.storage().instance().set(&DataKey::Governance, &governance);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::FeedCount, &0u32);
    }

    /// Register a price feed.
    pub fn register_feed(
        env: Env,
        asset: Bytes,
        oracle_address: Address,
        priority: u32,
        stale_threshold_seconds: u64,
    ) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        assert!(priority > 0, "Priority must be positive");
        assert!(
            stale_threshold_seconds > 0,
            "Stale threshold must be positive"
        );

        let feed = PriceFeed {
            asset: asset.clone(),
            oracle_address: oracle_address.clone(),
            priority,
            enabled: true,
            stale_threshold_seconds,
        };

        env.storage()
            .instance()
            .set(&DataKey::Feed(asset.clone()), &feed);

        let count: u32 = env.storage().instance().get(&DataKey::FeedCount).unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::FeedCount, &(count + 1));

        env.events()
            .publish(("register_feed", &asset), &oracle_address);
    }

    /// Update a feed's priority and stale threshold.
    pub fn update_feed(
        env: Env,
        asset: Bytes,
        priority: u32,
        stale_threshold_seconds: u64,
    ) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let mut feed: PriceFeed = env
            .storage()
            .instance()
            .get(&DataKey::Feed(asset.clone()))
            .expect("Feed not found");

        feed.priority = priority;
        feed.stale_threshold_seconds = stale_threshold_seconds;

        env.storage()
            .instance()
            .set(&DataKey::Feed(asset.clone()), &feed);

        env.events().publish(("update_feed", &asset), &priority);
    }

    /// Disable a feed.
    pub fn disable_feed(env: Env, asset: Bytes) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let mut feed: PriceFeed = env
            .storage()
            .instance()
            .get(&DataKey::Feed(asset.clone()))
            .expect("Feed not found");

        feed.enabled = false;

        env.storage()
            .instance()
            .set(&DataKey::Feed(asset.clone()), &feed);

        env.events().publish(("disable_feed",), &asset);
    }

    /// Report a price update from an oracle feed.
    pub fn report_price(env: Env, asset: Bytes, price: i128, confidence: u32) {
        let feed: PriceFeed = env
            .storage()
            .instance()
            .get(&DataKey::Feed(asset.clone()))
            .expect("Feed not found");

        feed.oracle_address.require_auth();
        assert!(feed.enabled, "Feed is disabled");
        assert!(price > 0, "Price must be positive");

        let price_point = PricePoint {
            asset: asset.clone(),
            price,
            timestamp: env.ledger().timestamp(),
            confidence,
        };

        env.storage()
            .instance()
            .set(&DataKey::LatestPrice(asset.clone()), &price_point);

        env.events()
            .publish(("report_price", &asset), (&price, &confidence));
    }

    /// Get the price for an asset, aggregating across feeds.
    pub fn get_price(env: Env, asset: Bytes) -> Option<PricePoint> {
        let price: Option<PricePoint> = env
            .storage()
            .instance()
            .get(&DataKey::LatestPrice(asset.clone()));

        if let Some(mut point) = price {
            let feed: Option<PriceFeed> = env
                .storage()
                .instance()
                .get(&DataKey::Feed(asset.clone()));

            if let Some(f) = feed {
                let current_time = env.ledger().timestamp();
                if current_time - point.timestamp > f.stale_threshold_seconds {
                    return None;
                }
            }

            return Some(point);
        }

        None
    }

    /// Check the health status of all feeds.
    pub fn check_feed_health(env: Env, asset: Bytes) -> FeedStatus {
        let feed: Option<PriceFeed> = env
            .storage()
            .instance()
            .get(&DataKey::Feed(asset.clone()));

        let price: Option<PricePoint> = env
            .storage()
            .instance()
            .get(&DataKey::LatestPrice(asset.clone()));

        let current_time = env.ledger().timestamp();

        let (status, last_update, is_stale) = match (feed, price) {
            (Some(f), Some(p)) => {
                let stale = current_time - p.timestamp > f.stale_threshold_seconds;
                let status = if !f.enabled {
                    FEED_STATUS_DISABLED
                } else if stale {
                    FEED_STATUS_STALE
                } else {
                    FEED_STATUS_ACTIVE
                };
                (status, p.timestamp, stale)
            }
            _ => (FEED_STATUS_DISABLED, 0, true),
        };

        FeedStatus {
            asset,
            status,
            last_update,
            is_stale,
        }
    }
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Governance,
    Admin,
    FeedCount,
    Feed(Bytes),
    LatestPrice(Bytes),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize() {
        let env = soroban_sdk::Env::default();
        let gov = soroban_sdk::Address::generate(&env);
        let admin = soroban_sdk::Address::generate(&env);

        OracleHubContract::initialize(env.clone(), gov.clone(), admin);

        let stored: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        assert_eq!(stored, gov);
    }
}
