#![no_std]
use soroban_sdk::{contract, contractimpl, contracterror, Address, Env, Symbol, symbol_short};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleError {
    PriceStale = 1,
    PriceZeroOrNegative = 2,
    TimestampInFuture = 3,
    OracleNotFound = 4,
}

#[contract]
pub struct OracleStalenessGuard;

#[contractimpl]
impl OracleStalenessGuard {
    pub const DEFAULT_MAX_STALENESS_SECONDS: u64 = 300; // 5 minutes

    pub fn validate_price(
        env: Env,
        asset: Address,
        price: i128,
        price_timestamp: u64,
        max_staleness: Option<u64>
    ) -> Result<i128, OracleError> {
        if price <= 0 {
            return Err(OracleError::PriceZeroOrNegative);
        }

        let now = env.ledger().timestamp();
        if price_timestamp > now + 30 {
            return Err(OracleError::TimestampInFuture);
        }

        let staleness_threshold = max_staleness.unwrap_or(Self::DEFAULT_MAX_STALENESS_SECONDS);
        if now > price_timestamp + staleness_threshold {
            return Err(OracleError::PriceStale);
        }

        env.events().publish((symbol_short!("PRICE_OK"), asset), (price, price_timestamp));
        Ok(price)
    }
}
