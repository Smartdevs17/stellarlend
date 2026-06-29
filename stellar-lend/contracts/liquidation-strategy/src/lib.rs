#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, Vec};

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum StrategyType {
    FixedDiscount,
    DutchAuction,
    TWAPBased,
    Hybrid,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct LiquidationStrategy {
    pub strategy_type: StrategyType,
    pub pool: Address,
    pub parameters: Bytes,
    pub enabled: bool,
    pub created_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct StrategyValidation {
    pub is_valid: bool,
    pub reason: Bytes,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct LiquidationDiscount {
    pub base_premium_bps: i128,
    pub calculated_discount: i128,
}

#[contract]
pub struct LiquidationStrategyContract;

#[contractimpl]
impl LiquidationStrategyContract {
    /// Initialize the liquidation strategy manager.
    pub fn initialize(env: Env, governance: Address, admin: Address) {
        env.storage().instance().set(&DataKey::Governance, &governance);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::StrategyCount, &0u32);
    }

    /// Register a liquidation strategy for a pool.
    pub fn register_strategy(
        env: Env,
        pool: Address,
        strategy_type: StrategyType,
        parameters: Bytes,
    ) -> u64 {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let strategy = LiquidationStrategy {
            strategy_type: strategy_type.clone(),
            pool: pool.clone(),
            parameters: parameters.clone(),
            enabled: true,
            created_at: env.ledger().timestamp(),
        };

        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::StrategyCount)
            .unwrap_or(0);
        let strategy_id = (count + 1) as u64;

        env.storage()
            .instance()
            .set(&DataKey::Strategy(strategy_id), &strategy);
        env.storage()
            .instance()
            .set(&DataKey::PoolStrategy(pool.clone()), &strategy_id);
        env.storage()
            .instance()
            .set(&DataKey::StrategyCount, &(count + 1));

        env.events()
            .publish(("register_strategy", &pool), &strategy_type);

        strategy_id
    }

    /// Validate strategy parameters before selection.
    pub fn validate_strategy(
        env: Env,
        strategy_type: StrategyType,
        parameters: Bytes,
    ) -> StrategyValidation {
        match strategy_type {
            StrategyType::FixedDiscount => validate_fixed_discount_params(&env, &parameters),
            StrategyType::DutchAuction => validate_dutch_auction_params(&env, &parameters),
            StrategyType::TWAPBased => validate_twap_params(&env, &parameters),
            StrategyType::Hybrid => validate_hybrid_params(&env, &parameters),
        }
    }

    /// Calculate liquidation discount based on strategy.
    pub fn calculate_discount(
        env: Env,
        strategy_id: u64,
        collateral_value: i128,
        debt_value: i128,
        time_since_unhealthy: u64,
    ) -> LiquidationDiscount {
        let strategy: LiquidationStrategy = env
            .storage()
            .instance()
            .get(&DataKey::Strategy(strategy_id))
            .expect("Strategy not found");

        assert!(strategy.enabled, "Strategy is disabled");

        match strategy.strategy_type {
            StrategyType::FixedDiscount => {
                calculate_fixed_discount(&strategy, collateral_value, debt_value)
            }
            StrategyType::DutchAuction => {
                calculate_dutch_auction(&strategy, collateral_value, debt_value, time_since_unhealthy)
            }
            StrategyType::TWAPBased => {
                calculate_twap_discount(&strategy, collateral_value, debt_value)
            }
            StrategyType::Hybrid => {
                calculate_hybrid_discount(
                    &strategy,
                    collateral_value,
                    debt_value,
                    time_since_unhealthy,
                )
            }
        }
    }

    /// Change the strategy for a pool.
    pub fn change_pool_strategy(env: Env, pool: Address, new_strategy_id: u64) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let _strategy: LiquidationStrategy = env
            .storage()
            .instance()
            .get(&DataKey::Strategy(new_strategy_id))
            .expect("Strategy not found");

        env.storage()
            .instance()
            .set(&DataKey::PoolStrategy(pool.clone()), &new_strategy_id);

        env.events()
            .publish(("change_pool_strategy", &pool), &new_strategy_id);
    }

    /// Disable a strategy.
    pub fn disable_strategy(env: Env, strategy_id: u64) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let mut strategy: LiquidationStrategy = env
            .storage()
            .instance()
            .get(&DataKey::Strategy(strategy_id))
            .expect("Strategy not found");

        strategy.enabled = false;

        env.storage()
            .instance()
            .set(&DataKey::Strategy(strategy_id), &strategy);

        env.events().publish(("disable_strategy",), &strategy_id);
    }

    /// Get a strategy.
    pub fn get_strategy(env: Env, strategy_id: u64) -> LiquidationStrategy {
        env.storage()
            .instance()
            .get(&DataKey::Strategy(strategy_id))
            .expect("Strategy not found")
    }

    /// Get the active strategy for a pool.
    pub fn get_pool_strategy(env: Env, pool: Address) -> Option<u64> {
        env.storage()
            .instance()
            .get(&DataKey::PoolStrategy(pool))
    }
}

fn validate_fixed_discount_params(env: &Env, _params: &Bytes) -> StrategyValidation {
    StrategyValidation {
        is_valid: true,
        reason: Bytes::new(env),
    }
}

fn validate_dutch_auction_params(env: &Env, _params: &Bytes) -> StrategyValidation {
    StrategyValidation {
        is_valid: true,
        reason: Bytes::new(env),
    }
}

fn validate_twap_params(env: &Env, _params: &Bytes) -> StrategyValidation {
    StrategyValidation {
        is_valid: true,
        reason: Bytes::new(env),
    }
}

fn validate_hybrid_params(env: &Env, _params: &Bytes) -> StrategyValidation {
    StrategyValidation {
        is_valid: true,
        reason: Bytes::new(env),
    }
}

fn calculate_fixed_discount(
    _strategy: &LiquidationStrategy,
    _collateral_value: i128,
    debt_value: i128,
) -> LiquidationDiscount {
    let base_premium = 1000;
    LiquidationDiscount {
        base_premium_bps: base_premium,
        calculated_discount: (debt_value * base_premium) / 10000,
    }
}

fn calculate_dutch_auction(
    _strategy: &LiquidationStrategy,
    _collateral_value: i128,
    debt_value: i128,
    time_since_unhealthy: u64,
) -> LiquidationDiscount {
    let base_premium = 1000;
    let time_factor = (time_since_unhealthy / 3600) as i128;
    let discount = base_premium + (100 * time_factor);
    LiquidationDiscount {
        base_premium_bps: base_premium,
        calculated_discount: (debt_value * discount) / 10000,
    }
}

fn calculate_twap_discount(
    _strategy: &LiquidationStrategy,
    _collateral_value: i128,
    debt_value: i128,
) -> LiquidationDiscount {
    let base_premium = 1200;
    LiquidationDiscount {
        base_premium_bps: base_premium,
        calculated_discount: (debt_value * base_premium) / 10000,
    }
}

fn calculate_hybrid_discount(
    _strategy: &LiquidationStrategy,
    _collateral_value: i128,
    debt_value: i128,
    time_since_unhealthy: u64,
) -> LiquidationDiscount {
    let base_premium = 1100;
    let time_factor = (time_since_unhealthy / 7200) as i128;
    let discount = base_premium + (50 * time_factor);
    LiquidationDiscount {
        base_premium_bps: base_premium,
        calculated_discount: (debt_value * discount) / 10000,
    }
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Governance,
    Admin,
    StrategyCount,
    Strategy(u64),
    PoolStrategy(Address),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize() {
        let env = soroban_sdk::Env::default();
        let gov = soroban_sdk::Address::generate(&env);
        let admin = soroban_sdk::Address::generate(&env);

        LiquidationStrategyContract::initialize(env.clone(), gov.clone(), admin);

        let stored: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        assert_eq!(stored, gov);
    }
}
